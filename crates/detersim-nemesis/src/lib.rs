//! Fault-injection data structures for `detersim`.
//!
//! This crate is intentionally pure: it defines plans and actions, but it does
//! not read clocks, spawn threads, touch sockets, or draw entropy by itself. A
//! simulator drives these actions from its own seeded entropy tape.

use std::collections::{BTreeMap, BTreeSet};

use detersim_core::{NodeId, SimTime};

/// A deterministic connectivity matrix over directed node pairs.
///
/// Missing nodes are considered disconnected. Registered nodes start connected
/// to every other registered node, including themselves.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConnectivityMatrix {
    nodes: BTreeSet<NodeId>,
    blocked: BTreeSet<(NodeId, NodeId)>,
}

impl ConnectivityMatrix {
    /// Create an empty matrix. Add nodes with [`ConnectivityMatrix::add_node`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a node in the matrix.
    pub fn add_node(&mut self, node: NodeId) {
        self.nodes.insert(node);
    }

    /// Registered nodes in deterministic order.
    pub fn nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes.iter().copied()
    }

    /// Returns true when `from` can send to `to`.
    pub fn is_connected(&self, from: NodeId, to: NodeId) -> bool {
        self.nodes.contains(&from)
            && self.nodes.contains(&to)
            && !self.blocked.contains(&(from, to))
    }

    /// Block one directed edge.
    pub fn block(&mut self, from: NodeId, to: NodeId) {
        self.blocked.insert((from, to));
    }

    /// Allow one directed edge again.
    pub fn allow(&mut self, from: NodeId, to: NodeId) {
        self.blocked.remove(&(from, to));
    }

    /// Set one directed edge to either connected (`up = true`) or disconnected.
    pub fn set_link(&mut self, from: NodeId, to: NodeId, up: bool) {
        self.add_node(from);
        self.add_node(to);
        if up {
            self.allow(from, to);
        } else {
            self.block(from, to);
        }
    }

    /// Restore full connectivity between all registered nodes.
    pub fn heal_all(&mut self) {
        self.blocked.clear();
    }

    /// Partition nodes into isolated groups. Cross-group traffic is blocked in
    /// both directions; traffic inside a group remains allowed.
    pub fn partition(&mut self, groups: &[Vec<NodeId>]) {
        self.heal_all();
        let mut group_of = std::collections::BTreeMap::new();
        for (idx, group) in groups.iter().enumerate() {
            for node in group {
                group_of.insert(*node, idx);
                self.add_node(*node);
            }
        }

        let nodes: Vec<NodeId> = self.nodes.iter().copied().collect();
        for from in &nodes {
            for to in &nodes {
                let same_group = group_of.get(from) == group_of.get(to);
                if !same_group {
                    self.block(*from, *to);
                }
            }
        }
    }
}

/// A deterministic, read-only snapshot exposed to nemesis plans.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldView {
    pub now: SimTime,
    pub nodes: Vec<NodeId>,
    pub crashed: BTreeSet<NodeId>,
    pub connectivity: ConnectivityMatrix,
}

impl WorldView {
    /// Create a view. Node order is normalized so plans never depend on caller
    /// collection iteration order.
    pub fn new(
        now: SimTime,
        mut nodes: Vec<NodeId>,
        crashed: BTreeSet<NodeId>,
        connectivity: ConnectivityMatrix,
    ) -> Self {
        nodes.sort_unstable();
        nodes.dedup();
        Self {
            now,
            nodes,
            crashed,
            connectivity,
        }
    }

    /// Nodes not currently crashed, in deterministic order.
    pub fn live_nodes(&self) -> Vec<NodeId> {
        self.nodes
            .iter()
            .copied()
            .filter(|node| !self.crashed.contains(node))
            .collect()
    }
}

