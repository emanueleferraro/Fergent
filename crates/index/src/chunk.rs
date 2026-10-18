//! Content chunkers and the chunk model (`SEARCH.md` §3).
//!
//! Chunkers are pure: each takes a `&str` and a [`ChunkConfig`], and returns the
//! [`Chunk`] list. Offsets are byte ranges into the input string, so for a note
//! the caller passes the note body (frontmatter already stripped) and gets
//! offsets relative to that body.
//!
//! Token counting uses `cl100k_base` from `tiktoken-rs` (bundled vocab, no
//! network) as a fast, language-agnostic proxy for chunk sizing.

use std::sync::OnceLock;

use tiktoken_rs::{cl100k_base, CoreBPE};

/// Default target chunk size, in tokens (`SEARCH.md` §3.3).
pub const DEFAULT_CHUNK_SIZE: u32 = 512;
/// Default overlap between adjacent chunks, in tokens.
pub const DEFAULT_CHUNK_OVERLAP: u32 = 64;
/// Default minimum chunk size, in tokens (smaller chunks are merged).
pub const DEFAULT_MIN_CHUNK_SIZE: u32 = 100;
/// Default maximum chunk size, in tokens (larger chunks are force-split).
pub const DEFAULT_MAX_CHUNK_SIZE: u32 = 1024;

/// Chunking parameters, configurable per brain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkConfig {
    /// Target chunk size, in tokens.
    pub chunk_size: u32,
    /// Overlap between consecutive chunks, in tokens.
    pub chunk_overlap: u32,
    /// Chunks below this size are merged with a neighbour.
    pub min_chunk_size: u32,
    /// Chunks above this size are force-split.
    pub max_chunk_size: u32,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            chunk_size: DEFAULT_CHUNK_SIZE,
            chunk_overlap: DEFAULT_CHUNK_OVERLAP,
            min_chunk_size: DEFAULT_MIN_CHUNK_SIZE,
            max_chunk_size: DEFAULT_MAX_CHUNK_SIZE,
        }
    }
}

impl ChunkConfig {
    /// Clamp nonsensical values so the algorithms always terminate.
    fn normalized(self) -> Self {
        let chunk_size = self.chunk_size.max(1);
        Self {
            chunk_size,
            chunk_overlap: self.chunk_overlap.min(chunk_size.saturating_sub(1)),
            min_chunk_size: self.min_chunk_size.min(chunk_size),
            max_chunk_size: self.max_chunk_size.max(chunk_size),
        }
    }
}

/// A single chunk of a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// 0-based position within the document's chunk sequence.
    pub ordinal: usize,
    /// The chunk text.
    pub content: String,
    /// Token count of `content`.
    pub token_count: u32,
    /// Byte offset of `content` start within the input string.
    pub start_offset: usize,
    /// Byte offset of `content` end (exclusive) within the input string.
    pub end_offset: usize,
    /// The heading ancestry, e.g. `"Title > Section > Sub"`.
    pub heading_path: Option<String>,
    /// Source language, for code chunks (deferred to Phase 3).
    pub language: Option<String>,
    /// Source symbol, for code chunks (deferred to Phase 3).
    pub symbol: Option<String>,
}

/// Count tokens in `text` using the `cl100k_base` encoding.
pub fn count_tokens(text: &str) -> u32 {
    bpe().encode_ordinary(text).len() as u32
}

fn bpe() -> &'static CoreBPE {
    static BPE: OnceLock<CoreBPE> = OnceLock::new();
    BPE.get_or_init(|| cl100k_base().expect("cl100k_base vocab is bundled with tiktoken-rs"))
}

/// Chunk a markdown document: headings → paragraphs → sentences, merged to the
/// target size, with overlap between consecutive chunks of the same section.
pub fn chunk_markdown(text: &str, config: &ChunkConfig) -> Vec<Chunk> {
    let cfg = config.normalized();
    let sections = split_sections(text);
    chunk_sections(text, &cfg, &sections)
}

