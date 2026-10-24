//! PTY sessions: open, input, output, resize, close, and the agent-facing
//! bounded reads (task 3.4).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use fergents_events::{now_ms, EventBus};
use fergents_ide::ProjectService;
use fergents_storage::{AuditEntry, Storage, Value};
use portable_pty::{native_pty_system, Child, ChildKiller, MasterPty, PtySize};
use serde_json::json;

use crate::error::TerminalError;
use crate::model::{SessionState, TerminalSession};
use crate::ring::{redact, RingBuffer};
use crate::shell;

/// Default scrollback per session (spec §4: 1 MB).
pub const DEFAULT_RING_BYTES: usize = 1024 * 1024;
/// Default output batch interval (spec §4: ~16 ms).
pub const DEFAULT_BATCH: Duration = Duration::from_millis(16);
/// Default bounded read size for agents.
pub const DEFAULT_READ_BYTES: usize = 8 * 1024;
/// Default bounded read lines for agents.
pub const DEFAULT_READ_LINES: usize = 200;
/// Default terminal size.
pub const DEFAULT_COLS: u16 = 80;
/// Default terminal rows.
pub const DEFAULT_ROWS: u16 = 24;

/// One live PTY session.
struct Session {
    record: TerminalSession,
    ring: Arc<Mutex<RingBuffer>>,
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    state: Mutex<SessionState>,
    exit_code: Mutex<Option<i64>>,
    dimensions: Mutex<(u16, u16)>,
}

/// The terminal subsystem.
pub struct TerminalService {
    storage: Arc<dyn Storage>,
    bus: Arc<EventBus>,
    projects: Arc<ProjectService>,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    ring_bytes: usize,
    batch: Duration,
}

impl TerminalService {
    /// Create the service.
    pub fn new(
        storage: Arc<dyn Storage>,
        bus: Arc<EventBus>,
        projects: Arc<ProjectService>,
    ) -> Self {
        Self::with_limits(storage, bus, projects, DEFAULT_RING_BYTES, DEFAULT_BATCH)
    }

    /// Create the service with explicit ring size and batch interval (tests).
    pub fn with_limits(
        storage: Arc<dyn Storage>,
        bus: Arc<EventBus>,
        projects: Arc<ProjectService>,
        ring_bytes: usize,
        batch: Duration,
    ) -> Self {
        Self {
            storage,
            bus,
            projects,
            sessions: Mutex::new(HashMap::new()),
            ring_bytes,
            batch,
        }
    }

    /// Mark sessions left `running` by a previous kernel as `closed`.
    pub fn reconcile(&self) -> Result<usize, TerminalError> {
        let affected = self.storage.execute(
            "UPDATE terminal_sessions SET state = 'closed', closed_at = ?1 WHERE state = 'running'",
            &[Value::from(now_ms())],
        )?;
        Ok(affected)
    }

