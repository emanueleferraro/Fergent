//! Query analysis, structured filters, and the BM25 leg of search
//! (`SEARCH.md` §5, §9, §10, §11).
//!
//! [`crate::SearchEngine`] orchestrates the full hybrid pipeline (BM25 + vector
//! → RRF fusion → rerank); this module provides the shared vocabulary types,
//! query analysis, the filter predicates, and the low-level SQL used by the
//! engine.
//!
//! Filters are applied **before** ranking (`SEARCH.md` §10.3): filtered-out
//! chunks never reach the FTS5 `MATCH` or the vector search, so they can never
//! appear in results.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use fergents_storage::{Storage, Value};

use crate::error::IndexError;
use crate::fusion::RankedList;

/// Default number of results (`SEARCH.md` §20, `top_k`).
pub const DEFAULT_TOP_K: usize = 8;
/// Per-stage candidate pool kept for fusion (`SEARCH.md` §5.3, §6.2).
pub const CANDIDATE_K: usize = 100;

/// Search mode (`SEARCH.md` §2, §20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    /// BM25 over FTS5.
    Bm25,
    /// Dense vector search via `sqlite-vec`.
    Vector,
    /// Sparse (BGE-M3) search.
    Sparse,
    /// Every available leg, fused with RRF.
    Hybrid,
}

impl SearchMode {
    /// The wire name (`SEARCH.md` §20).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bm25 => "bm25",
            Self::Vector => "vector",
            Self::Sparse => "sparse",
            Self::Hybrid => "hybrid",
        }
    }
}

impl FromStr for SearchMode {
    type Err = IndexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "bm25" => Ok(Self::Bm25),
            "vector" => Ok(Self::Vector),
            "sparse" => Ok(Self::Sparse),
            "hybrid" => Ok(Self::Hybrid),
            other => Err(IndexError::InvalidQuery(format!(
                "unknown search mode '{other}'"
            ))),
        }
    }
}

/// Structured search filters (`SEARCH.md` §10.1, §10.2).
///
/// Dates are ISO-ish strings: `YYYY`, `YYYY-MM`, or `YYYY-MM-DD`; they are
/// normalized to a day range when the query is built.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchFilters {
    /// Only documents carrying **all** of these tags.
    pub tags: Vec<String>,
    /// Exclude documents carrying **any** of these tags.
    pub exclude_tags: Vec<String>,
    /// Path prefix.
    pub path: Option<String>,
    /// Excluded path prefix.
    pub exclude_path: Option<String>,
    /// `note`, `attachment`, `code`, or `other` (`documents.kind`).
    pub doc_type: Option<String>,
    /// Chunk language (for code).
    pub language: Option<String>,
    /// Only documents updated on or after this date.
    pub after: Option<String>,
    /// Only documents updated on or before this date.
    pub before: Option<String>,
    /// `link`, `backlink`, or `attachment`.
    pub has: Option<String>,
    /// `frontmatter.<key>` filters: `(key, value)` pairs (all must match).
    pub frontmatter: Vec<(String, String)>,
}

impl SearchFilters {
    /// Fold `overlay`'s set fields into `self` (overlay wins for scalars; list
    /// fields are appended). Used to combine inline `key:value` filters with
    /// the structured `filters` param.
    pub(crate) fn absorb(&mut self, overlay: &SearchFilters) {
        self.tags.extend(overlay.tags.iter().cloned());
        self.exclude_tags
            .extend(overlay.exclude_tags.iter().cloned());
        self.frontmatter.extend(overlay.frontmatter.iter().cloned());
        if overlay.path.is_some() {
            self.path.clone_from(&overlay.path);
        }
        if overlay.exclude_path.is_some() {
            self.exclude_path.clone_from(&overlay.exclude_path);
        }
        if overlay.doc_type.is_some() {
            self.doc_type.clone_from(&overlay.doc_type);
        }
        if overlay.language.is_some() {
            self.language.clone_from(&overlay.language);
        }
        if overlay.after.is_some() {
            self.after.clone_from(&overlay.after);
        }
        if overlay.before.is_some() {
            self.before.clone_from(&overlay.before);
        }
        if overlay.has.is_some() {
            self.has.clone_from(&overlay.has);
        }
    }
}

