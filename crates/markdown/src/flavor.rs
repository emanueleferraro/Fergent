//! Walking the CommonMark/GFM AST to extract the Fergents flavor constructs.
//!
//! Code blocks, inline code, and raw HTML are never scanned for links or tags,
//! so wiki links and tags written inside code are correctly ignored.

use comrak::nodes::{AstNode, NodeValue};
use comrak::{parse_document, Arena, Options};

use crate::model::{Callout, Link, LinkKind, MathKind, MathSpan, MermaidDiagram};

/// Every flavor construct found in a body, in document order.
#[derive(Debug, Default)]
pub(crate) struct Findings {
    pub links: Vec<Link>,
    pub tags: Vec<String>,
    pub block_refs: Vec<String>,
    pub callouts: Vec<Callout>,
    pub math: Vec<MathSpan>,
    pub mermaid: Vec<MermaidDiagram>,
}

/// Extract every flavor construct from `body`.
pub(crate) fn extract(body: &str) -> Findings {
    let arena = Arena::new();
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.tasklist = true;
    options.extension.strikethrough = true;
    options.extension.math_dollars = true;

    let root = parse_document(&arena, body, &options);
    let mut findings = Findings::default();
    walk(root, &mut findings);
    findings
}

/// What to do with a node, decided without holding its borrow.
enum Action {
    None,
    Skip,
    Text(String),
    Mermaid(String),
    Math(MathSpan),
    MarkdownLink(Link),
    Callout(Callout),
}

fn walk<'a>(node: &'a AstNode<'a>, out: &mut Findings) {
    match classify(node) {
        Action::Skip => return,
        Action::Mermaid(code) => {
            out.mermaid.push(MermaidDiagram { code });
            return;
        }
        Action::Math(span) => {
            out.math.push(span);
            return;
        }
        Action::Text(text) => {
            scan_text(&text, out);
        }
        Action::MarkdownLink(link) => {
            out.links.push(link);
            return;
        }
        Action::Callout(callout) => {
            out.callouts.push(callout);
        }
        Action::None => {}
    }

    for child in node.children() {
        walk(child, out);
    }
}

fn classify<'a>(node: &'a AstNode<'a>) -> Action {
    let data = node.data.borrow();
    match &data.value {
        NodeValue::CodeBlock(block) => {
            if is_mermaid(&block.info) {
                Action::Mermaid(block.literal.clone())
            } else {
                Action::Skip
            }
        }
        NodeValue::Code(_) | NodeValue::HtmlBlock(_) | NodeValue::HtmlInline(_) => Action::Skip,
        NodeValue::Math(math) => Action::Math(MathSpan {
            kind: if math.display_math {
                MathKind::Block
            } else {
                MathKind::Inline
            },
            content: math.literal.clone(),
        }),
        NodeValue::Text(text) => Action::Text(text.to_string()),
        NodeValue::Link(link) => Action::MarkdownLink(Link {
            kind: LinkKind::Markdown,
            target: link.url.clone(),
            display: Some(collect_text(node)),
            heading: None,
            block_id: None,
        }),
        NodeValue::BlockQuote => match callout_from(node) {
            Some(callout) => Action::Callout(callout),
            None => Action::None,
        },
        _ => Action::None,
    }
}

/// `info` is the fenced code block's info string (` ```mermaid ` → `mermaid`).
fn is_mermaid(info: &str) -> bool {
    info.split_whitespace()
        .next()
        .is_some_and(|first| first.eq_ignore_ascii_case("mermaid"))
}

/// Build a callout from a blockquote whose first paragraph starts with `[!type]`.
fn callout_from<'a>(node: &'a AstNode<'a>) -> Option<Callout> {
    for child in node.children() {
        let is_paragraph = matches!(child.data.borrow().value, NodeValue::Paragraph);
        if is_paragraph {
            return parse_marker(&first_line_text(child));
        }
    }
    None
}

fn parse_marker(text: &str) -> Option<Callout> {
    let rest = text.trim_start().strip_prefix("[!")?;
    let close = rest.find(']')?;
    let kind = rest[..close].trim().to_ascii_lowercase();
    if kind.is_empty() {
        return None;
    }
    let mut after = &rest[close + 1..];
    if let Some(stripped) = after.strip_prefix(['+', '-']) {
        after = stripped;
    }
    let title = after.trim();
    Some(Callout {
        kind,
        title: (!title.is_empty()).then(|| title.to_string()),
    })
}

/// Concatenate the text content of a node's descendants, stopping at the
/// first line break (used to read just a callout's marker line).
fn first_line_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut out = String::new();
    first_line_into(node, &mut out);
    out
}

