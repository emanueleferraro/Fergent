//! The [`Embedder`] trait and its implementations (`SEARCH.md` §4).
//!
//! Two backends are provided:
//!
//! - [`StubEmbedder`] — a deterministic, dependency-free embedder used by tests
//!   and by default builds. It hashes the text into a fixed-dimension vector.
//! - [`FastEmbedder`] — the real BGE-M3 provider (`fastembed`), behind the
//!   `embed-real` feature so the default build and CI never compile the ORT
//!   runtime or download the model.

use crate::error::IndexError;

/// Output dimension of the default embedding model (BGE-M3).
pub const DEFAULT_EMBED_DIM: usize = 1024;
/// Default embedding batch size.
pub const DEFAULT_EMBED_BATCH: usize = 64;

/// A text embedder: maps text to dense vectors of dimension [`Embedder::dim`].
pub trait Embedder: Send + Sync {
    /// A stable identifier for the model (stored in `embedding_meta`).
    fn model(&self) -> &str;

    /// The output dimension.
    fn dim(&self) -> usize;

    /// Embed a batch of texts, returning one vector per input, in order.
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, IndexError>;

    /// Embed a single text.
    fn embed_one(&self, text: &str) -> Result<Vec<f32>, IndexError> {
        let mut vectors = self.embed(std::slice::from_ref(&text.to_string()))?;
        vectors
            .pop()
            .ok_or_else(|| IndexError::Embedding("embedder returned no vector".to_string()))
    }
}

/// The model id reported by [`StubEmbedder`].
pub const STUB_MODEL: &str = "stub";

/// A deterministic, dependency-free embedder for tests and default builds.
///
/// It uses signed feature hashing over whitespace-delimited words: identical
/// text always yields the identical unit vector, and texts that share words
/// overlap. It is **not** semantically meaningful — only [`FastEmbedder`] is.
#[derive(Debug, Clone)]
pub struct StubEmbedder {
    dim: usize,
}

impl StubEmbedder {
    /// Create a stub embedder with the given output dimension.
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }

    fn embed_text(&self, text: &str) -> Vec<f32> {
        let mut vector = vec![0f32; self.dim];
        for word in text.split_whitespace() {
            let word = word
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            if word.is_empty() {
                continue;
            }
            let hash = fnv1a(word.as_bytes());
            let index = (hash % self.dim as u64) as usize;
            vector[index] += if hash & (1 << 63) == 0 { 1.0 } else { -1.0 };

            let hash2 = fnv1a_seeded(word.as_bytes(), 0x9e37_79b9_7f4a_7c15);
            let index2 = (hash2 % self.dim as u64) as usize;
            vector[index2] += if hash2 & (1 << 63) == 0 { 1.0 } else { -1.0 };
        }
        normalize(&mut vector);
        vector
    }
}

impl Default for StubEmbedder {
    fn default() -> Self {
        Self::new(DEFAULT_EMBED_DIM)
    }
}

impl Embedder for StubEmbedder {
    fn model(&self) -> &str {
        STUB_MODEL
    }

    fn dim(&self) -> usize {
        self.dim
    }

    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, IndexError> {
        Ok(texts.iter().map(|text| self.embed_text(text)).collect())
    }
}

/// FNV-1a 64-bit hash (stable across runs, unlike `DefaultHasher`).
fn fnv1a(bytes: &[u8]) -> u64 {
    fnv1a_seeded(bytes, 0xcbf2_9ce4_8422_2325)
}

fn fnv1a_seeded(bytes: &[u8], seed: u64) -> u64 {
    let mut hash = seed;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Scale `vector` to unit L2 norm (a zero vector becomes `[1, 0, …]`).
fn normalize(vector: &mut [f32]) {
    let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm == 0.0 {
        if let Some(first) = vector.first_mut() {
            *first = 1.0;
        }
        return;
    }
    for value in vector.iter_mut() {
        *value /= norm;
    }
}

#[cfg(feature = "embed-real")]
mod real {
    use std::sync::Mutex;

    use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};

    use super::{Embedder, DEFAULT_EMBED_BATCH, DEFAULT_EMBED_DIM};
    use crate::error::IndexError;

    /// The model id reported by [`FastEmbedder`].
    pub const REAL_MODEL: &str = "local:bge-m3";

    /// The real BGE-M3 embedder (`fastembed`).
    pub struct FastEmbedder {
        inner: Mutex<TextEmbedding>,
    }

    impl FastEmbedder {
        /// Load the BGE-M3 model, downloading it on first use.
        ///
        /// Refuses to download (returning an error) when the kernel runs in
        /// offline mode (`FERGENTS_OFFLINE`).
        pub fn new() -> Result<Self, IndexError> {
            if fergents_storage::offline() {
                return Err(IndexError::Embedding(
                    "offline mode (FERGENTS_OFFLINE): refusing to download the model".to_string(),
                ));
            }
            let options = TextInitOptions::new(EmbeddingModel::BGEM3);
            let model = TextEmbedding::try_new(options)
                .map_err(|err| IndexError::Embedding(err.to_string()))?;
            Ok(Self {
                inner: Mutex::new(model),
            })
        }
    }

    impl Embedder for FastEmbedder {
        fn model(&self) -> &str {
            REAL_MODEL
        }

        fn dim(&self) -> usize {
            DEFAULT_EMBED_DIM
        }

        fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, IndexError> {
            let mut model = self
                .inner
                .lock()
                .map_err(|_| IndexError::Embedding("embedder lock poisoned".to_string()))?;
            model
                .embed(texts, Some(DEFAULT_EMBED_BATCH))
                .map_err(|err| IndexError::Embedding(err.to_string()))
        }
    }
}

#[cfg(feature = "embed-real")]
pub use real::{FastEmbedder, REAL_MODEL};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_is_deterministic_and_unit_norm() {
        let embedder = StubEmbedder::default();
        let a = embedder.embed_one("hello world").unwrap();
        let b = embedder.embed_one("hello world").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), DEFAULT_EMBED_DIM);

        let norm = a.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "norm = {norm}");
    }

    #[test]
    fn stub_differs_for_different_text() {
        let embedder = StubEmbedder::default();
        assert_ne!(
            embedder.embed_one("alpha beta").unwrap(),
            embedder.embed_one("gamma delta").unwrap()
        );
    }

    #[test]
    fn stub_empty_text_is_still_a_unit_vector() {
        let vector = StubEmbedder::default().embed_one("").unwrap();
        assert_eq!(vector[0], 1.0);
        assert!(vector[1..].iter().all(|v| *v == 0.0));
    }
}
