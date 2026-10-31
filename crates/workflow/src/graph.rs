//! Graph analysis for workflow documents (spec §1.1, §1.4, §3.1).
//!
//! The document is a DAG of nodes and edges. A `loop` node owns a body: the
//! node ids listed in its `body` config. Nodes owned by a loop form their own
//! scope, which must itself be acyclic; the top level is the scope of every
//! other node. Edges that cross a scope boundary are control edges and do not
//! participate in cycle detection.
//!
//! [`Graph::build`] never fails: it records what it finds (cycles, ranks, and
//! scopes) and lets [`crate::validate`] turn the findings into issues. That
//! keeps validation able to report *every* problem at once, as spec §1.1
//! requires.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::document::{Edge, LoopBody, NodeKind, WorkflowDocument};

/// An analysed workflow graph.
#[derive(Debug, Clone)]
pub struct Graph {
    kinds: BTreeMap<String, NodeKind>,
    scopes: BTreeMap<String, Option<String>>,
    edges: Vec<Edge>,
    outgoing: BTreeMap<String, Vec<usize>>,
    incoming: BTreeMap<String, Vec<usize>>,
    order: Vec<String>,
    rank: BTreeMap<String, usize>,
    cycles: Vec<Vec<String>>,
}

impl Graph {
    /// Analyse a document.
    pub fn build(doc: &WorkflowDocument) -> Graph {
        let kinds: BTreeMap<String, NodeKind> = doc
            .nodes
            .iter()
            .map(|node| (node.id.clone(), node.kind.clone()))
            .collect();

        let mut scopes: BTreeMap<String, Option<String>> =
            kinds.keys().map(|id| (id.clone(), None)).collect();
        for body in LoopBody::scopes(doc) {
            for id in body.nodes {
                if kinds.contains_key(&id) {
                    scopes.insert(id, Some(body.owner.clone()));
                }
            }
        }

        let mut outgoing: BTreeMap<String, Vec<usize>> =
            kinds.keys().map(|id| (id.clone(), Vec::new())).collect();
        let mut incoming: BTreeMap<String, Vec<usize>> =
            kinds.keys().map(|id| (id.clone(), Vec::new())).collect();
        for (index, edge) in doc.edges.iter().enumerate() {
            if kinds.contains_key(&edge.from) && kinds.contains_key(&edge.to) {
                outgoing.entry(edge.from.clone()).or_default().push(index);
                incoming.entry(edge.to.clone()).or_default().push(index);
            }
        }

        let graph = Graph {
            kinds,
            scopes,
            edges: doc.edges.clone(),
            outgoing,
            incoming,
            order: Vec::new(),
            rank: BTreeMap::new(),
            cycles: Vec::new(),
        };

        graph.analyse()
    }

    fn analyse(mut self) -> Graph {
        let mut order = Vec::new();
        let mut rank = BTreeMap::new();
        let mut cycles = Vec::new();

        for scope in self.scope_names() {
            let (sorted, mut stuck) = self.kahn(scope.as_deref());
            for (index, id) in sorted.iter().enumerate() {
                rank.insert(id.clone(), index);
            }
            order.extend(sorted);
            if !stuck.is_empty() {
                stuck.sort();
                cycles.push(stuck);
            }
        }

        self.order = order;
        self.rank = rank;
        self.cycles = cycles;
        self
    }

    /// Every distinct scope: the top level (`None`) and each loop body.
    fn scope_names(&self) -> Vec<Option<String>> {
        let mut scopes = vec![None];
        let mut owners: BTreeSet<String> = BTreeSet::new();
        for scope in self.scopes.values().flatten() {
            owners.insert(scope.clone());
        }
        scopes.extend(owners.into_iter().map(Some));
        scopes
    }

    /// Kahn's algorithm restricted to one scope.
    fn kahn(&self, scope: Option<&str>) -> (Vec<String>, Vec<String>) {
        let members: Vec<&String> = self
            .scopes
            .iter()
            .filter(|(_, owner)| owner.as_deref() == scope)
            .map(|(id, _)| id)
            .collect();

        let mut indegree: BTreeMap<&String, usize> =
            members.iter().map(|id| (*id, 0usize)).collect();
        for id in &members {
            for index in self.outgoing.get(*id).into_iter().flatten() {
                let to = &self.edges[*index].to;
                if self.scopes.get(to).map(|owner| owner.as_deref()) == Some(scope) {
                    if let Some(count) = indegree.get_mut(to) {
                        *count += 1;
                    }
                }
            }
        }

        let mut ready: VecDeque<String> = indegree
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(id, _)| (*id).clone())
            .collect();
        let mut sorted = Vec::with_capacity(members.len());