/// Returns `true` once a line break has been reached.
fn first_line_into<'a>(node: &'a AstNode<'a>, out: &mut String) -> bool {
    for child in node.children() {
        let mut stop = false;
        let recurse = {
            let data = child.data.borrow();
            match &data.value {
                NodeValue::Text(text) => {
                    out.push_str(text);
                    false
                }
                NodeValue::Code(code) => {
                    out.push_str(&code.literal);
                    false
                }
                NodeValue::SoftBreak | NodeValue::LineBreak => {
                    stop = true;
                    false
                }
                _ => true,
            }
        };
        if stop || (recurse && first_line_into(child, out)) {
            return true;
        }
    }
    false
}

/// Concatenate the text content of a node's descendants.
fn collect_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut out = String::new();
    collect_text_into(node, &mut out);
    out
}

fn collect_text_into<'a>(node: &'a AstNode<'a>, out: &mut String) {
    for child in node.children() {
        let recurse = {
            let data = child.data.borrow();
            match &data.value {
                NodeValue::Text(text) => {
                    out.push_str(text);
                    false
                }
                NodeValue::Code(code) => {
                    out.push_str(&code.literal);
                    false
                }
                NodeValue::SoftBreak | NodeValue::LineBreak => {
                    out.push(' ');
                    false
                }
                _ => true,
            }
        };
        if recurse {
            collect_text_into(child, out);
        }
    }
}

/// Scan a text node for wiki links, embeds, tags, and trailing block refs.
fn scan_text(text: &str, out: &mut Findings) {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'[' && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            if let Some((inner, end)) = find_wiki(text, i) {
                let embed = i > 0 && bytes[i - 1] == b'!';
                if let Some(link) = parse_wiki(inner, embed) {
                    out.links.push(link);
                }
                i = end;
                continue;
            }
        } else if bytes[i] == b'#' {
            if let Some((tag, end)) = scan_tag(text, i) {
                out.tags.push(tag);
                i = end;
                continue;
            }
        }
        i += 1;
    }

    if let Some(id) = trailing_block_ref(text) {
        out.block_refs.push(id);
    }
}

/// Find `[[ … ]]` starting at byte `start` (which points at the first `[`).
/// Returns the inner text and the byte index just past the closing `]]`.
fn find_wiki(text: &str, start: usize) -> Option<(&str, usize)> {
    let bytes = text.as_bytes();
    let mut j = start + 2;
    while j + 1 < bytes.len() {
        match bytes[j] {
            b'\n' => return None,
            b']' if bytes[j + 1] == b']' => return Some((&text[start + 2..j], j + 2)),
            _ => {}
        }
        j += 1;
    }
    None
}

fn parse_wiki(inner: &str, embed: bool) -> Option<Link> {
    let (target_part, display) = match inner.split_once('|') {
        Some((a, b)) => (a, Some(b.trim().to_string())),
        None => (inner, None),
    };
    let target_part = target_part.trim();
    if target_part.is_empty() {
        return None;
    }

    let (target, heading, block_id) = match target_part.split_once('#') {
        Some((t, anchor)) => {
            let anchor = anchor.trim();
            if let Some(id) = anchor.strip_prefix('^') {
                (t.trim(), None, Some(id.trim().to_string()))
            } else {
                (t.trim(), Some(anchor.to_string()), None)
            }
        }
        None => (target_part, None, None),
    };
    if target.is_empty() {
        return None;
    }

    Some(Link {
        kind: if embed {
            LinkKind::Embed
        } else {
            LinkKind::Wiki
        },
        target: target.to_string(),
        display: display.filter(|d| !d.is_empty()),
        heading: heading.filter(|h| !h.is_empty()),
        block_id: block_id.filter(|b| !b.is_empty()),
    })
}

/// Scan a tag starting at byte `i` (which points at `#`).
fn scan_tag(text: &str, i: usize) -> Option<(String, usize)> {
    if i > 0 {
        let prev = text[..i].chars().next_back()?;
        if !(prev.is_whitespace() || matches!(prev, '(' | '[' | '{' | '"' | '\'')) {
            return None;
        }
    }

    let rest = &text[i + 1..];
    let (_, first) = rest.char_indices().next()?;
    if !(first.is_alphanumeric() || first == '_') {
        return None;
    }

    let mut len = rest.len();
    for (idx, c) in rest.char_indices() {
        if is_tag_char(c) {
            continue;
        }
        len = idx;
        break;
    }
    let name = &rest[..len];
    if name.ends_with('/') || !name.chars().any(|c| !c.is_ascii_digit()) {
        return None;
    }
    Some((name.to_string(), i + 1 + len))
}

fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == '/'
}

/// A block reference id from a `^id` at the very end of the text.
fn trailing_block_ref(text: &str) -> Option<String> {
    let trimmed = text.trim_end();
    let caret = trimmed.rfind('^')?;
    if caret > 0 {
        let prev = trimmed[..caret].chars().next_back()?;
        if prev.is_alphanumeric() || prev == '^' {
            return None;
        }
    }
    let id = &trimmed[caret + 1..];
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return None;
    }
    Some(id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn body(text: &str) -> crate::ParsedDocument {
        parse(text)
    }

    #[test]
    fn extracts_wiki_link_variants() {
        let doc = body("[[Plain]] [[Note|Label]] [[Note#Heading]] [[Note#^block-1]]");
        assert_eq!(doc.links.len(), 4);
        assert_eq!(doc.links[0].target, "Plain");
        assert_eq!(doc.links[0].kind, LinkKind::Wiki);
        assert_eq!(doc.links[1].display.as_deref(), Some("Label"));
        assert_eq!(doc.links[2].heading.as_deref(), Some("Heading"));
        assert_eq!(doc.links[3].block_id.as_deref(), Some("block-1"));
    }

    #[test]
    fn extracts_embeds() {
        let doc = body("![[Image.png]] ![[Note]]");
        assert_eq!(doc.links.len(), 2);
        assert!(doc.links.iter().all(|l| l.kind == LinkKind::Embed));
        assert_eq!(doc.links[0].target, "Image.png");
    }

    #[test]
    fn extracts_tags_including_nested() {
        let doc = body("Some #tag and #nested/tag here.");
        assert_eq!(doc.tags, vec!["tag", "nested/tag"]);
    }

    #[test]
    fn ignores_links_and_tags_inside_code() {
        let doc = body("`[[inline]] #notag`\n\n```\n[[fenced]] #notag ^nope\n```\n");
        assert!(doc.links.is_empty(), "links: {:?}", doc.links);
        assert!(doc.tags.is_empty(), "tags: {:?}", doc.tags);
        assert!(doc.block_refs.is_empty());
    }

    #[test]
    fn extracts_callout_with_title() {
        let doc = body("> [!warning] Be careful\n> body text\n");
        assert_eq!(doc.callouts.len(), 1);
        assert_eq!(doc.callouts[0].kind, "warning");
        assert_eq!(doc.callouts[0].title.as_deref(), Some("Be careful"));
    }

    #[test]
    fn extracts_callout_without_title() {
        let doc = body("> [!note]\n> body\n");
        assert_eq!(doc.callouts[0].kind, "note");
        assert_eq!(doc.callouts[0].title, None);
    }

    #[test]
    fn plain_blockquote_is_not_a_callout() {
        let doc = body("> just a quote\n");
        assert!(doc.callouts.is_empty());
    }

    #[test]
    fn extracts_inline_and_block_math() {
        let doc = body("Inline $a^2 + b^2$ and block:\n\n$$\n\\int x\\,dx\n$$\n");
        assert_eq!(doc.math.len(), 2);
        assert_eq!(doc.math[0].kind, MathKind::Inline);
        assert_eq!(doc.math[0].content, "a^2 + b^2");
        assert_eq!(doc.math[1].kind, MathKind::Block);
    }

    #[test]
    fn extracts_mermaid_block() {
        let doc = body("```mermaid\ngraph TD;\n  A-->B;\n```\n");
        assert_eq!(doc.mermaid.len(), 1);
        assert!(doc.mermaid[0].code.contains("A-->B"));
    }

    #[test]
    fn non_mermaid_fence_is_not_mermaid() {
        let doc = body("```rust\nfn main() {}\n```\n");
        assert!(doc.mermaid.is_empty());
    }

    #[test]
    fn extracts_markdown_link() {
        let doc = body("[Docs](https://example.com)");
        assert_eq!(doc.links.len(), 1);
        assert_eq!(doc.links[0].kind, LinkKind::Markdown);
        assert_eq!(doc.links[0].target, "https://example.com");
        assert_eq!(doc.links[0].display.as_deref(), Some("Docs"));
    }

    #[test]
    fn extracts_block_ref() {
        let doc = body("A paragraph with a ref ^abc-123");
        assert_eq!(doc.block_refs, vec!["abc-123"]);
    }

    #[test]
    fn ignores_hash_inside_word_and_url_fragment() {
        let doc = body("see https://example.com/#frag and mid#word");
        assert!(doc.tags.is_empty(), "tags: {:?}", doc.tags);
    }
}
