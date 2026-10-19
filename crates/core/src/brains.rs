//! The kernel's brain subsystem.
//!
//! [`BrainService`] owns the [`BrainManager`] (the global brain registry plus
//! the on-disk brain folders), the [`BrainIndexer`] that drives the per-brain
//! indexing pipeline, and the resolved paths used by both (the per-brain
//! migrations directory and the templates root).
//!
//! The index and graph pipelines emit through the [`EventSink`] trait rather
//! than depending on the kernel's event bus. [`BusSink`] bridges the two: the
//! kernel wires it so `brain.*`, `index.*`, `file.*`, `search.executed`, and
//! `graph.updated` events are persisted and broadcast on the bus (`BRAIN.md`,
//! `SEARCH.md`, `GRAPH.md`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fergents_brain::{
    BrainConfig as BrainTomlConfig, BrainError, BrainIndexer, BrainManager, BrainPatch,
    BrainRecord, BrainRole, BrainTemplate, CreateBrainSpec, VaultIndexer, VaultLayout, INDEX_DB,
};
use fergents_events::EventBus;
use fergents_graph::{AlgoOptions, Algorithm, CozoGraphStore, Graph, GraphError};
use fergents_index::{
    create_template, list_templates, now_local, render, CandidateCounts, DailyNotes, Embedder,
    EventSink, IndexOutcome, NoteIndexer, SearchEngine, SearchFilters, SearchHit, SearchMode,
    SearchRequest, SearchResponse, SqliteVecStore, StubEmbedder, TemplateVariables, VectorStore,
    DEFAULT_EMBED_DIM,
};
use fergents_protocol::{
    BrainArchiveResult, BrainAttachParams, BrainAttachResult, BrainCloseResult,
    BrainConfig as ProtocolBrainConfig, BrainCreateParams, BrainCreateResult, BrainDeleteParams,
    BrainDeleteResult, BrainDetachParams, BrainDetachResult, BrainDuplicateParams,
    BrainDuplicateResult, BrainExportParams, BrainExportResult, BrainGetResult,
    BrainIdentityConfig, BrainImportParams, BrainImportResult, BrainListResult, BrainMoveParams,
    BrainMoveResult, BrainOpenResult, BrainReindexResult, BrainRole as ProtoBrainRole, BrainState,
    BrainSummary, BrainTemplateSaveParams, BrainTemplateSaveResult, BrainUnarchiveResult,
    BrainUpdateParams, BrainUpdateResult, DailyOpenParams, DailyOpenResult, DocWriteParams,
    DocWriteResult, Document, DocumentKind, Entity as ProtocolEntity, GraphAlgoParams,
    GraphAlgoResult, GraphEntitiesMergeParams, GraphEntitiesMergeResult, GraphNeighborsParams,
    GraphNeighborsResult, GraphQueryParams, GraphQueryResult, GraphSubgraphParams,
    GraphSubgraphResult, Relation as ProtocolRelation, SearchCandidates,
    SearchFilters as ProtoSearchFilters, SearchMeta, SearchMode as ProtoSearchMode,
    SearchQueryParams, SearchQueryResult, SearchResult, SearchResultMetadata, SearchScores,
    TemplateCreateParams, TemplateCreateResult, TemplateListResult, TemplateRef,
    TemplateRenderParams, TemplateRenderResult,
};
use fergents_storage::{load_migrations_from, register_sqlite_vec, SqliteStorage, Storage};
use serde_json::Value;

use crate::CoreError;

/// The graph database file name inside a brain folder.
const GRAPH_DB: &str = "graph.db";

/// The source tag attached to events the brain subsystem publishes.
const SOURCE: &str = "brain";

/// Bridges an [`EventBus`] to the index/graph [`EventSink`] trait.
///
/// The pipelines emit through `EventSink`; the kernel wires this adapter so
/// those events reach the bus. Publish failures are ignored: an event must
/// never abort the operation that produced it.
pub struct BusSink {
    bus: Arc<EventBus>,
    source: String,
}

impl BusSink {
    /// Wrap `bus`, tagging every emitted event with `source`.
    pub fn new(bus: Arc<EventBus>, source: impl Into<String>) -> Self {
        Self {
            bus,
            source: source.into(),
        }
    }
}

impl EventSink for BusSink {
    fn emit(&self, kind: &str, payload: Value) {
        let _ = self.bus.publish(kind, self.source.clone(), payload);
    }
}