/// The result of analyzing a query string (`SEARCH.md` §9.1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnalyzedQuery {
    /// Bare terms (matched as an implicit AND).
    pub terms: Vec<String>,
    /// Quoted phrases (exact match).
    pub phrases: Vec<String>,
    /// Negated terms/phrases (must not appear).
    pub excluded: Vec<String>,
    /// Filters extracted from `key:value` tokens.
    pub filters: SearchFilters,
    /// Brains requested via a `brain:` token or param.
    pub brains: Vec<String>,
}

/// A search request (`search.query`, `SEARCH.md` §20).
#[derive(Debug, Clone)]
pub struct SearchRequest {
    /// The raw query string.
    pub query: String,
    /// The requested mode. Defaults to [`SearchMode::Hybrid`].
    pub mode: SearchMode,
    /// Maximum number of results.
    pub top_k: usize,
    /// Brains to search (empty = the current brain).
    pub brains: Vec<String>,
    /// Structured filters (merged with inline `key:value` filters).
    pub filters: SearchFilters,
    /// Whether to rerank the fused candidates (`SEARCH.md` §8.3).
    pub rerank: bool,
}

impl SearchRequest {
    /// A hybrid request for `query` with the default filters, `top_k`, and
    /// reranking.
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            mode: SearchMode::Hybrid,
            top_k: DEFAULT_TOP_K,
            brains: Vec::new(),
            filters: SearchFilters::default(),
            rerank: true,
        }
    }
}

/// Per-stage scores for one result (`SEARCH.md` §11.1).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StageScores {
    /// BM25 score (`-bm25`, higher is better).
    pub bm25: Option<f64>,
    /// Vector similarity (higher is better).
    pub vector: Option<f64>,
    /// Sparse score.
    pub sparse: Option<f64>,
    /// RRF fusion score.
    pub fusion: Option<f64>,
    /// Cross-encoder rerank score.
    pub rerank: Option<f64>,
}

/// Metadata attached to a result (`SEARCH.md` §11.1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HitMetadata {
    /// The document's tags.
    pub tags: Vec<String>,
    /// The document's creation date (`YYYY-MM-DD`).
    pub created: Option<String>,
    /// The brain the result came from.
    pub brain: String,
}

/// One search result (`SEARCH.md` §11.1).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    /// The chunk id.
    pub chunk_id: String,
    /// The owning document.
    pub document_id: String,
    /// The vault-relative path.
    pub path: String,
    /// The document title.
    pub title: String,
    /// The heading path, if any.
    pub heading_path: Option<String>,
    /// The full chunk content.
    pub content: String,
    /// A highlighted snippet.
    pub snippet: String,
    /// The final relevance score (higher is better).
    pub score: f64,
    /// The per-stage score breakdown.
    pub scores: StageScores,
    /// The citation string (`SEARCH.md` §11.4).
    pub citation: String,
    /// The result metadata.
    pub metadata: HitMetadata,
}

/// How many candidates each stage produced (`SEARCH.md` §20).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CandidateCounts {
    /// BM25 candidates.
    pub bm25: usize,
    /// Vector candidates.
    pub vector: usize,
    /// Sparse candidates.
    pub sparse: usize,
}

/// A search response (`SEARCH.md` §20).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResponse {
    /// The ranked results.
    pub results: Vec<SearchHit>,
    /// The per-stage candidate counts.
    pub candidates: CandidateCounts,
    /// Wall-clock latency in milliseconds.
    pub latency_ms: i64,
}

