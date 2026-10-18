//! The indexing pipeline: detect → parse → chunk → embed → store (`SEARCH.md`
//! §13), driven by the persisted job queue.
//!
//! The `Indexer` is synchronous: [`Indexer::scan_and_enqueue`] diffs the vault
//! against `documents` and enqueues jobs, and [`Indexer::process_pending`]
//! drains the queue. The `notify` watcher (see [`crate::watcher`]) only enqueues
//! jobs, so the same processing path serves both.

use std::path::PathBuf;
use std::sync::Arc;

use fergents_markdown::{parse, Frontmatter, ParsedDocument};
use fergents_storage::{Storage, Value};
use serde_json::{json, Value as Json};
use sha2::{Digest, Sha256};

use crate::chunk::{chunk_markdown, chunk_plain_text, Chunk, ChunkConfig};
use crate::embed::Embedder;
use crate::embedding::{EmbedInput, EmbeddingPipeline};
use crate::error::IndexError;
use crate::events::EventSink;
use crate::notes;
use crate::queue::{IndexJob, IndexJobOp, IndexQueue, QueueStats};
use crate::scan::{DocumentKind, VaultScanner};
use crate::vector::VectorStore;

/// Default maximum file size that will be indexed (50 MB).
pub const DEFAULT_MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;
/// Default maximum tokens to embed per document (1M): larger documents are
/// chunked but not embedded.
pub const DEFAULT_MAX_EMBED_TOKENS: u32 = 1_000_000;

/// Indexer configuration.
#[derive(Debug, Clone)]
pub struct IndexerConfig {
    /// The brain id (used in event payloads).
    pub brain_id: String,
    /// The vault root on disk.
    pub vault_root: PathBuf,
    /// Chunking parameters.
    pub chunk: ChunkConfig,
    /// Embedding dimension.
    pub embed_dim: usize,
    /// Files larger than this are skipped.
    pub max_file_bytes: u64,
    /// Documents whose total tokens exceed this are chunked but not embedded.
    pub max_embed_tokens: u32,
}

impl IndexerConfig {
    /// A config with the spec defaults.
    pub fn new(brain_id: impl Into<String>, vault_root: impl Into<PathBuf>) -> Self {
        Self {
            brain_id: brain_id.into(),
            vault_root: vault_root.into(),
            chunk: ChunkConfig::default(),
            embed_dim: crate::embed::DEFAULT_EMBED_DIM,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_embed_tokens: DEFAULT_MAX_EMBED_TOKENS,
        }
    }
}

/// The result of a vault scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScanReport {
    /// Files discovered (after ignores).
    pub scanned: usize,
    /// Jobs enqueued (new or changed files, and removals).
    pub enqueued: usize,
    /// Files skipped as unchanged.
    pub unchanged: usize,
    /// Files skipped because they exceed the size limit.
    pub skipped_oversized: usize,
}

/// The result of draining the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProcessReport {
    /// Jobs processed successfully.
    pub processed: usize,
    /// Jobs that failed (kept for retry).
    pub failed: usize,
}

/// What happened to a single file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexOutcome {
    /// The document was indexed (created or updated).
    Created,
    /// The document was re-indexed.
    Updated,
    /// The document was soft-deleted.
    Deleted,
    /// The file was skipped (too large).
    Skipped,
    /// Nothing to do.
    Unchanged,
}

/// The indexing pipeline for one brain.
pub struct Indexer<E: Embedder, V: VectorStore> {
    storage: Arc<dyn Storage>,
    config: IndexerConfig,
    scanner: VaultScanner,
    queue: IndexQueue,
    embeddings: EmbeddingPipeline<E, V>,
    sink: Arc<dyn EventSink>,
}

impl<E: Embedder, V: VectorStore> Indexer<E, V> {
    /// Create an indexer over the per-brain `storage` with the given pipeline
    /// and event sink.
    pub fn new(
        storage: Arc<dyn Storage>,
        config: IndexerConfig,
        embeddings: EmbeddingPipeline<E, V>,
        sink: Arc<dyn EventSink>,
    ) -> Result<Self, IndexError> {
        let scanner = VaultScanner::new(config.vault_root.clone())?;
        let queue = IndexQueue::new(storage.clone());
        Ok(Self {
            storage,
            config,
            scanner,
            queue,
            embeddings,
            sink,
        })
    }

    /// The job queue.
    pub fn queue(&self) -> &IndexQueue {
        &self.queue
    }

    /// The vault scanner.
    pub fn scanner(&self) -> &VaultScanner {
        &self.scanner
    }

    /// Queue counts for `index.status`.
    pub fn stats(&self) -> Result<QueueStats, IndexError> {
        self.queue.stats()
    }