/// A fault action applied by a simulator at a deterministic logical time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NemesisAction {
    /// Isolate groups of nodes from one another.
    Partition { groups: Vec<Vec<NodeId>> },
    /// Block one directed edge.
    AsymmetricPartition { from: NodeId, to: NodeId },
    /// Set one directed edge to connected or disconnected.
    SetLink { from: NodeId, to: NodeId, up: bool },
    /// Heal all network partitions.
    HealAll,
    /// Crash a node: volatile tasks and unflushed storage are lost.
    Crash { node: NodeId },
    /// Restart a previously crashed node by re-running its registered entrypoint.
    Restart { node: NodeId },
    /// Set a node-local clock skew in nanoseconds.
    ClockSkew { node: NodeId, offset_ns: i64 },
    /// Enable a bit-rot fault for a node's storage.
    BitRot { node: NodeId },
    /// Enable a torn-write fault for a node's storage.
    TornWrite { node: NodeId },
    /// Enable lost-on-crash storage semantics for a node.
    LostOnCrash { node: NodeId },
    /// Commit overlapping pre-fsync writes in reverse order on flush.
    PreFsyncReorder { node: NodeId },
}

/// Entropy access supplied by the simulator to nemesis plans.
pub trait NemesisDraw {
    /// Draw one deterministic control-plane value.
    fn draw(&mut self, label: &'static str) -> u64;
}

/// A deterministic plan that may emit an action when the simulator asks.
pub trait NemesisPlan {
    /// Return the next `(time, action)`, or `None` when the plan is exhausted.
    fn next_action(
        &mut self,
        view: &WorldView,
        draw: &mut dyn NemesisDraw,
    ) -> Option<(SimTime, NemesisAction)>;
}

/// A fixed-time action plan useful for tests and examples.
#[derive(Clone, Debug, Default)]
pub struct ScriptedPlan {
    actions: Vec<(SimTime, NemesisAction)>,
    cursor: usize,
}

impl ScriptedPlan {
    /// Create a plan from `(time, action)` pairs. Ties keep input order.
    pub fn new(actions: Vec<(SimTime, NemesisAction)>) -> Self {
        Self { actions, cursor: 0 }
    }
}

impl NemesisPlan for ScriptedPlan {
    fn next_action(
        &mut self,
        _view: &WorldView,
        _draw: &mut dyn NemesisDraw,
    ) -> Option<(SimTime, NemesisAction)> {
        let next = self.actions.get(self.cursor).cloned();
        self.cursor += usize::from(next.is_some());
        next
    }
}

/// A deterministic one-shot directed link fault. The target edge is selected
/// from a caller-provided candidate list using the simulator's entropy tape.
#[derive(Clone, Debug)]
pub struct RandomLinkFault {
    at: SimTime,
    candidates: Vec<(NodeId, NodeId)>,
    up: bool,
    emitted: bool,
}

impl RandomLinkFault {
    pub fn new(at: SimTime, candidates: Vec<(NodeId, NodeId)>, up: bool) -> Self {
        Self {
            at,
            candidates,
            up,
            emitted: false,
        }
    }
}

impl NemesisPlan for RandomLinkFault {
    fn next_action(
        &mut self,
        _view: &WorldView,
        draw: &mut dyn NemesisDraw,
    ) -> Option<(SimTime, NemesisAction)> {
        if self.emitted || self.candidates.is_empty() {
            return None;
        }
        self.emitted = true;
        let idx = (draw.draw("nemesis.link") as usize) % self.candidates.len();
        let (from, to) = self.candidates[idx];
        Some((
            self.at,
            NemesisAction::SetLink {
                from,
                to,
                up: self.up,
            },
        ))
    }
}

/// A bounded random partition plan. Each emitted action partitions the current
/// live nodes into two non-empty groups using tape draws, then advances by the
/// configured interval.
#[derive(Clone, Debug)]
pub struct RandomPartition {
    next_at: SimTime,
    interval_ns: u64,
    remaining: usize,
}

impl RandomPartition {
    pub fn new(start_at: SimTime, interval_ns: u64, actions: usize) -> Self {
        Self {
            next_at: start_at,
            interval_ns,
            remaining: actions,
        }
    }
}

impl NemesisPlan for RandomPartition {
    fn next_action(
        &mut self,
        view: &WorldView,
        draw: &mut dyn NemesisDraw,
    ) -> Option<(SimTime, NemesisAction)> {
        if self.remaining == 0 {
            return None;
        }
        let nodes = view.live_nodes();
        if nodes.len() < 2 {
            return None;
        }

        self.remaining -= 1;
        let at = self.next_at;
        self.next_at = self
            .next_at
            .saturating_add(std::time::Duration::from_nanos(self.interval_ns));

        let pivot = 1 + (draw.draw("nemesis.partition") as usize % (nodes.len() - 1));
        let left = nodes[..pivot].to_vec();
        let right = nodes[pivot..].to_vec();
        Some((
            at,
            NemesisAction::Partition {
                groups: vec![left, right],
            },
        ))
    }
}

/// A deterministic merge of heterogeneous plans. It keeps one pending action per
/// child plan and emits the earliest `(time, plan_index)` pair.
pub struct Composite {
    plans: Vec<Box<dyn NemesisPlan>>,
    pending: BTreeMap<usize, (SimTime, NemesisAction)>,
}

impl std::fmt::Debug for Composite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Composite")
            .field("plans", &self.plans.len())
            .field("pending", &self.pending)
            .finish()
    }
}

