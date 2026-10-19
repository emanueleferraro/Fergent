//! NER extraction (`GRAPH.md` §3.2, decision D4).
//!
//! The NER extractor is built on a [`TokenClassifier`] seam so the
//! person/organization/place grouping is testable with a deterministic stub.
//! The real ONNX token-classification backend lives behind the `ner-real`
//! feature. When no classifier is available the pipeline stays regex-only and
//! never fails (spec: "NER degrades gracefully").

use crate::error::GraphError;
use crate::extract::{normalize_name, EntityExtractor, ExtractedEntity};
use crate::model::EntityType;

/// A single classified token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenLabel {
    /// The token's surface text.
    pub text: String,
    /// Start offset in the source text.
    pub start: i64,
    /// End offset in the source text.
    pub end: i64,
    /// A BIO tag: `O`, `B-PER`, `I-ORG`, `B-LOC`, ...
    pub label: String,
}

/// A token-classification backend (ONNX in production, a stub in tests).
pub trait TokenClassifier: Send + Sync {
    /// A stable identifier (also stored as entity metadata).
    fn name(&self) -> &'static str;

    /// Classify `text` into labeled tokens, in order.
    fn classify(&self, text: &str) -> Result<Vec<TokenLabel>, GraphError>;
}

/// Map a BIO tag's entity group to a built-in entity type (`None` = ignored).
fn group_to_type(group: &str) -> Option<&'static str> {
    match group.to_ascii_uppercase().as_str() {
        "PER" | "PERSON" => Some(EntityType::Person.as_str()),
        "ORG" | "ORGANIZATION" => Some(EntityType::Organization.as_str()),
        "LOC" | "LOCATION" | "GPE" => Some(EntityType::Place.as_str()),
        _ => None,
    }
}

/// Split a BIO tag into `(is_begin, group)`.
fn parse_bio(label: &str) -> (bool, Option<String>) {
    if let Some(rest) = label.strip_prefix("B-") {
        (true, Some(rest.to_string()))
    } else if let Some(rest) = label.strip_prefix("I-") {
        (false, Some(rest.to_string()))
    } else {
        (false, None)
    }
}

/// The NER extractor: turns labeled tokens into person/org/place entities.
pub struct NerExtractor {
    classifier: Box<dyn TokenClassifier>,
}

impl NerExtractor {
    /// Wrap a classifier.
    pub fn new(classifier: Box<dyn TokenClassifier>) -> Self {
        Self { classifier }
    }
}

impl EntityExtractor for NerExtractor {
    fn name(&self) -> &'static str {
        "ner"
    }

    fn extract(&self, text: &str) -> Result<Vec<ExtractedEntity>, GraphError> {
        let tokens = self.classifier.classify(text)?;
        let mut entities = Vec::new();
        let mut current: Option<(String, i64, i64, String)> = None;

        for token in tokens {
            let (is_begin, group) = parse_bio(&token.label);
            let group = group.and_then(|g| group_to_type(&g).map(|t| (g, t)));
            match group {
                Some((_, entity_type)) if is_begin => {
                    flush(&mut current, &mut entities);
                    current = Some((entity_type.to_string(), token.start, token.end, token.text));
                }
                Some((_, entity_type)) => match current.as_mut() {
                    Some((kind, _, end, name)) if *kind == entity_type => {
                        *end = token.end;
                        name.push(' ');
                        name.push_str(&token.text);
                    }
                    _ => {
                        flush(&mut current, &mut entities);
                        current =
                            Some((entity_type.to_string(), token.start, token.end, token.text));
                    }
                },
                None => flush(&mut current, &mut entities),
            }
        }
        flush(&mut current, &mut entities);
        Ok(entities)
    }
}

fn flush(current: &mut Option<(String, i64, i64, String)>, out: &mut Vec<ExtractedEntity>) {
    if let Some((entity_type, start, end, name)) = current.take() {
        let normalized_name = normalize_name(&name);
        if !normalized_name.is_empty() {
            out.push(ExtractedEntity {
                entity_type,
                name,
                normalized_name,
                start_offset: start,
                end_offset: end,
            });
        }
    }
}

/// A composite extractor: runs each child and concatenates the results.
pub struct CompositeExtractor {
    extractors: Vec<Box<dyn EntityExtractor>>,
}

impl CompositeExtractor {
    /// Build from a list of extractors.
    pub fn new(extractors: Vec<Box<dyn EntityExtractor>>) -> Self {
        Self { extractors }
    }
}

impl EntityExtractor for CompositeExtractor {
    fn name(&self) -> &'static str {
        "composite"
    }

    fn extract(&self, text: &str) -> Result<Vec<ExtractedEntity>, GraphError> {
        let mut out = Vec::new();
        for extractor in &self.extractors {
            out.extend(extractor.extract(text)?);
        }
        Ok(out)
    }
}

/// The default extractors: regex always, plus NER when a model is available
/// (`GRAPH.md` §3.2). Never fails: a missing NER model degrades to regex-only.
pub fn default_extractors() -> CompositeExtractor {
    let mut extractors: Vec<Box<dyn EntityExtractor>> =
        vec![Box::new(crate::extract::RegexExtractor::new())];
    if let Some(ner) = ner_from_env() {
        extractors.push(Box::new(ner));
    }
    CompositeExtractor::new(extractors)
}

