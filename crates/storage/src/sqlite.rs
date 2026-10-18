//! `SqliteStorage`: a SQLite-backed implementation of [`Storage`].
//!
//! # Concurrency model (spec: "Connection pool discipline")
//!
//! A file-backed store opens a pool of `N = min(4, cpu_count)` **read**
//! connections (WAL mode, foreign keys enabled) plus a single dedicated writer
//! task — an OS thread owning the only writing connection — fed through an
//! `mpsc` channel. Every write is therefore serialized through that one task,
//! so no concurrent writers exist.
//!
//! In-memory stores (a test-only helper) use a single connection shared by the
//! reader pool and the writer, because a SQLite in-memory database is private
//! to its connection.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;

use serde_json::Value;

use crate::error::StorageError;
use crate::migrations::{apply_migrations, load_migrations, Migration};
use crate::storage::{Key, Row, Storage};

/// A shared, individually-locked SQLite connection.
type SharedConn = Arc<Mutex<rusqlite::Connection>>;

/// A transactional unit of work for the dedicated writer task.
type TxWork = Box<dyn FnOnce(&rusqlite::Transaction<'_>) -> Result<(), StorageError> + Send>;

/// A unit of work for the dedicated writer task.
enum Job {
    /// Execute a single statement and report the affected row count.
    Execute {
        sql: String,
        params: Vec<Value>,
        reply: SyncSender<Result<usize, StorageError>>,
    },
    /// Run a closure inside a transaction (committed on `Ok`).
    Transaction {
        work: TxWork,
        reply: SyncSender<Result<(), StorageError>>,
    },
}

/// The write path: a dedicated task (file stores) or a shared connection
/// (in-memory stores).
enum WriteTarget {
    Thread {
        tx: Sender<Job>,
        _join: JoinHandle<()>,
    },
    Direct(SharedConn),
}

/// A SQLite-backed [`Storage`].
pub struct SqliteStorage {
    readers: Vec<SharedConn>,
    next_reader: AtomicUsize,
    writer: WriteTarget,
}

impl SqliteStorage {
    /// Open (or create) the database at `db_path` and apply migrations from
    /// `migrations_dir`.
    ///
    /// Creates a pool of `min(4, cpu_count)` read connections and a single
    /// writer task. WAL mode and foreign keys are enabled on every connection.
    pub fn open(db_path: &Path, migrations_dir: &Path) -> Result<Self, StorageError> {
        let migrations = load_migrations(migrations_dir)?;
        Self::open_with_migrations(db_path, &migrations)
    }

    /// Open (or create) the database at `db_path` and apply the given
    /// migrations.
    ///
    /// This is the parameterized entry point used for schemas that do not live
    /// under `<root>/global` (e.g. the per-brain schema in `migrations/brain`).
    pub fn open_with_migrations(
        db_path: &Path,
        migrations: &[Migration],
    ) -> Result<Self, StorageError> {
        // The writer connection applies migrations first so the pool of
        // read-only connections always observes a fully-migrated schema.
        let mut writer_conn = rusqlite::Connection::open(db_path)?;
        configure(&writer_conn)?;
        apply_migrations(&mut writer_conn, migrations)?;

        let mut readers = Vec::with_capacity(read_pool_size());
        for _ in 0..read_pool_size() {
            let conn = rusqlite::Connection::open(db_path)?;
            configure(&conn)?;
            readers.push(Arc::new(Mutex::new(conn)));
        }

        let (tx, rx) = std::sync::mpsc::channel::<Job>();
        let join = std::thread::Builder::new()
            .name("fergents-storage-writer".to_string())
            .spawn(move || writer_loop(writer_conn, &rx))?;

        Ok(Self {
            readers,
            next_reader: AtomicUsize::new(0),
            writer: WriteTarget::Thread { tx, _join: join },
        })
    }

    /// Open a private in-memory database and apply migrations. Useful for tests.
    pub fn open_in_memory(migrations_dir: &Path) -> Result<Self, StorageError> {
        let migrations = load_migrations(migrations_dir)?;
        Self::open_in_memory_with_migrations(&migrations)
    }

    /// Open a private in-memory database and apply the given migrations.
    pub fn open_in_memory_with_migrations(migrations: &[Migration]) -> Result<Self, StorageError> {
        let mut conn = rusqlite::Connection::open_in_memory()?;
        configure(&conn)?;
        apply_migrations(&mut conn, migrations)?;

        let shared: SharedConn = Arc::new(Mutex::new(conn));
        Ok(Self {
            readers: vec![shared.clone()],
            next_reader: AtomicUsize::new(0),
            writer: WriteTarget::Direct(shared),
        })
    }

    /// Run `f` inside a single SQLite transaction.
    ///
    /// If `f` returns `Err`, the transaction is rolled back and no partial
    /// writes are persisted.
    pub fn transaction<T>(
        &self,
        f: impl FnOnce(&rusqlite::Transaction) -> Result<T, StorageError> + Send + 'static,
    ) -> Result<T, StorageError>
    where
        T: Send + 'static,
    {
        match &self.writer {
            WriteTarget::Thread { tx, .. } => {
                let (value_tx, value_rx) = sync_channel::<T>(1);
                let (reply_tx, reply_rx) = sync_channel::<Result<(), StorageError>>(1);
                let work: TxWork = Box::new(move |transaction| {
                    let value = f(transaction)?;
                    let _ = value_tx.send(value);
                    Ok(())
                });
                tx.send(Job::Transaction {
                    work,
                    reply: reply_tx,
                })
                .map_err(|_| StorageError::WriterGone)?;
                reply_rx.recv().map_err(|_| StorageError::WriterGone)??;
                value_rx.recv().map_err(|_| StorageError::WriterGone)
            }
            WriteTarget::Direct(conn) => {
                let mut guard = lock(conn)?;
                let tx = guard.transaction()?;
                let value = f(&tx)?;
                tx.commit()?;
                Ok(value)
            }
        }
    }

    /// Acquire the next reader connection (round-robin).
    fn acquire_reader(&self) -> SharedConn {
        let index = self.next_reader.fetch_add(1, Ordering::Relaxed) % self.readers.len();
        self.readers[index].clone()
    }
}

impl Storage for SqliteStorage {
    fn get(&self, key: &Key) -> Result<Option<Value>, StorageError> {
        let rows = self.query(
            "SELECT value_json FROM settings WHERE key = ?1",
            &[Value::String(key.0.clone())],
        )?;
        let Some(row) = rows.into_iter().next() else {
            return Ok(None);
        };
        let raw = row
            .get("value_json")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                StorageError::Corrupt(format!(
                    "settings row for key '{}' has no string value_json",
                    key.0
                ))
            })?;
        Ok(Some(serde_json::from_str(raw)?))
    }

    fn put(&self, key: Key, value: Value) -> Result<(), StorageError> {
        let json = serde_json::to_string(&value)?;
        self.execute(
            "INSERT INTO settings (key, value_json, updated_at) VALUES (?1, ?2, ?3) \
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, \
             updated_at = excluded.updated_at",
            &[
                Value::String(key.0),
                Value::String(json),
                Value::Number(crate::now_ms().into()),
            ],
        )?;
        Ok(())
    }

    fn delete(&self, key: &Key) -> Result<(), StorageError> {
        self.execute(
            "DELETE FROM settings WHERE key = ?1",
            &[Value::String(key.0.clone())],
        )?;
        Ok(())
    }

    fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>, StorageError> {
        let conn = self.acquire_reader();
        let guard = lock(&conn)?;
        query_conn(&guard, sql, params)
    }

    fn execute(&self, sql: &str, params: &[Value]) -> Result<usize, StorageError> {
        match &self.writer {
            WriteTarget::Thread { tx, .. } => {
                let (reply_tx, reply_rx) = sync_channel(1);
                tx.send(Job::Execute {
                    sql: sql.to_string(),
                    params: params.to_vec(),
                    reply: reply_tx,
                })
                .map_err(|_| StorageError::WriterGone)?;
                reply_rx.recv().map_err(|_| StorageError::WriterGone)?
            }
            WriteTarget::Direct(conn) => {
                let guard = lock(conn)?;
                execute_conn(&guard, sql, params)
            }
        }
    }
}