    /// Full vault scan with hash comparison: enqueue jobs for files that are
    /// new, changed, or removed (`SEARCH.md` §13.1).
    pub fn scan_and_enqueue(&self) -> Result<ScanReport, IndexError> {
        let mut report = ScanReport::default();
        let mut seen: Vec<String> = Vec::new();

        for file in self.scanner.scan() {
            report.scanned += 1;
            seen.push(file.rel_path.clone());

            if file.size > self.config.max_file_bytes {
                report.skipped_oversized += 1;
                continue;
            }

            let hash = file_hash(&file.abs_path)?;
            match self.document_state(&file.rel_path)? {
                Some(state) if state.hash.as_deref() == Some(hash.as_str()) && !state.deleted => {
                    report.unchanged += 1;
                }
                _ => {
                    self.enqueue(&file.rel_path, IndexJobOp::Upsert, 0)?;
                    report.enqueued += 1;
                }
            }
        }

        // Documents on disk-record but no longer present: enqueue deletion.
        for state in self.live_documents()? {
            if !seen.contains(&state.path) {
                self.enqueue(&state.path, IndexJobOp::Delete, 0)?;
                report.enqueued += 1;
            }
        }
        Ok(report)
    }

    /// Drain the persisted queue, resuming any work left by a previous run.
    pub fn process_pending(&self) -> Result<ProcessReport, IndexError> {
        let mut report = ProcessReport::default();
        for job in self.queue.pending()? {
            match self.process_job(&job) {
                Ok(_) => {
                    self.queue.remove(&job.id)?;
                    report.processed += 1;
                }
                Err(err) => {
                    self.queue.record_attempt(&job.id)?;
                    self.sink.emit(
                        "index.failed",
                        json!({
                            "brain_id": self.config.brain_id,
                            "path": job.path,
                            "error": err.to_string(),
                        }),
                    );
                    report.failed += 1;
                }
            }
        }
        Ok(report)
    }

    /// Process a single job.
    pub fn process_job(&self, job: &IndexJob) -> Result<IndexOutcome, IndexError> {
        self.sink.emit(
            "index.started",
            json!({ "brain_id": self.config.brain_id, "path": job.path }),
        );
        self.sink.emit(
            "index.progress",
            json!({
                "brain_id": self.config.brain_id,
                "path": job.path,
                "phase": "detect",
            }),
        );

        let outcome = match job.op {
            IndexJobOp::Delete => self.delete_file(&job.path)?,
            IndexJobOp::Upsert | IndexJobOp::Reindex => self.index_file(&job.path)?,
        };

        self.sink.emit(
            "index.completed",
            json!({ "brain_id": self.config.brain_id, "path": job.path }),
        );
        Ok(outcome)
    }

