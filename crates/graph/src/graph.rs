//! The graph facade: extraction, structural relations, the `graph.*` commands,
//! and the `graph.updated` event (`GRAPH.md` §4, §5.3, §6, §10.3).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use fergents_index::EventSink;
use fergents_storage::Storage;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::algo;
use crate::error::GraphError;
use crate::extract::{entity_id, normalize_name, EntityExtractor, ExtractedEntity};
use crate::mirror::GraphMirror;
use crate::model::{
    relation_kind, Entity, EntityType, GraphStore, Mention, QueryRows, Relation, ScoredNode,
};
use crate::ner::CompositeExtractor;

/// A document to (re)index into the graph.
#[derive(Debug, Clone)]
pub struct DocumentGraphInput {
    /// The document id.
    pub document_id: String,
    /// The vault-relative path.
    pub path: String,
    /// The document title.
    pub title: String,
    /// The document body (markdown source).
    pub content: String,
    /// The document's tags.
    pub tags: Vec<String>,
    /// Resolved outgoing link targets (document ids).
    pub links_to: Vec<String>,
}

/// A set of entities and relations (`graph.neighbors`, `graph.subgraph`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SubgraphResult {
    /// The nodes.
    pub entities: Vec<Entity>,
    /// The edges.
    pub relations: Vec<Relation>,
}

/// A graph algorithm (`graph.algo`, `GRAPH.md` §10.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    /// PageRank centrality.
    PageRank,
    /// Shortest path between two nodes.
    ShortestPath,
}

/// Options for [`Graph::algorithm`].
#[derive(Debug, Clone, Default)]
pub struct AlgoOptions {
    /// Maximum results returned.
    pub top_k: usize,
    /// The start node (shortest path).
    pub from: Option<String>,
    /// The target node (shortest path).
    pub to: Option<String>,
}

/// The graph facade over a [`GraphStore`].
pub struct Graph<S: GraphStore> {
    store: Arc<S>,
    mirror: Option<GraphMirror>,
    extractors: CompositeExtractor,
    sink: Arc<dyn EventSink>,
    brain_id: String,
}

impl<S: GraphStore> Graph<S> {
    /// Create a graph with the default extractors (regex + NER if available).
    pub fn new(store: Arc<S>, brain_id: impl Into<String>, sink: Arc<dyn EventSink>) -> Self {
        Self {
            store,
            mirror: None,
            extractors: crate::ner::default_extractors(),
            sink,
            brain_id: brain_id.into(),
        }
    }

    /// Use explicit extractors.
    pub fn with_extractors(mut self, extractors: CompositeExtractor) -> Self {
        self.extractors = extractors;
        self
    }

    /// Mirror writes into `storage` (`GRAPH.md` §5.2).
    pub fn with_mirror(mut self, storage: Arc<dyn Storage>) -> Self {
        self.mirror = Some(GraphMirror::new(storage));
        self
    }

    /// The underlying store.
    pub fn store(&self) -> &Arc<S> {
        &self.store
    }