/// Analyze a query string into terms, phrases, negation, and inline filters
/// (`SEARCH.md` §9.1).
pub fn analyze_query(query: &str) -> AnalyzedQuery {
    let mut analyzed = AnalyzedQuery::default();
    for raw in tokenize(query) {
        if raw.quoted {
            if raw.negated {
                analyzed.excluded.push(raw.text);
            } else {
                analyzed.phrases.push(raw.text);
            }
            continue;
        }
        let text = raw.text.as_str();
        if let Some((key, value)) = text.split_once(':') {
            let key = key.to_ascii_lowercase();
            if !value.is_empty() && apply_filter(&mut analyzed, &key, value, raw.negated) {
                continue;
            }
        }
        if raw.negated {
            analyzed.excluded.push(raw.text);
        } else {
            analyzed.terms.push(raw.text);
        }
    }
    analyzed
}

// --- Query construction ----------------------------------------------------

/// The shared `FROM` clause: the FTS index joined to its content `chunks` and
/// their `documents`.
pub(crate) const FROM_CLAUSE: &str = "chunks_fts \
     JOIN chunks c ON c.rowid = chunks_fts.rowid \
     JOIN documents d ON d.id = c.document_id";

/// Build the FTS5 `MATCH` expression from the analyzed query. Returns an empty
/// string when there is nothing positive to match.
pub(crate) fn build_match(analyzed: &AnalyzedQuery) -> String {
    let mut positive: Vec<String> = Vec::new();
    for term in &analyzed.terms {
        positive.push(quote_fts(term));
    }
    for phrase in &analyzed.phrases {
        positive.push(quote_fts(phrase));
    }
    if positive.is_empty() {
        return String::new();
    }
    let positive = positive.join(" ");
    if analyzed.excluded.is_empty() {
        return positive;
    }
    let negated: Vec<String> = analyzed.excluded.iter().map(|t| quote_fts(t)).collect();
    format!("({positive}) NOT ({})", negated.join(" OR "))
}

/// Quote a token for FTS5 (wrapping in `"..."` disables operators and escaping
/// issues; embedded quotes are doubled).
fn quote_fts(token: &str) -> String {
    format!("\"{}\"", token.replace('"', "\"\""))
}

/// The BM25 leg: the top-`k` chunks for `match_expr` passing `filters`, best
/// first (`SEARCH.md` §5).
pub(crate) fn bm25_candidates(
    storage: &Arc<dyn Storage>,
    match_expr: &str,
    filters: &SearchFilters,
    k: usize,
) -> Result<RankedList, IndexError> {
    if match_expr.is_empty() {
        return Ok(RankedList::empty(crate::fusion::Stage::Bm25));
    }
    let (predicates, mut params) = filter_predicates(filters)?;
    let mut where_sql = String::from("WHERE chunks_fts MATCH ? AND d.deleted_at IS NULL");
    for predicate in &predicates {
        where_sql.push_str(" AND ");
        where_sql.push_str(predicate);
    }
    // The MATCH parameter comes first in the statement.
    params.insert(0, Value::String(match_expr.to_string()));

    let sql = format!(
        "SELECT c.id AS chunk_id, -bm25(chunks_fts) AS score \
         FROM {FROM_CLAUSE} {where_sql} ORDER BY bm25(chunks_fts) ASC LIMIT ?"
    );
    params.push(Value::from(k.max(1) as u64));

    let rows = storage.query(&sql, &params)?;
    Ok(RankedList {
        stage: crate::fusion::Stage::Bm25,
        hits: rows
            .iter()
            .map(|row| {
                (
                    string_field(row, "chunk_id"),
                    row.get("score").and_then(Value::as_f64).unwrap_or(0.0),
                )
            })
            .collect(),
    })
}

