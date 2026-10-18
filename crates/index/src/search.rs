//! BM25 search over the FTS5 index, plus query analysis and structured
//! filters (`SEARCH.md` §5, §9, §10).
//!
//! Task 10 ships the **BM25 leg only**: query analysis → structured filters →
//! FTS5 `MATCH` with `bm25()` ranking. Dense/sparse retrieval, RRF fusion, and
//! reranking are added in Task 11; [`SearchMode`] reserves their names so the
//! protocol surface stays stable.
//!
//! Filters are applied **before** ranking (`SEARCH.md` §10.3): filtered-out
//! chunks never reach the FTS5 `MATCH`, so they can never appear in results.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

use fergents_storage::{Storage, Value};

use crate::error::IndexError;

/// Default number of results (`SEARCH.md` §20, `top_k`).
pub const DEFAULT_TOP_K: usize = 8;

/// Search mode (`SEARCH.md` §2, §20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    /// BM25 over FTS5 (implemented).
    Bm25,
    /// Dense vector search via `sqlite-vec` (Task 11).
    Vector,
    /// Sparse (BGE-M3) search (Task 11).
    Sparse,
    /// BM25 + vector + sparse, fused with RRF (Task 11).
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
    fn absorb(&mut self, overlay: &SearchFilters) {
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

/// A BM25 search request (`search.query`, `SEARCH.md` §20).
#[derive(Debug, Clone)]
pub struct SearchRequest {
    /// The raw query string.
    pub query: String,
    /// The requested mode. Defaults to [`SearchMode::Bm25`] until Task 11.
    pub mode: SearchMode,
    /// Maximum number of results.
    pub top_k: usize,
    /// Brains to search (empty = the current brain).
    pub brains: Vec<String>,
    /// Structured filters (merged with inline `key:value` filters).
    pub filters: SearchFilters,
}

impl SearchRequest {
    /// A BM25 request for `query` with the default filters and `top_k`.
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            mode: SearchMode::Bm25,
            top_k: DEFAULT_TOP_K,
            brains: Vec::new(),
            filters: SearchFilters::default(),
        }
    }
}

/// One BM25 search result (`SEARCH.md` §11.1).
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
    /// The relevance score (higher is better; `-bm25`).
    pub score: f64,
    /// The BM25 component of the score.
    pub bm25: f64,
}

/// A BM25 search response (`SEARCH.md` §20).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResponse {
    /// The ranked results.
    pub results: Vec<SearchHit>,
    /// The number of chunks that matched (before `top_k`).
    pub candidates: usize,
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

