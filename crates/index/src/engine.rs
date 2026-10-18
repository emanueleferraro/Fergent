//! The hybrid search engine (`SEARCH.md` §2, §7, §8, §11, §20).
//!
//! Pipeline: query analysis → BM25 + vector legs (with filters applied before
//! retrieval) → RRF fusion → cross-encoder rerank of the top-20 → the top-N
//! results, each carrying per-stage scores and a citation.
//!
//! [`SearchEngine`] does not depend on the kernel's event bus; it emits
//! `search.executed` through the [`EventSink`] trait. Registering the
//! `search.query` command in `crates/api` is Task 15.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use fergents_storage::{Storage, Value};
use serde_json::json;

use crate::embed::Embedder;
use crate::error::IndexError;
use crate::events::EventSink;
use crate::fusion::{reciprocal_rank_fusion, FusedHit, RankedList, Stage, StageScore, RRF_K};
use crate::rerank::{Reranker, RERANK_MIN_TOKENS, RERANK_TOP_K};
use crate::search::{
    analyze_query, bm25_candidates, build_match, chunk_meta, citation, filter_predicates,
    format_date, snippet_for, tags_for, CandidateCounts, HitMetadata, SearchFilters, SearchHit,
    SearchMode, SearchRequest, SearchResponse, StageScores, CANDIDATE_K,
};
use crate::vector::VectorStore;

/// The hybrid search engine over one per-brain index.
pub struct SearchEngine {
    storage: Arc<dyn Storage>,
    brain_id: String,
    embedder: Arc<dyn Embedder>,
    vectors: Arc<dyn VectorStore>,
    reranker: Option<Arc<dyn Reranker>>,
    sink: Arc<dyn EventSink>,
}

impl SearchEngine {
    /// Create an engine for `brain_id`'s index.
    pub fn new(
        storage: Arc<dyn Storage>,
        brain_id: impl Into<String>,
        embedder: Arc<dyn Embedder>,
        vectors: Arc<dyn VectorStore>,
        reranker: Option<Arc<dyn Reranker>>,
        sink: Arc<dyn EventSink>,
    ) -> Self {
        Self {
            storage,
            brain_id: brain_id.into(),
            embedder,
            vectors,
            reranker,
            sink,
        }
    }

    /// The brain this engine searches.
    pub fn brain_id(&self) -> &str {
        &self.brain_id
    }

    /// Run a search (`search.query`, `SEARCH.md` §20) and emit `search.executed`.
    pub fn search(&self, request: &SearchRequest) -> Result<SearchResponse, IndexError> {
        let started = Instant::now();
        let analyzed = analyze_query(&request.query);
        let mut filters = analyzed.filters.clone();
        filters.absorb(&request.filters);

        // Cross-brain gate: a requested brain set that excludes this brain means
        // there is nothing to search here.
        if !self.brain_requested(request, &analyzed.brains) {
            return Ok(SearchResponse {
                results: Vec::new(),
                candidates: CandidateCounts::default(),
                latency_ms: started.elapsed().as_millis() as i64,
            });
        }

        let match_expr = build_match(&analyzed);

        if request.mode == SearchMode::Sparse {
            return Err(IndexError::InvalidQuery(
                "sparse search is not available: this brain has no sparse vectors indexed".into(),
            ));
        }

        let use_bm25 = matches!(request.mode, SearchMode::Bm25 | SearchMode::Hybrid);
        let use_vector = matches!(request.mode, SearchMode::Vector | SearchMode::Hybrid);

        let bm25 = if use_bm25 {
            bm25_candidates(&self.storage, &match_expr, &filters, CANDIDATE_K)?
        } else {
            RankedList::empty(Stage::Bm25)
        };
        let vector = if use_vector {
            self.vector_candidates(&request.query, &filters, CANDIDATE_K)?
        } else {
            RankedList::empty(Stage::Vector)
        };

        let candidates = CandidateCounts {
            bm25: bm25.hits.len(),
            vector: vector.hits.len(),
            sparse: 0,
        };

        let mut fused = match request.mode {
            SearchMode::Bm25 => single_stage(&bm25),
            SearchMode::Vector => single_stage(&vector),
            SearchMode::Hybrid => reciprocal_rank_fusion(&[bm25, vector], RRF_K),
            SearchMode::Sparse => unreachable!("sparse mode rejected above"),
        };

        self.rerank(&request.query, &analyzed, request, &mut fused)?;

        let results = self.build_hits(
            &fused,
            request.top_k,
            (!match_expr.is_empty()).then_some(match_expr.as_str()),
        )?;

        let response = SearchResponse {
            results,
            candidates,
            latency_ms: started.elapsed().as_millis() as i64,
        };
        self.emit(request, &response);
        Ok(response)
    }