/// The AND-joined filter predicates (referencing aliases `c` and `d`) and their
/// parameters. Shared by the BM25 and vector legs so filters apply identically.
pub(crate) fn filter_predicates(
    filters: &SearchFilters,
) -> Result<(Vec<String>, Vec<Value>), IndexError> {
    let mut predicates = Vec::new();
    let mut params = Vec::new();

    for tag in &filters.tags {
        predicates.push(
            "EXISTS (SELECT 1 FROM document_tags dt JOIN tags t ON t.id = dt.tag_id \
             WHERE dt.document_id = d.id AND t.name = ?)"
                .to_string(),
        );
        params.push(Value::String(tag.clone()));
    }
    for tag in &filters.exclude_tags {
        predicates.push(
            "NOT EXISTS (SELECT 1 FROM document_tags dt JOIN tags t ON t.id = dt.tag_id \
             WHERE dt.document_id = d.id AND t.name = ?)"
                .to_string(),
        );
        params.push(Value::String(tag.clone()));
    }
    if let Some(path) = &filters.path {
        predicates.push("d.path LIKE ? ESCAPE '\\'".to_string());
        params.push(Value::String(path_prefix(path)));
    }
    if let Some(path) = &filters.exclude_path {
        predicates.push("d.path NOT LIKE ? ESCAPE '\\'".to_string());
        params.push(Value::String(path_prefix(path)));
    }
    if let Some(kind) = &filters.doc_type {
        predicates.push("d.kind = ?".to_string());
        params.push(Value::String(kind.clone()));
    }
    if let Some(language) = &filters.language {
        predicates.push("c.language = ?".to_string());
        params.push(Value::String(language.clone()));
    }
    if let Some(after) = &filters.after {
        predicates.push("date(d.updated_at / 1000, 'unixepoch') >= ?".to_string());
        params.push(Value::String(normalize_date(after, DateBound::Start)?));
    }
    if let Some(before) = &filters.before {
        predicates.push("date(d.updated_at / 1000, 'unixepoch') <= ?".to_string());
        params.push(Value::String(normalize_date(before, DateBound::End)?));
    }
    if let Some(has) = &filters.has {
        predicates.push(has_predicate(has)?);
    }
    for (key, value) in &filters.frontmatter {
        predicates.push("json_extract(d.frontmatter, ?) = ?".to_string());
        params.push(Value::String(format!("$.\"{}\"", key.replace('"', "\\\""))));
        params.push(Value::String(value.clone()));
    }

    Ok((predicates, params))
}

/// The `has:` predicate (`SEARCH.md` §10.2).
fn has_predicate(has: &str) -> Result<String, IndexError> {
    match has.to_ascii_lowercase().as_str() {
        "link" => Ok("EXISTS (SELECT 1 FROM links l WHERE l.src_document_id = d.id)".to_string()),
        "backlink" => {
            Ok("EXISTS (SELECT 1 FROM links l WHERE l.dst_document_id = d.id)".to_string())
        }
        "attachment" => Ok(
            "EXISTS (SELECT 1 FROM links l JOIN documents a ON a.id = l.dst_document_id \
             WHERE l.src_document_id = d.id AND a.kind = 'attachment')"
                .to_string(),
        ),
        other => Err(IndexError::InvalidQuery(format!(
            "unknown 'has' filter '{other}' (expected link, backlink, or attachment)"
        ))),
    }
}

/// Escape `LIKE` metacharacters and append the prefix wildcard.
fn path_prefix(prefix: &str) -> String {
    let mut out = String::with_capacity(prefix.len() + 1);
    for ch in prefix.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('%');
    out
}

/// Apply an inline `key:value` token to the analyzed query. Returns `true` if
/// the token was a recognized filter.
fn apply_filter(analyzed: &mut AnalyzedQuery, key: &str, value: &str, negated: bool) -> bool {
    let value = value.to_string();
    match key {
        "tag" if negated => analyzed.filters.exclude_tags.push(value),
        "tag" => analyzed.filters.tags.push(value),
        "path" if negated => analyzed.filters.exclude_path = Some(value),
        "path" => analyzed.filters.path = Some(value),
        "type" => analyzed.filters.doc_type = Some(value),
        "language" => analyzed.filters.language = Some(value),
        "after" => analyzed.filters.after = Some(value),
        "before" => analyzed.filters.before = Some(value),
        "has" => analyzed.filters.has = Some(value),
        "brain" => analyzed.brains.push(value),
        _ => {
            if let Some(name) = key.strip_prefix("frontmatter.") {
                if !name.is_empty() {
                    analyzed.filters.frontmatter.push((name.to_string(), value));
                    return true;
                }
            }
            return false;
        }
    }
    true
}

// --- Hydration -------------------------------------------------------------

