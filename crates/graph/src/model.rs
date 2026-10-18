//! Graph domain model and the [`GraphStore`] trait (`GRAPH.md` §3, §5).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::GraphError;

/// Built-in entity types (`GRAPH.md` §3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityType {
    /// A person.
    Person,
    /// An organization.
    Organization,
    /// A place.
    Place,
    /// An event.
    Event,
    /// A concept or idea.
    Concept,
    /// A file or path.
    File,
    /// A URL or domain.
    Url,
    /// A date.
    Date,
    /// A code identifier.
    CodeSymbol,
    /// A user-defined type (tags, folders, emails, ...).
    Custom,
}

impl EntityType {
    /// The wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Organization => "organization",
            Self::Place => "place",
            Self::Event => "event",
            Self::Concept => "concept",
            Self::File => "file",
            Self::Url => "url",
            Self::Date => "date",
            Self::CodeSymbol => "code_symbol",
            Self::Custom => "custom",
        }
    }
}

impl From<&str> for EntityType {
    fn from(value: &str) -> Self {
        match value {
            "person" => Self::Person,
            "organization" => Self::Organization,
            "place" => Self::Place,
            "event" => Self::Event,
            "concept" => Self::Concept,
            "file" => Self::File,
            "url" => Self::Url,
            "date" => Self::Date,
            "code_symbol" => Self::CodeSymbol,
            _ => Self::Custom,
        }
    }
}

/// The structural relation kinds (`GRAPH.md` §4.1), all at confidence 1.0.
pub mod relation_kind {
    /// Document A links to document B.
    pub const LINKS_TO: &str = "links_to";
    /// A document mentions an entity.
    pub const MENTIONS: &str = "mentions";
    /// A document carries a tag.
    pub const TAGGED_WITH: &str = "tagged_with";
    /// A document lives in a folder.
    pub const IN_FOLDER: &str = "in_folder";
}

/// A graph entity (`GRAPH.md` §5.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    /// Stable id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// The entity type (`person`, `url`, ...).
    #[serde(rename = "type")]
    pub entity_type: String,
    /// The canonical name, if resolved.
    pub canonical_name: Option<String>,
    /// Free-form metadata.
    pub metadata: Option<Value>,
    /// First time the entity was seen (epoch ms).
    pub first_seen_at: i64,
    /// Last time the entity was seen (epoch ms).
    pub last_seen_at: i64,
}

/// A graph relation (`GRAPH.md` §5.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relation {
    /// Stable id.
    pub id: String,
    /// Source node id.
    pub src: String,
    /// Destination node id.
    pub dst: String,
    /// Relation kind (`links_to`, `mentions`, ...).
    pub kind: String,
    /// Weight (default 1.0).
    pub weight: f64,
    /// Confidence, 0.0-1.0.
    pub confidence: f64,
    /// Free-form metadata.
    pub metadata: Option<Value>,
    /// The document this relation was extracted from, if any.
    pub source_doc: Option<String>,
    /// Creation time (epoch ms).
    pub created_at: i64,
}

/// An entity mention (`GRAPH.md` §5.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mention {
    /// The mentioned entity.
    pub entity_id: String,
    /// The document containing the mention.
    pub document_id: String,
    /// The chunk containing the mention, if known.
    pub chunk_id: Option<String>,
    /// Start offset within the document (0 when unknown).
    pub start_offset: i64,
    /// End offset within the document.
    pub end_offset: i64,
    /// Confidence, 0.0-1.0.
    pub confidence: f64,
}

/// Rows returned by an ad-hoc graph query.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryRows {
    /// Column names.
    pub headers: Vec<String>,
    /// Row cells.
    pub rows: Vec<Vec<Value>>,
}

impl QueryRows {
    /// Rows as objects keyed by column name.
    pub fn as_objects(&self) -> Vec<Value> {
        self.rows
            .iter()
            .map(|row| {
                let mut object = serde_json::Map::new();
                for (header, cell) in self.headers.iter().zip(row) {
                    object.insert(header.clone(), cell.clone());
                }
                Value::Object(object)
            })
            .collect()
    }
}

/// A node and its score, for centrality-style algorithms.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoredNode {
    /// The node id.
    pub id: String,
    /// The score.
    pub score: f64,
}

/// The entity graph backend (`GRAPH.md` §5). Pure: it knows nothing about the
/// brain or the index pipeline, so it is testable in isolation.
pub trait GraphStore: Send + Sync {
    /// Create the Datalog schema if it does not exist.
    fn ensure_schema(&self) -> Result<(), GraphError>;

    /// Insert or update entities.
    fn upsert_entities(&self, entities: &[Entity]) -> Result<(), GraphError>;

    /// Insert or update relations.
    fn upsert_relations(&self, relations: &[Relation]) -> Result<(), GraphError>;

    /// Insert or update mentions.
    fn upsert_mentions(&self, mentions: &[Mention]) -> Result<(), GraphError>;

    /// Remove relations by id.
    fn retract_relations(&self, ids: &[String]) -> Result<(), GraphError>;

    /// Remove all mentions of `entity_id` in `document_id`.
    fn retract_mentions(&self, entity_id: &str, document_id: &str) -> Result<(), GraphError>;

    /// Remove a document's mentions and outbound relations (`GRAPH.md` §5.3).
    fn remove_document(&self, document_id: &str) -> Result<(), GraphError>;

    /// Remove an entity.
    fn remove_entity(&self, id: &str) -> Result<(), GraphError>;

    /// Fetch one entity.
    fn get_entity(&self, id: &str) -> Result<Option<Entity>, GraphError>;

    /// Fetch several entities.
    fn entities(&self, ids: &[String]) -> Result<Vec<Entity>, GraphError>;

    /// Fetch all relations (used by the algorithm helpers).
    fn all_relations(&self) -> Result<Vec<Relation>, GraphError>;

    /// The number of entities.
    fn entity_count(&self) -> Result<usize, GraphError>;

    /// The number of relations.
    fn relation_count(&self) -> Result<usize, GraphError>;

    /// Run an ad-hoc Datalog query (`graph.query`).
    fn run_query(
        &self,
        script: &str,
        params: &BTreeMap<String, Value>,
    ) -> Result<QueryRows, GraphError>;
}