/// Chunk plain text: paragraphs → sentences, merged to the target size.
pub fn chunk_plain_text(text: &str, config: &ChunkConfig) -> Vec<Chunk> {
    let cfg = config.normalized();
    let section = Section {
        heading_path: None,
        paragraphs: paragraph_ranges(text),
    };
    chunk_sections(text, &cfg, &[section])
}

/// Chunk extracted attachment text (PDF/plain). Behaves like plain text.
pub fn chunk_attachment_text(text: &str, config: &ChunkConfig) -> Vec<Chunk> {
    chunk_plain_text(text, config)
}

// --- Section model ---------------------------------------------------------

/// A heading-delimited section: its heading ancestry and its paragraph ranges.
#[derive(Debug)]
struct Section {
    heading_path: Option<String>,
    paragraphs: Vec<(usize, usize)>,
}

/// A contiguous, trimmed span of text with its token count.
#[derive(Debug, Clone, Copy)]
struct Unit {
    start: usize,
    end: usize,
    tokens: u32,
}

/// A block found while scanning markdown lines.
enum Block {
    Heading { level: u8, text: String },
    Paragraph { start: usize, end: usize },
}

/// Scan markdown into heading/paragraph blocks, fence-aware so that `#` and
/// blank lines inside fenced code are not misinterpreted.
fn scan_blocks(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut para_start: Option<usize> = None;
    let mut para_end = 0usize;
    let mut fence: Option<(u8, usize)> = None;
    let mut offset = 0usize;

    for line in text.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let raw = line.trim_end_matches(['\n', '\r']);
        let content_end = line_start + raw.len();

        if let Some((fence_char, fence_len)) = fence {
            if let Some((c, n)) = fence_marker(raw) {
                if c == fence_char && n >= fence_len {
                    fence = None;
                }
            }
            if para_start.is_none() {
                para_start = Some(line_start);
            }
            para_end = content_end;
            continue;
        }

        if let Some((c, n)) = fence_marker(raw) {
            fence = Some((c, n));
            if para_start.is_none() {
                para_start = Some(line_start);
            }
            para_end = content_end;
            continue;
        }

        if raw.trim().is_empty() {
            flush_paragraph(text, &mut blocks, &mut para_start, para_end);
            continue;
        }

        if let Some((level, heading_text)) = parse_atx_heading(raw) {
            flush_paragraph(text, &mut blocks, &mut para_start, para_end);
            blocks.push(Block::Heading {
                level,
                text: heading_text,
            });
            continue;
        }

        if para_start.is_none() {
            para_start = Some(line_start);
        }
        para_end = content_end;
    }

    flush_paragraph(text, &mut blocks, &mut para_start, para_end);
    blocks
}

fn flush_paragraph(text: &str, blocks: &mut Vec<Block>, start: &mut Option<usize>, end: usize) {
    if let Some(s) = start.take() {
        if let Some((s, e)) = trim_range(text, s, end) {
            blocks.push(Block::Paragraph { start: s, end: e });
        }
    }
}

/// Detect a code fence marker (` ``` `/`~~~`, ≥3 chars) after up to 3 spaces.
fn fence_marker(raw: &str) -> Option<(u8, usize)> {
    let stripped = strip_indent(raw, 3);
    let first = *stripped.as_bytes().first()?;
    if first != b'`' && first != b'~' {
        return None;
    }
    let n = stripped.bytes().take_while(|b| *b == first).count();
    if n >= 3 {
        Some((first, n))
    } else {
        None
    }
}

