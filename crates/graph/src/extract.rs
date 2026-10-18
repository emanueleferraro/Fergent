//! Entity extraction (`GRAPH.md` §3).
//!
//! The [`EntityExtractor`] trait is implemented by the always-on
//! [`RegexExtractor`] (URLs, emails, dates, file paths, markdown links, code
//! identifiers) and by the feature-gated NER extractor (`ner.rs`).

use std::sync::OnceLock;

use regex::Regex;
use sha2::{Digest, Sha256};

use crate::error::GraphError;
use crate::model::EntityType;

/// One extracted entity occurrence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedEntity {
    /// The entity type (`url`, `person`, ...).
    pub entity_type: String,
    /// The surface form.
    pub name: String,
    /// The normalized name (used for identity).
    pub normalized_name: String,
    /// Start offset within the source text.
    pub start_offset: i64,
    /// End offset within the source text.
    pub end_offset: i64,
}

/// An entity extractor.
pub trait EntityExtractor: Send + Sync {
    /// A stable identifier (also used as the entity metadata `extractor`).
    fn name(&self) -> &'static str;

    /// Extract entities from `text`.
    fn extract(&self, text: &str) -> Result<Vec<ExtractedEntity>, GraphError>;
}

/// Normalize a surface form for identity: lowercase, trimmed, single-spaced.
pub fn normalize_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// A stable entity id derived from its type and normalized name, so the same
/// entity keeps its id across reindexes.
pub fn entity_id(entity_type: &str, normalized_name: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(entity_type.as_bytes());
    hasher.update([0u8]);
    hasher.update(normalized_name.as_bytes());
    let digest = hex::encode(hasher.finalize());
    format!("{entity_type}:{}", &digest[..16])
}

/// A regex-based extractor, always on (`GRAPH.md` §3.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct RegexExtractor;

impl RegexExtractor {
    /// Create the extractor.
    pub fn new() -> Self {
        Self
    }
}

fn url_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"https?://[^\s<>")\]]+"#).unwrap())
}

fn email_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}").unwrap())
}

fn date_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b\d{4}-\d{2}-\d{2}\b").unwrap())
}

fn path_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?:\.{1,2}/|~/|/)?[A-Za-z0-9_.\-]+(?:/[A-Za-z0-9_.\-]+)*\.(?:md|rs|ts|tsx|js|jsx|py|go|toml|json|ya?ml|sql|txt|csv|png|jpe?g|gif|svg|pdf|html|css)\b",
        )
        .unwrap()
    })
}

fn md_link_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[[^\]]*\]\(([^)\s]+)\)").unwrap())
}

fn code_backtick_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"`([A-Za-z_][A-Za-z0-9_]*)`").unwrap())
}

fn code_snake_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b[a-z][a-z0-9]*(?:_[a-z0-9]+)+\b").unwrap())
}

/// Extraction priority: earlier wins on overlap.
const ORDER: &[&str] = &["url", "file", "date", "email", "code_symbol"];