/// The display data for one chunk.
pub(crate) struct ChunkMeta {
    /// The owning document.
    pub document_id: String,
    /// The vault-relative path.
    pub path: String,
    /// The document title.
    pub title: String,
    /// The heading path.
    pub heading_path: Option<String>,
    /// The chunk content.
    pub content: String,
    /// The document's creation time (epoch ms).
    pub created_at: i64,
}

/// Load display data for `chunk_ids`.
pub(crate) fn chunk_meta(
    storage: &Arc<dyn Storage>,
    chunk_ids: &[String],
) -> Result<HashMap<String, ChunkMeta>, IndexError> {
    let mut out = HashMap::new();
    if chunk_ids.is_empty() {
        return Ok(out);
    }
    let placeholders = vec!["?"; chunk_ids.len()].join(", ");
    let sql = format!(
        "SELECT c.id AS chunk_id, c.document_id AS document_id, c.heading_path AS heading_path, \
         c.content AS content, d.path AS path, d.title AS title, d.created_at AS created_at \
         FROM chunks c JOIN documents d ON d.id = c.document_id WHERE c.id IN ({placeholders})"
    );
    let params: Vec<Value> = chunk_ids
        .iter()
        .map(|id| Value::String(id.clone()))
        .collect();
    for row in &storage.query(&sql, &params)? {
        out.insert(
            string_field(row, "chunk_id"),
            ChunkMeta {
                document_id: string_field(row, "document_id"),
                path: string_field(row, "path"),
                title: string_field(row, "title"),
                heading_path: row
                    .get("heading_path")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                content: string_field(row, "content"),
                created_at: row.get("created_at").and_then(Value::as_i64).unwrap_or(0),
            },
        );
    }
    Ok(out)
}

/// A highlighted snippet for `chunk_id` when it matches `match_expr`, else an
/// excerpt of its content (`SEARCH.md` §11.3).
pub(crate) fn snippet_for(
    storage: &Arc<dyn Storage>,
    match_expr: Option<&str>,
    chunk_id: &str,
    content: &str,
) -> Result<String, IndexError> {
    if let Some(match_expr) = match_expr.filter(|m| !m.is_empty()) {
        let rows = storage.query(
            "SELECT snippet(chunks_fts, 0, '<mark>', '</mark>', '…', 12) AS snippet \
             FROM chunks_fts JOIN chunks c ON c.rowid = chunks_fts.rowid \
             WHERE chunks_fts MATCH ?1 AND c.id = ?2",
            &[
                Value::String(match_expr.to_string()),
                Value::String(chunk_id.to_string()),
            ],
        )?;
        if let Some(snippet) = rows
            .first()
            .and_then(|row| row.get("snippet"))
            .and_then(Value::as_str)
        {
            return Ok(snippet.to_string());
        }
    }
    Ok(excerpt(content))
}

/// The tags attached to a document.
pub(crate) fn tags_for(
    storage: &Arc<dyn Storage>,
    document_id: &str,
) -> Result<Vec<String>, IndexError> {
    let rows = storage.query(
        "SELECT t.name AS name FROM document_tags dt JOIN tags t ON t.id = dt.tag_id \
         WHERE dt.document_id = ?1 ORDER BY t.name ASC",
        &[Value::String(document_id.to_string())],
    )?;
    Ok(rows
        .iter()
        .map(|row| string_field(row, "name"))
        .filter(|name| !name.is_empty())
        .collect())
}

/// Build a citation string (`SEARCH.md` §11.4).
pub(crate) fn citation(index: usize, path: &str, heading_path: Option<&str>, score: f64) -> String {
    let anchor = heading_path
        .and_then(|h| h.rsplit(" > ").next())
        .map(slug)
        .filter(|s| !s.is_empty());
    match anchor {
        Some(anchor) => format!("[{index}] {path}#{anchor} (score {score:.2})"),
        None => format!("[{index}] {path} (score {score:.2})"),
    }
}

fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_end_matches('-').to_string()
}

