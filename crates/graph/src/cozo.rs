//! The CozoDB backend for the entity graph (`GRAPH.md` §5.1).
//!
//! Cozo stores `graph.db` with the **sled** engine (pure Rust). Cozo's
//! `storage-sqlite` engine links the native `sqlite3`, which conflicts with
//! `rusqlite`'s bundled SQLite in the same binary (`links = "sqlite3"`), so it
//! cannot be used here.

use std::collections::{BTreeMap, HashSet};

use cozo::{DataValue, DbInstance, ScriptMutability};
use serde_json::Value;

use crate::error::GraphError;
use crate::model::{Entity, GraphStore, Mention, QueryRows, Relation};

const SCHEMA: &[&str] = &[
    ":create entity { id: String => name: String, type: String, canonical_name: String?, \
     metadata: Json?, first_seen_at: Int, last_seen_at: Int }",
    ":create relation { id: String => src: String, dst: String, kind: String, weight: Float, \
     confidence: Float, metadata: Json?, source_doc: String?, created_at: Int }",
    ":create mention { entity_id: String, document_id: String, start_offset: Int => \
     chunk_id: String?, end_offset: Int?, confidence: Float }",
];

/// A [`GraphStore`] backed by CozoDB.
pub struct CozoGraphStore {
    db: DbInstance,
}

impl CozoGraphStore {
    /// Open (or create) a graph database at `path`.
    pub fn open(path: &str) -> Result<Self, GraphError> {
        let db = DbInstance::new("sled", path, "").map_err(backend)?;
        let store = Self { db };
        store.ensure_schema()?;
        Ok(store)
    }

    /// Open a transient in-memory graph (for tests).
    pub fn open_in_memory() -> Result<Self, GraphError> {
        let db = DbInstance::new("mem", "", "").map_err(backend)?;
        let store = Self { db };
        store.ensure_schema()?;
        Ok(store)
    }

    fn run(
        &self,
        script: &str,
        params: BTreeMap<String, DataValue>,
        mutable: bool,
    ) -> Result<QueryRows, GraphError> {
        let mutability = if mutable {
            ScriptMutability::Mutable
        } else {
            ScriptMutability::Immutable
        };
        let named = self
            .db
            .run_script(script, params, mutability)
            .map_err(backend)?;
        Ok(QueryRows {
            headers: named.headers,
            rows: named
                .rows
                .iter()
                .map(|row| row.iter().map(data_value_to_json).collect())
                .collect(),
        })
    }

    fn run_rows(&self, script: &str, rows_param: &str, rows: DataValue) -> Result<(), GraphError> {
        let mut params = BTreeMap::new();
        params.insert(rows_param.to_string(), rows);
        self.run(script, params, true)?;
        Ok(())
    }

    fn run_one(&self, script: &str, id: &str) -> Result<(), GraphError> {
        let mut params = BTreeMap::new();
        params.insert("id".to_string(), DataValue::from(id));
        self.run(script, params, true)?;
        Ok(())
    }
}

impl GraphStore for CozoGraphStore {
    fn ensure_schema(&self) -> Result<(), GraphError> {
        let existing = self.run("::relations", BTreeMap::new(), false)?;
        let names: HashSet<String> = existing
            .rows
            .iter()
            .filter_map(|row| row.first().and_then(Value::as_str).map(str::to_string))
            .collect();
        for script in SCHEMA {
            let name = script
                .trim_start_matches(":create ")
                .split_whitespace()
                .next()
                .unwrap_or_default();
            if !names.contains(name) {
                self.run(script, BTreeMap::new(), true)?;
            }
        }
        Ok(())
    }

    fn upsert_entities(&self, entities: &[Entity]) -> Result<(), GraphError> {
        if entities.is_empty() {
            return Ok(());
        }
        let rows = entities
            .iter()
            .map(|e| {
                DataValue::List(vec![
                    DataValue::from(e.id.as_str()),
                    DataValue::from(e.name.as_str()),
                    DataValue::from(e.entity_type.as_str()),
                    opt_str(e.canonical_name.as_deref()),
                    DataValue::from(e.metadata.clone().unwrap_or(Value::Null)),
                    DataValue::from(e.first_seen_at),
                    DataValue::from(e.last_seen_at),
                ])
            })
            .collect();
        self.run_rows(
            "?[id, name, type, canonical_name, metadata, first_seen_at, last_seen_at] <- $rows \
             :put entity {id => name, type, canonical_name, metadata, first_seen_at, last_seen_at}",
            "rows",
            DataValue::List(rows),
        )
    }