/// The kernel's brain subsystem: the manager, the indexer, and resolved paths.
pub struct BrainService {
    manager: Arc<BrainManager>,
    indexer: Arc<dyn BrainIndexer>,
    brain_migrations_dir: PathBuf,
    templates_root: PathBuf,
    brains_root: PathBuf,
    sink: Arc<dyn EventSink>,
    embed_dim: usize,
}

impl BrainService {
    /// Build the service over the global `storage` and the kernel `bus`.
    ///
    /// `brain_migrations_dir` is the per-brain migrations directory
    /// (`<migrations>/brain`); `templates_root` is `<data_dir>/templates/brains`;
    /// `brains_root` is the default parent for new brain folders
    /// (`<data_dir>/brains`).
    pub fn new(
        storage: Arc<dyn Storage>,
        bus: Arc<EventBus>,
        brain_migrations_dir: PathBuf,
        templates_root: PathBuf,
        brains_root: PathBuf,
    ) -> Self {
        let sink: Arc<dyn EventSink> = Arc::new(BusSink::new(bus, SOURCE));
        let manager = Arc::new(BrainManager::new(
            storage,
            sink.clone(),
            brain_migrations_dir.clone(),
            templates_root.clone(),
        ));
        let indexer: Arc<dyn BrainIndexer> =
            Arc::new(VaultIndexer::<StubEmbedder, SqliteVecStore>::new(
                brain_migrations_dir.clone(),
                sink.clone(),
                DEFAULT_EMBED_DIM,
                || StubEmbedder::new(DEFAULT_EMBED_DIM),
                |storage| SqliteVecStore::new(storage, DEFAULT_EMBED_DIM),
            ));
        Self {
            manager,
            indexer,
            brain_migrations_dir,
            templates_root,
            brains_root,
            sink,
            embed_dim: DEFAULT_EMBED_DIM,
        }
    }

    /// The underlying brain manager.
    pub fn manager(&self) -> &BrainManager {
        &self.manager
    }

    /// The shared brain manager handle.
    pub fn manager_arc(&self) -> &Arc<BrainManager> {
        &self.manager
    }

    /// The indexer driving per-brain indexing.
    pub fn indexer(&self) -> &dyn BrainIndexer {
        self.indexer.as_ref()
    }

    /// The per-brain migrations directory (`<migrations>/brain`).
    pub fn brain_migrations_dir(&self) -> &Path {
        &self.brain_migrations_dir
    }

    /// The templates root (`<data_dir>/templates/brains`).
    pub fn templates_root(&self) -> &Path {
        &self.templates_root
    }

    /// The default parent for new brain folders (`<data_dir>/brains`).
    pub fn brains_root(&self) -> &Path {
        &self.brains_root
    }

    /// The output dimension of the wired embedder.
    pub fn embed_dim(&self) -> usize {
        self.embed_dim
    }

    /// Open the per-brain index store (`<brain_dir>/index.sqlite`).
    pub fn open_index_storage(&self, brain_dir: &Path) -> Result<Arc<dyn Storage>, CoreError> {
        register_sqlite_vec();
        let migrations = load_migrations_from(&self.brain_migrations_dir)?;
        let storage = SqliteStorage::open_with_migrations(&brain_dir.join(INDEX_DB), &migrations)?;
        Ok(Arc::new(storage))
    }

