//! The parsed representation of a Fergents markdown document.
//!
//! These structures are pure data: they describe what was found in the source
//! text, with no I/O and no index/database types. Downstream tasks map them to
//! the per-brain index tables (`links`, `document_tags`, `notes_meta`, ...).

/// A parsed markdown document: frontmatter, body, and the flavor constructs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedDocument {
    /// YAML frontmatter, if the document begins with a `---` fenced block.
    pub frontmatter: Option<Frontmatter>,
    /// The markdown body, with the frontmatter block removed.
    pub body: String,
    /// Outgoing links in document order (wiki, embed, and standard markdown).
    pub links: Vec<Link>,
    /// Tag names in document order (e.g. `tag`, `nested/tag`), with duplicates.
    pub tags: Vec<String>,
    /// Block reference ids (`^id`), in document order.
    pub block_refs: Vec<String>,
    /// Callouts (`> [!note] ...`), in document order.
    pub callouts: Vec<Callout>,
    /// Inline (`$…$`) and block (`$$…$$`) math spans, in document order.
    pub math: Vec<MathSpan>,
    /// Mermaid fenced code blocks, in document order.
    pub mermaid: Vec<MermaidDiagram>,
}

/// YAML frontmatter split from the document head.
///
/// The `raw` text is always preserved verbatim. `parsed` holds the decoded
/// YAML (unknown keys included) when it parses; `error` is set when it does
/// not, so a malformed head never loses data and never panics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frontmatter {
    /// The YAML text between the fences, without the `---` delimiters.
    pub raw: String,
    /// The decoded YAML value, or `None` if decoding failed.
    pub parsed: Option<serde_yaml::Value>,
    /// The decoding error message, if any.
    pub error: Option<String>,
}

/// The kind of an outgoing link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    /// A wiki link: `[[note]]`, `[[note|text]]`, `[[note#heading]]`.
    Wiki,
    /// An embed: `![[note]]`, `![[image.png]]`.
    Embed,
    /// A standard markdown link: `[text](target)`.
    Markdown,
}

/// An outgoing link extracted from the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// The link kind.
    pub kind: LinkKind,
    /// The link target (wiki note, embed target, or markdown URL).
    pub target: String,
    /// The display text, when given (`[[note|text]]`, `[text](url)`).
    pub display: Option<String>,
    /// The heading anchor (`[[note#heading]]`), when the target is a heading.
    pub heading: Option<String>,
    /// The block id (`[[note#^id]]`), when the target is a block reference.
    pub block_id: Option<String>,
}

/// A callout: `> [!type] Optional title`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Callout {
    /// The callout type, lowercased (e.g. `note`, `warning`, `tip`).
    pub kind: String,
    /// The optional title on the marker line, when present.
    pub title: Option<String>,
}

/// Whether a math span is inline or block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathKind {
    /// Inline math: `$…$`.
    Inline,
    /// Display math: `$$…$$`.
    Block,
}

/// A math span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MathSpan {
    /// Inline or block.
    pub kind: MathKind,
    /// The TeX source between the dollar signs.
    pub content: String,
}

/// A Mermaid diagram fenced block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MermaidDiagram {
    /// The diagram source inside the fence.
    pub code: String,
}
