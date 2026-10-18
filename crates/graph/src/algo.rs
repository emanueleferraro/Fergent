//! Graph algorithms (`GRAPH.md` §10): PageRank, shortest path, neighbors, and
//! subgraph extraction. Implemented in Rust over [`GraphStore`] reads so they
//! are deterministic and testable without CozoDB; the store provides the data.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use crate::error::GraphError;
use crate::model::{GraphStore, Relation, ScoredNode};

/// PageRank over the relation graph (`GRAPH.md` §10.1), best first.
pub fn pagerank(store: &dyn GraphStore, top_k: usize) -> Result<Vec<ScoredNode>, GraphError> {
    let relations = store.all_relations()?;
    let mut nodes: BTreeSet<String> = BTreeSet::new();
    let mut edges: HashMap<String, Vec<String>> = HashMap::new();
    for relation in &relations {
        nodes.insert(relation.src.clone());
        nodes.insert(relation.dst.clone());
        edges
            .entry(relation.src.clone())
            .or_default()
            .push(relation.dst.clone());
    }

    let count = nodes.len();
    if count == 0 {
        return Ok(Vec::new());
    }
    let damping = 0.85;
    let base = (1.0 - damping) / count as f64;
    let mut rank: HashMap<String, f64> = nodes
        .iter()
        .map(|n| (n.clone(), 1.0 / count as f64))
        .collect();

    for _ in 0..30 {
        let mut next: HashMap<String, f64> = nodes.iter().map(|n| (n.clone(), base)).collect();
        let mut dangling = 0.0;
        for node in &nodes {
            let links = edges.get(node).map(Vec::as_slice).unwrap_or(&[]);
            if links.is_empty() {
                dangling += rank[node];
                continue;
            }
            let share = rank[node] * damping / links.len() as f64;
            for target in links {
                *next.entry(target.clone()).or_insert(0.0) += share;
            }
        }
        let dangling_share = dangling * damping / count as f64;
        for value in next.values_mut() {
            *value += dangling_share;
        }
        rank = next;
    }

    let mut scored: Vec<ScoredNode> = rank
        .into_iter()
        .map(|(id, score)| ScoredNode { id, score })
        .collect();
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    scored.truncate(top_k);
    Ok(scored)
}

/// The shortest path between two nodes, treating relations as undirected
/// (`GRAPH.md` §10.1).
pub fn shortest_path(
    store: &dyn GraphStore,
    from: &str,
    to: &str,
) -> Result<Option<Vec<String>>, GraphError> {
    if from == to {
        return Ok(Some(vec![from.to_string()]));
    }
    let adjacency = undirected_adjacency(&store.all_relations()?);
    let mut previous: HashMap<String, String> = HashMap::new();
    let mut queue = VecDeque::new();
    queue.push_back(from.to_string());
    previous.insert(from.to_string(), from.to_string());

    while let Some(node) = queue.pop_front() {
        for neighbour in adjacency.get(&node).map(Vec::as_slice).unwrap_or(&[]) {
            if previous.contains_key(neighbour) {
                continue;
            }
            previous.insert(neighbour.clone(), node.clone());
            if neighbour == to {
                return Ok(Some(reconstruct(&previous, from, to)));
            }
            queue.push_back(neighbour.clone());
        }
    }
    Ok(None)
}

fn reconstruct(previous: &HashMap<String, String>, from: &str, to: &str) -> Vec<String> {
    let mut path = vec![to.to_string()];
    let mut current = to.to_string();
    while current != from {
        let Some(prev) = previous.get(&current) else {
            break;
        };
        current = prev.clone();
        path.push(current.clone());
    }
    path.reverse();
    path
}

fn undirected_adjacency(relations: &[Relation]) -> HashMap<String, Vec<String>> {
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
    for relation in relations {
        adjacency
            .entry(relation.src.clone())
            .or_default()
            .push(relation.dst.clone());
        adjacency
            .entry(relation.dst.clone())
            .or_default()
            .push(relation.src.clone());
    }
    adjacency
}

/// The set of node ids within `depth` hops of `seeds`, plus the relations that
/// connect them (`GRAPH.md` §10.1).
pub fn neighborhood(
    store: &dyn GraphStore,
    seeds: &[String],
    depth: u32,
) -> Result<(BTreeSet<String>, Vec<Relation>), GraphError> {
    let relations = store.all_relations()?;
    let adjacency = undirected_adjacency(&relations);

    let mut visited: BTreeSet<String> = seeds.iter().cloned().collect();
    let mut frontier: Vec<String> = seeds.to_vec();
    for _ in 0..depth {
        let mut next = Vec::new();
        for node in &frontier {
            for neighbour in adjacency.get(node).map(Vec::as_slice).unwrap_or(&[]) {
                if visited.insert(neighbour.clone()) {
                    next.push(neighbour.clone());
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }

    let in_scope: HashSet<&String> = visited.iter().collect();
    let kept: Vec<Relation> = relations
        .iter()
        .filter(|r| in_scope.contains(&r.src) && in_scope.contains(&r.dst))
        .cloned()
        .collect();
    Ok((visited, kept))
}
