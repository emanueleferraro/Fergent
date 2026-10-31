//! Deterministic node dispatch (spec §1.4, §3.1).
//!
//! The scheduler owns no state of its own: it reads the graph and the outcomes
//! recorded so far, and names the next node to consider. Dispatch order is
//! `(rank, node_id)`, so the same document always produces the same order.

use std::collections::BTreeMap;

use crate::document::NodeKind;
use crate::graph::Graph;

/// What a node produced, as far as control flow is concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The node succeeded and activated `ports` (empty means every port).
    Done { ports: Vec<String> },
    /// The node failed; `continued` means `on_error: continue`, so downstream
    /// nodes still run and can inspect the error marker.
    Failed { continued: bool },
    /// The node was not executed because no inbound edge activated.
    Skipped,
}

impl Outcome {
    /// A successful node that activated the given output ports.
    pub fn done(ports: impl IntoIterator<Item = impl Into<String>>) -> Outcome {
        Outcome::Done {
            ports: ports.into_iter().map(Into::into).collect(),
        }
    }

    /// A successful node that activates every output port.
    pub fn success() -> Outcome {
        Outcome::Done { ports: Vec::new() }
    }

    fn activates(&self, from_port: Option<&str>) -> bool {
        match self {
            Outcome::Done { ports } => {
                from_port.is_none()
                    || ports.is_empty()
                    || ports.iter().any(|p| p == from_port.unwrap_or_default())
            }
            Outcome::Failed { continued } => *continued,
            Outcome::Skipped => false,
        }
    }
}

/// What the engine should do with a node whose inbound edges are settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// At least one inbound edge activated.
    Run,
    /// Every inbound edge resolved but none activated.
    Skip,
    /// Some inbound source has not finished yet.
    Wait,
}

struct Inbound {
    from: String,
    from_port: Option<String>,
}

/// The top-level dispatch plan of a run.
pub struct Scheduler {
    order: Vec<String>,
    inbound: BTreeMap<String, Vec<Inbound>>,
    kinds: BTreeMap<String, NodeKind>,
}

impl Scheduler {
    /// Analyse `graph` and fix the top-level dispatch order.
    pub fn new(graph: &Graph) -> Self {
        Self::for_scope(graph, None)
    }

    /// Analyse `graph` and fix the dispatch order of one scope.
    ///
    /// `None` is the top level; `Some(loop_id)` is the body of that loop. A
    /// node belongs to a scope when [`Graph::scope`] says so, and only edges
    /// whose source is in the same scope count as inbound.
    pub fn for_scope(graph: &Graph, scope: Option<&str>) -> Self {
        let mut order: Vec<String> = graph
            .topological_order()
            .iter()
            .filter(|id| graph.scope(id) == scope)
            .cloned()
            .collect();
        order.sort_by_key(|id| (graph.rank(id).unwrap_or(usize::MAX), id.clone()));

        let mut inbound: BTreeMap<String, Vec<Inbound>> = BTreeMap::new();
        let mut kinds = BTreeMap::new();
        for id in &order {
            kinds.insert(id.clone(), graph.kind(id).cloned().unwrap_or(NodeKind::End));
            let edges: Vec<Inbound> = graph
                .incoming(id)
                .filter(|edge| graph.scope(&edge.from) == scope)
                .map(|edge| Inbound {
                    from: edge.from.clone(),
                    from_port: edge.from_port.clone(),
                })
                .collect();
            inbound.insert(id.clone(), edges);
        }

        Self {
            order,
            inbound,
            kinds,
        }
    }

    /// The dispatch order: rank, then node id.
    pub fn order(&self) -> &[String] {
        &self.order
    }

    /// Classify a node against the outcomes recorded so far.
    pub fn decision(&self, node: &str, outcomes: &BTreeMap<String, Outcome>) -> Decision {
        let Some(inbound) = self.inbound.get(node) else {
            return Decision::Run;
        };
        if inbound.is_empty() {
            return Decision::Run;
        }
        let mut settled = true;
        let mut active = false;
        for edge in inbound {
            match outcomes.get(&edge.from) {
                None => settled = false,
                Some(outcome) => {
                    if outcome.activates(edge.from_port.as_deref()) {
                        active = true;
                    }
                }
            }
        }
        if !settled {
            Decision::Wait
        } else if active {
            Decision::Run
        } else {
            Decision::Skip
        }
    }

    /// The next node to act on, in dispatch order.
    pub fn next(&self, outcomes: &BTreeMap<String, Outcome>) -> Option<(&str, Decision)> {
        for node in &self.order {
            if outcomes.contains_key(node) {
                continue;
            }
            match self.decision(node, outcomes) {
                Decision::Wait => continue,
                decision => return Some((node.as_str(), decision)),
            }
        }
        None
    }

    /// Nodes that have not produced an outcome yet.
    pub fn pending(&self, outcomes: &BTreeMap<String, Outcome>) -> Vec<String> {
        self.order
            .iter()
            .filter(|node| !outcomes.contains_key(*node))
            .cloned()
            .collect()
    }

    /// Whether a node is an `end` node.
    pub fn is_end(&self, node: &str) -> bool {
        matches!(self.kinds.get(node), Some(NodeKind::End))
    }

    /// The `end` nodes, in dispatch order.
    pub fn end_nodes(&self) -> Vec<String> {
        self.order
            .iter()
            .filter(|node| self.is_end(node))
            .cloned()
            .collect()
    }

    /// A node's kind.
    pub fn kind(&self, node: &str) -> Option<&NodeKind> {
        self.kinds.get(node)
    }
}
