//! Deterministic token counting (`LLM.md` §13, spec decision D8).
//!
//! Prefers the `cl100k_base` BPE (bundled with `tiktoken-rs`, the same
//! encoding the indexer uses) and falls back to a deterministic estimator when
//! the vocabulary is unavailable. The counter is the single source of truth for
//! both budgeting and the Context Viewer.

use std::sync::OnceLock;

use tiktoken_rs::{cl100k_base, CoreBPE};

use crate::types::{ContentBlock, Message};

fn bpe() -> Option<&'static CoreBPE> {
    static BPE: OnceLock<Option<CoreBPE>> = OnceLock::new();
    BPE.get_or_init(|| cl100k_base().ok()).as_ref()
}

/// Counts tokens for budgeting and context accounting.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokenCounter;

impl TokenCounter {
    /// Create a counter.
    pub fn new() -> Self {
        Self
    }

    /// Count the tokens in `text`.
    pub fn count(&self, text: &str) -> usize {
        match bpe() {
            Some(bpe) => bpe.encode_with_special_tokens(text).len(),
            None => Self::estimate(text),
        }
    }

    /// A deterministic fallback: roughly one token per four characters.
    pub fn estimate(text: &str) -> usize {
        let chars = text.chars().count();
        chars.div_ceil(4)
    }

    /// Count the tokens across a set of messages.
    pub fn count_messages(&self, messages: &[Message]) -> usize {
        let mut total = 0;
        for message in messages {
            // A small per-message overhead mirrors chat-format framing.
            total += 3;
            for block in &message.content {
                total += match block {
                    ContentBlock::Text { text } => self.count(text),
                    ContentBlock::Image { .. } => 85,
                    ContentBlock::ToolCall {
                        name, arguments, ..
                    } => self.count(name) + self.count(&arguments.to_string()),
                    ContentBlock::ToolResult { content, .. } => self.count(content),
                };
            }
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counting_is_deterministic_and_monotonic() {
        let counter = TokenCounter::new();
        let a = counter.count("hello world");
        assert_eq!(a, counter.count("hello world"));
        let b = counter.count("hello world, this is a longer sentence");
        assert!(b >= a, "{b} >= {a}");
        assert!(a > 0);
    }

    #[test]
    fn estimator_is_used_without_the_vocabulary() {
        assert_eq!(TokenCounter::estimate("abcd"), 1);
        assert_eq!(TokenCounter::estimate("abcde"), 2);
        assert_eq!(TokenCounter::estimate(""), 0);
    }

    #[test]
    fn message_count_includes_text_content() {
        let counter = TokenCounter::new();
        let single = counter.count_messages(&[Message::user("hello")]);
        let double = counter.count_messages(&[Message::user("hello"), Message::assistant("hi")]);
        assert!(double > single);
    }
}