impl EntityExtractor for RegexExtractor {
    fn name(&self) -> &'static str {
        "regex"
    }

    fn extract(&self, text: &str) -> Result<Vec<ExtractedEntity>, GraphError> {
        let mut found: Vec<(i64, i64, &'static str, String)> = Vec::new();

        for m in url_re().find_iter(text) {
            found.push((
                m.start() as i64,
                m.end() as i64,
                "url",
                m.as_str().to_string(),
            ));
        }
        for caps in md_link_re().captures_iter(text) {
            if let Some(target) = caps.get(1) {
                let kind = if target.as_str().starts_with("http") {
                    EntityType::Url
                } else {
                    EntityType::File
                };
                // The markdown link target, excluding the surrounding syntax.
                found.push((
                    target.start() as i64,
                    target.end() as i64,
                    if kind == EntityType::Url {
                        "url"
                    } else {
                        "file"
                    },
                    target.as_str().to_string(),
                ));
            }
        }
        for m in email_re().find_iter(text) {
            found.push((
                m.start() as i64,
                m.end() as i64,
                "email",
                m.as_str().to_string(),
            ));
        }
        for m in date_re().find_iter(text) {
            found.push((
                m.start() as i64,
                m.end() as i64,
                "date",
                m.as_str().to_string(),
            ));
        }
        for m in path_re().find_iter(text) {
            found.push((
                m.start() as i64,
                m.end() as i64,
                "file",
                m.as_str().to_string(),
            ));
        }
        for caps in code_backtick_re().captures_iter(text) {
            if let Some(ident) = caps.get(1) {
                found.push((
                    ident.start() as i64,
                    ident.end() as i64,
                    "code_symbol",
                    ident.as_str().to_string(),
                ));
            }
        }
        for m in code_snake_re().find_iter(text) {
            found.push((
                m.start() as i64,
                m.end() as i64,
                "code_symbol",
                m.as_str().to_string(),
            ));
        }

        found.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| priority(a.2).cmp(&priority(b.2)))
                .then_with(|| (b.1 - b.0).cmp(&(a.1 - a.0)))
        });

        let mut entities: Vec<ExtractedEntity> = Vec::new();
        let mut last_end = i64::MIN;
        for (start, end, raw_type, name) in found {
            if start < last_end {
                continue; // overlaps a higher-priority match
            }
            last_end = end;
            let entity_type = classify(raw_type);
            let normalized_name = normalize_name(&name);
            if normalized_name.is_empty() {
                continue;
            }
            entities.push(ExtractedEntity {
                entity_type,
                name,
                normalized_name,
                start_offset: start,
                end_offset: end,
            });
        }
        Ok(entities)
    }
}

fn priority(kind: &str) -> usize {
    ORDER.iter().position(|k| *k == kind).unwrap_or(ORDER.len())
}

fn classify(raw_type: &str) -> String {
    match raw_type {
        "date" => EntityType::Date.as_str().to_string(),
        "file" => EntityType::File.as_str().to_string(),
        "url" => EntityType::Url.as_str().to_string(),
        "code_symbol" => EntityType::CodeSymbol.as_str().to_string(),
        // Emails have no built-in type; they are user-extensible `custom`.
        _ => EntityType::Custom.as_str().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types(entities: &[ExtractedEntity]) -> Vec<(String, String)> {
        entities
            .iter()
            .map(|e| (e.entity_type.clone(), e.normalized_name.clone()))
            .collect()
    }

    #[test]
    fn extracts_urls_emails_dates_paths_and_code() {
        let text = "See https://example.com/a?b=1 or mail me at alice@example.org on 2025-10-01. \
                    Edit `parse_document` in src/index/notes.rs and call my_helper_fn.";
        let entities = RegexExtractor::new().extract(text).unwrap();
        let got = types(&entities);
        assert!(
            got.contains(&("url".into(), "https://example.com/a?b=1".into())),
            "{got:?}"
        );
        assert!(
            got.contains(&("custom".into(), "alice@example.org".into())),
            "{got:?}"
        );
        assert!(
            got.contains(&("date".into(), "2025-10-01".into())),
            "{got:?}"
        );
        assert!(
            got.contains(&("code_symbol".into(), "parse_document".into())),
            "{got:?}"
        );
        assert!(
            got.contains(&("code_symbol".into(), "my_helper_fn".into())),
            "{got:?}"
        );
    }

    #[test]
    fn extracts_markdown_link_targets() {
        let entities = RegexExtractor::new()
            .extract("Read [the docs](https://docs.example.com/x) now.")
            .unwrap();
        assert!(types(&entities).contains(&("url".into(), "https://docs.example.com/x".into())));
    }

    #[test]
    fn entity_ids_are_stable_and_type_scoped() {
        let a = entity_id("person", "alice");
        assert_eq!(a, entity_id("person", "alice"));
        assert_ne!(a, entity_id("organization", "alice"));
    }
}