fn excerpt(content: &str) -> String {
    const LIMIT: usize = 200;
    let trimmed = content.trim();
    if trimmed.chars().count() <= LIMIT {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(LIMIT).collect();
    out.push('…');
    out
}

/// Format an epoch-ms timestamp as `YYYY-MM-DD`.
pub(crate) fn format_date(ms: i64) -> String {
    let (year, month, day) = civil_from_days(ms.div_euclid(86_400_000));
    format!("{year:04}-{month:02}-{day:02}")
}

/// Howard Hinnant's `civil_from_days`.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// --- Tokenizer -------------------------------------------------------------

/// A raw token before classification.
struct RawToken {
    text: String,
    quoted: bool,
    negated: bool,
}

/// Split a query into tokens, honoring `"quoted phrases"` and a leading `-`
/// for negation (`SEARCH.md` §9.1).
fn tokenize(query: &str) -> Vec<RawToken> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = query.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let mut negated = false;
        if chars[i] == '-' {
            negated = true;
            i += 1;
        }
        if i < chars.len() && chars[i] == '"' {
            i += 1;
            let mut text = String::new();
            while i < chars.len() && chars[i] != '"' {
                text.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                i += 1; // closing quote
            }
            if !text.is_empty() {
                tokens.push(RawToken {
                    text,
                    quoted: true,
                    negated,
                });
            }
            continue;
        }
        let mut text = String::new();
        while i < chars.len() && !chars[i].is_whitespace() {
            text.push(chars[i]);
            i += 1;
        }
        if !text.is_empty() {
            tokens.push(RawToken {
                text,
                quoted: false,
                negated,
            });
        }
    }
    tokens
}

// --- Dates -----------------------------------------------------------------

/// Which end of a period a date filter targets.
enum DateBound {
    /// `after:` — the first day of the period.
    Start,
    /// `before:` — the last day of the period.
    End,
}

/// Normalize `YYYY`, `YYYY-MM`, or `YYYY-MM-DD` to a full `YYYY-MM-DD`.
fn normalize_date(input: &str, bound: DateBound) -> Result<String, IndexError> {
    let invalid = || IndexError::InvalidQuery(format!("invalid date filter '{input}'"));
    let parts: Vec<&str> = input.split('-').collect();
    let year: i64 = parts
        .first()
        .and_then(|y| y.parse().ok())
        .filter(|y| *y > 0)
        .ok_or_else(invalid)?;
    match parts.len() {
        1 => {
            let (month, day) = match bound {
                DateBound::Start => (1, 1),
                DateBound::End => (12, 31),
            };
            Ok(format!("{year:04}-{month:02}-{day:02}"))
        }
        2 => {
            let month: u32 = parts[1].parse().map_err(|_| invalid())?;
            if !(1..=12).contains(&month) {
                return Err(invalid());
            }
            let day = match bound {
                DateBound::Start => 1,
                DateBound::End => days_in_month(year, month),
            };
            Ok(format!("{year:04}-{month:02}-{day:02}"))
        }
        3 => {
            let month: u32 = parts[1].parse().map_err(|_| invalid())?;
            let day: u32 = parts[2].parse().map_err(|_| invalid())?;
            if !(1..=12).contains(&month) || !(1..=days_in_month(year, month)).contains(&day) {
                return Err(invalid());
            }
            Ok(format!("{year:04}-{month:02}-{day:02}"))
        }
        _ => Err(invalid()),
    }
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 30,
    }
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn string_field(row: &fergents_storage::Row, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_formatting_round_trips_known_days() {
        assert_eq!(format_date(0), "1970-01-01");
        assert_eq!(format_date(1_717_200_000_000), "2024-06-01");
        assert_eq!(format_date(1_748_736_000_000), "2025-06-01");
    }

    #[test]
    fn citation_uses_the_last_heading_as_anchor() {
        assert_eq!(
            citation(
                1,
                "notes/mcp.md",
                Some("MCP > Threats > Prompt Injection"),
                0.87
            ),
            "[1] notes/mcp.md#prompt-injection (score 0.87)"
        );
        assert_eq!(citation(2, "a.md", None, 0.5), "[2] a.md (score 0.50)");
    }
}