    fn brain_requested(&self, request: &SearchRequest, inline: &[String]) -> bool {
        let mut brains = request.brains.clone();
        for brain in inline {
            if !brains.iter().any(|b| b == brain) {
                brains.push(brain.clone());
            }
        }
        brains.is_empty() || brains.iter().any(|b| b == &self.brain_id)
    }

    /// The vector leg: embed the query, retrieve the `k` nearest chunks, and
    /// drop candidates that fail the filters (`SEARCH.md` §6).
    fn vector_candidates(
        &self,
        query: &str,
        filters: &SearchFilters,
        k: usize,
    ) -> Result<RankedList, IndexError> {
        if self.vectors.is_empty()? {
            return Ok(RankedList::empty(Stage::Vector));
        }
        let embedding = self.embedder.embed_one(query)?;
        let hits = self.vectors.search(&embedding, k)?;
        let ids: Vec<String> = hits.iter().map(|h| h.chunk_id.clone()).collect();
        let allowed = allowed_chunks(&self.storage, &ids, filters)?;
        let ranked = hits
            .into_iter()
            .filter(|hit| allowed.contains(&hit.chunk_id))
            .map(|hit| (hit.chunk_id, similarity(hit.distance)))
            .collect();
        Ok(RankedList {
            stage: Stage::Vector,
            hits: ranked,
        })
    }