/// The dedicated writer loop: owns the only writing connection.
fn writer_loop(mut conn: rusqlite::Connection, rx: &Receiver<Job>) {
    while let Ok(job) = rx.recv() {
        match job {
            Job::Execute { sql, params, reply } => {
                let _ = reply.send(execute_conn(&conn, &sql, &params));
            }
            Job::Transaction { work, reply } => {
                let _ = reply.send(run_transaction(&mut conn, work));
            }
        }
    }
}

/// Run `work` inside a transaction on the writer connection.
fn run_transaction(conn: &mut rusqlite::Connection, work: TxWork) -> Result<(), StorageError> {
    let tx = conn.transaction()?;
    work(&tx)?;
    tx.commit()?;
    Ok(())
}

/// Apply the per-connection pragmas (WAL, foreign keys).
fn configure(conn: &rusqlite::Connection) -> Result<(), StorageError> {
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
    Ok(())
}

/// The read-pool size: `min(4, cpu_count)`, at least 1.
fn read_pool_size() -> usize {
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .clamp(1, 4)
}

fn lock(conn: &SharedConn) -> Result<MutexGuard<'_, rusqlite::Connection>, StorageError> {
    conn.lock().map_err(|_| StorageError::Lock)
}

/// Execute a statement, returning the affected row count.
fn execute_conn(
    conn: &rusqlite::Connection,
    sql: &str,
    params: &[Value],
) -> Result<usize, StorageError> {
    let bound: Vec<rusqlite::types::Value> = params.iter().map(json_to_sqlite).collect();
    Ok(conn.execute(sql, rusqlite::params_from_iter(bound))?)
}