    /// (Re)index a document: re-extract entities, mentions, and structural
    /// relations, and emit `graph.updated` (`GRAPH.md` §5.3).
    pub fn index_document(&self, input: &DocumentGraphInput) -> Result<(), GraphError> {
        // Keep in sync: drop this document's previous mentions + relations.
        self.store.remove_document(&input.document_id)?;
        if let Some(mirror) = &self.mirror {
            mirror.remove_document(&input.document_id)?;
        }

        let extracted = self.extractors.extract(&input.content)?;
        let now = now_ms();

        let mut entities: Vec<Entity> = Vec::new();
        let mut relations: Vec<Relation> = Vec::new();
        let mut mentions: Vec<Mention> = Vec::new();

        // The document node itself.
        entities.push(Entity {
            id: input.document_id.clone(),
            name: input.title.clone(),
            entity_type: EntityType::File.as_str().to_string(),
            canonical_name: None,
            metadata: Some(json!({"kind": "document", "path": input.path})),
            first_seen_at: now,
            last_seen_at: now,
        });

        // Extracted entities + `mentions` relations.
        for item in &extracted {
            let node = entity_from_extracted(item, now);
            relations.push(structural_relation(
                &input.document_id,
                &node.id,
                relation_kind::MENTIONS,
                &input.document_id,
                now,
            ));
            mentions.push(Mention {
                entity_id: node.id.clone(),
                document_id: input.document_id.clone(),
                chunk_id: None,
                start_offset: item.start_offset,
                end_offset: item.end_offset,
                confidence: 1.0,
            });
            entities.push(node);
        }

        // `links_to` relations.
        for target in &input.links_to {
            relations.push(structural_relation(
                &input.document_id,
                target,
                relation_kind::LINKS_TO,
                &input.document_id,
                now,
            ));
        }

        // `tagged_with` relations (tags are `custom` nodes).
        for tag in &input.tags {
            let id = entity_id("tag", &normalize_name(tag));
            entities.push(Entity {
                id: id.clone(),
                name: format!("#{tag}"),
                entity_type: EntityType::Custom.as_str().to_string(),
                canonical_name: None,
                metadata: Some(json!({"kind": "tag"})),
                first_seen_at: now,
                last_seen_at: now,
            });
            relations.push(structural_relation(
                &input.document_id,
                &id,
                relation_kind::TAGGED_WITH,
                &input.document_id,
                now,
            ));
        }

        // `in_folder` relation (folders are `custom` nodes).
        if let Some(folder) = folder_of(&input.path) {
            let id = entity_id("folder", &normalize_name(&folder));
            entities.push(Entity {
                id: id.clone(),
                name: folder,
                entity_type: EntityType::Custom.as_str().to_string(),
                canonical_name: None,
                metadata: Some(json!({"kind": "folder"})),
                first_seen_at: now,
                last_seen_at: now,
            });
            relations.push(structural_relation(
                &input.document_id,
                &id,
                relation_kind::IN_FOLDER,
                &input.document_id,
                now,
            ));
        }

        self.store.upsert_entities(&entities)?;
        self.store.upsert_relations(&relations)?;
        self.store.upsert_mentions(&mentions)?;
        if let Some(mirror) = &self.mirror {
            mirror.upsert_entities(&entities)?;
            mirror.upsert_relations(&relations)?;
            mirror.upsert_mentions(&mentions)?;
        }

        self.emit_updated();
        Ok(())
    }

    /// Remove a document from the graph.
    pub fn remove_document(&self, document_id: &str) -> Result<(), GraphError> {
        self.store.remove_document(document_id)?;
        if let Some(mirror) = &self.mirror {
            mirror.remove_document(document_id)?;
        }
        self.emit_updated();
        Ok(())
    }

    /// Merge `source` into `target` (`GRAPH.md` §5.3).
    pub fn merge_entities(&self, source: &str, target: &str) -> Result<Entity, GraphError> {
        let target_entity = self
            .store
            .get_entity(target)?
            .ok_or_else(|| GraphError::EntityNotFound(target.to_string()))?;

        // Re-point relations: retract the old rows and write the rewritten ones.
        let relations = self.store.all_relations()?;
        let mut retract = Vec::new();
        let mut rewritten = Vec::new();
        for relation in relations {
            if relation.src != source && relation.dst != source {
                continue;
            }
            retract.push(relation.id.clone());
            let mut updated = relation.clone();
            if updated.src == source {
                updated.src = target.to_string();
            }
            if updated.dst == source {
                updated.dst = target.to_string();
            }
            rewritten.push(updated);
        }
        self.store.retract_relations(&retract)?;
        self.store.upsert_relations(&rewritten)?;

        self.store.remove_entity(source)?;
        if let Some(mirror) = &self.mirror {
            mirror.merge_entity(source, target)?;
        }
        self.emit_updated();
        Ok(target_entity)
    }