impl Composite {
    pub fn new(plans: Vec<Box<dyn NemesisPlan>>) -> Self {
        Self {
            plans,
            pending: BTreeMap::new(),
        }
    }
}

impl NemesisPlan for Composite {
    fn next_action(
        &mut self,
        view: &WorldView,
        draw: &mut dyn NemesisDraw,
    ) -> Option<(SimTime, NemesisAction)> {
        for (idx, plan) in self.plans.iter_mut().enumerate() {
            if self.pending.contains_key(&idx) {
                continue;
            }
            if let Some(next) = plan.next_action(view, draw) {
                self.pending.insert(idx, next);
            }
        }

        let selected = self
            .pending
            .iter()
            .min_by_key(|(idx, (time, _))| (time.as_nanos(), **idx))
            .map(|(idx, _)| *idx)?;
        self.pending.remove(&selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Draws {
        values: Vec<u64>,
        cursor: usize,
    }

    impl NemesisDraw for Draws {
        fn draw(&mut self, _label: &'static str) -> u64 {
            let value = self.values.get(self.cursor).copied().unwrap_or(0);
            self.cursor += 1;
            value
        }
    }

    #[test]
    fn partition_blocks_cross_group_traffic() {
        let mut c = ConnectivityMatrix::new();
        c.partition(&[vec![0, 1], vec![2]]);
        assert!(c.is_connected(0, 1));
        assert!(!c.is_connected(0, 2));
        assert!(!c.is_connected(2, 1));
        c.heal_all();
        assert!(c.is_connected(0, 2));
    }

    #[test]
    fn random_link_fault_draws_candidate_from_tape() {
        let view = WorldView::new(SimTime::ZERO, vec![0, 1], BTreeSet::new(), {
            let mut c = ConnectivityMatrix::new();
            c.add_node(0);
            c.add_node(1);
            c
        });
        let mut draw = Draws {
            values: vec![1],
            cursor: 0,
        };
        let mut plan = RandomLinkFault::new(SimTime::from_nanos(5), vec![(1, 0), (0, 1)], false);
        assert_eq!(
            plan.next_action(&view, &mut draw),
            Some((
                SimTime::from_nanos(5),
                NemesisAction::SetLink {
                    from: 0,
                    to: 1,
                    up: false
                }
            ))
        );
        assert_eq!(plan.next_action(&view, &mut draw), None);
    }

    #[test]
    fn composite_emits_by_time_then_plan_index() {
        let view = WorldView::new(SimTime::ZERO, vec![0, 1], BTreeSet::new(), {
            let mut c = ConnectivityMatrix::new();
            c.add_node(0);
            c.add_node(1);
            c
        });
        let mut draw = Draws {
            values: Vec::new(),
            cursor: 0,
        };
        let mut plan = Composite::new(vec![
            Box::new(ScriptedPlan::new(vec![(
                SimTime::from_nanos(20),
                NemesisAction::HealAll,
            )])),
            Box::new(ScriptedPlan::new(vec![(
                SimTime::from_nanos(10),
                NemesisAction::AsymmetricPartition { from: 0, to: 1 },
            )])),
        ]);
        assert_eq!(
            plan.next_action(&view, &mut draw),
            Some((
                SimTime::from_nanos(10),
                NemesisAction::AsymmetricPartition { from: 0, to: 1 }
            ))
        );
        assert_eq!(
            plan.next_action(&view, &mut draw),
            Some((SimTime::from_nanos(20), NemesisAction::HealAll))
        );
        assert_eq!(plan.next_action(&view, &mut draw), None);
    }
}
