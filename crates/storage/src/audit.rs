//! Tamper-evident audit log.
//!
//! Each entry's `hash` is the SHA-256 of the previous entry's hash followed by
//! a canonical JSON serialization of the entry's fields. The first entry chains
//! from [`GENESIS_PREV_HASH`] (64 zero hex characters).

use serde_json::Value;

use crate::error::StorageError;

/// The `prev_hash` used to anchor the first entry in the chain.
pub const GENESIS_PREV_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// A single audit-log entry, matching the `audit_log` columns.
#[derive(Debug, Clone)]
pub struct AuditEntry<'a> {
    /// Primary key.
    pub id: &'a str,
    /// Millisecond epoch timestamp.
    pub ts: i64,
    /// Actor type (e.g. `user`, `agent`, `system`).
    pub actor_type: &'a str,
    /// Actor identifier.
    pub actor_id: &'a str,
    /// Originating session, if any.
    pub session_id: Option<&'a str>,
    /// The action performed.
    pub action: &'a str,
    /// The target of the action, if any.
    pub target: Option<&'a str>,
    /// JSON-encoded arguments.
    pub args_json: &'a str,
    /// Outcome of the action.
    pub result: &'a str,
    /// Human-readable reason, if any.
    pub reason: Option<&'a str>,
    /// Correlation id linking related entries, if any.
    pub correlation_id: Option<&'a str>,
    /// Whether the action can be reverted.
    pub reversible: bool,
    /// Reference to the revert record, if any.
    pub revert_ref: Option<&'a str>,
}

/// Deterministically serialize an entry (with its `prev_hash`) to bytes.
///
/// The output is `prev_hash` bytes, a newline, then a canonical JSON object of
/// the entry fields. `serde_json`'s default map orders keys, so the result is
/// stable across runs.
pub fn canonical_bytes(entry: &AuditEntry<'_>, prev_hash: &str) -> Vec<u8> {
    let mut object = serde_json::Map::new();
    object.insert("id".to_string(), Value::String(entry.id.to_string()));
    object.insert("ts".to_string(), Value::Number(entry.ts.into()));
    object.insert(
        "actor_type".to_string(),
        Value::String(entry.actor_type.to_string()),
    );
    object.insert(
        "actor_id".to_string(),
        Value::String(entry.actor_id.to_string()),
    );
    object.insert("session_id".to_string(), opt_string(entry.session_id));
    object.insert(
        "action".to_string(),
        Value::String(entry.action.to_string()),
    );
    object.insert("target".to_string(), opt_string(entry.target));
    object.insert(
        "args_json".to_string(),
        Value::String(entry.args_json.to_string()),
    );
    object.insert(
        "result".to_string(),
        Value::String(entry.result.to_string()),
    );
    object.insert("reason".to_string(), opt_string(entry.reason));
    object.insert(
        "correlation_id".to_string(),
        opt_string(entry.correlation_id),
    );
    object.insert("reversible".to_string(), Value::Bool(entry.reversible));
    object.insert("revert_ref".to_string(), opt_string(entry.revert_ref));

    let json = serde_json::to_string(&Value::Object(object))
        .expect("audit entry JSON serialization cannot fail");

    let mut bytes = Vec::with_capacity(prev_hash.len() + 1 + json.len());
    bytes.extend_from_slice(prev_hash.as_bytes());
    bytes.push(b'\n');
    bytes.extend_from_slice(json.as_bytes());
    bytes
}

/// Hex-encoded SHA-256 of [`canonical_bytes`].
pub fn entry_hash(entry: &AuditEntry<'_>, prev_hash: &str) -> String {
    crate::migrations::sha256_hex(&canonical_bytes(entry, prev_hash))
}

/// Append an entry to the chain, returning the new entry's hash.
///
/// Reads the last entry's `hash` (or [`GENESIS_PREV_HASH`] if the log is empty)
/// as the `prev_hash`.
pub fn append(conn: &rusqlite::Connection, entry: &AuditEntry<'_>) -> Result<String, StorageError> {
    let prev_hash = match conn.query_row(
        "SELECT hash FROM audit_log ORDER BY rowid DESC LIMIT 1",
        [],
        |row| row.get::<_, String>(0),
    ) {
        Ok(hash) => hash,
        Err(rusqlite::Error::QueryReturnedNoRows) => GENESIS_PREV_HASH.to_string(),
        Err(err) => return Err(err.into()),
    };

    let hash = entry_hash(entry, &prev_hash);
    conn.execute(
        "INSERT INTO audit_log (id, ts, actor_type, actor_id, session_id, action, target, \
         args_json, result, reason, correlation_id, reversible, revert_ref, prev_hash, hash) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        rusqlite::params![
            entry.id,
            entry.ts,
            entry.actor_type,
            entry.actor_id,
            entry.session_id,
            entry.action,
            entry.target,
            entry.args_json,
            entry.result,
            entry.reason,
            entry.correlation_id,
            entry.reversible as i64,
            entry.revert_ref,
            prev_hash,
            hash,
        ],
    )?;
    Ok(hash)
}