    /// Build a [`SearchEngine`] over one brain's index.
    pub fn search_engine(
        &self,
        brain_id: &str,
        brain_dir: &Path,
    ) -> Result<SearchEngine, CoreError> {
        let storage = self.open_index_storage(brain_dir)?;
        let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(self.embed_dim));
        let vectors: Arc<dyn VectorStore> =
            Arc::new(SqliteVecStore::new(storage.clone(), self.embed_dim));
        Ok(SearchEngine::new(
            storage,
            brain_id,
            embedder,
            vectors,
            None,
            self.sink.clone(),
        ))
    }

    /// Build a [`Graph`] over one brain's graph store (`<brain_dir>/graph.db`),
    /// mirroring writes into the brain's index store.
    pub fn graph(
        &self,
        brain_id: &str,
        brain_dir: &Path,
    ) -> Result<Graph<CozoGraphStore>, CoreError> {
        let path = brain_dir.join(GRAPH_DB);
        let store = Arc::new(CozoGraphStore::open(&path.to_string_lossy())?);
        let mirror = self.open_index_storage(brain_dir)?;
        Ok(Graph::new(store, brain_id, self.sink.clone()).with_mirror(mirror))
    }

    // --- Brain lifecycle commands -----------------------------------------

    /// List registered brains, honoring `brain.list`'s `include_archived`.
    pub fn brains(&self, include_archived: bool) -> Result<BrainListResult, CoreError> {
        let records = self.manager.list(include_archived)?;
        Ok(BrainListResult {
            brains: records.iter().map(summary_of).collect(),
        })
    }

    /// Create a brain (`brain.create`).
    pub fn create(&self, params: &BrainCreateParams) -> Result<BrainCreateResult, CoreError> {
        let template = match &params.template {
            Some(name) => BrainTemplate::parse(name)?,
            None => BrainTemplate::Blank,
        };
        let path = match &params.path {
            Some(path) => PathBuf::from(path),
            None => self.brains_root.join(brain_dir_name(&params.name)),
        };
        let mut spec = CreateBrainSpec::new(params.name.clone(), path).with_template(template);
        spec.color = params.color.clone();
        spec.icon = params.icon.clone();
        let record = self.manager.create(self.indexer.as_ref(), &spec)?;
        Ok(BrainCreateResult {
            brain: summary_of(&record),
            state: state_of(&record),
        })
    }

    /// Open a brain (`brain.open`).
    pub fn open(&self, brain_id: &str) -> Result<BrainOpenResult, CoreError> {
        let record = self.manager.open(self.indexer.as_ref(), brain_id)?;
        Ok(BrainOpenResult {
            brain: summary_of(&record),
            state: state_of(&record),
        })
    }

    /// Close a brain (`brain.close`).
    pub fn close(&self, brain_id: &str) -> Result<BrainCloseResult, CoreError> {
        self.manager.close(self.indexer.as_ref(), brain_id)?;
        Ok(BrainCloseResult {
            brain_id: brain_id.to_string(),
        })
    }

    /// Fetch a brain and its on-disk configuration (`brain.get`).
    pub fn get(&self, brain_id: &str) -> Result<BrainGetResult, CoreError> {
        let record = self.manager.get(brain_id)?;
        let config = BrainTomlConfig::load(&record.path)?;
        Ok(BrainGetResult {
            brain: summary_of(&record),
            config: to_protocol_config(&config),
            state: state_of(&record),
        })
    }

    /// Update a brain's name, color, or icon (`brain.update`).
    pub fn update(&self, params: &BrainUpdateParams) -> Result<BrainUpdateResult, CoreError> {
        let patch = BrainPatch {
            name: params.name.clone(),
            color: params.color.clone(),
            icon: params.icon.clone(),
        };
        let record = self.manager.update(&params.brain_id, &patch)?;
        Ok(BrainUpdateResult {
            brain: summary_of(&record),
        })
    }

    /// Archive a brain (`brain.archive`).
    pub fn archive(&self, brain_id: &str) -> Result<BrainArchiveResult, CoreError> {
        let record = self.manager.archive(brain_id)?;
        Ok(BrainArchiveResult {
            brain_id: record.brain_id,
            state: BrainState::Archived,
        })
    }

    /// Un-archive a brain (`brain.unarchive`).
    pub fn unarchive(&self, brain_id: &str) -> Result<BrainUnarchiveResult, CoreError> {
        let record = self.manager.unarchive(brain_id)?;
        Ok(BrainUnarchiveResult {
            brain_id: record.brain_id.clone(),
            state: state_of(&record),
        })
    }

    /// Delete a brain, optionally exporting first (`brain.delete`).
    pub fn delete(&self, params: &BrainDeleteParams) -> Result<BrainDeleteResult, CoreError> {
        let export_to = params.export_first.unwrap_or(false).then(|| {
            self.brains_root
                .join(format!("{}.fergbrain", params.brain_id))
        });
        self.manager.delete(
            self.indexer.as_ref(),
            &params.brain_id,
            &params.confirm_name,
            export_to.as_deref(),
        )?;
        Ok(BrainDeleteResult {
            brain_id: params.brain_id.clone(),
        })
    }

    /// Duplicate a brain into a new folder (`brain.duplicate`).
    pub fn duplicate(
        &self,
        params: &BrainDuplicateParams,
    ) -> Result<BrainDuplicateResult, CoreError> {
        let target = match &params.path {
            Some(path) => PathBuf::from(path),
            None => {
                let source = self.manager.get(&params.brain_id)?;
                let name = params
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("{} copy", source.name));
                self.brains_root.join(brain_dir_name(&name))
            }
        };
        let record = self.manager.duplicate(
            self.indexer.as_ref(),
            &params.brain_id,
            &target,
            params.name.as_deref(),
        )?;
        Ok(BrainDuplicateResult {
            brain: summary_of(&record),
        })
    }

    /// Move a brain folder (`brain.move`).
    pub fn mv(&self, params: &BrainMoveParams) -> Result<BrainMoveResult, CoreError> {
        let record = self.manager.mv(&params.brain_id, Path::new(&params.path))?;
        Ok(BrainMoveResult {
            brain: summary_of(&record),
        })
    }

    /// Export a brain to a `.fergbrain` archive (`brain.export`).
    pub fn export(&self, params: &BrainExportParams) -> Result<BrainExportResult, CoreError> {
        self.manager
            .export(&params.brain_id, Path::new(&params.output))?;
        Ok(BrainExportResult {
            output: params.output.clone(),
        })
    }

    /// Import a `.fergbrain` archive (`brain.import`).
    pub fn import(&self, params: &BrainImportParams) -> Result<BrainImportResult, CoreError> {
        let target = match &params.target_path {
            Some(path) => PathBuf::from(path),
            None => {
                let stem = Path::new(&params.path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("imported-brain");
                self.brains_root.join(brain_dir_name(stem))
            }
        };
        let record =
            self.manager
                .import(self.indexer.as_ref(), Path::new(&params.path), &target)?;
        Ok(BrainImportResult {
            brain: summary_of(&record),
        })
    }

    /// Attach a brain to a project with a role (`brain.attach`).
    pub fn attach(&self, params: &BrainAttachParams) -> Result<BrainAttachResult, CoreError> {
        self.manager.attach(
            &params.project_id,
            &params.brain_id,
            proto_role_to_brain(params.role),
        )?;
        Ok(BrainAttachResult {
            brain_id: params.brain_id.clone(),
            project_id: params.project_id.clone(),
            role: params.role,
        })
    }

    /// Detach a brain from a project (`brain.detach`).
    pub fn detach(&self, params: &BrainDetachParams) -> Result<BrainDetachResult, CoreError> {
        self.manager.detach(&params.project_id, &params.brain_id)?;
        Ok(BrainDetachResult {
            brain_id: params.brain_id.clone(),
            project_id: params.project_id.clone(),
        })
    }

    /// Re-run the index for a brain (`brain.reindex`).
    pub fn reindex(&self, brain_id: &str) -> Result<BrainReindexResult, CoreError> {
        self.manager.reindex(self.indexer.as_ref(), brain_id)?;
        Ok(BrainReindexResult {
            brain_id: brain_id.to_string(),
            state: BrainState::Ready,
        })
    }

    /// Save a brain as a template (`brain.template.save`).
    pub fn save_template(
        &self,
        params: &BrainTemplateSaveParams,
    ) -> Result<BrainTemplateSaveResult, CoreError> {
        let path = self.manager.save_template(&params.brain_id, &params.name)?;
        Ok(BrainTemplateSaveResult {
            name: params.name.clone(),
            path: path.to_string_lossy().into_owned(),
        })
    }

    /// Write a note into a brain's vault and index it (`doc.write`).
    pub fn write_note(&self, params: &DocWriteParams) -> Result<DocWriteResult, CoreError> {
        let record = self.default_brain()?;
        let abs = self.manager.write_note(
            self.indexer.as_ref(),
            None,
            &record.brain_id,
            &params.document_id,
            &params.content,
        )?;
        Ok(DocWriteResult {
            document: document_for(&params.document_id, &abs),
        })
    }

    // --- Search -----------------------------------------------------------

    /// Search one or more brains and merge the results by score (`search.query`).
    ///
    /// When `request.brains` is empty every active brain is searched.
    pub fn search(&self, request: &SearchRequest) -> Result<SearchResponse, CoreError> {
        let records = self.resolve_search_brains(&request.brains)?;
        let mut merged: Vec<SearchHit> = Vec::new();
        let mut candidates = CandidateCounts::default();
        let mut latency_ms = 0i64;
        for record in &records {
            let engine = self.search_engine(&record.brain_id, &record.path)?;
            let response = engine.search(request)?;
            candidates.bm25 += response.candidates.bm25;
            candidates.vector += response.candidates.vector;
            candidates.sparse += response.candidates.sparse;
            latency_ms = latency_ms.max(response.latency_ms);
            merged.extend(response.results);
        }
        merged.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        merged.truncate(request.top_k);
        Ok(SearchResponse {
            results: merged,
            candidates,
            latency_ms,
        })
    }

    /// The `search.query` command adapter: maps the wire params to a
    /// [`SearchRequest`] and the response back to the wire result.
    pub fn search_query(&self, params: &SearchQueryParams) -> Result<SearchQueryResult, CoreError> {
        let response = self.search(&search_request_from_params(params))?;
        Ok(search_result_from_response(&response))
    }

    // --- Daily notes and templates ----------------------------------------

    /// Open (creating if needed) the daily note (`daily.open`).
    pub fn open_daily(&self, params: &DailyOpenParams) -> Result<DailyOpenResult, CoreError> {
        let record = self.default_brain()?;
        let config = BrainTomlConfig::load(&record.path)?;
        let layout = VaultLayout::from_config(record.path.clone(), &config);
        let daily = DailyNotes::new(
            layout.vault_root.clone(),
            layout.daily_dir.clone(),
            layout.templates_dir.clone(),
        )
        .with_format(config.vault.daily_format.clone());
        let now = match &params.date {
            Some(date) => chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|e| {
                    CoreError::Index(fergents_index::IndexError::Invalid(format!(
                        "invalid date {date:?}: {e}"
                    )))
                })?
                .and_hms_opt(0, 0, 0)
                .expect("midnight is a valid time"),
            None => now_local(),
        };
        let note_indexer = BrainNoteIndexer {
            indexer: self.indexer.as_ref(),
            brain_id: &record.brain_id,
            brain_dir: &record.path,
            vault_root: &layout.vault_root,
        };
        let note = daily.open(&note_indexer, now)?;
        Ok(DailyOpenResult {
            created: note.created,
            document: document_for(&note.rel_path, &note.path),
        })
    }

    /// List a brain's note templates (`template.list`).
    pub fn list_templates(&self, brain_id: Option<&str>) -> Result<TemplateListResult, CoreError> {
        let record = self.target_brain(brain_id)?;
        let config = BrainTomlConfig::load(&record.path)?;
        let layout = VaultLayout::from_config(record.path.clone(), &config);
        let templates = list_templates(&layout.templates_dir)?;
        Ok(TemplateListResult {
            templates: templates
                .into_iter()
                .map(|t| TemplateRef {
                    name: t.name,
                    path: t.path.to_string_lossy().into_owned(),
                })
                .collect(),
        })
    }

    /// Create a note template (`template.create`).
    pub fn create_template(
        &self,
        params: &TemplateCreateParams,
    ) -> Result<TemplateCreateResult, CoreError> {
        let record = self.default_brain()?;
        let config = BrainTomlConfig::load(&record.path)?;
        let layout = VaultLayout::from_config(record.path.clone(), &config);
        let content = params.content.clone().unwrap_or_default();
        let path = create_template(&layout.templates_dir, &params.name, &content)?;
        Ok(TemplateCreateResult {
            template: TemplateRef {
                name: params.name.clone(),
                path: path.to_string_lossy().into_owned(),
            },
        })
    }

    /// Render a note template (`template.render`).
    pub fn render_template(
        &self,
        params: &TemplateRenderParams,
    ) -> Result<TemplateRenderResult, CoreError> {
        let record = self.default_brain()?;
        let config = BrainTomlConfig::load(&record.path)?;
        let layout = VaultLayout::from_config(record.path.clone(), &config);
        let file_name = fergents_index::template_file_name(&params.name)?;
        let path = layout.templates_dir.join(file_name);
        let template = std::fs::read_to_string(&path)
            .map_err(|e| CoreError::Index(fergents_index::IndexError::Io(e.to_string())))?;
        let mut vars = TemplateVariables::from_now(now_local());
        if let Some(title) = &params.title {
            vars.title = title.clone();
        }
        if let Some(selection) = &params.selection {
            vars.selection = selection.clone();
        }
        if let Some(clipboard) = &params.clipboard {
            vars.clipboard = clipboard.clone();
        }
        let rendered = render(&template, &vars);
        Ok(TemplateRenderResult {
            content: rendered.content,
            cursor: rendered.cursor.map(|offset| offset as i64),
        })
    }

    // --- Graph ------------------------------------------------------------

    /// Run an ad-hoc Datalog query (`graph.query`).
    pub fn graph_query(&self, params: &GraphQueryParams) -> Result<GraphQueryResult, CoreError> {
        let record = self.target_brain(params.brain.as_deref())?;
        let graph = self.graph(&record.brain_id, &record.path)?;
        let rows = graph.query(&params.query, &BTreeMap::new())?;
        Ok(GraphQueryResult {
            rows: object_maps(rows.as_objects()),
        })
    }

    /// Run a graph algorithm (`graph.algo`).
    pub fn graph_algo(&self, params: &GraphAlgoParams) -> Result<GraphAlgoResult, CoreError> {
        let record = self.target_brain(params.brain.as_deref())?;
        let graph = self.graph(&record.brain_id, &record.path)?;
        let algorithm = match params.algo.to_ascii_lowercase().as_str() {
            "pagerank" | "page_rank" => Algorithm::PageRank,
            "shortest_path" | "shortest-path" => Algorithm::ShortestPath,
            other => {
                return Err(CoreError::Graph(GraphError::Invalid(format!(
                    "unknown algorithm: {other}"
                ))))
            }
        };
        let options = AlgoOptions {
            top_k: params.top_k.unwrap_or(0).max(0) as usize,
            from: params.from.clone(),
            to: params.to.clone(),
        };
        let results = graph.algorithm(algorithm, &options)?;
        Ok(GraphAlgoResult {
            results: object_maps(results),
        })
    }

    /// The neighbors of a node (`graph.neighbors`).
    pub fn graph_neighbors(
        &self,
        params: &GraphNeighborsParams,
    ) -> Result<GraphNeighborsResult, CoreError> {
        let record = self.target_brain(params.brain.as_deref())?;
        let graph = self.graph(&record.brain_id, &record.path)?;
        let depth = params.depth.unwrap_or(1).max(0) as u32;
        let sub = graph.neighbors(&params.node, depth)?;
        Ok(GraphNeighborsResult {
            entities: sub.entities.iter().map(proto_entity).collect(),
            relations: sub.relations.iter().map(proto_relation).collect(),
        })
    }

    /// The subgraph around a set of nodes (`graph.subgraph`).
    pub fn graph_subgraph(
        &self,
        params: &GraphSubgraphParams,
    ) -> Result<GraphSubgraphResult, CoreError> {
        let record = self.target_brain(params.brain.as_deref())?;
        let graph = self.graph(&record.brain_id, &record.path)?;
        let depth = params.depth.unwrap_or(1).max(0) as u32;
        let sub = graph.subgraph(&params.nodes, depth)?;
        Ok(GraphSubgraphResult {
            entities: sub.entities.iter().map(proto_entity).collect(),
            relations: sub.relations.iter().map(proto_relation).collect(),
        })
    }

    /// Merge two entities (`graph.entities.merge`).
    pub fn graph_merge_entities(
        &self,
        params: &GraphEntitiesMergeParams,
    ) -> Result<GraphEntitiesMergeResult, CoreError> {
        let record = self.default_brain()?;
        let graph = self.graph(&record.brain_id, &record.path)?;
        let entity = graph.merge_entities(&params.source_id, &params.target_id)?;
        Ok(GraphEntitiesMergeResult {
            entity: proto_entity(&entity),
        })
    }

    // --- Resolution helpers ----------------------------------------------

    /// Resolve the brain a command targets when the wire params name none.
    ///
    /// Phase 1 has no project context, so a command without a `brain_id` is
    /// only unambiguous when exactly one active brain exists.
    fn default_brain(&self) -> Result<BrainRecord, CoreError> {
        let records = self.manager.list(false)?;
        match records.len() {
            0 => Err(CoreError::Brain(BrainError::invalid(
                "no brains are registered",
            ))),
            1 => Ok(records.into_iter().next().expect("length checked")),
            n => Err(CoreError::Brain(BrainError::invalid(format!(
                "expected exactly one brain, found {n}; specify a brain explicitly"
            )))),
        }
    }

    /// Resolve `brain_id`, falling back to [`Self::default_brain`].
    fn target_brain(&self, brain_id: Option<&str>) -> Result<BrainRecord, CoreError> {
        match brain_id {
            Some(id) => Ok(self.manager.get(id)?),
            None => self.default_brain(),
        }
    }

    /// Resolve the brains to search: the requested ones, or every active brain.
    fn resolve_search_brains(&self, requested: &[String]) -> Result<Vec<BrainRecord>, CoreError> {
        if requested.is_empty() {
            let records = self.manager.list(false)?;
            if records.is_empty() {
                return Err(CoreError::Brain(BrainError::invalid(
                    "no brains are registered",
                )));
            }
            Ok(records)
        } else {
            requested
                .iter()
                .map(|id| self.manager.get(id).map_err(CoreError::from))
                .collect()
        }
    }
}