/// Parse an ATX heading (`#`…`######` followed by a space or end of line).
fn parse_atx_heading(raw: &str) -> Option<(u8, String)> {
    let stripped = strip_indent(raw, 3);
    let hashes = stripped.bytes().take_while(|b| *b == b'#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let after = &stripped[hashes..];
    if !(after.is_empty() || after.starts_with(' ') || after.starts_with('\t')) {
        return None;
    }
    let text = after.trim().trim_end_matches('#').trim_end();
    Some((hashes as u8, text.to_string()))
}

/// Remove up to `max` leading space characters.
fn strip_indent(raw: &str, max: usize) -> &str {
    let mut n = 0;
    for (i, c) in raw.char_indices() {
        if c == ' ' && n < max {
            n += 1;
        } else {
            return &raw[i..];
        }
    }
    ""
}

/// Build sections from heading/paragraph blocks, tracking the heading ancestry.
fn split_sections(text: &str) -> Vec<Section> {
    let blocks = scan_blocks(text);
    let mut stack: Vec<(u8, String)> = Vec::new();
    let mut sections: Vec<Section> = Vec::new();
    let mut current = Section {
        heading_path: None,
        paragraphs: Vec::new(),
    };

    for block in blocks {
        match block {
            Block::Heading {
                level,
                text: heading,
            } => {
                if !current.paragraphs.is_empty() {
                    sections.push(current);
                }
                while matches!(stack.last(), Some((l, _)) if *l >= level) {
                    stack.pop();
                }
                stack.push((level, heading));
                let path = stack
                    .iter()
                    .map(|(_, t)| t.as_str())
                    .collect::<Vec<_>>()
                    .join(" > ");
                current = Section {
                    heading_path: Some(path),
                    paragraphs: Vec::new(),
                };
            }
            Block::Paragraph { start, end } => current.paragraphs.push((start, end)),
        }
    }

    if !current.paragraphs.is_empty() {
        sections.push(current);
    }
    sections
}

/// Split plain text into paragraph ranges on blank lines.
fn paragraph_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut end = 0usize;
    let mut offset = 0usize;

    for line in text.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let raw = line.trim_end_matches(['\n', '\r']);
        let content_end = line_start + raw.len();

        if raw.trim().is_empty() {
            if let Some(s) = start.take() {
                if let Some(r) = trim_range(text, s, end) {
                    out.push(r);
                }
            }
        } else {
            if start.is_none() {
                start = Some(line_start);
            }
            end = content_end;
        }
    }
    if let Some(s) = start.take() {
        if let Some(r) = trim_range(text, s, end) {
            out.push(r);
        }
    }
    out
}

// --- Chunking pipeline -----------------------------------------------------

fn chunk_sections(text: &str, cfg: &ChunkConfig, sections: &[Section]) -> Vec<Chunk> {
    let mut out = Vec::new();
    for section in sections {
        let units = section_units(text, section, cfg);
        if units.is_empty() {
            continue;
        }
        let mut groups = merge_units(&units, cfg);
        merge_small(&mut groups, &units, cfg);
        let mut chunks = build_chunks(text, section.heading_path.as_deref(), &units, &groups);
        apply_overlap(text, &mut chunks, cfg);
        out.extend(chunks);
    }
    for (i, chunk) in out.iter_mut().enumerate() {
        chunk.ordinal = i;
    }
    out
}

/// Turn a section's paragraphs into units, splitting long paragraphs into
/// sentences and force-splitting sentences that exceed `max_chunk_size`.
fn section_units(text: &str, section: &Section, cfg: &ChunkConfig) -> Vec<Unit> {
    let mut units = Vec::new();
    for &(pstart, pend) in &section.paragraphs {
        let paragraph = &text[pstart..pend];
        if count_tokens(paragraph) <= cfg.chunk_size {
            units.push(Unit {
                start: pstart,
                end: pend,
                tokens: count_tokens(paragraph),
            });
            continue;
        }
        for (sstart, send) in split_sentences(text, pstart, pend) {
            let tokens = count_tokens(&text[sstart..send]);
            if tokens > cfg.max_chunk_size {
                for (ws, we) in force_split(text, sstart, send, cfg.max_chunk_size) {
                    let wt = count_tokens(&text[ws..we]);
                    units.push(Unit {
                        start: ws,
                        end: we,
                        tokens: wt,
                    });
                }
            } else {
                units.push(Unit {
                    start: sstart,
                    end: send,
                    tokens,
                });
            }
        }
    }
    units
}