    /// The neighbors of `node` within `depth` hops.
    pub fn neighbors(&self, node: &str, depth: u32) -> Result<SubgraphResult, GraphError> {
        self.subgraph(&[node.to_string()], depth)
    }

    /// The subgraph around `nodes` within `depth` hops.
    pub fn subgraph(&self, nodes: &[String], depth: u32) -> Result<SubgraphResult, GraphError> {
        let (ids, relations) = algo::neighborhood(self.store.as_ref(), nodes, depth)?;
        let id_list: Vec<String> = ids.into_iter().collect();
        let entities = self.store.entities(&id_list)?;
        Ok(SubgraphResult {
            entities,
            relations,
        })
    }

    /// Run an ad-hoc Datalog query (`graph.query`).
    pub fn query(
        &self,
        script: &str,
        params: &BTreeMap<String, Value>,
    ) -> Result<QueryRows, GraphError> {
        self.store.run_query(script, params)
    }

    /// Run a graph algorithm (`graph.algo`).
    pub fn algorithm(
        &self,
        algorithm: Algorithm,
        options: &AlgoOptions,
    ) -> Result<Vec<Value>, GraphError> {
        match algorithm {
            Algorithm::PageRank => {
                let top_k = if options.top_k == 0 {
                    20
                } else {
                    options.top_k
                };
                let ranked: Vec<ScoredNode> = algo::pagerank(self.store.as_ref(), top_k)?;
                Ok(ranked
                    .into_iter()
                    .map(|node| json!({"node": node.id, "score": node.score}))
                    .collect())
            }
            Algorithm::ShortestPath => {
                let from = options
                    .from
                    .clone()
                    .ok_or_else(|| GraphError::Invalid("shortest_path requires 'from'".into()))?;
                let to = options
                    .to
                    .clone()
                    .ok_or_else(|| GraphError::Invalid("shortest_path requires 'to'".into()))?;
                match algo::shortest_path(self.store.as_ref(), &from, &to)? {
                    Some(path) => Ok(vec![json!({"path": path})]),
                    None => Ok(Vec::new()),
                }
            }
        }
    }

    /// The number of entities and relations.
    pub fn counts(&self) -> Result<(usize, usize), GraphError> {
        Ok((self.store.entity_count()?, self.store.relation_count()?))
    }

    fn emit_updated(&self) {
        self.sink
            .emit("graph.updated", json!({ "brain_id": self.brain_id }));
    }
}

fn entity_from_extracted(item: &ExtractedEntity, now: i64) -> Entity {
    Entity {
        id: entity_id(&item.entity_type, &item.normalized_name),
        name: item.name.clone(),
        entity_type: item.entity_type.clone(),
        canonical_name: None,
        metadata: Some(json!({"extractor": extractor_for(&item.entity_type)})),
        first_seen_at: now,
        last_seen_at: now,
    }
}

fn extractor_for(entity_type: &str) -> &'static str {
    match entity_type {
        "person" | "organization" | "place" => "ner",
        _ => "regex",
    }
}

/// A structural relation (`GRAPH.md` §4.1) at confidence 1.0, with a stable id.
fn structural_relation(src: &str, dst: &str, kind: &str, source_doc: &str, now: i64) -> Relation {
    Relation {
        id: relation_id(src, dst, kind),
        src: src.to_string(),
        dst: dst.to_string(),
        kind: kind.to_string(),
        weight: 1.0,
        confidence: 1.0,
        metadata: None,
        source_doc: Some(source_doc.to_string()),
        created_at: now,
    }
}

fn relation_id(src: &str, dst: &str, kind: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(src.as_bytes());
    hasher.update([0u8]);
    hasher.update(kind.as_bytes());
    hasher.update([0u8]);
    hasher.update(dst.as_bytes());
    format!("rel:{}", &hex::encode(hasher.finalize())[..16])
}

fn folder_of(path: &str) -> Option<String> {
    path.rsplit_once('/')
        .map(|(folder, _)| folder.to_string())
        .filter(|folder| !folder.is_empty())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}
