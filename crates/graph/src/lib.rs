//! Fergents graph store.
//!
//! Owns the entity graph (`graph.db` via CozoDB) behind a `GraphStore` trait,
//! the SQLite mirror (`entities`, `relations`, `entity_mentions`), and entity
//! extraction (`GRAPH.md`).
