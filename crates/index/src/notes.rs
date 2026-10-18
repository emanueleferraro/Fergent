//! Links, tags, `notes_meta`, and the graph queries over them (`GRAPH.md` §7,
//! `BRAIN.md` §4.2/§4.4, spec Appendix B).
//!
//! Persistence is called by the pipeline while indexing a note; the query
//! functions back `doc.backlinks`, `links.broken`, `doc.outline`,
//! `doc.unlinked_mentions`, `links.orphans`, `links.dead_ends`, and `tag.*`.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use fergents_markdown::{Frontmatter, Link as MdLink, LinkKind as MdLinkKind};
use fergents_storage::{Row, Storage, Value};

use crate::error::IndexError;

/// `links.kind` values (spec Appendix B).
pub mod link_kinds {
    /// `[[note]]`.
    pub const WIKI: &str = "wiki";
    /// `[text](target)`.
    pub const MD: &str = "md";
    /// `![[note]]`.
    pub const EMBED: &str = "embed";
    /// `[[brain:note]]`.
    pub const CROSS_BRAIN: &str = "cross_brain";
    /// `[[id:…]]`.
    pub const BY_ID: &str = "by_id";
}

/// A row of the `links` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkRecord {
    /// The link id.
    pub id: String,
    /// The source document.
    pub src_document_id: String,
    /// The resolved target document, if any.
    pub dst_document_id: Option<String>,
    /// The unresolved target path/URL.
    pub dst_path: Option<String>,
    /// The heading anchor.
    pub dst_heading: Option<String>,
    /// The block id.
    pub dst_block: Option<String>,
    /// The display text.
    pub display_text: Option<String>,
    /// The surrounding paragraph.
    pub anchor: Option<String>,
    /// One of [`link_kinds`].
    pub kind: String,
    /// True when the target does not exist.
    pub unresolved: bool,
}

/// A lightweight reference to a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteRef {
    /// The document id.
    pub document_id: String,
    /// The vault-relative path.
    pub path: String,
    /// The note title.
    pub title: String,
}

/// A heading in a note's outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineEntry {
    /// Heading level (1-6).
    pub level: u8,
    /// Heading text.
    pub text: String,
}

/// A place where a note's title appears as plain text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnlinkedMention {
    /// The document containing the mention.
    pub document_id: String,
    /// Its vault-relative path.
    pub path: String,
    /// Its title.
    pub title: String,
}

/// A row of the `tags` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    /// The tag id.
    pub id: String,
    /// The tag name (e.g. `nested/tag`).
    pub name: String,
    /// The parent tag name, if any.
    pub parent: Option<String>,
}

// --- Persistence -----------------------------------------------------------

/// Remove a document's links, tags, and note metadata (before re-persisting, or
/// on soft delete).
pub fn clear_note_data(storage: &Arc<dyn Storage>, document_id: &str) -> Result<(), IndexError> {
    let params = [Value::String(document_id.to_string())];
    storage.execute("DELETE FROM links WHERE src_document_id = ?1", &params)?;
    storage.execute("DELETE FROM document_tags WHERE document_id = ?1", &params)?;
    storage.execute("DELETE FROM notes_meta WHERE document_id = ?1", &params)?;
    Ok(())
}

/// Persist a note's outgoing links, resolving internal targets against
/// `documents`. Returns the number of links written.
pub fn persist_links(
    storage: &Arc<dyn Storage>,
    document_id: &str,
    links: &[MdLink],
    body: &str,
) -> Result<usize, IndexError> {
    let mut written = 0;
    for link in links {
        let (kind, target) = classify_link(link);
        let (dst_document_id, dst_path, unresolved) = match kind {
            link_kinds::WIKI | link_kinds::EMBED => match resolve_target(storage, target)? {
                Some((id, _path)) => (Some(id), None, false),
                None => (None, Some(target.to_string()), true),
            },
            link_kinds::MD => {
                if is_external(target) {
                    (None, Some(target.to_string()), false)
                } else {
                    match resolve_target(storage, target)? {
                        Some((id, _)) => (Some(id), None, false),
                        None => (None, Some(target.to_string()), true),
                    }
                }
            }
            _ => (None, Some(target.to_string()), true),
        };

        let anchor = paragraph_for_link(body, link);
        storage.execute(
            "INSERT INTO links \
             (id, src_document_id, dst_document_id, dst_path, dst_heading, dst_block, \
              display_text, anchor, kind, unresolved) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            &[
                Value::String(ulid::Ulid::new().to_string()),
                Value::String(document_id.to_string()),
                opt_string(dst_document_id),
                opt_string(dst_path),
                opt_string(link.heading.clone()),
                opt_string(link.block_id.clone()),
                opt_string(link.display.clone()),
                anchor.map_or(Value::Null, Value::String),
                Value::String(kind.to_string()),
                Value::from(u64::from(unresolved)),
            ],
        )?;
        written += 1;
    }
    Ok(written)
}