    /// Index (create or update) one vault-relative file.
    pub fn index_file(&self, rel_path: &str) -> Result<IndexOutcome, IndexError> {
        let abs_path = self.config.vault_root.join(rel_path);
        let metadata = std::fs::metadata(&abs_path).map_err(|e| IndexError::Io(e.to_string()))?;
        let size = metadata.len();
        if size > self.config.max_file_bytes {
            return Ok(IndexOutcome::Skipped);
        }

        let bytes = std::fs::read(&abs_path).map_err(|e| IndexError::Io(e.to_string()))?;
        let hash = hex::encode(Sha256::digest(&bytes));
        let (kind, mime, is_text) = crate::scan::classify(rel_path);

        let parsed = if is_text {
            Some(parse(&String::from_utf8_lossy(&bytes)))
        } else {
            None
        };
        let text = parsed.as_ref().map(|p| p.body.clone());
        let word_count = text.as_deref().map_or(0, count_words);
        let reading_time_s = word_count / 200;
        let title = derive_title(parsed.as_ref(), rel_path);

        let existing = self.document_state(rel_path)?;
        let was_deleted = existing.as_ref().is_some_and(|s| s.deleted);
        let created = existing.is_none();
        let document_id = existing
            .as_ref()
            .map(|s| s.id.clone())
            .unwrap_or_else(|| ulid::Ulid::new().to_string());

        let now = now_ms();
        let frontmatter = parsed
            .as_ref()
            .and_then(|p| p.frontmatter.as_ref())
            .and_then(|f| serde_json::to_string(&f.parsed).ok());

        self.storage.execute(
            "INSERT INTO documents \
             (id, path, kind, title, mime, size, hash, frontmatter, word_count, reading_time_s, \
              created_at, updated_at, indexed_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11, ?11) \
             ON CONFLICT(path) DO UPDATE SET kind = excluded.kind, title = excluded.title, \
             mime = excluded.mime, size = excluded.size, hash = excluded.hash, \
             frontmatter = excluded.frontmatter, word_count = excluded.word_count, \
             reading_time_s = excluded.reading_time_s, updated_at = excluded.updated_at, \
             indexed_at = excluded.indexed_at, deleted_at = NULL",
            &[
                Value::String(document_id.clone()),
                Value::String(rel_path.to_string()),
                Value::String(kind.as_str().to_string()),
                Value::String(title.clone()),
                Value::String(mime.clone()),
                Value::from(size),
                Value::String(hash.clone()),
                frontmatter.map_or(Value::Null, Value::String),
                Value::from(u64::from(word_count)),
                Value::from(u64::from(reading_time_s)),
                Value::from(now),
            ],
        )?;

        // Refresh the note's links, tags, and metadata (Task 9).
        notes::clear_note_data(&self.storage, &document_id)?;
        // Resolve other notes' earlier links that now point at this document,
        // so resolution never depends on indexing order.
        notes::resolve_incoming_links(&self.storage, &document_id, rel_path, &title)?;
        if kind == DocumentKind::Note {
            if let Some(parsed) = &parsed {
                notes::persist_links(&self.storage, &document_id, &parsed.links, &parsed.body)?;
                let mut tags = parsed.tags.clone();
                tags.extend(notes::frontmatter_tags(parsed.frontmatter.as_ref()));
                notes::persist_tags(&self.storage, &document_id, &tags)?;
                notes::upsert_notes_meta(&self.storage, &document_id, parsed.frontmatter.as_ref())?;
            }
        }

        if let Some(text) = &text {
            self.replace_chunks(&document_id, text, kind)?;
        }
        let document = json!({
            "id": document_id,
            "path": rel_path,
            "kind": kind.as_str(),
            "title": title,
            "mime": mime,
            "size": size,
            "hash": hash,
            "word_count": word_count,
            "reading_time_s": reading_time_s,
            "created_at": now,
            "updated_at": now,
            "indexed_at": now,
            "deleted_at": Json::Null,
        });
        if kind == DocumentKind::Note {
            let note_kind = if created || was_deleted {
                "note.created"
            } else {
                "note.updated"
            };
            self.sink.emit(note_kind, json!({ "document": document }));
        }

        Ok(if created || was_deleted {
            IndexOutcome::Created
        } else {
            IndexOutcome::Updated
        })
    }

    /// Soft-delete the document for `rel_path` (`SEARCH.md` §13.1).
    pub fn delete_file(&self, rel_path: &str) -> Result<IndexOutcome, IndexError> {
        let Some(state) = self.document_state(rel_path)? else {
            return Ok(IndexOutcome::Unchanged);
        };
        self.storage.execute(
            "UPDATE documents SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2",
            &[Value::from(now_ms()), Value::String(state.id.clone())],
        )?;
        self.remove_chunks(&state.id)?;
        notes::clear_note_data(&self.storage, &state.id)?;
        Ok(IndexOutcome::Deleted)
    }

    /// Replace a document's chunks and (re-)embed them.
    fn replace_chunks(
        &self,
        document_id: &str,
        text: &str,
        kind: DocumentKind,
    ) -> Result<usize, IndexError> {
        self.remove_chunks(document_id)?;

        let chunks = if kind == DocumentKind::Note {
            chunk_markdown(text, &self.config.chunk)
        } else {
            chunk_plain_text(text, &self.config.chunk)
        };
        let mut total_tokens: u32 = 0;
        let mut inputs = Vec::with_capacity(chunks.len());
        for (ordinal, chunk) in chunks.iter().enumerate() {
            let chunk_id = ulid::Ulid::new().to_string();
            self.insert_chunk(document_id, ordinal, &chunk_id, chunk)?;
            inputs.push(EmbedInput {
                chunk_id,
                content: chunk.content.clone(),
            });
            total_tokens = total_tokens.saturating_add(chunk.token_count);
        }

        if total_tokens > self.config.max_embed_tokens {
            return Ok(0);
        }
        let report = self.embeddings.embed(&inputs)?;
        Ok(report.embedded + report.cache_hits)
    }