/// Run a query, returning rows as JSON maps.
fn query_conn(
    conn: &rusqlite::Connection,
    sql: &str,
    params: &[Value],
) -> Result<Vec<Row>, StorageError> {
    let mut stmt = conn.prepare(sql)?;
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let bound: Vec<rusqlite::types::Value> = params.iter().map(json_to_sqlite).collect();
    let mut rows = stmt.query(rusqlite::params_from_iter(bound))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let mut map = Row::new();
        for (i, name) in names.iter().enumerate() {
            map.insert(name.clone(), sqlite_to_json(row.get(i)?));
        }
        out.push(map);
    }
    Ok(out)
}

/// Convert a JSON value into a SQLite bind value.
fn json_to_sqlite(value: &Value) -> rusqlite::types::Value {
    match value {
        Value::Null => rusqlite::types::Value::Null,
        Value::Bool(b) => rusqlite::types::Value::Integer(i64::from(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                rusqlite::types::Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                rusqlite::types::Value::Real(f)
            } else {
                rusqlite::types::Value::Null
            }
        }
        Value::String(s) => rusqlite::types::Value::Text(s.clone()),
        other => rusqlite::types::Value::Text(other.to_string()),
    }
}

/// Convert a SQLite value into a JSON value.
fn sqlite_to_json(value: rusqlite::types::Value) -> Value {
    match value {
        rusqlite::types::Value::Null => Value::Null,
        rusqlite::types::Value::Integer(i) => Value::Number(i.into()),
        rusqlite::types::Value::Real(f) => {
            serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number)
        }
        rusqlite::types::Value::Text(s) => Value::String(s),
        rusqlite::types::Value::Blob(b) => Value::Array(
            b.into_iter()
                .map(|byte| Value::Number(byte.into()))
                .collect(),
        ),
    }
}
