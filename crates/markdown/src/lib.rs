//! Fergents markdown flavor parser.
//!
//! Parses the Fergents markdown flavor (`BRAIN.md` §4.2): CommonMark + GFM
//! tables and task lists, wiki links `[[note]]`, embeds `![[note]]`, block
//! refs `^id`, tags `#tag`, callouts `> [!note]`, math `$…$` / `$$…$$`,
//! Mermaid fenced blocks, and YAML frontmatter. The extracted structures feed
//! the per-brain index (`crates/index`).
