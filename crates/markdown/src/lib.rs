//! Fergents markdown flavor parser.
//!
//! Parses the Fergents markdown flavor (`BRAIN.md` §4.2): CommonMark + GFM
//! tables and task lists, wiki links `[[note]]`, embeds `![[note]]`, block
//! refs `^id`, tags `#tag`, callouts `> [!note]`, math `$…$` / `$$…$$`,
//! Mermaid fenced blocks, and YAML frontmatter. The extracted structures feed
//! the per-brain index (`crates/index`).
//!
//! The parser is pure: [`parse`] takes a `&str` and returns a
//! [`ParsedDocument`], with no I/O and no database types.

mod flavor;
mod frontmatter;
mod model;

pub use model::{
    Callout, Frontmatter, Link, LinkKind, MathKind, MathSpan, MermaidDiagram, ParsedDocument,
};

/// Parse a Fergents markdown document into its structured form.
///
/// Frontmatter decoding errors are never fatal: they are surfaced on
/// [`Frontmatter::error`] while the raw text and the body are still returned.
pub fn parse(input: &str) -> ParsedDocument {
    let (frontmatter, body) = frontmatter::split(input);
    let findings = flavor::extract(body);

    ParsedDocument {
        frontmatter,
        body: body.to_string(),
        links: findings.links,
        tags: findings.tags,
        block_refs: findings.block_refs,
        callouts: findings.callouts,
        math: findings.math,
        mermaid: findings.mermaid,
    }
}
