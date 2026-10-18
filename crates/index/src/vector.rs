//! Dense-vector storage behind the [`VectorStore`] trait (`SEARCH.md` §4.3, §6).
//!
//! All `sqlite-vec` access lives in [`SqliteVecStore`]; no other code writes raw
//! vector SQL. The trait keeps the backend swappable (e.g. an in-memory store in
//! tests) and lets the search pipeline depend on the abstraction, not on
//! `vec_chunks`.

use std::sync::Arc;

use fergents_storage::{Storage, Value};
use serde_json::json;

use crate::error::IndexError;

/// A nearest-neighbour hit.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorHit {
    /// The chunk id.
    pub chunk_id: String,
    /// The distance reported by the backend (smaller is nearer).
    pub distance: f32,
}

/// Dense-vector storage.
pub trait VectorStore: Send + Sync {
    /// The vector dimension this store expects.
    fn dim(&self) -> usize;

    /// Insert or replace the vector for `chunk_id`.
    fn upsert(&self, chunk_id: &str, vector: &[f32]) -> Result<(), IndexError>;

    /// Fetch the stored vector for `chunk_id`, if any.
    fn get(&self, chunk_id: &str) -> Result<Option<Vec<f32>>, IndexError>;

    /// Remove the vector for `chunk_id`. A missing id is not an error.
    fn delete(&self, chunk_id: &str) -> Result<(), IndexError>;

    /// Return the `k` nearest chunk ids to `query`, nearest first.
    fn search(&self, query: &[f32], k: usize) -> Result<Vec<VectorHit>, IndexError>;

    /// The number of stored vectors.
    fn count(&self) -> Result<usize, IndexError>;

    /// Whether the store holds no vectors.
    fn is_empty(&self) -> Result<bool, IndexError> {
        Ok(self.count()? == 0)
    }
}

/// A [`VectorStore`] backed by the per-brain `vec_chunks` table (sqlite-vec).
pub struct SqliteVecStore {
    storage: Arc<dyn Storage>,
    dim: usize,
}

impl SqliteVecStore {
    /// Create a store over `storage` expecting `dim`-dimensional vectors.
    pub fn new(storage: Arc<dyn Storage>, dim: usize) -> Self {
        Self { storage, dim }
    }

    fn check_dim(&self, vector: &[f32]) -> Result<(), IndexError> {
        if vector.len() != self.dim {
            return Err(IndexError::DimensionMismatch {
                expected: self.dim,
                got: vector.len(),
            });
        }
        Ok(())
    }
}

impl VectorStore for SqliteVecStore {
    fn dim(&self) -> usize {
        self.dim
    }

    fn upsert(&self, chunk_id: &str, vector: &[f32]) -> Result<(), IndexError> {
        self.check_dim(vector)?;
        // `vec0` tables do not support UPSERT, so replace by delete + insert.
        self.storage.execute(
            "DELETE FROM vec_chunks WHERE chunk_id = ?1",
            &[Value::String(chunk_id.to_string())],
        )?;
        self.storage.execute(
            "INSERT INTO vec_chunks (chunk_id, embedding) VALUES (?1, ?2)",
            &[Value::String(chunk_id.to_string()), floats_to_json(vector)],
        )?;
        Ok(())
    }

    fn get(&self, chunk_id: &str) -> Result<Option<Vec<f32>>, IndexError> {
        let rows = self.storage.query(
            "SELECT embedding FROM vec_chunks WHERE chunk_id = ?1",
            &[Value::String(chunk_id.to_string())],
        )?;
        let Some(row) = rows.into_iter().next() else {
            return Ok(None);
        };
        let value = row.get("embedding").ok_or_else(|| {
            IndexError::CorruptVector(format!(
                "row for chunk '{chunk_id}' has no embedding column"
            ))
        })?;
        let floats = value_to_floats(value)
            .ok_or_else(|| IndexError::CorruptVector(chunk_id.to_string()))?;
        Ok(Some(floats))
    }

    fn delete(&self, chunk_id: &str) -> Result<(), IndexError> {
        self.storage.execute(
            "DELETE FROM vec_chunks WHERE chunk_id = ?1",
            &[Value::String(chunk_id.to_string())],
        )?;
        Ok(())
    }

    fn search(&self, query: &[f32], k: usize) -> Result<Vec<VectorHit>, IndexError> {
        self.check_dim(query)?;
        let rows = self.storage.query(
            "SELECT chunk_id, distance FROM vec_chunks WHERE embedding MATCH ?1 AND k = ?2 \
             ORDER BY distance",
            &[floats_to_json(query), json!(k as u64)],
        )?;
        rows.into_iter()
            .map(|row| {
                let chunk_id = row
                    .get("chunk_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        IndexError::Vector("vec_chunks row has no string chunk_id".to_string())
                    })?
                    .to_string();
                let distance = row.get("distance").and_then(Value::as_f64).ok_or_else(|| {
                    IndexError::Vector("vec_chunks row has no numeric distance".to_string())
                })? as f32;
                Ok(VectorHit { chunk_id, distance })
            })
            .collect()
    }

    fn count(&self) -> Result<usize, IndexError> {
        let rows = self
            .storage
            .query("SELECT count(*) AS n FROM vec_chunks", &[])?;
        let n = rows
            .first()
            .and_then(|r| r.get("n"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        Ok(n as usize)
    }
}

/// Encode a vector as a JSON array, which sqlite-vec accepts for both writes
/// and `MATCH` queries.
fn floats_to_json(vector: &[f32]) -> Value {
    Value::Array(vector.iter().map(|f| json!(f)).collect())
}

/// Decode a vector returned by sqlite-vec. Stored `vec0` vectors come back as a
/// float32 BLOB, which the storage layer decodes to a JSON array of bytes.
fn value_to_floats(value: &Value) -> Option<Vec<f32>> {
    let items = value.as_array()?;
    if items.is_empty() || items.len() % 4 != 0 {
        return None;
    }
    let mut bytes = Vec::with_capacity(items.len());
    for item in items {
        bytes.push(u8::try_from(item.as_u64()?).ok()?);
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
    )
}