        while let Some(id) = ready.pop_front() {
            sorted.push(id.clone());
            for index in self.outgoing.get(&id).into_iter().flatten() {
                let to = &self.edges[*index].to;
                if self.scopes.get(to).map(|owner| owner.as_deref()) != Some(scope) {
                    continue;
                }
                if let Some(count) = indegree.get_mut(to) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        ready.push_back(to.clone());
                    }
                }
            }
        }

        let stuck = members
            .iter()
            .map(|id| (*id).clone())
            .filter(|id| !sorted.contains(id))
            .collect();
        (sorted, stuck)
    }

    /// The topological order of every node, scope by scope.
    pub fn topological_order(&self) -> &[String] {
        &self.order
    }

    /// A node's position in the topological order of its scope.
    pub fn rank(&self, id: &str) -> Option<usize> {
        self.rank.get(id).copied()
    }

    /// Whether the graph contains a cycle inside a single scope.
    pub fn has_cycle(&self) -> bool {
        !self.cycles.is_empty()
    }

    /// The cycles found, as sorted node-id lists, one per affected scope.
    pub fn cycles(&self) -> &[Vec<String>] {
        &self.cycles
    }

    /// Whether a node exists.
    pub fn has_node(&self, id: &str) -> bool {
        self.kinds.contains_key(id)
    }

    /// A node's kind.
    pub fn kind(&self, id: &str) -> Option<&NodeKind> {
        self.kinds.get(id)
    }

    /// The loop node that owns a node, or `None` at the top level.
    pub fn scope(&self, id: &str) -> Option<&str> {
        self.scopes.get(id).and_then(|owner| owner.as_deref())
    }

    /// Whether the node belongs to a loop body.
    pub fn is_loop_body(&self, id: &str) -> bool {
        self.scope(id).is_some()
    }

    /// The outgoing edges of a node.
    pub fn outgoing(&self, id: &str) -> impl Iterator<Item = &Edge> {
        self.outgoing
            .get(id)
            .into_iter()
            .flatten()
            .map(move |index| &self.edges[*index])
    }

    /// The incoming edges of a node.
    pub fn incoming(&self, id: &str) -> impl Iterator<Item = &Edge> {
        self.incoming
            .get(id)
            .into_iter()
            .flatten()
            .map(move |index| &self.edges[*index])
    }

    /// Node ids reachable from any `trigger` node, including the triggers.
    pub fn reachable_from_triggers(&self) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut queue: VecDeque<String> = self
            .kinds
            .iter()
            .filter(|(_, kind)| matches!(kind, NodeKind::Trigger))
            .map(|(id, _)| id.clone())
            .collect();
        while let Some(id) = queue.pop_front() {
            if !seen.insert(id.clone()) {
                continue;
            }
            for index in self.outgoing.get(&id).into_iter().flatten() {
                let to = &self.edges[*index].to;
                if !seen.contains(to) {
                    queue.push_back(to.clone());
                }
            }
        }
        seen
    }

    /// Whether `source` can reach `target`, following control edges.
    pub fn is_upstream(&self, source: &str, target: &str) -> bool {
        if source == target {
            return false;
        }
        let mut seen = BTreeSet::new();
        let mut queue: VecDeque<String> = VecDeque::from([source.to_string()]);
        while let Some(id) = queue.pop_front() {
            if !seen.insert(id.clone()) {
                continue;
            }
            if id == target {
                return true;
            }
            for index in self.outgoing.get(&id).into_iter().flatten() {
                queue.push_back(self.edges[*index].to.clone());
            }
        }
        false
    }
}

impl LoopBody {
    /// Every loop body in the document, outermost first.
    pub fn scopes(doc: &WorkflowDocument) -> Vec<LoopBody> {
        let mut bodies = Vec::new();
        for node in &doc.nodes {
            if !matches!(node.kind, NodeKind::Loop) {
                continue;
            }
            let Ok(config) = node.config_as::<crate::document::config::LoopConfig>() else {
                continue;
            };
            bodies.push(LoopBody {
                owner: node.id.clone(),
                nodes: config.body,
            });
        }
        bodies
    }
}