    /// Rerank the fused candidates in place when enabled and the query is not
    /// trivial (`SEARCH.md` §8.3).
    fn rerank(
        &self,
        query: &str,
        analyzed: &crate::search::AnalyzedQuery,
        request: &SearchRequest,
        fused: &mut Vec<FusedHit>,
    ) -> Result<(), IndexError> {
        let Some(reranker) = &self.reranker else {
            return Ok(());
        };
        if !request.rerank {
            return Ok(());
        }
        let tokens = analyzed.terms.len() + analyzed.phrases.len();
        if tokens < RERANK_MIN_TOKENS {
            return Ok(());
        }
        fused.truncate(RERANK_TOP_K);
        if fused.is_empty() {
            return Ok(());
        }

        let ids: Vec<String> = fused.iter().map(|h| h.chunk_id.clone()).collect();
        let meta = chunk_meta(&self.storage, &ids)?;
        let documents: Vec<String> = ids
            .iter()
            .map(|id| meta.get(id).map_or(String::new(), |m| m.content.clone()))
            .collect();
        let scores = reranker.rerank(query, &documents)?;

        for (hit, score) in fused.iter_mut().zip(scores) {
            hit.fusion = score as f64;
            hit.rerank = Some(score as f64);
        }
        // Stable sort keeps the fusion order for equal scores.
        fused.sort_by(|a, b| {
            b.rerank
                .partial_cmp(&a.rerank)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(())
    }

    /// Hydrate the final results (`SEARCH.md` §11.1).
    fn build_hits(
        &self,
        fused: &[FusedHit],
        top_k: usize,
        match_expr: Option<&str>,
    ) -> Result<Vec<SearchHit>, IndexError> {
        let limit = top_k.max(1);
        let ids: Vec<String> = fused
            .iter()
            .take(limit)
            .map(|h| h.chunk_id.clone())
            .collect();
        let meta = chunk_meta(&self.storage, &ids)?;

        let mut results = Vec::with_capacity(ids.len());
        for (rank, hit) in fused.iter().take(limit).enumerate() {
            let Some(chunk) = meta.get(&hit.chunk_id) else {
                continue;
            };
            let score = hit.rerank.unwrap_or(hit.fusion);
            let snippet = snippet_for(&self.storage, match_expr, &hit.chunk_id, &chunk.content)?;
            let created = (chunk.created_at > 0).then(|| format_date(chunk.created_at));
            let metadata = HitMetadata {
                tags: tags_for(&self.storage, &chunk.document_id)?,
                created,
                brain: self.brain_id.clone(),
            };
            results.push(SearchHit {
                chunk_id: hit.chunk_id.clone(),
                document_id: chunk.document_id.clone(),
                path: chunk.path.clone(),
                title: chunk.title.clone(),
                heading_path: chunk.heading_path.clone(),
                content: chunk.content.clone(),
                snippet,
                score,
                scores: stage_scores(hit),
                citation: citation(rank + 1, &chunk.path, chunk.heading_path.as_deref(), score),
                metadata,
            });
        }
        Ok(results)
    }

    fn emit(&self, request: &SearchRequest, response: &SearchResponse) {
        let top_score = response
            .results
            .first()
            .map_or(0.0, |hit| (hit.score * 1000.0).round() / 1000.0);
        self.sink.emit(
            "search.executed",
            json!({
                "query": request.query,
                "mode": request.mode.as_str(),
                "brain": self.brain_id,
                "latency_ms": response.latency_ms,
                "candidates": {
                    "bm25": response.candidates.bm25,
                    "vector": response.candidates.vector,
                    "sparse": response.candidates.sparse,
                },
                "results": response.results.len(),
                "top_score": top_score,
            }),
        );
    }
}

/// Convert a single stage's ranked list into fused hits (native score kept).
fn single_stage(list: &RankedList) -> Vec<FusedHit> {
    list.hits
        .iter()
        .enumerate()
        .map(|(i, (chunk_id, score))| {
            let stage_score = StageScore {
                rank: i + 1,
                score: *score,
            };
            let mut hit = FusedHit {
                chunk_id: chunk_id.clone(),
                fusion: *score,
                bm25: None,
                vector: None,
                sparse: None,
                rerank: None,
            };
            match list.stage {
                Stage::Bm25 => hit.bm25 = Some(stage_score),
                Stage::Vector => hit.vector = Some(stage_score),
                Stage::Sparse => hit.sparse = Some(stage_score),
            }
            hit
        })
        .collect()
}

fn stage_scores(hit: &FusedHit) -> StageScores {
    StageScores {
        bm25: hit.bm25.map(|s| s.score),
        vector: hit.vector.map(|s| s.score),
        sparse: hit.sparse.map(|s| s.score),
        fusion: Some(hit.fusion),
        rerank: hit.rerank,
    }
}

/// Cosine similarity from an L2 distance between unit vectors.
fn similarity(distance: f32) -> f64 {
    let d = distance as f64;
    (1.0 - d * d / 2.0).clamp(-1.0, 1.0)
}

/// The subset of `chunk_ids` that pass `filters` (filters applied before
/// ranking, `SEARCH.md` §10.3).
fn allowed_chunks(
    storage: &Arc<dyn Storage>,
    chunk_ids: &[String],
    filters: &SearchFilters,
) -> Result<HashSet<String>, IndexError> {
    let mut allowed = HashSet::new();
    if chunk_ids.is_empty() {
        return Ok(allowed);
    }
    let (predicates, mut params) = filter_predicates(filters)?;
    let placeholders = vec!["?"; chunk_ids.len()].join(", ");
    let mut sql = format!(
        "SELECT c.id AS id FROM chunks c JOIN documents d ON d.id = c.document_id \
         WHERE c.id IN ({placeholders}) AND d.deleted_at IS NULL"
    );
    for predicate in &predicates {
        sql.push_str(" AND ");
        sql.push_str(predicate);
    }
    let mut all_params: Vec<Value> = chunk_ids
        .iter()
        .map(|id| Value::String(id.clone()))
        .collect();
    all_params.append(&mut params);

    for row in &storage.query(&sql, &all_params)? {
        if let Some(id) = row.get("id").and_then(Value::as_str) {
            allowed.insert(id.to_string());
        }
    }
    Ok(allowed)
}