/// Tag names declared in frontmatter (`tags: [a, b]`).
pub fn frontmatter_tags(frontmatter: Option<&Frontmatter>) -> Vec<String> {
    frontmatter
        .and_then(|f| f.parsed.as_ref())
        .and_then(|v| v.get("tags"))
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Persist a note's tags, creating `tags` rows as needed.
pub fn persist_tags(
    storage: &Arc<dyn Storage>,
    document_id: &str,
    tags: &[String],
) -> Result<usize, IndexError> {
    let mut seen = HashSet::new();
    let mut written = 0;
    for tag in tags {
        let name = tag.trim().trim_start_matches('#');
        if name.is_empty() || !seen.insert(name.to_string()) {
            continue;
        }
        let tag_id = ensure_tag(storage, name)?;
        storage.execute(
            "INSERT OR IGNORE INTO document_tags (document_id, tag_id) VALUES (?1, ?2)",
            &[
                Value::String(document_id.to_string()),
                Value::String(tag_id),
            ],
        )?;
        written += 1;
    }
    Ok(written)
}

/// Persist `notes_meta` (aliases, cssclasses, publish) from frontmatter.
pub fn upsert_notes_meta(
    storage: &Arc<dyn Storage>,
    document_id: &str,
    frontmatter: Option<&Frontmatter>,
) -> Result<(), IndexError> {
    let parsed = frontmatter.and_then(|f| f.parsed.as_ref());
    let aliases = yaml_array(parsed, "aliases");
    let cssclasses = yaml_array(parsed, "cssclasses");
    let publish = parsed
        .and_then(|v| v.get("publish"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    storage.execute(
        "INSERT INTO notes_meta (document_id, aliases, cssclasses, publish) \
         VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(document_id) DO UPDATE SET aliases = excluded.aliases, \
         cssclasses = excluded.cssclasses, publish = excluded.publish",
        &[
            Value::String(document_id.to_string()),
            Value::String(aliases),
            Value::String(cssclasses),
            Value::from(u64::from(publish)),
        ],
    )?;
    Ok(())
}

// --- Queries ---------------------------------------------------------------

/// Incoming links to `document_id`, including unresolved targets
/// (`GRAPH.md` §7.1).
pub fn backlinks(
    storage: &Arc<dyn Storage>,
    document_id: &str,
) -> Result<Vec<LinkRecord>, IndexError> {
    let Some(doc) = document_by_id(storage, document_id)? else {
        return Ok(Vec::new());
    };
    let candidates = target_candidates(&doc.path, &doc.title);
    let rows = storage.query(
        "SELECT * FROM links WHERE dst_document_id = ?1 OR LOWER(dst_path) IN (?2, ?3, ?4, ?5)",
        &[
            Value::String(document_id.to_string()),
            Value::String(candidates[0].clone()),
            Value::String(candidates[1].clone()),
            Value::String(candidates[2].clone()),
            Value::String(candidates[3].clone()),
        ],
    )?;
    Ok(rows.iter().map(row_to_link).collect())
}

/// All links whose target is unresolved (spec `links.unresolved`).
pub fn broken_links(storage: &Arc<dyn Storage>) -> Result<Vec<LinkRecord>, IndexError> {
    let rows = storage.query("SELECT * FROM links WHERE unresolved = 1", &[])?;
    Ok(rows.iter().map(row_to_link).collect())
}

/// Notes with no incoming links (`GRAPH.md` §7.4).
pub fn orphans(storage: &Arc<dyn Storage>) -> Result<Vec<NoteRef>, IndexError> {
    let notes = live_notes(storage)?;
    let links = all_links(storage)?;
    let mut linked: HashSet<String> = HashSet::new();
    for link in &links {
        if let Some(dst) = &link.dst_document_id {
            linked.insert(dst.clone());
        }
        if let Some(dst_path) = &link.dst_path {
            for note in &notes {
                if matches_target(&note.path, &note.title, dst_path) {
                    linked.insert(note.document_id.clone());
                }
            }
        }
    }
    Ok(notes
        .into_iter()
        .filter(|note| !linked.contains(&note.document_id))
        .collect())
}

/// Notes with no outgoing links (`GRAPH.md` §7.4).
pub fn dead_ends(storage: &Arc<dyn Storage>) -> Result<Vec<NoteRef>, IndexError> {
    let notes = live_notes(storage)?;
    let sources: HashSet<String> = all_links(storage)?
        .into_iter()
        .map(|link| link.src_document_id)
        .collect();
    Ok(notes
        .into_iter()
        .filter(|note| !sources.contains(&note.document_id))
        .collect())
}

/// Notes whose title appears as plain text but that do not link to
/// `document_id` (`GRAPH.md` §7.3).
pub fn unlinked_mentions(
    storage: &Arc<dyn Storage>,
    document_id: &str,
) -> Result<Vec<UnlinkedMention>, IndexError> {
    let Some(target) = document_by_id(storage, document_id)? else {
        return Ok(Vec::new());
    };
    let title = target.title.trim();
    if title.len() < 3 {
        return Ok(Vec::new());
    }

    let linked_to: HashSet<String> = backlinks(storage, document_id)?
        .into_iter()
        .map(|link| link.src_document_id)
        .collect();
    let needle = title.to_lowercase();

    let mut mentions = Vec::new();
    for note in live_notes(storage)? {
        if note.document_id == document_id || linked_to.contains(&note.document_id) {
            continue;
        }
        let rows = storage.query(
            "SELECT content FROM chunks WHERE document_id = ?1",
            &[Value::String(note.document_id.clone())],
        )?;
        let contains = rows.iter().any(|row| {
            row.get("content")
                .and_then(Value::as_str)
                .is_some_and(|content| content.to_lowercase().contains(&needle))
        });
        if contains {
            mentions.push(UnlinkedMention {
                document_id: note.document_id,
                path: note.path,
                title: note.title,
            });
        }
    }
    Ok(mentions)
}

/// The heading outline of a note, in document order.
pub fn outline(
    storage: &Arc<dyn Storage>,
    vault_root: &Path,
    document_id: &str,
) -> Result<Vec<OutlineEntry>, IndexError> {
    let Some(doc) = document_by_id(storage, document_id)? else {
        return Ok(Vec::new());
    };
    let path = vault_root.join(&doc.path);
    let body = std::fs::read_to_string(&path).map_err(|e| IndexError::Io(e.to_string()))?;
    Ok(headings(&body))
}

/// All tags, ordered by name.
pub fn list_tags(storage: &Arc<dyn Storage>) -> Result<Vec<Tag>, IndexError> {
    let rows = storage.query("SELECT id, name, parent FROM tags ORDER BY name ASC", &[])?;
    Ok(rows.iter().map(row_to_tag).collect())
}

/// Rename a tag (`tag.rename`).
pub fn rename_tag(
    storage: &Arc<dyn Storage>,
    tag_id: &str,
    new_name: &str,
) -> Result<Tag, IndexError> {
    let name = new_name.trim().trim_start_matches('#');
    let parent = parent_of(name);
    storage.execute(
        "UPDATE tags SET name = ?1, parent = ?2 WHERE id = ?3",
        &[
            Value::String(name.to_string()),
            parent.map_or(Value::Null, Value::String),
            Value::String(tag_id.to_string()),
        ],
    )?;
    let rows = storage.query(
        "SELECT id, name, parent FROM tags WHERE id = ?1",
        &[Value::String(tag_id.to_string())],
    )?;
    rows.first()
        .map(row_to_tag)
        .ok_or_else(|| IndexError::Io(format!("tag '{tag_id}' not found after rename")))
}

/// Delete a tag and its document associations (`tag.delete`).
pub fn delete_tag(storage: &Arc<dyn Storage>, tag_id: &str) -> Result<(), IndexError> {
    let params = [Value::String(tag_id.to_string())];
    storage.execute("DELETE FROM document_tags WHERE tag_id = ?1", &params)?;
    storage.execute("DELETE FROM tags WHERE id = ?1", &params)?;
    Ok(())
}

// --- Helpers ---------------------------------------------------------------

/// Classify a parsed link into `(kind, target)`.
pub fn classify_link(link: &MdLink) -> (&'static str, &str) {
    let target = link.target.trim();
    match link.kind {
        MdLinkKind::Embed => (link_kinds::EMBED, target),
        MdLinkKind::Markdown => (link_kinds::MD, target),
        MdLinkKind::Wiki => {
            if target.starts_with("brain:") {
                (link_kinds::CROSS_BRAIN, target)
            } else if target.starts_with("id:") {
                (link_kinds::BY_ID, target)
            } else {
                (link_kinds::WIKI, target)
            }
        }
    }
}

/// Resolve an internal target to `(document_id, path)` using path/stem/title
/// matching, preferring an exact path match.
pub fn resolve_target(
    storage: &Arc<dyn Storage>,
    target: &str,
) -> Result<Option<(String, String)>, IndexError> {
    let target = target.trim();
    if target.is_empty() {
        return Ok(None);
    }
    let rows = storage.query(
        "SELECT id, path, title FROM documents WHERE deleted_at IS NULL",
        &[],
    )?;

    let target_no_ext = target.strip_suffix(".md").unwrap_or(target);
    let mut by_title: Option<(String, String)> = None;
    for row in &rows {
        let path = row.get("path").and_then(Value::as_str).unwrap_or_default();
        let title = row.get("title").and_then(Value::as_str).unwrap_or_default();
        let id = row.get("id").and_then(Value::as_str).unwrap_or_default();
        let stem = Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let path_no_ext = path.strip_suffix(".md").unwrap_or(path);

        if path.eq_ignore_ascii_case(target)
            || path.eq_ignore_ascii_case(target_no_ext)
            || path_no_ext.eq_ignore_ascii_case(target_no_ext)
            || stem.eq_ignore_ascii_case(target_no_ext)
        {
            return Ok(Some((id.to_string(), path.to_string())));
        }
        if by_title.is_none() && title.eq_ignore_ascii_case(target) {
            by_title = Some((id.to_string(), path.to_string()));
        }
    }
    Ok(by_title)
}

fn is_external(target: &str) -> bool {
    let lower = target.to_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("tel:")
        || target.starts_with('#')
}

fn paragraph_for_link(body: &str, link: &MdLink) -> Option<String> {
    let needle = match link.kind {
        MdLinkKind::Embed => format!("![[{}", link.target),
        MdLinkKind::Wiki => format!("[[{}", link.target),
        MdLinkKind::Markdown => format!("]({})", link.target),
    };
    for paragraph in body.split("\n\n") {
        if paragraph.contains(&needle) {
            let trimmed = paragraph.trim();
            if !trimmed.is_empty() {
                return Some(truncate(trimmed, 400));
            }
        }
    }
    None
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

fn ensure_tag(storage: &Arc<dyn Storage>, name: &str) -> Result<String, IndexError> {
    let rows = storage.query(
        "SELECT id FROM tags WHERE name = ?1",
        &[Value::String(name.to_string())],
    )?;
    if let Some(id) = rows
        .first()
        .and_then(|r| r.get("id"))
        .and_then(Value::as_str)
    {
        return Ok(id.to_string());
    }
    let id = ulid::Ulid::new().to_string();
    storage.execute(
        "INSERT OR IGNORE INTO tags (id, name, parent) VALUES (?1, ?2, ?3)",
        &[
            Value::String(id.clone()),
            Value::String(name.to_string()),
            parent_of(name).map_or(Value::Null, Value::String),
        ],
    )?;
    Ok(id)
}

fn parent_of(name: &str) -> Option<String> {
    name.rsplit_once('/').map(|(parent, _)| parent.to_string())
}

fn yaml_array(value: Option<&serde_yaml::Value>, key: &str) -> String {
    let items: Vec<String> = value
        .and_then(|v| v.get(key))
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string())
}

struct DocumentRow {
    path: String,
    title: String,
}

fn document_by_id(
    storage: &Arc<dyn Storage>,
    document_id: &str,
) -> Result<Option<DocumentRow>, IndexError> {
    let rows = storage.query(
        "SELECT path, title FROM documents WHERE id = ?1",
        &[Value::String(document_id.to_string())],
    )?;
    Ok(rows.first().map(|row| DocumentRow {
        path: string_field(row, "path"),
        title: string_field(row, "title"),
    }))
}

fn live_notes(storage: &Arc<dyn Storage>) -> Result<Vec<NoteRef>, IndexError> {
    let rows = storage.query(
        "SELECT id, path, title FROM documents WHERE kind = 'note' AND deleted_at IS NULL \
         ORDER BY path ASC",
        &[],
    )?;
    Ok(rows
        .iter()
        .map(|row| NoteRef {
            document_id: string_field(row, "id"),
            path: string_field(row, "path"),
            title: string_field(row, "title"),
        })
        .collect())
}

fn all_links(storage: &Arc<dyn Storage>) -> Result<Vec<LinkRecord>, IndexError> {
    let rows = storage.query("SELECT * FROM links", &[])?;
    Ok(rows.iter().map(row_to_link).collect())
}

fn target_candidates(path: &str, title: &str) -> [String; 4] {
    let stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let no_ext = path.strip_suffix(".md").unwrap_or(path);
    [
        path.to_lowercase(),
        no_ext.to_lowercase(),
        stem.to_lowercase(),
        title.to_lowercase(),
    ]
}

fn matches_target(path: &str, title: &str, dst_path: &str) -> bool {
    let candidates = target_candidates(path, title);
    candidates
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(dst_path.trim()))
}

fn row_to_link(row: &Row) -> LinkRecord {
    LinkRecord {
        id: string_field(row, "id"),
        src_document_id: string_field(row, "src_document_id"),
        dst_document_id: row
            .get("dst_document_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        dst_path: row
            .get("dst_path")
            .and_then(Value::as_str)
            .map(str::to_string),
        dst_heading: row
            .get("dst_heading")
            .and_then(Value::as_str)
            .map(str::to_string),
        dst_block: row
            .get("dst_block")
            .and_then(Value::as_str)
            .map(str::to_string),
        display_text: row
            .get("display_text")
            .and_then(Value::as_str)
            .map(str::to_string),
        anchor: row
            .get("anchor")
            .and_then(Value::as_str)
            .map(str::to_string),
        kind: string_field(row, "kind"),
        unresolved: row.get("unresolved").and_then(Value::as_i64).unwrap_or(0) != 0,
    }
}

fn row_to_tag(row: &Row) -> Tag {
    Tag {
        id: string_field(row, "id"),
        name: string_field(row, "name"),
        parent: row
            .get("parent")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

fn string_field(row: &Row, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn opt_string(value: Option<String>) -> Value {
    value.map_or(Value::Null, Value::String)
}

/// Extract ATX headings (level + text) in document order, ignoring code fences.
fn headings(body: &str) -> Vec<OutlineEntry> {
    let mut out = Vec::new();
    let mut fence: Option<char> = None;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(marker) = fence_marker_char(trimmed) {
            match fence {
                Some(open) if open == marker => fence = None,
                None => fence = Some(marker),
                _ => {}
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        let leading = line.len() - trimmed.len();
        if leading > 3 {
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
                out.push(OutlineEntry {
                    level: hashes as u8,
                    text: text.to_string(),
                });
            }
        }
    }
    out
}

fn fence_marker_char(trimmed: &str) -> Option<char> {
    let mut chars = trimmed.chars();
    let first = chars.next()?;
    if first != '`' && first != '~' {
        return None;
    }
    if trimmed.chars().take_while(|c| *c == first).count() >= 3 {
        Some(first)
    } else {
        None
    }
}