    fn upsert_relations(&self, relations: &[Relation]) -> Result<(), GraphError> {
        if relations.is_empty() {
            return Ok(());
        }
        let rows = relations
            .iter()
            .map(|r| {
                DataValue::List(vec![
                    DataValue::from(r.id.as_str()),
                    DataValue::from(r.src.as_str()),
                    DataValue::from(r.dst.as_str()),
                    DataValue::from(r.kind.as_str()),
                    num_f64(r.weight),
                    num_f64(r.confidence),
                    DataValue::from(r.metadata.clone().unwrap_or(Value::Null)),
                    opt_str(r.source_doc.as_deref()),
                    DataValue::from(r.created_at),
                ])
            })
            .collect();
        self.run_rows(
            "?[id, src, dst, kind, weight, confidence, metadata, source_doc, created_at] <- $rows \
             :put relation {id => src, dst, kind, weight, confidence, metadata, source_doc, created_at}",
            "rows",
            DataValue::List(rows),
        )
    }

    fn upsert_mentions(&self, mentions: &[Mention]) -> Result<(), GraphError> {
        if mentions.is_empty() {
            return Ok(());
        }
        let rows = mentions
            .iter()
            .map(|m| {
                DataValue::List(vec![
                    DataValue::from(m.entity_id.as_str()),
                    DataValue::from(m.document_id.as_str()),
                    DataValue::from(m.start_offset),
                    opt_str(m.chunk_id.as_deref()),
                    DataValue::from(m.end_offset),
                    num_f64(m.confidence),
                ])
            })
            .collect();
        self.run_rows(
            "?[entity_id, document_id, start_offset, chunk_id, end_offset, confidence] <- $rows \
             :put mention {entity_id, document_id, start_offset => chunk_id, end_offset, confidence}",
            "rows",
            DataValue::List(rows),
        )
    }

    fn retract_relations(&self, ids: &[String]) -> Result<(), GraphError> {
        if ids.is_empty() {
            return Ok(());
        }
        let rows: Vec<DataValue> = ids
            .iter()
            .map(|id| DataValue::List(vec![DataValue::from(id.as_str())]))
            .collect();
        self.run_rows(
            "?[id] <- $rows :rm relation {id}",
            "rows",
            DataValue::List(rows),
        )
    }

    fn retract_mentions(&self, entity_id: &str, document_id: &str) -> Result<(), GraphError> {
        let mut params = BTreeMap::new();
        params.insert("entity_id".to_string(), DataValue::from(entity_id));
        params.insert("document_id".to_string(), DataValue::from(document_id));
        self.run(
            "?[entity_id, document_id, start_offset] := \
             *mention{entity_id, document_id, start_offset}, \
             entity_id == $entity_id, document_id == $document_id \
             :rm mention {entity_id, document_id, start_offset}",
            params,
            true,
        )?;
        Ok(())
    }

    fn remove_document(&self, document_id: &str) -> Result<(), GraphError> {
        let mut params = BTreeMap::new();
        params.insert("document_id".to_string(), DataValue::from(document_id));
        self.run(
            "?[entity_id, document_id, start_offset] := \
             *mention{entity_id, document_id, start_offset}, document_id == $document_id \
             :rm mention {entity_id, document_id, start_offset}",
            params.clone(),
            true,
        )?;
        self.run(
            "?[id] := *relation{id, source_doc}, source_doc == $document_id :rm relation {id}",
            params,
            true,
        )?;
        Ok(())
    }

    fn remove_entity(&self, id: &str) -> Result<(), GraphError> {
        self.run_one("?[id] <- [[$id]] :rm entity {id}", id)
    }

    fn get_entity(&self, id: &str) -> Result<Option<Entity>, GraphError> {
        let mut params = BTreeMap::new();
        params.insert("id".to_string(), DataValue::from(id));
        let rows = self.run(
            "?[id, name, type, canonical_name, metadata, first_seen_at, last_seen_at] := \
             *entity{id, name, type, canonical_name, metadata, first_seen_at, last_seen_at}, id == $id",
            params,
            false,
        )?;
        Ok(rows.rows.first().map(|row| row_to_entity(row)))
    }

