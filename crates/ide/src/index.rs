//! File indexing, fuzzy matching, and bounded text search.

use std::io::Read;
use std::path::Path;

use ignore::WalkBuilder;

use crate::model::SearchHit;
use crate::path::relative;

/// Files larger than this are not indexed for content search.
const MAX_INDEXED_FILE_BYTES: u64 = 5 * 1024 * 1024;

/// How much of a file is inspected to decide whether it is binary.
const BINARY_PROBE_BYTES: usize = 8 * 1024;

/// Build the ignore-aware file index (project-relative, forward slashes).
pub fn build_index(root: &Path, limit: usize) -> Vec<String> {
    let mut files = Vec::new();
    let walker = WalkBuilder::new(root)
        .hidden(true)
        .follow_links(false)
        .git_ignore(true)
        .require_git(false)
        .git_global(false)
        .git_exclude(true)
        .add_custom_ignore_filename(".fergentsignore")
        .max_filesize(Some(MAX_INDEXED_FILE_BYTES))
        .build();

    for entry in walker.flatten() {
        if files.len() >= limit {
            break;
        }
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let rel = relative(root, entry.path());
        if rel.is_empty() || rel.starts_with(".fergents/") {
            continue;
        }
        files.push(rel);
    }
    files.sort();
    files
}

/// Rank indexed paths against a fuzzy query (subsequence match).
pub fn fuzzy(query: &str, files: &[String], limit: usize) -> Vec<String> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return files.iter().take(limit).cloned().collect();
    }

    let mut scored: Vec<(i64, &String)> = files
        .iter()
        .filter_map(|path| fuzzy_score(&needle, path).map(|score| (score, path)))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.len().cmp(&b.1.len())));
    scored
        .into_iter()
        .take(limit)
        .map(|(_, path)| path.clone())
        .collect()
}

fn fuzzy_score(needle: &str, path: &str) -> Option<i64> {
    let haystack = path.to_lowercase();
    let basename = haystack.rsplit('/').next().unwrap_or(&haystack);
    let bonus = if haystack.starts_with(needle) {
        1000
    } else if basename.starts_with(needle) {
        800
    } else if basename.contains(needle) {
        500
    } else {
        0
    };

    let mut hay_iter = haystack.chars().enumerate();
    let mut last_index = 0usize;
    let mut gaps = 0i64;
    for wanted in needle.chars() {
        let mut found = false;
        for (index, candidate) in hay_iter.by_ref() {
            if candidate == wanted {
                gaps += index.saturating_sub(last_index) as i64;
                last_index = index;
                found = true;
                break;
            }
        }
        if !found {
            return None;
        }
    }
    Some(bonus + 100 - gaps.min(100))
}

/// Search file contents for a case-insensitive substring, bounded by `limit`
/// hits and `max_files` scanned files.
pub fn search(root: &Path, query: &str, limit: usize, max_files: usize) -> Vec<SearchHit> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() || limit == 0 {
        return Vec::new();
    }

    let files = build_index(root, max_files);
    let mut hits = Vec::new();
    for rel in files {
        if hits.len() >= limit {
            break;
        }
        let path = root.join(&rel);
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        if metadata.len() > MAX_INDEXED_FILE_BYTES {
            continue;
        }
        let Ok(mut file) = std::fs::File::open(&path) else {
            continue;
        };
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        if file.read_to_end(&mut bytes).is_err() {
            continue;
        }
        let probe = &bytes[..bytes.len().min(BINARY_PROBE_BYTES)];
        if probe.contains(&0) {
            continue;
        }
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        for (index, line) in text.lines().enumerate() {
            if hits.len() >= limit {
                break;
            }
            if line.to_lowercase().contains(&needle) {
                let snippet: String = line.trim().chars().take(200).collect();
                hits.push(SearchHit {
                    path: rel.clone(),
                    line: (index + 1) as u32,
                    text: snippet,
                });
            }
        }
    }
    hits
}

/// Whether a path looks like a git repository root.
pub fn has_git_dir(path: &Path) -> bool {
    path.join(".git").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_prefers_prefix_matches() {
        let files = vec![
            "src/main.rs".to_string(),
            "main.rs".to_string(),
            "docs/domain.md".to_string(),
        ];
        let ranked = fuzzy("main", &files, 2);
        assert_eq!(
            ranked,
            vec!["main.rs".to_string(), "src/main.rs".to_string()]
        );
    }

    #[test]
    fn fuzzy_subsequence_matches() {
        let files = vec!["src/features/chat/useChat.ts".to_string()];
        assert_eq!(fuzzy("uc", &files, 5), files);
        assert!(fuzzy("zzz", &files, 5).is_empty());
    }
}
