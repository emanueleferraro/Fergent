// @generated — do not edit
// Generated from protocol/schema/protocol.schema.json by `pnpm --filter @fergents/protocol gen`.

/**
 * A kernel event emitted on the bus (ARCHITECTURE.md §4.2).
 */
export interface Event {
  /**
   * ULID, monotonic.
   */
  id: string;
  /**
   * Milliseconds since the Unix epoch.
   */
  ts: number;
  /**
   * Namespaced topic, e.g. kernel.started.
   */
  kind: string;
  /**
   * Who emitted the event, e.g. agent:ferg-01.
   */
  source: string;
  /**
   * Groups events in one logical operation.
   */
  correlation_id?: string | null;
  /**
   * Optional brain scope.
   */
  brain_id?: string | null;
  /**
   * Optional project scope.
   */
  project_id?: string | null;
  /**
   * Topic-specific payload (arbitrary JSON).
   */
  payload: {
    [k: string]: unknown;
  };
  /**
   * Event schema version.
   */
  version: number;
}

export type HealthStatus = 'healthy' | 'degraded' | 'unhealthy';

export type StorageStatus = 'ok' | 'readonly' | 'error';

/**
 * Result of the system.health command.
 */
export interface HealthSnapshot {
  status: HealthStatus;
  uptime_ms: number;
  kernel_version: string;
  protocol_version: number;
  storage: StorageStatus;
}

/**
 * First command sent by a client after connecting.
 */
export interface HelloParams {
  protocol_version: number;
  /**
   * desktop | cli | mobile | mcp
   */
  client_kind: string;
  client_version?: string | null;
  capabilities?: string[] | null;
}

/**
 * Kernel reply to hello when the client version is compatible.
 */
export interface WelcomeResult {
  protocol_version: number;
  kernel_version: string;
  session_id: string;
  server_capabilities?: string[] | null;
}

/**
 * Liveness probe reply; carries no diagnostics.
 */
export interface PingResult {
  pong: boolean;
  ts: number;
}

export interface SystemInfoResult {
  kernel_version: string;
  protocol_version: number;
  os: string;
  pid: number;
}

export interface SubscribeParams {
  /**
   * Topic prefixes, e.g. kernel. or file.changed.
   */
  topics: string[];
}

export interface UnsubscribeParams {
  topics: string[];
}

export interface ReplayParams {
  topics: string[];
  /**
   * Deliver events strictly after this id.
   */
  from_id?: string | null;
  limit?: number | null;
}

export interface BrainSummary {
  brain_id: string;
  name: string;
  path: string;
  role?: string | null;
}

export interface BrainListResult {
  brains: BrainSummary[];
}

/**
 * Params of brain.list. `brain.list` is extended in Phase 1 with an archived filter.
 */
export interface BrainListParams {
  /**
   * Include archived brains (default false).
   */
  include_archived?: boolean | null;
}

/**
 * Brain lifecycle state (BRAIN.md §9.1).
 */
export type BrainState = 'created' | 'indexing' | 'ready' | 'archived' | 'deleted';

/**
 * A brain's role within a project (BRAIN.md §8.1).
 */
export type BrainRole = 'primary' | 'read-only' | 'scratch';

/**
 * Payload of the brain.created event.
 */
export interface BrainCreatedPayload {
  brain: BrainSummary;
}

/**
 * Payload of the brain.opened event.
 */
export interface BrainOpenedPayload {
  brain: BrainSummary;
}

/**
 * Payload of the brain.closed event.
 */
export interface BrainClosedPayload {
  brain_id: string;
}

/**
 * Payload of the brain.updated event.
 */
export interface BrainUpdatedPayload {
  brain: BrainSummary;
}

/**
 * Payload of the brain.archived event.
 */
export interface BrainArchivedPayload {
  brain_id: string;
}

/**
 * Payload of the brain.deleted event.
 */
export interface BrainDeletedPayload {
  brain_id: string;
}

/**
 * Payload of the brain.exported event.
 */
export interface BrainExportedPayload {
  brain_id: string;
  output: string;
}

/**
 * Payload of the brain.imported event.
 */
export interface BrainImportedPayload {
  brain: BrainSummary;
}

/**
 * Payload of the brain.indexed event.
 */
export interface BrainIndexedPayload {
  brain_id: string;
}

/**
 * Params of brain.create (BRAIN.md §9.2, §7.1, §18.1).
 */
export interface BrainCreateParams {
  name: string;
  /**
   * Template name, e.g. blank, personal, research, coding, team (BRAIN.md §7.1).
   */
  template?: string | null;
  /**
   * Filesystem path for the new brain.
   */
  path?: string | null;
  color?: string | null;
  icon?: string | null;
}

export interface BrainCreateResult {
  brain: BrainSummary;
  state: BrainState;
}

/**
 * Params of brain.open (BRAIN.md §9.3).
 */
export interface BrainOpenParams {
  brain_id: string;
}

export interface BrainOpenResult {
  brain: BrainSummary;
  state: BrainState;
}

/**
 * Params of brain.close (BRAIN.md §9.4).
 */
export interface BrainCloseParams {
  brain_id: string;
}

export interface BrainCloseResult {
  brain_id: string;
}

/**
 * Params of brain.get.
 */
export interface BrainGetParams {
  brain_id: string;
}

/**
 * Result of brain.get: the brain summary, its lifecycle state, and its brain.toml config (BRAIN.md §6.1).
 */
export interface BrainGetResult {
  brain: BrainSummary;
  state: BrainState;
  config: BrainConfig;
}

/**
 * Params of brain.update: mutable identity fields (BRAIN.md §6.1 [brain]).
 */
export interface BrainUpdateParams {
  brain_id: string;
  name?: string | null;
  color?: string | null;
  icon?: string | null;
}