/// Split a range into sentence units (breaking after `.`, `!`, `?`).
fn split_sentences(text: &str, start: usize, end: usize) -> Vec<(usize, usize)> {
    let slice = &text[start..end];
    let chars: Vec<(usize, char)> = slice.char_indices().collect();
    let mut out = Vec::new();
    let mut seg_start = 0usize;

    for (idx, &(i, c)) in chars.iter().enumerate() {
        if !matches!(c, '.' | '!' | '?') {
            continue;
        }
        let at_boundary = match chars.get(idx + 1) {
            None => true,
            Some((_, next)) => next.is_whitespace(),
        };
        if at_boundary {
            let seg_end = i + c.len_utf8();
            if let Some(r) = trim_range(text, start + seg_start, start + seg_end) {
                out.push(r);
            }
            seg_start = seg_end;
        }
    }
    if seg_start < slice.len() {
        if let Some(r) = trim_range(text, start + seg_start, end) {
            out.push(r);
        }
    }
    out
}

/// Force-split an oversized span into word-delimited pieces of ≤ `max_tokens`.
fn force_split(text: &str, start: usize, end: usize, max_tokens: u32) -> Vec<(usize, usize)> {
    let slice = &text[start..end];
    let mut out = Vec::new();
    let mut seg_start = 0usize;
    let mut count = 0u32;

    for (ws, we) in word_ranges(slice) {
        let word_tokens = count_tokens(&slice[ws..we]);
        if count > 0 && count + word_tokens > max_tokens {
            if let Some(r) = trim_range(text, start + seg_start, start + ws) {
                out.push(r);
            }
            seg_start = ws;
            count = 0;
        }
        count += word_tokens;
    }
    if seg_start < slice.len() {
        if let Some(r) = trim_range(text, start + seg_start, end) {
            out.push(r);
        }
    }
    out
}

/// Byte ranges of whitespace-delimited words within `slice`.
fn word_ranges(slice: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in slice.char_indices() {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                out.push((s, i));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        out.push((s, slice.len()));
    }
    out
}

/// Greedily merge adjacent units while the combined size stays ≤ `chunk_size`.
fn merge_units(units: &[Unit], cfg: &ChunkConfig) -> Vec<(usize, usize)> {
    let mut groups = Vec::new();
    let mut i = 0usize;
    while i < units.len() {
        let first = i;
        let mut last = i;
        let mut total = units[i].tokens;
        while last + 1 < units.len() && total + units[last + 1].tokens <= cfg.chunk_size {
            last += 1;
            total += units[last].tokens;
        }
        groups.push((first, last));
        i = last + 1;
    }
    groups
}

/// Merge chunks smaller than `min_chunk_size` into the previous one, if the
/// combined size stays ≤ `max_chunk_size`.
fn merge_small(groups: &mut Vec<(usize, usize)>, units: &[Unit], cfg: &ChunkConfig) {
    let mut k = 1;
    while k < groups.len() {
        let (pa, pb) = groups[k - 1];
        let (a, b) = groups[k];
        let prev_tokens: u32 = units[pa..=pb].iter().map(|u| u.tokens).sum();
        let cur_tokens: u32 = units[a..=b].iter().map(|u| u.tokens).sum();
        if cur_tokens < cfg.min_chunk_size && prev_tokens + cur_tokens <= cfg.max_chunk_size {
            groups[k - 1] = (pa, b);
            groups.remove(k);
        } else {
            k += 1;
        }
    }
}