    fn insert_chunk(
        &self,
        document_id: &str,
        ordinal: usize,
        chunk_id: &str,
        chunk: &Chunk,
    ) -> Result<(), IndexError> {
        self.storage.execute(
            "INSERT INTO chunks \
             (id, document_id, ordinal, content, token_count, start_offset, end_offset, \
              heading_path, language, symbol) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            &[
                Value::String(chunk_id.to_string()),
                Value::String(document_id.to_string()),
                Value::from(ordinal as u64),
                Value::String(chunk.content.clone()),
                Value::from(u64::from(chunk.token_count)),
                Value::from(chunk.start_offset as u64),
                Value::from(chunk.end_offset as u64),
                chunk
                    .heading_path
                    .clone()
                    .map_or(Value::Null, Value::String),
                chunk.language.clone().map_or(Value::Null, Value::String),
                chunk.symbol.clone().map_or(Value::Null, Value::String),
            ],
        )?;
        // Mirror the new row into the external-content FTS index (Task 10).
        self.storage.execute(
            "INSERT INTO chunks_fts (rowid, content, heading_path) \
             SELECT rowid, content, heading_path FROM chunks WHERE id = ?1",
            &[Value::String(chunk_id.to_string())],
        )?;
        Ok(())
    }

    fn remove_chunks(&self, document_id: &str) -> Result<(), IndexError> {
        let rows = self.storage.query(
            "SELECT id, rowid, content, heading_path FROM chunks WHERE document_id = ?1",
            &[Value::String(document_id.to_string())],
        )?;
        for row in &rows {
            if let Some(id) = row.get("id").and_then(Value::as_str) {
                self.embeddings.store().delete(id)?;
                self.embeddings.cache().remove(id)?;
            }
            // `chunks_fts` is external-content, so its rows must be removed with
            // the special `'delete'` command, which needs the original values
            // (a plain `DELETE` is a silent no-op for FTS5 external content).
            self.storage.execute(
                "INSERT INTO chunks_fts (chunks_fts, rowid, content, heading_path) \
                 VALUES ('delete', ?1, ?2, ?3)",
                &[
                    row.get("rowid").cloned().unwrap_or(Value::Null),
                    row.get("content").cloned().unwrap_or(Value::Null),
                    row.get("heading_path").cloned().unwrap_or(Value::Null),
                ],
            )?;
        }
        self.storage.execute(
            "DELETE FROM chunks WHERE document_id = ?1",
            &[Value::String(document_id.to_string())],
        )?;
        Ok(())
    }

    fn enqueue(&self, rel_path: &str, op: IndexJobOp, priority: i64) -> Result<(), IndexError> {
        self.queue.enqueue(rel_path, op, priority)?;
        self.sink.emit(
            "index.queued",
            json!({ "brain_id": self.config.brain_id, "path": rel_path, "op": op.as_str() }),
        );
        Ok(())
    }

    fn document_state(&self, rel_path: &str) -> Result<Option<DocumentState>, IndexError> {
        let rows = self.storage.query(
            "SELECT id, path, hash, deleted_at FROM documents WHERE path = ?1",
            &[Value::String(rel_path.to_string())],
        )?;
        Ok(rows.first().map(row_to_state))
    }

    fn live_documents(&self) -> Result<Vec<DocumentState>, IndexError> {
        let rows = self.storage.query(
            "SELECT id, path, hash, deleted_at FROM documents WHERE deleted_at IS NULL",
            &[],
        )?;
        Ok(rows.iter().map(row_to_state).collect())
    }
}

/// A document's identity and freshness, as needed by the scan diff.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DocumentState {
    id: String,
    path: String,
    hash: Option<String>,
    deleted: bool,
}

fn row_to_state(row: &fergents_storage::Row) -> DocumentState {
    DocumentState {
        id: row
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        path: row
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        hash: row.get("hash").and_then(Value::as_str).map(str::to_string),
        deleted: row.get("deleted_at").is_some_and(|v| !v.is_null()),
    }
}

fn file_hash(path: &std::path::Path) -> Result<String, IndexError> {
    let bytes = std::fs::read(path).map_err(|e| IndexError::Io(e.to_string()))?;
    Ok(hex::encode(Sha256::digest(&bytes)))
}

fn count_words(text: &str) -> u32 {
    text.split_whitespace().count() as u32
}

fn derive_title(parsed: Option<&ParsedDocument>, rel_path: &str) -> String {
    if let Some(title) = parsed
        .and_then(|p| p.frontmatter.as_ref())
        .and_then(frontmatter_title)
    {
        return title;
    }
    if let Some(heading) = parsed.and_then(|p| first_heading(&p.body)) {
        return heading;
    }
    std::path::Path::new(rel_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(rel_path)
        .to_string()
}

fn frontmatter_title(frontmatter: &Frontmatter) -> Option<String> {
    frontmatter
        .parsed
        .as_ref()?
        .get("title")?
        .as_str()
        .map(str::to_string)
}

fn first_heading(body: &str) -> Option<String> {
    for line in body.lines() {
        let trimmed = line.trim_start_matches(' ');
        if line.len() - trimmed.len() > 3 {
            continue;
        }
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if hashes == 0 || hashes > 6 {
            continue;
        }
        let rest = &trimmed[hashes..];
        if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
            let text = rest.trim().trim_end_matches('#').trim();
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    None
}

/// Wall-clock time in ms since the Unix epoch.
fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}