export interface BrainUpdateResult {
  brain: BrainSummary;
}

/**
 * Params of brain.duplicate (BRAIN.md §8.5).
 */
export interface BrainDuplicateParams {
  brain_id: string;
  /**
   * Name for the copy.
   */
  name?: string | null;
  /**
   * Filesystem path for the copy.
   */
  path?: string | null;
  /**
   * Rewrite internal links to the new brain id (BRAIN.md §8.5).
   */
  rewrite_links?: boolean | null;
  /**
   * Copy attachments and large files.
   */
  include_attachments?: boolean | null;
}

export interface BrainDuplicateResult {
  brain: BrainSummary;
}

/**
 * Params of brain.move: relocate a brain folder (BRAIN.md §10.1).
 */
export interface BrainMoveParams {
  brain_id: string;
  path: string;
}

export interface BrainMoveResult {
  brain: BrainSummary;
}

/**
 * Params of brain.archive (BRAIN.md §9.5).
 */
export interface BrainArchiveParams {
  brain_id: string;
}

export interface BrainArchiveResult {
  brain_id: string;
  state: BrainState;
}

/**
 * Params of brain.unarchive (BRAIN.md §9.5).
 */
export interface BrainUnarchiveParams {
  brain_id: string;
}

export interface BrainUnarchiveResult {
  brain_id: string;
  state: BrainState;
}

/**
 * Params of brain.delete (BRAIN.md §9.6).
 */
export interface BrainDeleteParams {
  brain_id: string;
  /**
   * The brain name, typed to confirm deletion (BRAIN.md §9.6).
   */
  confirm_name: string;
  /**
   * Export an archive before deleting (BRAIN.md §9.6).
   */
  export_first?: boolean | null;
  /**
   * Delete permanently instead of moving to trash (BRAIN.md §9.6).
   */
  permanent?: boolean | null;
}

export interface BrainDeleteResult {
  brain_id: string;
}

/**
 * Params of brain.export (BRAIN.md §14.1, §14.3, §18.4).
 */
export interface BrainExportParams {
  brain_id: string;
  /**
   * Path of the .fergbrain archive to write.
   */
  output: string;
  /**
   * Include index.sqlite and graph.db (excluded by default; BRAIN.md §14.1).
   */
  include_index?: boolean | null;
}

export interface BrainExportResult {
  output: string;
}

/**
 * Params of brain.import (BRAIN.md §14.2).
 */
export interface BrainImportParams {
  /**
   * Path of the .fergbrain archive to import.
   */
  path: string;
  /**
   * Filesystem path to extract the brain into.
   */
  target_path?: string | null;
}

export interface BrainImportResult {
  brain: BrainSummary;
}

/**
 * Params of brain.attach (BRAIN.md §8.1, §18.2).
 */
export interface BrainAttachParams {
  brain_id: string;
  project_id: string;
  role: BrainRole;
}

export interface BrainAttachResult {
  brain_id: string;
  project_id: string;
  role: BrainRole;
}

/**
 * Params of brain.detach (BRAIN.md §8.1).
 */
export interface BrainDetachParams {
  brain_id: string;
  project_id: string;
}

export interface BrainDetachResult {
  brain_id: string;
  project_id: string;
}

/**
 * Params of brain.reindex (BRAIN.md §5.2, §5.3).
 */
export interface BrainReindexParams {
  brain_id: string;
  /**
   * Force a full reindex instead of an incremental one.
   */
  full?: boolean | null;
}

export interface BrainReindexResult {
  brain_id: string;
  state: BrainState;
}

/**
 * Params of brain.template.save (BRAIN.md §7.2).
 */
export interface BrainTemplateSaveParams {
  brain_id: string;
  /**
   * Template name to create.
   */
  name: string;
  /**
   * Exclude personal data from the template (BRAIN.md §7.2).
   */
  exclude_private?: boolean | null;
}

export interface BrainTemplateSaveResult {
  name: string;
  path: string;
}

/**
 * The [brain] section of brain.toml (BRAIN.md §6.1).
 */
export interface BrainIdentityConfig {
  id?: string | null;
  name?: string | null;
  slug?: string | null;
  template?: string | null;
  color?: string | null;
  icon?: string | null;
  created_at?: string | null;
}

/**
 * The [vault] section of brain.toml (BRAIN.md §6.1).
 */
export interface VaultConfig {
  path?: string | null;
  default_note_folder?: string | null;
  daily_folder?: string | null;
  daily_format?: string | null;
  attachments_folder?: string | null;
  templates_folder?: string | null;
}

/**
 * The [markdown] section of brain.toml (BRAIN.md §6.1).
 */
export interface MarkdownConfig {
  wiki_links?: boolean | null;
  embeds?: boolean | null;
  block_refs?: boolean | null;
  callouts?: boolean | null;
  math?: boolean | null;
  mermaid?: boolean | null;
}

/**
 * The [index] section of brain.toml (BRAIN.md §6.1).
 */
export interface IndexConfig {
  embedding_model?: string | null;
  chunk_size?: number | null;
  chunk_overlap?: number | null;
  auto_index?: boolean | null;
  watch?: boolean | null;
}

/**
 * The [graph] section of brain.toml (BRAIN.md §6.1).
 */
export interface GraphConfig {
  enabled?: boolean | null;
  extract_entities?: boolean | null;
  extract_relations?: boolean | null;
  llm_extraction?: boolean | null;
}

/**
 * The [search] section of brain.toml (BRAIN.md §6.1).
 */
export interface SearchConfig {
  /**
   * hybrid | bm25 | vector | sparse.
   */
  default_mode?: string | null;
  default_top_k?: number | null;
  rerank?: boolean | null;
}