fn build_chunks(
    text: &str,
    heading_path: Option<&str>,
    units: &[Unit],
    groups: &[(usize, usize)],
) -> Vec<Chunk> {
    groups
        .iter()
        .map(|&(a, b)| {
            let start = units[a].start;
            let end = units[b].end;
            let content = text[start..end].to_string();
            Chunk {
                ordinal: 0,
                token_count: count_tokens(&content),
                content,
                start_offset: start,
                end_offset: end,
                heading_path: heading_path.map(str::to_string),
                language: None,
                symbol: None,
            }
        })
        .collect()
}

/// Prepend each chunk (after the first in a section) with the trailing
/// `chunk_overlap` tokens of the previous chunk, extending its start offset.
fn apply_overlap(text: &str, chunks: &mut [Chunk], cfg: &ChunkConfig) {
    if cfg.chunk_overlap == 0 || chunks.len() < 2 {
        return;
    }
    for i in 1..chunks.len() {
        let prev_end = chunks[i - 1].end_offset;
        let tail = {
            let prev = &chunks[i - 1];
            overlap_tail(&prev.content, cfg.chunk_overlap)
        };
        let Some(tail) = tail else { continue };
        let Some(new_start) = prev_end.checked_sub(tail.len()) else {
            continue;
        };
        if new_start >= chunks[i].start_offset {
            continue;
        }
        let end = chunks[i].end_offset;
        chunks[i].start_offset = new_start;
        chunks[i].content = text[new_start..end].to_string();
        chunks[i].token_count = count_tokens(&chunks[i].content);
    }
}

/// The trailing `n` tokens of `content`, or `None` if it cannot be sliced
/// cleanly (in which case overlap is skipped).
fn overlap_tail(content: &str, n: u32) -> Option<String> {
    let tokens = bpe().encode_ordinary(content);
    if tokens.len() as u32 <= n {
        return None;
    }
    let start = tokens.len() - n as usize;
    let bytes = bpe().decode_bytes(&tokens[start..]).ok()?;
    let tail = String::from_utf8(bytes).ok()?;
    content.ends_with(&tail).then_some(tail)
}

