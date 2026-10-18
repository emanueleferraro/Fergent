//! The SQLite mirror of the entity graph (`GRAPH.md` §5.2).
//!
//! The same entities/relations/mentions are written to the per-brain SQLite
//! tables (`entities`, `relations`, `entity_mentions`) for fast lookups and
//! cross-references, and kept in sync on reindex.

use std::sync::Arc;

use fergents_storage::{Storage, Value};
use ulid::Ulid;

use crate::error::GraphError;
use crate::extract::normalize_name;
use crate::model::{Entity, Mention, Relation};

/// Mirrors graph data into the per-brain SQLite database.
pub struct GraphMirror {
    storage: Arc<dyn Storage>,
}

impl GraphMirror {
    /// Create a mirror over `storage`.
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self { storage }
    }

    /// Insert or update entities.
    pub fn upsert_entities(&self, entities: &[Entity]) -> Result<(), GraphError> {
        for entity in entities {
            let metadata = entity
                .metadata
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|e| GraphError::Backend(e.to_string()))?;
            self.storage.execute(
                "INSERT INTO entities \
                 (id, name, type, canonical_name, normalized_name, metadata, first_seen_at, last_seen_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, type = excluded.type, \
                 canonical_name = excluded.canonical_name, normalized_name = excluded.normalized_name, \
                 metadata = excluded.metadata, last_seen_at = excluded.last_seen_at",
                &[
                    Value::String(entity.id.clone()),
                    Value::String(entity.name.clone()),
                    Value::String(entity.entity_type.clone()),
                    opt_string(entity.canonical_name.clone()),
                    Value::String(normalize_name(&entity.name)),
                    opt_string(metadata),
                    Value::from(entity.first_seen_at),
                    Value::from(entity.last_seen_at),
                ],
            )?;
        }
        Ok(())
    }

    /// Insert or update relations.
    pub fn upsert_relations(&self, relations: &[Relation]) -> Result<(), GraphError> {
        for relation in relations {
            let metadata = relation
                .metadata
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|e| GraphError::Backend(e.to_string()))?;
            self.storage.execute(
                "INSERT INTO relations \
                 (id, src, dst, kind, weight, confidence, metadata, source_doc, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
                 ON CONFLICT(id) DO UPDATE SET src = excluded.src, dst = excluded.dst, \
                 kind = excluded.kind, weight = excluded.weight, confidence = excluded.confidence, \
                 metadata = excluded.metadata, source_doc = excluded.source_doc",
                &[
                    Value::String(relation.id.clone()),
                    Value::String(relation.src.clone()),
                    Value::String(relation.dst.clone()),
                    Value::String(relation.kind.clone()),
                    Value::from(relation.weight),
                    Value::from(relation.confidence),
                    opt_string(metadata),
                    opt_string(relation.source_doc.clone()),
                    Value::from(relation.created_at),
                ],
            )?;
        }
        Ok(())
    }

    /// Insert or update mentions.
    pub fn upsert_mentions(&self, mentions: &[Mention]) -> Result<(), GraphError> {
        for mention in mentions {
            self.storage.execute(
                "INSERT INTO entity_mentions \
                 (id, entity_id, document_id, chunk_id, start_offset, end_offset, confidence) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
                 ON CONFLICT(entity_id, document_id, start_offset) DO UPDATE SET \
                 chunk_id = excluded.chunk_id, end_offset = excluded.end_offset, \
                 confidence = excluded.confidence",
                &[
                    Value::String(Ulid::new().to_string()),
                    Value::String(mention.entity_id.clone()),
                    Value::String(mention.document_id.clone()),
                    opt_string(mention.chunk_id.clone()),
                    Value::from(mention.start_offset),
                    Value::from(mention.end_offset),
                    Value::from(mention.confidence),
                ],
            )?;
        }
        Ok(())
    }

    /// Remove a document's mentions and outbound relations (`GRAPH.md` §5.3).
    pub fn remove_document(&self, document_id: &str) -> Result<(), GraphError> {
        self.storage.execute(
            "DELETE FROM entity_mentions WHERE document_id = ?1",
            &[Value::String(document_id.to_string())],
        )?;
        self.storage.execute(
            "DELETE FROM relations WHERE source_doc = ?1",
            &[Value::String(document_id.to_string())],
        )?;
        Ok(())
    }

    /// Re-point mentions/relations from `source` to `target` and drop `source`.
    pub fn merge_entity(&self, source: &str, target: &str) -> Result<(), GraphError> {
        self.storage.execute(
            "DELETE FROM entity_mentions WHERE entity_id = ?1 AND \
             (document_id, start_offset) IN \
             (SELECT document_id, start_offset FROM entity_mentions WHERE entity_id = ?2)",
            &[
                Value::String(target.to_string()),
                Value::String(source.to_string()),
            ],
        )?;
        self.storage.execute(
            "UPDATE entity_mentions SET entity_id = ?1 WHERE entity_id = ?2",
            &[
                Value::String(target.to_string()),
                Value::String(source.to_string()),
            ],
        )?;
        self.storage.execute(
            "UPDATE relations SET src = ?1 WHERE src = ?2",
            &[
                Value::String(target.to_string()),
                Value::String(source.to_string()),
            ],
        )?;
        self.storage.execute(
            "UPDATE relations SET dst = ?1 WHERE dst = ?2",
            &[
                Value::String(target.to_string()),
                Value::String(source.to_string()),
            ],
        )?;
        self.storage.execute(
            "DELETE FROM entities WHERE id = ?1",
            &[Value::String(source.to_string())],
        )?;
        Ok(())
    }

    /// The mirrored entity count.
    pub fn entity_count(&self) -> Result<usize, GraphError> {
        self.count("SELECT count(*) AS n FROM entities")
    }

    /// The mirrored relation count.
    pub fn relation_count(&self) -> Result<usize, GraphError> {
        self.count("SELECT count(*) AS n FROM relations")
    }

    /// The mirrored mention count.
    pub fn mention_count(&self) -> Result<usize, GraphError> {
        self.count("SELECT count(*) AS n FROM entity_mentions")
    }

    fn count(&self, sql: &str) -> Result<usize, GraphError> {
        Ok(self
            .storage
            .query(sql, &[])?
            .first()
            .and_then(|row| row.get("n"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize)
    }
}

fn opt_string(value: Option<String>) -> Value {
    value.map_or(Value::Null, Value::String)
}