/**
 * The [sync] section of brain.toml. Parsed and preserved only in Phase 1 (no runtime effect).
 */
export interface SyncConfig {
  enabled?: boolean | null;
  /**
   * full | notes-only | custom.
   */
  scope?: string | null;
  peers?: string[] | null;
}

/**
 * The [git] section of brain.toml. Parsed and preserved only in Phase 1 (no runtime effect).
 */
export interface GitConfig {
  enabled?: boolean | null;
  auto_commit?: boolean | null;
  commit_message_template?: string | null;
}

/**
 * The full brain.toml configuration (BRAIN.md §6.1).
 */
export interface BrainConfig {
  brain?: BrainIdentityConfig;
  vault?: VaultConfig;
  markdown?: MarkdownConfig;
  index?: IndexConfig;
  graph?: GraphConfig;
  search?: SearchConfig;
  sync?: SyncConfig;
  git?: GitConfig;
}

/**
 * Document kind (spec Appendix B, documents.kind).
 */
export type DocumentKind = 'note' | 'attachment' | 'code' | 'other';

/**
 * Link kind (spec Appendix B, links.kind).
 */
export type LinkKind = 'wiki' | 'md' | 'embed' | 'cross_brain' | 'by_id';

/**
 * A row of the documents table (spec Appendix B).
 */
export interface Document {
  /**
   * ULID.
   */
  id: string;
  /**
   * Vault-relative path.
   */
  path: string;
  kind: DocumentKind;
  title: string;
  mime?: string | null;
  size?: number | null;
  hash?: string | null;
  /**
   * Raw YAML frontmatter as JSON (single source of truth).
   */
  frontmatter?: string | null;
  word_count: number;
  reading_time_s?: number | null;
  created_at: number;
  updated_at: number;
  indexed_at?: number | null;
  /**
   * Set when the document is soft-deleted.
   */
  deleted_at?: number | null;
}

/**
 * A lightweight reference to a note.
 */
export interface NoteRef {
  document_id: string;
  path: string;
  title: string;
}

/**
 * A row of the links table (spec Appendix B).
 */
export interface LinkRef {
  id?: string | null;
  src_document_id: string;
  dst_document_id?: string | null;
  dst_path?: string | null;
  dst_heading?: string | null;
  dst_block?: string | null;
  display_text?: string | null;
  /**
   * Surrounding context (the paragraph).
   */
  anchor?: string | null;
  kind: LinkKind;
  /**
   * True when the target does not exist yet.
   */
  unresolved: boolean;
}

/**
 * A heading in a note's outline, in document order (BRAIN.md §4.2). The tree is derived from `level`.
 */
export interface OutlineEntry {
  /**
   * Heading level (1-6).
   */
  level: number;
  text: string;
}

/**
 * A place where a note's title appears as plain text, not a link (GRAPH.md §7.3).
 */
export interface UnlinkedMention {
  document_id: string;
  path: string;
  title: string;
}

/**
 * A row of the tags table (spec Appendix B).
 */
export interface Tag {
  /**
   * ULID.
   */
  id: string;
  name: string;
  parent?: string | null;
}

/**
 * A note template in vault/templates/ (BRAIN.md §4.5).
 */
export interface TemplateRef {
  name: string;
  path: string;
}

/**
 * Params of index.status.
 */
export interface IndexStatusParams {
  brain_id: string;
}

/**
 * Index status: the persisted, observable job queue (SEARCH.md §13.2, §13.4; spec Appendix B index_jobs).
 */
export interface IndexStatusResult {
  brain_id: string;
  /**
   * Whether indexing is currently active.
   */
  indexing: boolean;
  /**
   * Jobs waiting in the persisted queue.
   */
  queued: number;
  /**
   * Jobs currently being processed.
   */
  running: number;
  /**
   * Total retry attempts across queued/running jobs (index_jobs.attempts).
   */
  attempts: number;
}

/**
 * Params of doc.list.
 */
export interface DocListParams {
  /**
   * Filter by document kind.
   */
  kind?: string | null;
  /**
   * Include soft-deleted documents (default false).
   */
  include_deleted?: boolean | null;
}

export interface DocListResult {
  documents: Document[];
}

/**
 * Params of doc.get.
 */
export interface DocGetParams {
  document_id: string;
}

export interface DocGetResult {
  document: Document;
  /**
   * The note's markdown source.
   */
  content: string;
}

/**
 * Params of doc.create.
 */
export interface DocCreateParams {
  /**
   * Vault-relative path of the new note.
   */
  path: string;
  title?: string | null;
  content?: string | null;
}

export interface DocCreateResult {
  document: Document;
}

/**
 * Params of doc.write.
 */
export interface DocWriteParams {
  document_id: string;
  content: string;
}

export interface DocWriteResult {
  document: Document;
}

/**
 * Params of doc.rename.
 */
export interface DocRenameParams {
  document_id: string;
  title: string;
}

export interface DocRenameResult {
  document: Document;
}

/**
 * Params of doc.move.
 */
export interface DocMoveParams {
  document_id: string;
  /**
   * New vault-relative path.
   */
  path: string;
}

export interface DocMoveResult {
  document: Document;
}

/**
 * Params of doc.delete (soft delete; spec Task 8.4).
 */
export interface DocDeleteParams {
  document_id: string;
}

export interface DocDeleteResult {
  document_id: string;
}

/**
 * Params of doc.restore.
 */
export interface DocRestoreParams {
  document_id: string;
}

export interface DocRestoreResult {
  document: Document;
}

/**
 * Params of doc.backlinks (GRAPH.md §7.1).
 */
export interface DocBacklinksParams {
  document_id: string;
}