/// Recompute the chain from genesis. Returns an error naming the first entry
/// whose stored hash (or `prev_hash` link) does not match.
pub fn verify_chain(conn: &rusqlite::Connection) -> Result<(), StorageError> {
    let mut stmt = conn.prepare(
        "SELECT id, ts, actor_type, actor_id, session_id, action, target, args_json, result, \
         reason, correlation_id, reversible, revert_ref, prev_hash, hash \
         FROM audit_log ORDER BY rowid ASC",
    )?;
    let mut rows = stmt.query([])?;
    let mut expected_prev = GENESIS_PREV_HASH.to_string();

    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let ts: i64 = row.get(1)?;
        let actor_type: String = row.get(2)?;
        let actor_id: String = row.get(3)?;
        let session_id: Option<String> = row.get(4)?;
        let action: String = row.get(5)?;
        let target: Option<String> = row.get(6)?;
        let args_json: String = row.get(7)?;
        let result: String = row.get(8)?;
        let reason: Option<String> = row.get(9)?;
        let correlation_id: Option<String> = row.get(10)?;
        let reversible: i64 = row.get(11)?;
        let revert_ref: Option<String> = row.get(12)?;
        let stored_prev: String = row.get(13)?;
        let stored_hash: String = row.get(14)?;

        if stored_prev != expected_prev {
            return Err(StorageError::AuditChain { id });
        }

        let entry = AuditEntry {
            id: &id,
            ts,
            actor_type: &actor_type,
            actor_id: &actor_id,
            session_id: session_id.as_deref(),
            action: &action,
            target: target.as_deref(),
            args_json: &args_json,
            result: &result,
            reason: reason.as_deref(),
            correlation_id: correlation_id.as_deref(),
            reversible: reversible != 0,
            revert_ref: revert_ref.as_deref(),
        };

        if entry_hash(&entry, &expected_prev) != stored_hash {
            return Err(StorageError::AuditChain { id });
        }
        expected_prev = stored_hash;
    }
    Ok(())
}

fn opt_string(value: Option<&str>) -> Value {
    match value {
        Some(s) => Value::String(s.to_string()),
        None => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo_migrations_dir() -> PathBuf {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            let candidate = dir.join("migrations");
            if candidate.is_dir() {
                return candidate;
            }
            assert!(dir.pop(), "could not locate migrations/ directory");
        }
    }

    fn conn() -> rusqlite::Connection {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        let migrations = crate::load_migrations(&repo_migrations_dir()).unwrap();
        crate::migrations::apply_migrations(&mut conn, &migrations).unwrap();
        conn
    }

    fn entry<'a>(id: &'a str, ts: i64, action: &'a str) -> AuditEntry<'a> {
        AuditEntry {
            id,
            ts,
            actor_type: "user",
            actor_id: "u1",
            session_id: None,
            action,
            target: Some("thing"),
            args_json: "{}",
            result: "ok",
            reason: None,
            correlation_id: None,
            reversible: false,
            revert_ref: None,
        }
    }

    #[test]
    fn genesis_prev_hash_is_all_zeros() {
        assert_eq!(GENESIS_PREV_HASH, "0".repeat(64));
        assert_eq!(GENESIS_PREV_HASH.len(), 64);
    }

    #[test]
    fn canonical_bytes_is_deterministic() {
        let e = entry("a", 1, "create");
        assert_eq!(
            canonical_bytes(&e, GENESIS_PREV_HASH),
            canonical_bytes(&e, GENESIS_PREV_HASH)
        );
        assert_ne!(
            canonical_bytes(&e, GENESIS_PREV_HASH),
            canonical_bytes(&e, "ffff")
        );
    }

    #[test]
    fn chain_of_three_verifies_and_links() {
        let conn = conn();
        let h1 = append(&conn, &entry("a", 1, "create")).unwrap();
        let h2 = append(&conn, &entry("b", 2, "update")).unwrap();
        let h3 = append(&conn, &entry("c", 3, "delete")).unwrap();

        // The first entry anchors to genesis.
        let prev_a: String = conn
            .query_row("SELECT prev_hash FROM audit_log WHERE id = 'a'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(prev_a, GENESIS_PREV_HASH);

        // Each subsequent entry links to the previous hash.
        let prev_b: String = conn
            .query_row("SELECT prev_hash FROM audit_log WHERE id = 'b'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let prev_c: String = conn
            .query_row("SELECT prev_hash FROM audit_log WHERE id = 'c'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(prev_b, h1);
        assert_eq!(prev_c, h2);
        assert_eq!(h1.len(), 64);
        assert_eq!(h3.len(), 64);

        verify_chain(&conn).unwrap();
    }

    #[test]
    fn tampering_fails_verification_naming_first_mismatch() {
        let conn = conn();
        append(&conn, &entry("a", 1, "create")).unwrap();
        append(&conn, &entry("b", 2, "update")).unwrap();
        append(&conn, &entry("c", 3, "delete")).unwrap();
        verify_chain(&conn).unwrap();

        // Modify a field out of band without updating the hash.
        conn.execute(
            "UPDATE audit_log SET result = 'tampered' WHERE id = 'b'",
            [],
        )
        .unwrap();

        match verify_chain(&conn) {
            Err(StorageError::AuditChain { id }) => assert_eq!(id, "b"),
            other => panic!("expected AuditChain error for 'b', got {other:?}"),
        }
    }
}