    /// Open a terminal in a trusted project.
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        &self,
        project_id: &str,
        shell_name: Option<&str>,
        cwd: Option<&str>,
        cols: Option<u16>,
        rows: Option<u16>,
        actor: &str,
    ) -> Result<TerminalSession, TerminalError> {
        let project = self.projects.require_trusted(project_id)?;
        let shell_name = shell::resolve_shell(shell_name)?;
        let cwd = match cwd.map(str::trim).filter(|value| !value.is_empty()) {
            Some(relative) => self.projects.resolve(&project, relative)?,
            None => PathBuf::from(&project.path),
        };
        if !cwd.is_dir() {
            return Err(TerminalError::Invalid(format!(
                "{} is not a directory",
                cwd.display()
            )));
        }
        let cols = cols.unwrap_or(DEFAULT_COLS).clamp(1, 1000);
        let rows = rows.unwrap_or(DEFAULT_ROWS).clamp(1, 1000);

        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|err| TerminalError::Pty(err.to_string()))?;
        let command = shell::command(&shell_name, &cwd);
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|err| TerminalError::Pty(err.to_string()))?;
        // The slave handle must be released so the child owns its side only.
        drop(pair.slave);

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|err| TerminalError::Pty(err.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|err| TerminalError::Pty(err.to_string()))?;
        let killer = child.clone_killer();

        let id = ulid::Ulid::new().to_string();
        let now = now_ms();
        let record = TerminalSession {
            id: id.clone(),
            project_id: project_id.to_string(),
            shell: shell_name.clone(),
            cwd: cwd.to_string_lossy().to_string(),
            cols,
            rows,
            state: SessionState::Running,
            exit_code: None,
            actor: actor.to_string(),
            created_at: now,
            closed_at: None,
        };
        self.insert(&record)?;

        let ring = Arc::new(Mutex::new(RingBuffer::new(self.ring_bytes)));
        let session = Arc::new(Session {
            record: record.clone(),
            ring: ring.clone(),
            writer: Mutex::new(writer),
            master: Mutex::new(pair.master),
            killer: Mutex::new(killer),
            state: Mutex::new(SessionState::Running),
            exit_code: Mutex::new(None),
            dimensions: Mutex::new((cols, rows)),
        });

        self.spawn_reader(id.clone(), reader, ring);
        self.spawn_watchdog(id.clone(), session.clone(), child);
        self.sessions
            .lock()
            .map_err(|_| TerminalError::Invalid("session table poisoned".to_string()))?
            .insert(id.clone(), session);

        self.bus.publish(
            "term.opened",
            "terminal",
            json!({
                "session_id": record.id,
                "project_id": record.project_id,
                "shell": record.shell,
                "cwd": record.cwd,
                "cols": record.cols,
                "rows": record.rows,
                "actor": record.actor,
            }),
        )?;
        Ok(record)
    }

    /// Send input to a session.
    pub fn write(&self, session_id: &str, data: &str) -> Result<(), TerminalError> {
        let session = self.session(session_id)?;
        let mut writer = session
            .writer
            .lock()
            .map_err(|_| TerminalError::Invalid("session writer poisoned".to_string()))?;
        writer.write_all(data.as_bytes())?;
        writer.flush()?;
        Ok(())
    }

    /// Resize the PTY.
    pub fn resize(&self, session_id: &str, cols: u16, rows: u16) -> Result<(), TerminalError> {
        let session = self.session(session_id)?;
        let cols = cols.clamp(1, 1000);
        let rows = rows.clamp(1, 1000);
        let master = session
            .master
            .lock()
            .map_err(|_| TerminalError::Invalid("session master poisoned".to_string()))?;
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|err| TerminalError::Pty(err.to_string()))?;
        if let Ok(mut dimensions) = session.dimensions.lock() {
            *dimensions = (cols, rows);
        }
        self.storage.execute(
            "UPDATE terminal_sessions SET cols = ?1, rows = ?2 WHERE id = ?3",
            &[
                Value::from(i64::from(cols)),
                Value::from(i64::from(rows)),
                string(session_id),
            ],
        )?;
        Ok(())
    }

    /// Close a session, killing the shell if it still runs.
    pub fn close(&self, session_id: &str) -> Result<bool, TerminalError> {
        let session = self
            .sessions
            .lock()
            .map_err(|_| TerminalError::Invalid("session table poisoned".to_string()))?
            .remove(session_id);
        let Some(session) = session else {
            return Ok(false);
        };
        if let Ok(mut killer) = session.killer.lock() {
            let _ = killer.kill();
        }
        if let Ok(mut state) = session.state.lock() {
            *state = SessionState::Closed;
        }
        self.storage.execute(
            "UPDATE terminal_sessions SET state = 'closed', closed_at = ?1 WHERE id = ?2",
            &[Value::from(now_ms()), string(session_id)],
        )?;
        self.bus.publish(
            "term.closed",
            "terminal",
            json!({ "session_id": session_id, "project_id": session.record.project_id }),
        )?;
        Ok(true)
    }

    /// List sessions, optionally scoped to a project.
    pub fn list(&self, project_id: Option<&str>) -> Result<Vec<TerminalSession>, TerminalError> {
        let rows = match project_id {
            Some(project_id) => self.storage.query(
                "SELECT * FROM terminal_sessions WHERE project_id = ?1 ORDER BY created_at",
                &[string(project_id)],
            )?,
            None => self
                .storage
                .query("SELECT * FROM terminal_sessions ORDER BY created_at", &[])?,
        };
        Ok(rows.iter().map(row_to_session).collect())
    }

    /// A bounded, redacted tail of a session's output.
    pub fn read(
        &self,
        session_id: &str,
        max_lines: Option<usize>,
        max_bytes: Option<usize>,
    ) -> Result<String, TerminalError> {
        let session = self.session(session_id)?;
        let ring = session
            .ring
            .lock()
            .map_err(|_| TerminalError::Invalid("session ring poisoned".to_string()))?;
        let text = ring.tail_lines(
            max_lines.unwrap_or(DEFAULT_READ_LINES),
            max_bytes.unwrap_or(DEFAULT_READ_BYTES),
        );
        Ok(redact(&text))
    }

    /// Run a command in a **visible** session and audit it.
    pub fn exec(
        &self,
        project_id: &str,
        command: &str,
        actor: &str,
    ) -> Result<TerminalSession, TerminalError> {
        if command.trim().is_empty() {
            return Err(TerminalError::Invalid("empty command".to_string()));
        }
        self.projects.require_trusted(project_id)?;
        let record = match self
            .list(Some(project_id))?
            .into_iter()
            .find(|session| session.state == SessionState::Running)
        {
            Some(record) => record,
            None => self.open(project_id, None, None, None, None, actor)?,
        };
        self.write(&record.id, &format!("{command}\r"))?;
        self.storage.audit(&AuditEntry {
            id: &ulid::Ulid::new().to_string(),
            ts: now_ms(),
            actor_type: if actor.starts_with("agent:") {
                "agent"
            } else {
                "user"
            },
            actor_id: actor,
            session_id: Some(&record.id),
            action: "term.exec",
            target: Some(project_id),
            args_json: &json!({ "command": command, "session_id": record.id }).to_string(),
            result: "ok",
            reason: Some("agent ran a command in a visible terminal"),
            correlation_id: None,
            reversible: false,
            revert_ref: None,
        })?;
        Ok(record)
    }

    /// The session state, when the session is still live.
    pub fn state(&self, session_id: &str) -> Option<SessionState> {
        let session = self.session(session_id).ok()?;
        session.state.lock().ok().map(|state| *state)
    }

    /// The exit code, once the shell exited.
    pub fn exit_code(&self, session_id: &str) -> Option<i64> {
        let session = self.session(session_id).ok()?;
        session.exit_code.lock().ok().and_then(|code| *code)
    }

    // --- internals --------------------------------------------------------

    fn session(&self, session_id: &str) -> Result<Arc<Session>, TerminalError> {
        self.sessions
            .lock()
            .map_err(|_| TerminalError::Invalid("session table poisoned".to_string()))?
            .get(session_id)
            .cloned()
            .ok_or_else(|| TerminalError::UnknownSession(session_id.to_string()))
    }

    fn insert(&self, record: &TerminalSession) -> Result<(), TerminalError> {
        self.storage.execute(
            "INSERT INTO terminal_sessions \
             (id, project_id, shell, cwd, cols, rows, state, exit_code, actor, created_at, closed_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9, NULL)",
            &[
                string(&record.id),
                string(&record.project_id),
                string(&record.shell),
                string(&record.cwd),
                Value::from(i64::from(record.cols)),
                Value::from(i64::from(record.rows)),
                string(record.state.as_str()),
                string(&record.actor),
                Value::from(record.created_at),
            ],
        )?;
        Ok(())
    }

    fn spawn_reader(
        &self,
        session_id: String,
        mut reader: Box<dyn Read + Send>,
        ring: Arc<Mutex<RingBuffer>>,
    ) {
        let bus = self.bus.clone();
        let batch = self.batch;
        let (tx, rx) = mpsc::channel::<Vec<u8>>();

        // Flusher: coalesce output into ~16 ms `term.output` events.
        thread::Builder::new()
            .name("fergents-term-flush".to_string())
            .spawn(move || {
                let mut pending = String::new();
                loop {
                    match rx.recv_timeout(batch) {
                        Ok(chunk) => {
                            pending.push_str(&String::from_utf8_lossy(&chunk));
                            while let Ok(chunk) = rx.try_recv() {
                                pending.push_str(&String::from_utf8_lossy(&chunk));
                            }
                            if !pending.is_empty() {
                                let _ = bus.publish(
                                    "term.output",
                                    "terminal",
                                    json!({ "session_id": session_id, "data": std::mem::take(&mut pending) }),
                                );
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if !pending.is_empty() {
                                let _ = bus.publish(
                                    "term.output",
                                    "terminal",
                                    json!({ "session_id": session_id, "data": std::mem::take(&mut pending) }),
                                );
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .ok();

        thread::Builder::new()
            .name("fergents-term-reader".to_string())
            .spawn(move || {
                let mut buffer = [0u8; 8192];
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(size) => {
                            if let Ok(mut ring) = ring.lock() {
                                ring.push(&buffer[..size]);
                            }
                            if tx.send(buffer[..size].to_vec()).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            })
            .ok();
    }

    fn spawn_watchdog(
        &self,
        session_id: String,
        session: Arc<Session>,
        mut child: Box<dyn Child + Send + Sync>,
    ) {
        let bus = self.bus.clone();
        let storage = self.storage.clone();
        thread::Builder::new()
            .name("fergents-term-exit".to_string())
            .spawn(move || {
                let code = child
                    .wait()
                    .ok()
                    .map(|status| i64::from(status.exit_code()));
                if let Ok(mut exit_code) = session.exit_code.lock() {
                    *exit_code = code;
                }
                if let Ok(mut state) = session.state.lock() {
                    if *state == SessionState::Running {
                        *state = SessionState::Exited;
                    }
                }
                let _ = storage.execute(
                    "UPDATE terminal_sessions SET state = 'exited', exit_code = ?1 \
                     WHERE id = ?2 AND state = 'running'",
                    &[
                        code.map(Value::from).unwrap_or(Value::Null),
                        string(&session_id),
                    ],
                );
                let _ = bus.publish(
                    "term.exit",
                    "terminal",
                    json!({
                        "session_id": session_id,
                        "project_id": session.record.project_id,
                        "exit_code": code,
                    }),
                );
            })
            .ok();
    }
}

fn row_to_session(row: &fergents_storage::Row) -> TerminalSession {
    let text = |key: &str| row.get(key).and_then(Value::as_str).map(str::to_string);
    TerminalSession {
        id: text("id").unwrap_or_default(),
        project_id: text("project_id").unwrap_or_default(),
        shell: text("shell").unwrap_or_default(),
        cwd: text("cwd").unwrap_or_default(),
        cols: row.get("cols").and_then(Value::as_i64).unwrap_or(0) as u16,
        rows: row.get("rows").and_then(Value::as_i64).unwrap_or(0) as u16,
        state: SessionState::parse(text("state").unwrap_or_default().as_str()),
        exit_code: row.get("exit_code").and_then(Value::as_i64),
        actor: text("actor").unwrap_or_else(|| "user".to_string()),
        created_at: row.get("created_at").and_then(Value::as_i64).unwrap_or(0),
        closed_at: row.get("closed_at").and_then(Value::as_i64),
    }
}

fn string(value: &str) -> Value {
    Value::String(value.to_string())
}