/**
 * Incoming links, including unresolved targets (GRAPH.md §7.2).
 */
export interface DocBacklinksResult {
  backlinks: LinkRef[];
}

/**
 * Params of doc.outline.
 */
export interface DocOutlineParams {
  document_id: string;
}

/**
 * The heading outline of a note, in document order (levels imply nesting).
 */
export interface DocOutlineResult {
  outline: OutlineEntry[];
}

/**
 * Params of doc.unlinked_mentions (GRAPH.md §7.3).
 */
export interface DocUnlinkedMentionsParams {
  document_id: string;
}

export interface DocUnlinkedMentionsResult {
  mentions: UnlinkedMention[];
}

/**
 * Params of links.broken.
 */
export interface LinksBrokenParams {}

/**
 * Links whose target is unresolved (spec Appendix B, links.unresolved).
 */
export interface LinksBrokenResult {
  links: LinkRef[];
}

/**
 * Params of links.orphans (GRAPH.md §7.4).
 */
export interface LinksOrphansParams {}

/**
 * Notes with no incoming links (GRAPH.md §7.4).
 */
export interface LinksOrphansResult {
  notes: NoteRef[];
}

/**
 * Params of links.dead_ends (GRAPH.md §7.4).
 */
export interface LinksDeadEndsParams {}

/**
 * Notes with no outgoing links (GRAPH.md §7.4).
 */
export interface LinksDeadEndsResult {
  notes: NoteRef[];
}

/**
 * Params of tag.list.
 */
export interface TagListParams {}

export interface TagListResult {
  tags: Tag[];
}

/**
 * Params of tag.rename.
 */
export interface TagRenameParams {
  tag_id: string;
  new_name: string;
}

export interface TagRenameResult {
  tag: Tag;
}

/**
 * Params of tag.delete.
 */
export interface TagDeleteParams {
  tag_id: string;
}

export interface TagDeleteResult {
  tag_id: string;
}

/**
 * Params of daily.open (BRAIN.md §4.4).
 */
export interface DailyOpenParams {
  /**
   * ISO date (defaults to today).
   */
  date?: string | null;
}

/**
 * The daily note, created on first open of the day (BRAIN.md §4.4).
 */
export interface DailyOpenResult {
  document: Document;
  /**
   * True when the note was created by this call.
   */
  created: boolean;
}

/**
 * Params of template.list.
 */
export interface TemplateListParams {}

export interface TemplateListResult {
  templates: TemplateRef[];
}

/**
 * Params of template.create (BRAIN.md §4.5).
 */
export interface TemplateCreateParams {
  name: string;
  content?: string | null;
}

export interface TemplateCreateResult {
  template: TemplateRef;
}

/**
 * Params of template.render. Variables from BRAIN.md §4.5 (date/time/datetime are computed; title/selection/clipboard are provided).
 */
export interface TemplateRenderParams {
  name: string;
  title?: string | null;
  selection?: string | null;
  clipboard?: string | null;
}

export interface TemplateRenderResult {
  content: string;
  /**
   * Byte offset of the {{cursor}} placeholder in content, if present.
   */
  cursor?: number | null;
}

/**
 * Search mode (SEARCH.md §2, §20).
 */
export type SearchMode = 'hybrid' | 'bm25' | 'vector' | 'sparse';

/**
 * Result grouping (SEARCH.md §11.2).
 */
export type SearchGroupBy = 'document' | 'chunk' | 'section';

/**
 * Structured search filters (SEARCH.md §10.1, §10.2).
 */
export interface SearchFilters {
  tags?: string[] | null;
  exclude_tags?: string[] | null;
  /**
   * Path prefix match.
   */
  path?: string | null;
  exclude_path?: string | null;
  /**
   * note, attachment, or code.
   */
  type?: string | null;
  /**
   * For code chunks.
   */
  language?: string | null;
  /**
   * ISO date.
   */
  after?: string | null;
  /**
   * ISO date.
   */
  before?: string | null;
  /**
   * link, backlink, or attachment.
   */
  has?: string | null;
  /**
   * frontmatter.<key> filters.
   */
  frontmatter?: {} | null;
  brain?: string | null;
}

/**
 * Per-signal scores (SEARCH.md §11.1).
 */
export interface SearchScores {
  bm25?: number | null;
  vector?: number | null;
  sparse?: number | null;
  fusion?: number | null;
  rerank?: number | null;
}

/**
 * Result metadata (SEARCH.md §11.1).
 */
export interface SearchResultMetadata {
  tags?: string[] | null;
  created?: string | null;
  brain?: string | null;
}

/**
 * One search result (SEARCH.md §11.1, verbatim shape).
 */
export interface SearchResult {
  chunk_id: string;
  document_id: string;
  path: string;
  title: string;
  heading_path?: string | null;
  content: string;
  snippet: string;
  score: number;
  scores?: SearchScores;
  metadata?: SearchResultMetadata;
}

/**
 * Candidate counts per signal (SEARCH.md §20).
 */
export interface SearchCandidates {
  bm25?: number | null;
  vector?: number | null;
}

/**
 * Cache hit flags (SEARCH.md §20).
 */
export interface SearchCacheInfo {
  query?: boolean | null;
  embedding?: boolean | null;
  rerank?: boolean | null;
}

/**
 * Search response metadata (SEARCH.md §20).
 */
export interface SearchMeta {
  latency_ms?: number | null;
  candidates?: SearchCandidates;
  cache?: SearchCacheInfo;
}

/**
 * Params of search.query (SEARCH.md §20, verbatim).
 */
export interface SearchQueryParams {
  query: string;
  brains?: string[] | null;
  mode?: SearchMode;
  top_k?: number | null;
  filters?: SearchFilters;
  group_by?: SearchGroupBy;
  rerank?: boolean | null;
  explain?: boolean | null;
}

