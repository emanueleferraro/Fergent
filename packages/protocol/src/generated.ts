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
}

/**
 * Payload of the events.dropped notice.
 */
export interface EventsDroppedNotice {
  dropped: number;
  last_seen_id: string | null;
}