/// Adapts the [`BrainIndexer`] to the [`NoteIndexer`] hook `daily.open` needs:
/// indexing a single note re-runs the brain's incremental index.
///
/// The real `BrainIndexer` (`VaultIndexer<E, V>`) deliberately does not
/// implement `NoteIndexer` itself: it is generic over the embedder and vector
/// store and owns the whole scan/process pipeline, so a per-note hook would
/// leak its internals. `daily.open` only needs "this one file changed", which
/// the incremental `index_brain` already handles correctly, so this thin
/// adapter is enough.
struct BrainNoteIndexer<'a> {
    indexer: &'a dyn BrainIndexer,
    brain_id: &'a str,
    brain_dir: &'a Path,
    vault_root: &'a Path,
}

impl NoteIndexer for BrainNoteIndexer<'_> {
    fn index_note(&self, _rel_path: &str) -> Result<IndexOutcome, fergents_index::IndexError> {
        self.indexer
            .index_brain(self.brain_id, self.brain_dir, self.vault_root)
            .map(|_| IndexOutcome::Updated)
            .map_err(|e| fergents_index::IndexError::Invalid(e.to_string()))
    }
}

/// A filesystem-friendly folder name derived from a brain name.
fn brain_dir_name(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "brain".to_string()
    } else {
        slug
    }
}