/**
 * Result of search.query (SEARCH.md §20).
 */
export interface SearchQueryResult {
  results: SearchResult[];
  meta?: SearchMeta;
}

/**
 * Payload of the search.executed event (SEARCH.md §19).
 */
export interface SearchExecutedPayload {
  query: string;
  results: number;
  latency_ms?: number | null;
}

/**
 * A graph entity (GRAPH.md §5.1, CozoDB `entity`).
 */
export interface Entity {
  id: string;
  name: string;
  type: string;
  canonical_name?: string | null;
  metadata?: {} | null;
  first_seen_at?: number | null;
  last_seen_at?: number | null;
}

/**
 * A graph relation (GRAPH.md §5.1, CozoDB `relation`).
 */
export interface Relation {
  id: string;
  src: string;
  dst: string;
  kind: string;
  weight?: number | null;
  confidence?: number | null;
  metadata?: {} | null;
  source_doc?: string | null;
  created_at?: number | null;
}

/**
 * An entity mention (GRAPH.md §5.1, CozoDB `mention`).
 */
export interface Mention {
  entity_id: string;
  document_id: string;
  chunk_id?: string | null;
  start_offset?: number | null;
  end_offset?: number | null;
  confidence: number;
}

/**
 * Params of graph.query (GRAPH.md §6.3).
 */
export interface GraphQueryParams {
  query: string;
  /**
   * Query language; the doc example uses datalog.
   */
  language?: string | null;
  brain?: string | null;
}

/**
 * Rows returned by a graph query (columns depend on the query).
 */
export interface GraphQueryResult {
  rows: {}[];
}

/**
 * Params of graph.algo (GRAPH.md §10.3). Algorithms: shortest_path, neighbors, centrality, pagerank, communities, similarity, cycles, subgraph (§10.1).
 */
export interface GraphAlgoParams {
  algo: string;
  brain?: string | null;
  /**
   * Source node id (shortest_path).
   */
  from?: string | null;
  /**
   * Target node id (shortest_path).
   */
  to?: string | null;
  top_k?: number | null;
}

/**
 * Rows returned by a graph algorithm (shape depends on the algorithm).
 */
export interface GraphAlgoResult {
  results: {}[];
}

/**
 * Params of graph.neighbors (GRAPH.md §10.1).
 */
export interface GraphNeighborsParams {
  /**
   * Entity id.
   */
  node: string;
  depth?: number | null;
  brain?: string | null;
}

export interface GraphNeighborsResult {
  entities: Entity[];
  relations: Relation[];
}

/**
 * Params of graph.subgraph (GRAPH.md §10.1).
 */
export interface GraphSubgraphParams {
  nodes: string[];
  depth?: number | null;
  brain?: string | null;
}

export interface GraphSubgraphResult {
  entities: Entity[];
  relations: Relation[];
}

/**
 * Params of graph.entities.merge (GRAPH.md §5.3).
 */
export interface GraphEntitiesMergeParams {
  /**
   * Entity to merge.
   */
  source_id: string;
  /**
   * Canonical entity kept after the merge.
   */
  target_id: string;
}

/**
 * The canonical entity after merging (GRAPH.md §5.3).
 */
export interface GraphEntitiesMergeResult {
  entity: Entity;
}

/**
 * Payload of the graph.updated event.
 */
export interface GraphUpdatedPayload {
  brain_id: string;
}

/**
 * Index job operation (spec Appendix B, index_jobs.op).
 */
export type IndexJobOp = 'upsert' | 'delete' | 'reindex';

/**
 * Indexing pipeline step (SEARCH.md §13.2).
 */
export type IndexPhase =
  'detect' | 'parse' | 'chunk' | 'embed' | 'store' | 'extract_entities' | 'update_graph';

/**
 * Payload of the file.created event.
 */
export interface FileCreatedPayload {
  path: string;
}

/**
 * Payload of the file.changed event.
 */
export interface FileChangedPayload {
  path: string;
}

/**
 * Payload of the file.deleted event.
 */
export interface FileDeletedPayload {
  path: string;
}

/**
 * Payload of the file.renamed event.
 */
export interface FileRenamedPayload {
  from_path: string;
  to_path: string;
}

/**
 * Payload of the note.created event.
 */
export interface NoteCreatedPayload {
  document: Document;
}

/**
 * Payload of the note.updated event.
 */
export interface NoteUpdatedPayload {
  document: Document;
}

/**
 * Payload of the note.linked event.
 */
export interface NoteLinkedPayload {
  link: LinkRef;
}

/**
 * Payload of the index.queued event (spec Appendix B, index_jobs).
 */
export interface IndexQueuedPayload {
  brain_id: string;
  path: string;
  op: IndexJobOp;
}

/**
 * Payload of the index.started event.
 */
export interface IndexStartedPayload {
  brain_id: string;
  path: string;
}

/**
 * Payload of the index.progress event (SEARCH.md §13.2).
 */
export interface IndexProgressPayload {
  brain_id: string;
  path?: string | null;
  phase: IndexPhase;
  done?: number | null;
  total?: number | null;
}

/**
 * Payload of the index.completed event.
 */
export interface IndexCompletedPayload {
  brain_id: string;
  path: string;
}

/**
 * Payload of the index.failed event.
 */
export interface IndexFailedPayload {
  brain_id: string;
  path: string;
  error: string;
}

/**
 * Payload of the embedding.created event (SEARCH.md §4).
 */
export interface EmbeddingCreatedPayload {
  chunk_id: string;
}

/**
 * Payload of the embedding.failed event.
 */
export interface EmbeddingFailedPayload {
  chunk_id: string;
  error: string;
}

