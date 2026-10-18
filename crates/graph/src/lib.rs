//! Fergents graph store.
//!
//! Owns the entity graph (`graph.db` via CozoDB) behind a [`GraphStore`] trait,
//! the SQLite mirror (`entities`, `relations`, `entity_mentions`), and entity
//! extraction (`GRAPH.md`).
//!
//! ```no_run
//! use std::sync::Arc;
//! use fergents_graph::{CozoGraphStore, Graph, NullSink, DocumentGraphInput};
//!
//! let store = Arc::new(CozoGraphStore::open("/path/to/brain/graph.db").unwrap());
//! let graph = Graph::new(store, "personal", Arc::new(NullSink));
//! graph
//!     .index_document(&DocumentGraphInput {
//!         document_id: "doc1".into(),
//!         path: "notes/a.md".into(),
//!         title: "A".into(),
//!         content: "See https://example.com and call `parse_document`.".into(),
//!         tags: vec!["notes".into()],
//!         links_to: vec![],
//!     })
//!     .unwrap();
//! ```

mod algo;
mod cozo;
mod error;
mod extract;
mod graph;
mod mirror;
mod model;
mod ner;

pub use cozo::CozoGraphStore;
pub use error::GraphError;
pub use extract::{entity_id, normalize_name, EntityExtractor, ExtractedEntity, RegexExtractor};
pub use graph::{AlgoOptions, Algorithm, DocumentGraphInput, Graph, SubgraphResult};
pub use mirror::GraphMirror;
pub use model::{
    relation_kind, Entity, EntityType, GraphStore, Mention, QueryRows, Relation, ScoredNode,
};
pub use ner::{default_extractors, CompositeExtractor, NerExtractor, TokenClassifier, TokenLabel};

// Re-exported so callers can build a graph without importing `fergents-index`.
pub use fergents_index::{EventSink, NullSink, RecordingSink};