/// Run a search against the per-brain `storage`, which represents `brain_id`.
///
/// Only [`SearchMode::Bm25`] is implemented in Task 10; the other modes return
/// [`IndexError::InvalidQuery`] until Task 11. When `request.brains` is
/// non-empty and does not include `brain_id`, the brain is skipped (the
/// cross-brain filter, `SEARCH.md` §10.2).
pub fn search(
    storage: &Arc<dyn Storage>,
    brain_id: &str,
    request: &SearchRequest,
) -> Result<SearchResponse, IndexError> {
    if request.mode != SearchMode::Bm25 {
        return Err(IndexError::InvalidQuery(format!(
            "search mode '{}' is not implemented yet (Task 11)",
            request.mode.as_str()
        )));
    }

    let analyzed = analyze_query(&request.query);
    let mut filters = analyzed.filters.clone();
    filters.absorb(&request.filters);

    // Cross-brain gate: a requested brain set that excludes this brain means
    // there is nothing to search here.
    let mut brains = request.brains.clone();
    for brain in &analyzed.brains {
        if !brains.iter().any(|b| b == brain) {
            brains.push(brain.clone());
        }
    }
    if !brains.is_empty() && !brains.iter().any(|b| b == brain_id) {
        return Ok(SearchResponse {
            results: Vec::new(),
            candidates: 0,
            latency_ms: 0,
        });
    }

    let match_expr = build_match(&analyzed);
    if match_expr.is_empty() {
        return Ok(SearchResponse {
            results: Vec::new(),
            candidates: 0,
            latency_ms: 0,
        });
    }

    let started = Instant::now();
    let (where_sql, mut params) = build_where(&match_expr, &filters)?;

    let count_sql = format!("SELECT count(*) AS n FROM {FROM_CLAUSE} {where_sql}");
    let candidates = storage
        .query(&count_sql, &params)?
        .first()
        .and_then(|row| row.get("n"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;

    let limit = request.top_k.max(1) as u64;
    let search_sql = format!(
        "SELECT {SELECT_COLUMNS} FROM {FROM_CLAUSE} {where_sql} \
         ORDER BY rank ASC LIMIT ?"
    );
    params.push(Value::from(limit));
    let rows = storage.query(&search_sql, &params)?;

    let results = rows.iter().map(row_to_hit).collect();

    Ok(SearchResponse {
        results,
        candidates,
        latency_ms: started.elapsed().as_millis() as i64,
    })
}

// --- Query construction ----------------------------------------------------

/// The shared `FROM` clause: the FTS index joined to its content `chunks` and
/// their `documents`.
const FROM_CLAUSE: &str = "chunks_fts \
     JOIN chunks c ON c.rowid = chunks_fts.rowid \
     JOIN documents d ON d.id = c.document_id";

/// The result columns, including the highlighted snippet and BM25 rank.
const SELECT_COLUMNS: &str = "c.id AS chunk_id, c.document_id AS document_id, \
     d.path AS path, d.title AS title, c.heading_path AS heading_path, \
     c.content AS content, \
     snippet(chunks_fts, 0, '<mark>', '</mark>', '…', 12) AS snippet, \
     bm25(chunks_fts) AS rank";

/// Build the FTS5 `MATCH` expression from the analyzed query. Returns an empty
/// string when there is nothing positive to match.
fn build_match(analyzed: &AnalyzedQuery) -> String {
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

/// Build the `WHERE` clause (filters applied before ranking) and its params.
fn build_where(
    match_expr: &str,
    filters: &SearchFilters,
) -> Result<(String, Vec<Value>), IndexError> {
    let mut sql = String::from("WHERE chunks_fts MATCH ? AND d.deleted_at IS NULL");
    let mut params = vec![Value::String(match_expr.to_string())];

    for tag in &filters.tags {
        sql.push_str(
            " AND EXISTS (SELECT 1 FROM document_tags dt JOIN tags t ON t.id = dt.tag_id \
             WHERE dt.document_id = d.id AND t.name = ?)",
        );
        params.push(Value::String(tag.clone()));
    }
    for tag in &filters.exclude_tags {
        sql.push_str(
            " AND NOT EXISTS (SELECT 1 FROM document_tags dt JOIN tags t ON t.id = dt.tag_id \
             WHERE dt.document_id = d.id AND t.name = ?)",
        );
        params.push(Value::String(tag.clone()));
    }
    if let Some(path) = &filters.path {
        sql.push_str(" AND d.path LIKE ? ESCAPE '\\'");
        params.push(Value::String(path_prefix(path)));
    }
    if let Some(path) = &filters.exclude_path {
        sql.push_str(" AND d.path NOT LIKE ? ESCAPE '\\'");
        params.push(Value::String(path_prefix(path)));
    }
    if let Some(kind) = &filters.doc_type {
        sql.push_str(" AND d.kind = ?");
        params.push(Value::String(kind.clone()));
    }
    if let Some(language) = &filters.language {
        sql.push_str(" AND c.language = ?");
        params.push(Value::String(language.clone()));
    }
    if let Some(after) = &filters.after {
        sql.push_str(" AND date(d.updated_at / 1000, 'unixepoch') >= ?");
        params.push(Value::String(normalize_date(after, DateBound::Start)?));
    }
    if let Some(before) = &filters.before {
        sql.push_str(" AND date(d.updated_at / 1000, 'unixepoch') <= ?");
        params.push(Value::String(normalize_date(before, DateBound::End)?));
    }
    if let Some(has) = &filters.has {
        sql.push_str(&has_clause(has)?);
    }
    for (key, value) in &filters.frontmatter {
        sql.push_str(" AND json_extract(d.frontmatter, ?) = ?");
        params.push(Value::String(format!("$.\"{}\"", key.replace('"', "\\\""))));
        params.push(Value::String(value.clone()));
    }

    Ok((sql, params))
}

/// The `has:` predicate (`SEARCH.md` §10.2).
fn has_clause(has: &str) -> Result<String, IndexError> {
    match has.to_ascii_lowercase().as_str() {
        "link" => {
            Ok(" AND EXISTS (SELECT 1 FROM links l WHERE l.src_document_id = d.id)".to_string())
        }
        "backlink" => {
            Ok(" AND EXISTS (SELECT 1 FROM links l WHERE l.dst_document_id = d.id)".to_string())
        }
        "attachment" => Ok(
            " AND EXISTS (SELECT 1 FROM links l JOIN documents a ON a.id = l.dst_document_id \
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

// --- Row decoding ----------------------------------------------------------

fn row_to_hit(row: &fergents_storage::Row) -> SearchHit {
    let rank = row.get("rank").and_then(Value::as_f64).unwrap_or(0.0);
    let score = -rank;
    SearchHit {
        chunk_id: string_field(row, "chunk_id"),
        document_id: string_field(row, "document_id"),
        path: string_field(row, "path"),
        title: string_field(row, "title"),
        heading_path: row
            .get("heading_path")
            .and_then(Value::as_str)
            .map(str::to_string),
        content: string_field(row, "content"),
        snippet: string_field(row, "snippet"),
        score,
        bm25: score,
    }
}

fn string_field(row: &fergents_storage::Row, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}