/// Map a domain [`BrainRecord`] to its wire summary.
fn summary_of(record: &BrainRecord) -> BrainSummary {
    BrainSummary {
        brain_id: record.brain_id.clone(),
        name: record.name.clone(),
        path: record.path.to_string_lossy().into_owned(),
        role: record.role.map(|r| r.as_str().to_string()),
    }
}

/// The wire state of a record: `archived` or `ready`.
fn state_of(record: &BrainRecord) -> BrainState {
    if record.is_archived() {
        BrainState::Archived
    } else {
        BrainState::Ready
    }
}

/// Map the on-disk `brain.toml` identity to the wire configuration.
fn to_protocol_config(config: &BrainTomlConfig) -> ProtocolBrainConfig {
    ProtocolBrainConfig {
        brain: Some(BrainIdentityConfig {
            color: config.brain.color.clone(),
            created_at: Some(config.brain.created_at.clone()),
            icon: config.brain.icon.clone(),
            id: Some(config.brain.id.clone()),
            name: Some(config.brain.name.clone()),
            slug: Some(config.brain.slug.clone()),
            template: config.brain.template.clone(),
        }),
        git: None,
        graph: None,
        index: None,
        markdown: None,
        search: None,
        sync: None,
        vault: None,
    }
}

/// Map a wire role to the brain crate's role.
fn proto_role_to_brain(role: ProtoBrainRole) -> BrainRole {
    match role {
        ProtoBrainRole::Primary => BrainRole::Primary,
        ProtoBrainRole::ReadOnly => BrainRole::ReadOnly,
        ProtoBrainRole::Scratch => BrainRole::Scratch,
    }
}