/**
 * Payload of the tag.updated event.
 */
export interface TagUpdatedPayload {
  tag: Tag;
}

/**
 * Payload of the link.updated event.
 */
export interface LinkUpdatedPayload {
  link: LinkRef;
}

/**
 * Payload of the events.dropped notice.
 */
export interface EventsDroppedNotice {
  dropped: number;
  last_seen_id: string | null;
}

/**
 * A capability as reported by capability.list / capability.get (spec Appendix D).
 */
export interface CapabilityView {
  /**
   * Namespaced id, e.g. capability.charts.
   */
  id: string;
  name: string;
  version: string;
  /**
   * kernel or plugin:<id>.
   */
  source: string;
  description: string | null;
  dependencies: string[];
  resources: string[];
  enabled: boolean;
  default_enabled: boolean;
  hot_toggle: boolean;
  restart_required: boolean;
  /**
   * Optional configuration.
   */
  config: {
    [k: string]: unknown;
  };
}

/**
 * Result of capability.list.
 */
export interface CapabilityListResult {
  capabilities: CapabilityView[];
}

/**
 * Params of capability.get.
 */
export interface CapabilityGetParams {
  id: string;
}

/**
 * Params of capability.set.
 */
export interface CapabilitySetParams {
  id: string;
  enabled: boolean;
  /**
   * Enable disabled dependencies instead of failing.
   */
  auto_enable_deps?: boolean;
}

/**
 * Payload of the capability.registered event.
 */
export interface CapabilityRegisteredPayload {
  id: string;
}

/**
 * Payload of capability.enabled / capability.disabled / capability.changed.
 */
export interface CapabilityChangedPayload {
  id: string;
  enabled: boolean;
  restart_required?: boolean | null;
}

/**
 * The loop pattern an agent uses (AGENTS.md §5.3).
 */
export type AgentLoopKind =
  'react' | 'plan_execute' | 'reflexion' | 'tot' | 'debate' | 'map_reduce';

/**
 * Per-run budgets (AGENTS.md §13).
 */
export interface AgentLimits {
  max_tokens_per_run?: number;
  max_cost_usd_per_run?: number;
  max_wall_time_s?: number;
  max_tool_calls_per_run?: number;
  max_sub_agents?: number;
  max_iterations?: number;
}

/**
 * Memory settings of an agent definition (AGENTS.md §8).
 */
export interface AgentMemoryConfig {
  episodic?: boolean;
  semantic?: boolean;
  working_window?: number | null;
  summarize_at?: number | null;
  rag_top_k?: number | null;
}

/**
 * Sub-agent settings of an agent definition (AGENTS.md §9).
 */
export interface AgentSubAgentsConfig {
  allow?: boolean;
  max_depth?: number | null;
  default_tools?: string[];
}

/**
 * A parsed agents/<id>.md definition (AGENTS.md §4). The Markdown body is the system prompt and is not part of this payload.
 */
export interface AgentDefinition {
  id: string;
  name: string;
  description?: string;
  version?: string;
  model: string;
  model_fallback?: string[];
  temperature?: number | null;
  max_tokens?: number | null;
  reasoning?: boolean;
  loop?: AgentLoopKind;
  tools?: string[];
  skills?: string[];
  mcp?: string[];
  limits?: AgentLimits;
  memory?: AgentMemoryConfig;
  sub_agents?: AgentSubAgentsConfig;
  tags?: string[];
  icon?: string | null;
  color?: string | null;
}

/**
 * Result of agent.list.
 */
export interface AgentListResult {
  agents: AgentDefinition[];
}

/**
 * Params of agent.create: the full Markdown definition.
 */
export interface AgentCreateParams {
  /**
   * YAML frontmatter followed by the Markdown system prompt.
   */
  definition: string;
}

/**
 * Params carrying an agent id.
 */
export interface AgentIdParams {
  id: string;
}

/**
 * Result of agent.delete.
 */
export interface AgentDeletedResult {
  deleted: string;
}

/**
 * Params of agent.run.
 */
export interface AgentRunParams {
  id: string;
  task: string;
  session_id?: string | null;
}

/**
 * Result of agent.run: the run outcome (AGENTS.md §5.2).
 */
export interface AgentRunResult {
  session_id: string;
  agent_id: string;
  final_text: string;
  messages: {
    [k: string]: unknown;
  }[];
  tool_calls: {
    [k: string]: unknown;
  }[];
  /**
   * Accumulated token usage.
   */
  usage: {
    [k: string]: unknown;
  };
  iterations: number;
  /**
   * Why the run stopped.
   */
  stop: string;
}

/**
 * Session lifecycle state (AGENTS.md §14.1).
 */
export type AgentRunState =
  'created' | 'running' | 'paused' | 'waiting_permission' | 'done' | 'error' | 'cancelled';

/**
 * A persisted agent session.
 */
export interface AgentSessionRecord {
  id: string;
  agent_id: string;
  title: string | null;
  state: AgentRunState;
  created_at: number;
  updated_at: number;
  summary: string | null;
}

/**
 * Result of agent.session.list.
 */
export interface AgentSessionListResult {
  sessions: AgentSessionRecord[];
}

/**
 * A persisted agent message.
 */
export interface AgentMessageRecord {
  id: string;
  session_id: string;
  role: string;
  content: string;
  created_at: number;
}

/**
 * Result of agent.session.messages.
 */
export interface AgentSessionMessagesResult {
  messages: AgentMessageRecord[];
}

/**
 * The memory tier (AGENTS.md §8.1).
 */
export type MemoryKind = 'episodic' | 'fact';

/**
 * A persisted agent_memory entry.
 */