/// Trim a byte range and return it if non-empty.
fn trim_range(text: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    if start >= end {
        return None;
    }
    let slice = text.get(start..end)?;
    let trimmed = slice.trim();
    if trimmed.is_empty() {
        return None;
    }
    let new_start = start + (slice.len() - slice.trim_start().len());
    Some((new_start, new_start + trimmed.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ChunkConfig {
        ChunkConfig::default()
    }

    fn words(n: usize) -> String {
        "alpha ".repeat(n)
    }

    #[test]
    fn offsets_index_the_source() {
        let text = "# Title\n\nHello world. Second sentence here.\n";
        let chunks = chunk_markdown(text, &cfg());
        for chunk in &chunks {
            assert_eq!(&text[chunk.start_offset..chunk.end_offset], chunk.content);
        }
    }

    #[test]
    fn heading_path_tracks_ancestry() {
        let text = "# Title\n\nintro paragraph\n\n## Section\n\nbody paragraph\n\n### Sub\n\ndeep paragraph\n";
        let chunks = chunk_markdown(text, &cfg());

        let find = |needle: &str| {
            chunks
                .iter()
                .find(|c| c.content.contains(needle))
                .unwrap_or_else(|| panic!("no chunk with {needle}: {chunks:?}"))
        };
        assert_eq!(find("intro").heading_path.as_deref(), Some("Title"));
        assert_eq!(
            find("body").heading_path.as_deref(),
            Some("Title > Section")
        );
        assert_eq!(
            find("deep").heading_path.as_deref(),
            Some("Title > Section > Sub")
        );
    }

    #[test]
    fn heading_path_is_none_without_headings() {
        let chunks = chunk_plain_text("Just some text without headings.", &cfg());
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].heading_path, None);
    }

    #[test]
    fn token_count_matches_content() {
        let text = "# T\n\nsome words here that form a chunk\n";
        let chunks = chunk_markdown(text, &cfg());
        for chunk in &chunks {
            assert_eq!(chunk.token_count, count_tokens(&chunk.content));
        }
    }

    #[test]
    fn tiny_paragraphs_are_merged() {
        let text = "one two three\n\nfour five six\n\nseven eight nine\n";
        let chunks = chunk_markdown(text, &cfg());
        assert_eq!(chunks.len(), 1, "chunks: {chunks:?}");
        assert!(chunks[0].content.contains("seven"));
    }

    #[test]
    fn min_chunk_merges_into_previous() {
        // A big paragraph (≈500 tokens) then a small one (≈50 tokens): greedy
        // merging leaves the small one alone, then min-merge folds it back in.
        let text = format!("{}\n\n{}\n", words(500), words(50));
        let chunks = chunk_markdown(&text, &cfg());
        assert_eq!(chunks.len(), 1, "chunks: {chunks:?}");
        assert!(chunks[0].token_count > DEFAULT_CHUNK_SIZE);
    }

    #[test]
    fn oversized_single_sentence_is_force_split() {
        // ≈1300 tokens with no sentence punctuation → force-split at max size.
        let text = format!("{}\n", words(1300));
        let chunks = chunk_markdown(&text, &cfg());
        assert!(chunks.len() >= 2, "chunks: {}", chunks.len());
        for chunk in &chunks {
            assert!(chunk.token_count <= DEFAULT_MAX_CHUNK_SIZE, "{chunk:?}");
        }
    }

    #[test]
    fn adjacent_chunks_overlap() {
        // Two ≈400-token paragraphs in one section → two chunks that overlap.
        let text = format!("{}\n\n{}\n", words(400), words(400));
        let chunks = chunk_markdown(&text, &cfg());
        assert_eq!(chunks.len(), 2, "chunks: {chunks:?}");
        assert!(chunks[1].start_offset < chunks[0].end_offset);

        let shared = &text[chunks[1].start_offset..chunks[0].end_offset];
        assert!(!shared.is_empty());
        assert!(chunks[0].content.ends_with(shared));
        assert!(chunks[1].content.starts_with(shared));
    }

    #[test]
    fn ordinals_are_sequential() {
        let text = format!("{}\n\n{}\n\n{}\n", words(400), words(400), words(400));
        let chunks = chunk_markdown(&text, &cfg());
        let ordinals: Vec<_> = chunks.iter().map(|c| c.ordinal).collect();
        assert_eq!(ordinals, (0..chunks.len()).collect::<Vec<_>>());
    }

    #[test]
    fn plain_text_splits_on_paragraphs() {
        let text = "First paragraph body.\n\nSecond paragraph body.\n";
        let chunks = chunk_plain_text(text, &cfg());
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].content.contains("First"));
        assert!(chunks[0].content.contains("Second"));
    }

    #[test]
    fn attachment_text_matches_plain_text() {
        let text = "Attachment text body.\n\nMore attachment text.\n";
        assert_eq!(
            chunk_attachment_text(text, &cfg()),
            chunk_plain_text(text, &cfg())
        );
    }

    #[test]
    fn headings_inside_code_fences_are_not_sections() {
        let text = "# Real\n\n```\n# Not a heading\n```\n\nbody\n";
        let chunks = chunk_markdown(text, &cfg());
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].heading_path.as_deref(), Some("Real"));
        assert!(chunks[0].content.contains("# Not a heading"));
    }

    #[test]
    fn defaults_match_the_spec() {
        let c = ChunkConfig::default();
        assert_eq!(c.chunk_size, 512);
        assert_eq!(c.chunk_overlap, 64);
        assert_eq!(c.min_chunk_size, 100);
        assert_eq!(c.max_chunk_size, 1024);
    }

    #[test]
    fn count_tokens_is_nonzero_for_text() {
        assert!(count_tokens("hello world") >= 2);
    }
}