#[cfg(feature = "ner-real")]
fn ner_from_env() -> Option<NerExtractor> {
    // Offline mode (`FERGENTS_OFFLINE`): never load the ONNX classifier, so
    // extraction stays regex-only (graceful degradation, never fails).
    if fergents_storage::offline() {
        return None;
    }
    real::OrtTokenClassifier::from_env().map(|classifier| NerExtractor::new(Box::new(classifier)))
}

#[cfg(not(feature = "ner-real"))]
fn ner_from_env() -> Option<NerExtractor> {
    // No ONNX backend compiled in (decision D4): regex-only.
    None
}

#[cfg(feature = "ner-real")]
mod real {
    use std::sync::Mutex;

    use ort::session::Session;
    use ort::value::Tensor;
    use tokenizers::Tokenizer;

    use super::{TokenClassifier, TokenLabel};
    use crate::error::GraphError;

    /// The default CoNLL label order used when no labels file is configured.
    const DEFAULT_LABELS: &[&str] = &[
        "O", "B-PER", "I-PER", "B-ORG", "I-ORG", "B-LOC", "I-LOC", "B-MISC", "I-MISC",
    ];

    /// The ONNX token-classification classifier (`ort`), behind `ner-real`.
    pub struct OrtTokenClassifier {
        session: Mutex<Session>,
        tokenizer: Tokenizer,
        labels: Vec<String>,
        model: String,
    }

    impl OrtTokenClassifier {
        /// Load the model + tokenizer from `FERGENTS_NER_MODEL` and
        /// `FERGENTS_NER_TOKENIZER`. Returns `None` when unset or unloadable so
        /// extraction degrades to regex-only (never fails).
        pub fn from_env() -> Option<Self> {
            let model = std::env::var("FERGENTS_NER_MODEL").ok()?;
            let tokenizer_path = std::env::var("FERGENTS_NER_TOKENIZER").ok()?;
            Self::load(&model, &tokenizer_path).ok()
        }

        /// Load a model and its tokenizer.
        pub fn load(model_path: &str, tokenizer_path: &str) -> Result<Self, GraphError> {
            let session = Session::builder()
                .and_then(|mut b| b.commit_from_file(model_path))
                .map_err(|e| GraphError::Backend(format!("ner model: {e}")))?;
            let tokenizer = Tokenizer::from_file(tokenizer_path)
                .map_err(|e| GraphError::Backend(format!("ner tokenizer: {e}")))?;
            Ok(Self {
                session: Mutex::new(session),
                tokenizer,
                labels: DEFAULT_LABELS.iter().map(|s| s.to_string()).collect(),
                model: model_path.to_string(),
            })
        }
    }

    impl TokenClassifier for OrtTokenClassifier {
        fn name(&self) -> &'static str {
            "ner:onnx"
        }

        fn classify(&self, text: &str) -> Result<Vec<TokenLabel>, GraphError> {
            let encoding = self
                .tokenizer
                .encode(text, true)
                .map_err(|e| GraphError::Backend(format!("ner encode: {e}")))?;
            let ids: Vec<i64> = encoding.get_ids().iter().map(|&i| i as i64).collect();
            let mask: Vec<i64> = encoding
                .get_attention_mask()
                .iter()
                .map(|&i| i as i64)
                .collect();
            let len = ids.len();
            if len == 0 {
                return Ok(Vec::new());
            }

            let ids_tensor = Tensor::from_array(([1usize, len], ids))
                .map_err(|e| GraphError::Backend(format!("ner tensor: {e}")))?;
            let mask_tensor = Tensor::from_array(([1usize, len], mask))
                .map_err(|e| GraphError::Backend(format!("ner tensor: {e}")))?;

            let mut session = self
                .session
                .lock()
                .map_err(|_| GraphError::Backend("ner session lock poisoned".to_string()))?;
            let outputs = session
                .run(ort::inputs![
                    "input_ids" => ids_tensor,
                    "attention_mask" => mask_tensor,
                ])
                .map_err(|e| GraphError::Backend(format!("ner run: {e}")))?;
            let (shape, logits) = outputs[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| GraphError::Backend(format!("ner output: {e}")))?;

            let labels = shape.get(2).copied().unwrap_or(0).max(0) as usize;
            let offsets = encoding.get_offsets();
            let mut tokens = Vec::new();
            for index in 0..len {
                let (start, end) = offsets.get(index).copied().unwrap_or((0, 0));
                if start == end {
                    continue; // special/padding token
                }
                let base = index * labels;
                let row = logits.get(base..base + labels).unwrap_or(&[]);
                let best = row
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                let label = self
                    .labels
                    .get(best)
                    .cloned()
                    .unwrap_or_else(|| "O".to_string());
                if label == "O" {
                    continue;
                }
                tokens.push(TokenLabel {
                    text: text.chars().skip(start).take(end - start).collect(),
                    start: start as i64,
                    end: end as i64,
                    label,
                });
            }
            let _ = &self.model;
            Ok(tokens)
        }
    }
}
