//! The [`Reranker`] trait and its implementations (`SEARCH.md` §8).
//!
//! Two backends are provided:
//!
//! - [`StubReranker`] — a deterministic, dependency-free cross-encoder stand-in
//!   used by tests and by default builds.
//! - [`CrossEncoderReranker`] — the real BGE reranker (`fastembed`), behind the
//!   `rerank-real` feature so the default build and CI never compile the ORT
//!   runtime or download the model.

use crate::error::IndexError;

/// Default number of fused candidates handed to the reranker (`SEARCH.md` §8.2).
pub const RERANK_TOP_K: usize = 20;
/// Queries with fewer tokens than this skip reranking (`SEARCH.md` §8.3).
pub const RERANK_MIN_TOKENS: usize = 3;

/// A cross-encoder reranker: scores each document jointly with the query.
pub trait Reranker: Send + Sync {
    /// A stable identifier for the model.
    fn model(&self) -> &str;

    /// Score each document against `query`, returning one score per document in
    /// the same order (higher is more relevant).
    fn rerank(&self, query: &str, documents: &[String]) -> Result<Vec<f32>, IndexError>;
}

/// The model id reported by [`StubReranker`].
pub const STUB_RERANK_MODEL: &str = "stub";

/// A deterministic, dependency-free reranker for tests and default builds.
///
/// It scores each document by the fraction of distinct query terms it
/// contains. It is **not** semantically meaningful — only
/// [`CrossEncoderReranker`] is.
#[derive(Debug, Clone, Copy, Default)]
pub struct StubReranker;

impl StubReranker {
    /// Create a stub reranker.
    pub fn new() -> Self {
        Self
    }
}

impl Reranker for StubReranker {
    fn model(&self) -> &str {
        STUB_RERANK_MODEL
    }

    fn rerank(&self, query: &str, documents: &[String]) -> Result<Vec<f32>, IndexError> {
        let terms = query_terms(query);
        Ok(documents
            .iter()
            .map(|doc| {
                if terms.is_empty() {
                    return 0.0;
                }
                let doc_terms = query_terms(doc);
                let matched = terms
                    .iter()
                    .filter(|term| doc_terms.iter().any(|d| d == *term))
                    .count();
                matched as f32 / terms.len() as f32
            })
            .collect())
    }
}

/// Lowercased alphanumeric words longer than one character.
fn query_terms(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() > 1)
        .map(str::to_lowercase)
        .collect()
}

#[cfg(feature = "rerank-real")]
mod real {
    use std::sync::Mutex;

    use fastembed::{RerankInitOptions, RerankerModel, TextRerank};

    use super::Reranker;
    use crate::error::IndexError;

    /// The model id reported by [`CrossEncoderReranker`].
    pub const REAL_RERANK_MODEL: &str = "local:bge-reranker-v2-m3";

    /// The real cross-encoder reranker (`fastembed`).
    pub struct CrossEncoderReranker {
        inner: Mutex<TextRerank>,
        model: &'static str,
    }

    impl CrossEncoderReranker {
        /// Load the BGE reranker, downloading it on first use.
        ///
        /// Refuses to download (returning an error) when the kernel runs in
        /// offline mode (`FERGENTS_OFFLINE`).
        pub fn new() -> Result<Self, IndexError> {
            if fergents_storage::offline() {
                return Err(IndexError::Embedding(
                    "offline mode (FERGENTS_OFFLINE): refusing to download the model".to_string(),
                ));
            }
            let options = RerankInitOptions::new(RerankerModel::BGERerankerV2M3);
            let model = TextRerank::try_new(options)
                .map_err(|err| IndexError::Embedding(err.to_string()))?;
            Ok(Self {
                inner: Mutex::new(model),
                model: REAL_RERANK_MODEL,
            })
        }
    }

    impl Reranker for CrossEncoderReranker {
        fn model(&self) -> &str {
            self.model
        }

        fn rerank(&self, query: &str, documents: &[String]) -> Result<Vec<f32>, IndexError> {
            if documents.is_empty() {
                return Ok(Vec::new());
            }
            let mut model = self
                .inner
                .lock()
                .map_err(|_| IndexError::Embedding("reranker lock poisoned".to_string()))?;
            // `rerank` returns results sorted by score, each carrying the index
            // of the original document.
            let documents: Vec<&str> = documents.iter().map(String::as_str).collect();
            let results = model
                .rerank(query, &documents, false, None)
                .map_err(|err| IndexError::Embedding(err.to_string()))?;
            let mut scores = vec![0f32; documents.len()];
            for result in results {
                if let Some(slot) = scores.get_mut(result.index) {
                    *slot = result.score;
                }
            }
            Ok(scores)
        }
    }
}

#[cfg(feature = "rerank-real")]
pub use real::{CrossEncoderReranker, REAL_RERANK_MODEL};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_scores_the_fraction_of_query_terms_present() {
        let reranker = StubReranker::new();
        let docs = vec![
            "alpha beta gamma".to_string(),
            "alpha beta".to_string(),
            "delta".to_string(),
        ];
        let scores = reranker.rerank("alpha beta gamma", &docs).unwrap();
        assert_eq!(scores, vec![1.0, 2.0 / 3.0, 0.0]);
    }

    #[test]
    fn stub_is_deterministic_and_handles_empty_queries() {
        let reranker = StubReranker::new();
        let docs = vec!["anything".to_string()];
        assert_eq!(
            reranker.rerank("a b", &docs).unwrap(),
            reranker.rerank("a b", &docs).unwrap()
        );
        assert_eq!(reranker.rerank("", &docs).unwrap(), vec![0.0]);
    }
}