    fn entities(&self, ids: &[String]) -> Result<Vec<Entity>, GraphError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let wanted: HashSet<&str> = ids.iter().map(String::as_str).collect();
        let rows = self.run(
            "?[id, name, type, canonical_name, metadata, first_seen_at, last_seen_at] := \
             *entity{id, name, type, canonical_name, metadata, first_seen_at, last_seen_at}",
            BTreeMap::new(),
            false,
        )?;
        Ok(rows
            .rows
            .iter()
            .map(|row| row_to_entity(row))
            .filter(|entity| wanted.contains(entity.id.as_str()))
            .collect())
    }

    fn all_relations(&self) -> Result<Vec<Relation>, GraphError> {
        let rows = self.run(
            "?[id, src, dst, kind, weight, confidence, metadata, source_doc, created_at] := \
             *relation{id, src, dst, kind, weight, confidence, metadata, source_doc, created_at}",
            BTreeMap::new(),
            false,
        )?;
        Ok(rows.rows.iter().map(|row| row_to_relation(row)).collect())
    }

    fn entity_count(&self) -> Result<usize, GraphError> {
        let rows = self.run("?[count(id)] := *entity{id}", BTreeMap::new(), false)?;
        Ok(count_of(&rows))
    }

    fn relation_count(&self) -> Result<usize, GraphError> {
        let rows = self.run("?[count(id)] := *relation{id}", BTreeMap::new(), false)?;
        Ok(count_of(&rows))
    }

    fn run_query(
        &self,
        script: &str,
        params: &BTreeMap<String, Value>,
    ) -> Result<QueryRows, GraphError> {
        let params = params
            .iter()
            .map(|(k, v)| (k.clone(), json_to_data_value(v)))
            .collect();
        self.run(script, params, false)
    }
}

fn backend(err: impl std::fmt::Display) -> GraphError {
    GraphError::Backend(err.to_string())
}

fn opt_str(value: Option<&str>) -> DataValue {
    value.map_or(DataValue::Null, DataValue::from)
}

fn num_f64(value: f64) -> DataValue {
    DataValue::Num(cozo::Num::Float(value))
}

fn count_of(rows: &QueryRows) -> usize {
    rows.rows
        .first()
        .and_then(|row| row.first())
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize
}

fn data_value_to_json(value: &DataValue) -> Value {
    match value {
        DataValue::Null | DataValue::Bot => Value::Null,
        DataValue::Bool(flag) => Value::Bool(*flag),
        DataValue::Num(number) => match number {
            cozo::Num::Int(int) => Value::from(*int),
            cozo::Num::Float(float) => serde_json::Number::from_f64(*float)
                .map(Value::Number)
                .unwrap_or(Value::Null),
        },
        DataValue::Str(text) => Value::String(text.to_string()),
        DataValue::Json(json) => json.0.clone(),
        DataValue::List(items) => Value::Array(items.iter().map(data_value_to_json).collect()),
        other => Value::String(format!("{other:?}")),
    }
}

fn json_to_data_value(value: &Value) -> DataValue {
    match value {
        Value::Null => DataValue::Null,
        Value::Bool(b) => DataValue::from(*b),
        Value::Number(n) => n
            .as_i64()
            .map(DataValue::from)
            .or_else(|| n.as_f64().map(num_f64))
            .unwrap_or(DataValue::Null),
        Value::String(s) => DataValue::from(s.as_str()),
        Value::Array(_) | Value::Object(_) => DataValue::from(value.clone()),
    }
}

fn row_to_entity(row: &[Value]) -> Entity {
    Entity {
        id: str_at(row, 0),
        name: str_at(row, 1),
        entity_type: str_at(row, 2),
        canonical_name: row.get(3).and_then(Value::as_str).map(str::to_string),
        metadata: row.get(4).cloned().filter(|v| !v.is_null()),
        first_seen_at: i64_at(row, 5),
        last_seen_at: i64_at(row, 6),
    }
}

fn row_to_relation(row: &[Value]) -> Relation {
    Relation {
        id: str_at(row, 0),
        src: str_at(row, 1),
        dst: str_at(row, 2),
        kind: str_at(row, 3),
        weight: row.get(4).and_then(Value::as_f64).unwrap_or(1.0),
        confidence: row.get(5).and_then(Value::as_f64).unwrap_or(1.0),
        metadata: row.get(6).cloned().filter(|v| !v.is_null()),
        source_doc: row.get(7).and_then(Value::as_str).map(str::to_string),
        created_at: i64_at(row, 8),
    }
}

fn str_at(row: &[Value], index: usize) -> String {
    row.get(index)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn i64_at(row: &[Value], index: usize) -> i64 {
    row.get(index).and_then(Value::as_i64).unwrap_or(0)
}