export interface MemoryEntry {
  id: string;
  /**
   * global, agent:<id>, project:<id>, or brain:<id>.
   */
  scope: string;
  kind: MemoryKind;
  content: string;
  importance: number;
  created_at: number;
  updated_at: number;
  last_used_at: number | null;
}

/**
 * Result of memory.recall / memory.list.
 */
export interface MemoryEntriesResult {
  entries: MemoryEntry[];
}

/**
 * Params of memory.remember.
 */
export interface MemoryRememberParams {
  content: string;
  scope?: string;
  kind?: MemoryKind;
  importance?: number;
}

/**
 * Params of memory.recall.
 */
export interface MemoryRecallParams {
  query: string;
  scope?: string;
  k?: number;
}

/**
 * Result of memory.forget.
 */
export interface MemoryForgetResult {
  removed: boolean;
}

/**
 * Payload of the memory.written event.
 */
export interface MemoryWrittenPayload {
  id: string;
  scope: string;
  kind: MemoryKind;
}

/**
 * One assembled context section (spec §2, Context Viewer).
 */
export interface ContextSection {
  /**
   * system, capabilities, rag, ...
   */
  name: string;
  enabled: boolean;
  cacheable: boolean;
  tokens: number;
  source_refs: string[];
  content: string;
}

/**
 * A persisted context_snapshots row.
 */
export interface ContextSnapshot {
  id: string;
  session_id: string;
  turn_seq: number;
  sections: ContextSection[];
  total_tokens: number;
  budget: number;
}

/**
 * Per-session context toggles and budget.
 */
export interface ContextSettings {
  enabled: {
    [k: string]: boolean;
  };
  budget: number;
}

/**
 * Params of context.inspect.
 */
export interface ContextInspectParams {
  session_id: string;
  agent_id?: string | null;
  input?: string | null;
}

/**
 * Params of context.set_section.
 */
export interface ContextSetSectionParams {
  section: string;
  enabled: boolean;
}

/**
 * Params of context.set_budget.
 */
export interface ContextSetBudgetParams {
  budget: number;
}

/**
 * Payload of the context.built event.
 */
export interface ContextBuiltPayload {
  session_id: string;
  turn_seq: number;
  total_tokens: number;
  sections: {
    name: string;
    enabled: boolean;
    tokens: number;
  }[];
}

/**
 * How far a spec workflow runs before pausing (decision D7).
 */
export type AutonomyMode = 'supervised' | 'batch' | 'auto';

/**
 * A spec-workflow approval gate.
 */
export type RunGate = 'spec' | 'plan' | 'task' | 'diff';

/**
 * A spec-workflow run (runs table).
 */
export interface WorkflowRun {
  id: string;
  session_id: string | null;
  slug: string;
  mode: AutonomyMode;
  /**
   * running, awaiting_spec_approval, awaiting_plan_approval, ...
   */
  state: string;
  pending_gate: RunGate | null;
  /**
   * Written artifact paths keyed by gate label.
   */
  artifacts: {
    [k: string]: string;
  };
  created_at: number;
  updated_at: number;
}

/**
 * Params of spec.start.
 */
export interface SpecStartParams {
  slug: string;
  task: string;
  mode?: AutonomyMode;
  session_id?: string | null;
}

/**
 * Result of spec.get.
 */
export interface SpecGetResult {
  run: WorkflowRun;
  /**
   * Artifact Markdown keyed by gate label.
   */
  artifacts: {
    [k: string]: string;
  };
}

/**
 * Params carrying a run id.
 */
export interface RunIdParams {
  run_id: string;
}

/**
 * Params of run.approve / spec.approve.
 */
export interface RunApproveParams {
  run_id: string;
  gate?: RunGate;
}

/**
 * Params of run.reject.
 */
export interface RunRejectParams {
  run_id: string;
  reason?: string | null;
}

/**
 * Result of run.list.
 */
export interface RunListResult {
  runs: WorkflowRun[];
}

/**
 * An approvals row.
 */
export interface Approval {
  id: string;
  gate: RunGate;
  /**
   * pending, approved, rejected, or edited.
   */
  status: string;
  created_at: number;
}

/**
 * Payload of run.approval_requested / run.approved / run.rejected.
 */
export interface RunApprovalPayload {
  run_id: string;
  gate: RunGate;
  status: string;
}

/**
 * The session permission mode (SECURITY.md §4).
 */
export type PermissionMode = 'manual' | 'auto' | 'full';

/**
 * A permission category.
 */
export type PermissionCategory =
  | 'fs.read'
  | 'fs.write'
  | 'shell.exec'
  | 'net.http'
  | 'net.p2p'
  | 'llm.call'
  | 'mcp.connect'
  | 'plugin.install'
  | 'agent.spawn'
  | 'workflow.edit'
  | 'brain.write'
  | 'system.config';

/**
 * allow, prompt, or deny.
 */
export type PermissionDecision = 'allow' | 'prompt' | 'deny';

/**
 * A tool call awaiting a permission decision.
 */
export interface PermissionRequest {
  tool: string;
  arguments: unknown;
  target?: string | null;
  session_id?: string | null;
  agent_id?: string | null;
}

/**
 * The payload the UI renders in a permission card (UI.md §12).
 */
export interface PermissionCard {
  action: string;
  target: string | null;
  reason: string;
  category: string;
  choices: string[];
}

/**
 * The outcome of a permission check.
 */
export interface PermissionResolved {
  decision: PermissionDecision;
  category: PermissionCategory;
  reason: string;
  card: PermissionCard | null;
}

/**
 * Params of permission.check.
 */
export interface PermissionCheckParams {
  tool: string;
  arguments?: unknown;
  target?: string | null;
  session_id?: string | null;
  agent_id?: string | null;
}