/// Build a wire [`Document`] describing the file at `abs` (`rel_path` is the
/// vault-relative path).
fn document_for(rel_path: &str, abs: &Path) -> Document {
    let now = now_ms();
    let content = std::fs::read_to_string(abs).unwrap_or_default();
    let size = std::fs::metadata(abs).map(|m| m.len() as i64).ok();
    let title = Path::new(rel_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(rel_path)
        .to_string();
    Document {
        created_at: now,
        deleted_at: None,
        frontmatter: None,
        hash: None,
        id: ulid::Ulid::new().to_string(),
        indexed_at: Some(now),
        kind: DocumentKind::Note,
        mime: Some("text/markdown".to_string()),
        path: rel_path.to_string(),
        reading_time_s: None,
        size,
        title,
        updated_at: now,
        word_count: content.split_whitespace().count() as i64,
    }
}

/// Map a graph entity to its wire shape.
fn proto_entity(entity: &fergents_graph::Entity) -> ProtocolEntity {
    ProtocolEntity {
        canonical_name: entity.canonical_name.clone(),
        first_seen_at: Some(entity.first_seen_at),
        id: entity.id.clone(),
        last_seen_at: Some(entity.last_seen_at),
        metadata: entity.metadata.as_ref().and_then(Value::as_object).cloned(),
        name: entity.name.clone(),
        type_: entity.entity_type.clone(),
    }
}

/// Map a graph relation to its wire shape.
fn proto_relation(relation: &fergents_graph::Relation) -> ProtocolRelation {
    ProtocolRelation {
        confidence: Some(relation.confidence),
        created_at: Some(relation.created_at),
        dst: relation.dst.clone(),
        id: relation.id.clone(),
        kind: relation.kind.clone(),
        metadata: relation
            .metadata
            .as_ref()
            .and_then(Value::as_object)
            .cloned(),
        source_doc: relation.source_doc.clone(),
        src: relation.src.clone(),
        weight: Some(relation.weight),
    }
}

/// Keep only the object rows from `values` (graph queries always yield objects).
fn object_maps(values: Vec<Value>) -> Vec<serde_json::Map<String, Value>> {
    values
        .into_iter()
        .filter_map(|value| match value {
            Value::Object(map) => Some(map),
            _ => None,
        })
        .collect()
}

/// Build a [`SearchRequest`] from the `search.query` wire params.
fn search_request_from_params(params: &SearchQueryParams) -> SearchRequest {
    let mut request = SearchRequest::new(params.query.clone());
    request.mode = match params.mode {
        Some(ProtoSearchMode::Bm25) => SearchMode::Bm25,
        Some(ProtoSearchMode::Vector) => SearchMode::Vector,
        Some(ProtoSearchMode::Sparse) => SearchMode::Sparse,
        Some(ProtoSearchMode::Hybrid) | None => SearchMode::Hybrid,
    };
    if let Some(top_k) = params.top_k {
        request.top_k = top_k.max(0) as usize;
    }
    if let Some(brains) = &params.brains {
        request.brains = brains.clone();
    }
    if let Some(rerank) = params.rerank {
        request.rerank = rerank;
    }
    if let Some(filters) = &params.filters {
        request.filters = search_filters_from_params(filters);
    }
    request
}

/// Map the wire search filters to the index crate's filters.
fn search_filters_from_params(filters: &ProtoSearchFilters) -> SearchFilters {
    SearchFilters {
        tags: filters.tags.clone().unwrap_or_default(),
        exclude_tags: filters.exclude_tags.clone().unwrap_or_default(),
        path: filters.path.clone(),
        exclude_path: filters.exclude_path.clone(),
        doc_type: filters.type_.clone(),
        language: filters.language.clone(),
        after: filters.after.clone(),
        before: filters.before.clone(),
        has: filters.has.clone(),
        frontmatter: filters
            .frontmatter
            .as_ref()
            .map(|map| {
                map.iter()
                    .filter_map(|(key, value)| value.as_str().map(|v| (key.clone(), v.to_string())))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// Map an index [`SearchResponse`] to the `search.query` wire result.
fn search_result_from_response(response: &SearchResponse) -> SearchQueryResult {
    SearchQueryResult {
        meta: Some(SearchMeta {
            cache: None,
            candidates: Some(SearchCandidates {
                bm25: Some(response.candidates.bm25 as i64),
                vector: Some(response.candidates.vector as i64),
            }),
            latency_ms: Some(response.latency_ms),
        }),
        results: response
            .results
            .iter()
            .map(search_result_from_hit)
            .collect(),
    }
}

/// Map one index hit to the wire result.
fn search_result_from_hit(hit: &SearchHit) -> SearchResult {
    SearchResult {
        chunk_id: hit.chunk_id.clone(),
        content: hit.content.clone(),
        document_id: hit.document_id.clone(),
        heading_path: hit.heading_path.clone(),
        metadata: Some(SearchResultMetadata {
            brain: Some(hit.metadata.brain.clone()),
            created: hit.metadata.created.clone(),
            tags: Some(hit.metadata.tags.clone()),
        }),
        path: hit.path.clone(),
        score: hit.score,
        scores: Some(SearchScores {
            bm25: hit.scores.bm25,
            fusion: hit.scores.fusion,
            rerank: hit.scores.rerank,
            sparse: hit.scores.sparse,
            vector: hit.scores.vector,
        }),
        snippet: hit.snippet.clone(),
        title: hit.title.clone(),
    }
}

/// Milliseconds since the Unix epoch.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}
