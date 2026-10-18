//! Reciprocal Rank Fusion (`SEARCH.md` §7).
//!
//! RRF combines ranked lists without score normalization:
//! `score(d) = Σ over lists L: 1 / (k + rank_L(d))`, with ranks starting at 1
//! and `k = 60` by default.

use std::collections::HashMap;

/// The default RRF constant (`SEARCH.md` §7.1).
pub const RRF_K: usize = 60;

/// A retrieval stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    /// FTS5 BM25.
    Bm25,
    /// Dense vector search.
    Vector,
    /// Sparse (BGE-M3) search.
    Sparse,
}

/// A chunk's position and native score in one stage's ranked list.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StageScore {
    /// 1-based rank within the stage.
    pub rank: usize,
    /// The stage's native score.
    pub score: f64,
}

/// One ranked list from a retrieval stage, best first.
#[derive(Debug, Clone)]
pub struct RankedList {
    /// Which stage produced it.
    pub stage: Stage,
    /// `(chunk_id, native score)` pairs, best first.
    pub hits: Vec<(String, f64)>,
}

impl RankedList {
    /// An empty list for `stage`.
    pub fn empty(stage: Stage) -> Self {
        Self {
            stage,
            hits: Vec::new(),
        }
    }
}

/// A fused result with its per-stage contributions.
#[derive(Debug, Clone, PartialEq)]
pub struct FusedHit {
    /// The chunk id.
    pub chunk_id: String,
    /// The RRF score.
    pub fusion: f64,
    /// The BM25 contribution, if present.
    pub bm25: Option<StageScore>,
    /// The vector contribution, if present.
    pub vector: Option<StageScore>,
    /// The sparse contribution, if present.
    pub sparse: Option<StageScore>,
    /// The cross-encoder rerank score, if the candidate was reranked.
    pub rerank: Option<f64>,
}

impl FusedHit {
    /// The contribution from a given stage.
    pub fn stage(&self, stage: Stage) -> Option<StageScore> {
        match stage {
            Stage::Bm25 => self.bm25,
            Stage::Vector => self.vector,
            Stage::Sparse => self.sparse,
        }
    }
}

/// Fuse ranked lists with RRF (`k = 60` by default), best first.
///
/// The output is ordered by descending fusion score; ties are broken by
/// chunk id ascending so the result is deterministic.
pub fn reciprocal_rank_fusion(lists: &[RankedList], k: usize) -> Vec<FusedHit> {
    let k = k.max(1);
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut fused: Vec<FusedHit> = Vec::new();

    for list in lists {
        for (rank0, (chunk_id, score)) in list.hits.iter().enumerate() {
            let i = *index.entry(chunk_id.clone()).or_insert_with(|| {
                fused.push(FusedHit {
                    chunk_id: chunk_id.clone(),
                    fusion: 0.0,
                    bm25: None,
                    vector: None,
                    sparse: None,
                    rerank: None,
                });
                fused.len() - 1
            });
            let stage_score = StageScore {
                rank: rank0 + 1,
                score: *score,
            };
            let hit = &mut fused[i];
            hit.fusion += 1.0 / (k + rank0 + 1) as f64;
            match list.stage {
                Stage::Bm25 => hit.bm25 = Some(stage_score),
                Stage::Vector => hit.vector = Some(stage_score),
                Stage::Sparse => hit.sparse = Some(stage_score),
            }
        }
    }

    fused.sort_by(|a, b| {
        b.fusion
            .partial_cmp(&a.fusion)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.chunk_id.cmp(&b.chunk_id))
    });
    fused
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(stage: Stage, ids: &[&str]) -> RankedList {
        RankedList {
            stage,
            hits: ids
                .iter()
                .enumerate()
                .map(|(i, id)| (id.to_string(), 1.0 - i as f64 * 0.1))
                .collect(),
        }
    }

    #[test]
    fn rrf_rewards_documents_appearing_in_both_lists() {
        let bm25 = list(Stage::Bm25, &["a", "b", "c"]);
        let vector = list(Stage::Vector, &["b", "a", "d"]);
        let fused = reciprocal_rank_fusion(&[bm25, vector], RRF_K);

        // "a" is rank 1 + rank 2; "b" is rank 2 + rank 1 → equal, tie broken by id.
        assert_eq!(fused[0].chunk_id, "a");
        assert_eq!(fused[1].chunk_id, "b");
        // Documents in only one list rank lower.
        let pos_c = fused.iter().position(|h| h.chunk_id == "c").unwrap();
        let pos_d = fused.iter().position(|h| h.chunk_id == "d").unwrap();
        assert!(pos_c >= 2 && pos_d >= 2);
        assert!(fused[0].fusion > fused[pos_c].fusion);
    }

    #[test]
    fn rrf_records_per_stage_ranks_and_scores() {
        let bm25 = list(Stage::Bm25, &["x"]);
        let vector = list(Stage::Vector, &["y", "x"]);
        let fused = reciprocal_rank_fusion(&[bm25, vector], RRF_K);
        let x = fused.iter().find(|h| h.chunk_id == "x").unwrap();
        assert_eq!(x.bm25.unwrap().rank, 1);
        assert_eq!(x.vector.unwrap().rank, 2);
        assert!(x.sparse.is_none());
    }

    #[test]
    fn rrf_of_nothing_is_empty() {
        assert!(reciprocal_rank_fusion(&[], RRF_K).is_empty());
        assert!(reciprocal_rank_fusion(&[RankedList::empty(Stage::Bm25)], RRF_K).is_empty());
    }
}