/**
 * Params of permission.mode.get / permission.mode.set.
 */
export interface PermissionModeParams {
  session_id?: string | null;
  mode?: PermissionMode;
}

/**
 * Result of permission.mode.get / permission.mode.set.
 */
export interface PermissionModeResult {
  mode: PermissionMode;
}

/**
 * One ordered permission policy rule (first match wins).
 */
export interface PermissionPolicyRule {
  category?: string | null;
  tool?: string | null;
  action?: PermissionDecision;
  scope?: string[] | null;
  note?: string | null;
}

/**
 * Result of permission.policy.get / permission.policy.set.
 */
export interface PermissionPolicyResult {
  toml?: string | null;
  rules: PermissionPolicyRule[];
}

/**
 * A reversibility snapshot.
 */
export interface Backup {
  id: string;
  path: string;
  backup_path: string;
  existed: boolean;
  created_at: number;
  run_id: string | null;
  session_id: string | null;
}

/**
 * Params of backup.snapshot.
 */
export interface BackupSnapshotParams {
  path: string;
  run_id?: string | null;
  session_id?: string | null;
  agent_id?: string | null;
}

/**
 * Params of backup.list.
 */
export interface BackupListParams {
  run_id?: string | null;
  limit?: number;
}

/**
 * Result of backup.list.
 */
export interface BackupListResult {
  backups: Backup[];
}

/**
 * Params of backup.restore.
 */
export interface BackupRestoreParams {
  backup_id: string;
}

/**
 * Result of backup.restore.
 */
export interface BackupRestoreResult {
  restored: string;
}

/**
 * Result of backup.restore_run.
 */
export interface BackupRestoreRunResult {
  restored: string[];
}

/**
 * A chat session (agent_sessions row).
 */
export interface ChatSession {
  id: string;
  agent_id: string;
  title: string | null;
  model: string | null;
  state: string;
  parent_session_id: string | null;
  permission_mode: PermissionMode;
  summary: string | null;
  tokens_in: number;
  tokens_out: number;
  cost_usd: number;
  created_at: number;
  updated_at: number;
}

/**
 * A chat message (agent_messages row).
 */
export interface ChatMessage {
  id: string;
  session_id: string;
  seq: number;
  /**
   * user, assistant, tool, or system.
   */
  role: string;
  content: string;
  /**
   * Tool calls, when this is an assistant tool-call turn.
   */
  tool_calls: {
    [k: string]: unknown;
  };
  tokens: number;
  created_at: number;
}

/**
 * One full-text search hit.
 */
export interface ChatSearchHit {
  session_id: string;
  message_id: string;
  role: string;
  snippet: string;
}

/**
 * Params of chat.session.create.
 */
export interface ChatSessionCreateParams {
  agent_id: string;
  title?: string | null;
  model?: string | null;
}

/**
 * Result of chat.session.list.
 */
export interface ChatSessionListResult {
  sessions: ChatSession[];
}

/**
 * Params carrying a chat session id.
 */
export interface ChatSessionIdParams {
  session_id: string;
}

/**
 * Params of chat.session.rename.
 */
export interface ChatSessionRenameParams {
  session_id: string;
  title: string;
}

/**
 * Result of chat.session.delete.
 */
export interface ChatDeletedResult {
  deleted: string;
}

/**
 * Params of chat.session.search.
 */
export interface ChatSessionSearchParams {
  query: string;
  limit?: number;
}

/**
 * Result of chat.session.search.
 */
export interface ChatSessionSearchResult {
  hits: ChatSearchHit[];
}

/**
 * Params of chat.message.send.
 */
export interface ChatMessageSendParams {
  session_id: string;
  text: string;
}

/**
 * Result of chat.message.list.
 */
export interface ChatMessageListResult {
  messages: ChatMessage[];
}

/**
 * Result of chat.message.regenerate: the prompt to re-answer.
 */
export interface ChatRegenerateResult {
  prompt: ChatMessage;
}

/**
 * Params of chat.message.fork.
 */
export interface ChatForkParams {
  session_id: string;
  title?: string | null;
}

/**
 * Params of chat.continue.
 */
export interface ChatContinueParams {
  session_id: string;
  /**
   * Capped by the kernel (default 3).
   */
  requested?: number;
}

/**
 * Result of chat.continue.
 */
export interface ChatContinueResult {
  turns: number;
}

/**
 * How an attachment is handled.
 */
export type AttachmentKind = 'text' | 'image' | 'pdf' | 'other';

/**
 * An attachment reference.
 */
export interface Attachment {
  name: string;
  media_type: string;
  size: number;
  kind: AttachmentKind;
}

/**
 * How an attachment was routed into the conversation.
 */
export interface AttachmentRouting {
  attachment: Attachment;
  /**
   * inline, vision, or metadata_only.
   */
  routed: string;
  note: string | null;
}

/**
 * Params of chat.attach.
 */
export interface ChatAttachParams {
  session_id: string;
  name: string;
  media_type?: string;
  size?: number;
}

/**
 * A saved prompt (chat_prompts row).
 */
export interface PromptTemplate {
  id: string;
  title: string;
  body: string;
  tags: string[];
  created_at: number;
}

/**
 * Result of prompt.list.
 */
export interface PromptListResult {
  prompts: PromptTemplate[];
}

/**
 * Params of prompt.create.
 */
export interface PromptCreateParams {
  title: string;
  body: string;
  tags?: string[];
}

/**
 * Payload of the chat.session.created event.
 */
export interface ChatSessionCreatedPayload {
  session_id: string;
  agent_id: string;
}

/**
 * Payload of the chat.message.created event.
 */
export interface ChatMessageCreatedPayload {
  session_id: string;
  message_id: string;
  role: string;
  seq: number;
}
