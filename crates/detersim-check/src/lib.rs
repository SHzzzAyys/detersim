//! Consistency checkers for `detersim`.
//!
//! This crate is deliberately small but strict: histories are structured,
//! search budgets are deterministic step counts, and candidate enumeration uses
//! input order rather than hash iteration.

use std::collections::BTreeMap;

use detersim_core::SimTime;

/// A recorded client operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpRecord<I, O> {
    pub id: u64,
    pub process: u32,
    pub invoke: SimTime,
    pub complete: Option<SimTime>,
    pub input: I,
    pub output: Option<O>,
}

impl<I, O> OpRecord<I, O> {
    pub fn completed(
        id: u64,
        process: u32,
        input: I,
        output: O,
        call_index: u64,
        return_index: u64,
    ) -> Self {
        Self::completed_at(
            id,
            process,
            input,
            output,
            SimTime::from_nanos(call_index),
            SimTime::from_nanos(return_index),
        )
    }

    pub fn completed_at(
        id: u64,
        process: u32,
        input: I,
        output: O,
        invoke: SimTime,
        complete: SimTime,
    ) -> Self {
        Self {
            id,
            process,
            invoke,
            complete: Some(complete),
            input,
            output: Some(output),
        }
    }

    pub fn in_flight(id: u64, process: u32, input: I, call_index: u64) -> Self {
        Self {
            id,
            process,
            invoke: SimTime::from_nanos(call_index),
            complete: None,
            input,
            output: None,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.output.is_some() && self.complete.is_some()
    }
}

pub type History<I, O> = Vec<OpRecord<I, O>>;

/// A sequential specification.
pub trait Model {
    type State: Clone + Eq;
    type Input: Clone;
    type Output: Clone + Eq;

    fn init(&self) -> Self::State;

    /// Apply one operation to `state`. Return the next state and expected output,
    /// or `None` if the input is invalid in this state.
    fn step(&self, state: &Self::State, input: &Self::Input)
        -> Option<(Self::State, Self::Output)>;
}

/// Deterministic checker budget. This is a step count, never wall-clock time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckBudget {
    pub max_steps: u64,
}

impl Default for CheckBudget {
    fn default() -> Self {
        Self { max_steps: 100_000 }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinearizabilityResult {
    Linearizable {
        order: Vec<u64>,
    },
    NotLinearizable {
        reason: String,
        conflict: Option<(u64, u64)>,
        minimal_subhistory: Vec<u64>,
        explored_states: u64,
        budget_exhausted: bool,
    },
    Inconclusive {
        explored_states: u64,
        budget_exhausted: bool,
    },
}

pub type LinResult = LinearizabilityResult;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// High-level checker family used in artifacts and docs.
pub enum ConsistencyModel {
    Linearizability,
    Serializability,
    AppendLog,
}

/// Alias for the built-in append-only log linearizability model.
pub type AppendLogModel<T> = models::AppendOnlyLog<T>;

/// Stable checker artifact fields that callers can serialize or attach to a
/// debug report without matching every result variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckerStats {
    pub witness_order: Vec<u64>,
    pub conflict_ops: Vec<u64>,
    pub minimal_subhistory: Vec<u64>,
    pub explored_states: u64,
    pub budget_exhausted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Schema-stable checker artifact.
///
/// This is a compact JSON-ready view of a checker result. It deliberately
/// avoids embedding generic operation payloads so artifacts can stay stable.
pub struct CheckerArtifact {
    pub model: String,
    pub outcome: String,
    pub stats: CheckerStats,
    pub details: Vec<String>,
}

impl LinearizabilityResult {
    /// Convert a checker result into a stable stats artifact.
    pub fn checker_stats(&self) -> CheckerStats {
        match self {
            LinearizabilityResult::Linearizable { order } => CheckerStats {
                witness_order: order.clone(),
                conflict_ops: Vec::new(),
                minimal_subhistory: Vec::new(),
                explored_states: 0,
                budget_exhausted: false,
            },
            LinearizabilityResult::NotLinearizable {
                conflict,
                minimal_subhistory,
                explored_states,
                budget_exhausted,
                ..
            } => CheckerStats {
                witness_order: Vec::new(),
                conflict_ops: conflict
                    .map(|(left, right)| {
                        if left == right {
                            vec![left]
                        } else {
                            vec![left, right]
                        }
                    })
                    .unwrap_or_default(),
                minimal_subhistory: minimal_subhistory.clone(),
                explored_states: *explored_states,
                budget_exhausted: *budget_exhausted,
            },
            LinearizabilityResult::Inconclusive {
                explored_states,
                budget_exhausted,
            } => CheckerStats {
                witness_order: Vec::new(),
                conflict_ops: Vec::new(),
                minimal_subhistory: Vec::new(),
                explored_states: *explored_states,
                budget_exhausted: *budget_exhausted,
            },
        }
    }

    pub fn checker_artifact(&self, model: impl Into<String>) -> CheckerArtifact {
        let outcome = match self {
            LinearizabilityResult::Linearizable { .. } => "linearizable",
            LinearizabilityResult::NotLinearizable { .. } => "not-linearizable",
            LinearizabilityResult::Inconclusive { .. } => "inconclusive",
        };
        CheckerArtifact {
            model: model.into(),
            outcome: outcome.to_string(),
            stats: self.checker_stats(),
            details: Vec::new(),
        }
    }
}

impl CheckerArtifact {
    /// Serialize this artifact as schema-versioned JSON.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"schema_version\":3,\"model\":\"{}\",\"outcome\":\"{}\",\"witness_order\":{},\"conflict_ops\":{},\"minimal_subhistory\":{},\"explored_states\":{},\"budget_exhausted\":{},\"details\":{}}}",
            escape_json(&self.model),
            escape_json(&self.outcome),
            u64_array(&self.stats.witness_order),
            u64_array(&self.stats.conflict_ops),
            u64_array(&self.stats.minimal_subhistory),
            self.stats.explored_states,
            self.stats.budget_exhausted,
            string_array(&self.details)
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// One operation inside a compact transaction history.
pub enum TxnAction<K, V> {
    Read { key: K, value: Option<V> },
    Write { key: K, value: V },
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// A committed or incomplete transaction operation.
///
/// The Elle-lite checker currently considers committed transactions with a
/// completion time. Incomplete or aborted transactions are ignored.
pub struct TxnOpRecord<K, V> {
    pub id: u64,
    pub process: u32,
    pub invoke: SimTime,
    pub complete: Option<SimTime>,
    pub actions: Vec<TxnAction<K, V>>,
    pub committed: bool,
}

impl<K, V> TxnOpRecord<K, V> {
    /// Construct a completed committed transaction using logical nanosecond
    /// indexes for invocation and completion.
    pub fn committed(
        id: u64,
        process: u32,
        actions: Vec<TxnAction<K, V>>,
        call_index: u64,
        return_index: u64,
    ) -> Self {
        Self {
            id,
            process,
            invoke: SimTime::from_nanos(call_index),
            complete: Some(SimTime::from_nanos(return_index)),
            actions,
            committed: true,
        }
    }
}

pub type TxnHistory<K, V> = Vec<TxnOpRecord<K, V>>;

#[derive(Clone, Debug, PartialEq, Eq)]
/// Result of the small serializability checker.
///
/// `Inconclusive` is not a failure. It means the deterministic search budget was
/// exhausted before a definitive result was found.
pub enum SerializableResult {
    Serializable {
        order: Vec<u64>,
    },
    NotSerializable {
        reason: String,
        conflict: Option<(u64, u64)>,
        minimal_subhistory: Vec<u64>,
        explored_states: u64,
        budget_exhausted: bool,
    },
    Inconclusive {
        explored_states: u64,
        budget_exhausted: bool,
    },
}

impl SerializableResult {
    /// Convert the result into a schema-stable checker artifact.
    pub fn checker_artifact(&self, model: impl Into<String>) -> CheckerArtifact {
        let (outcome, stats) = match self {
            SerializableResult::Serializable { order } => (
                "serializable",
                CheckerStats {
                    witness_order: order.clone(),
                    conflict_ops: Vec::new(),
                    minimal_subhistory: Vec::new(),
                    explored_states: 0,
                    budget_exhausted: false,
                },
            ),
            SerializableResult::NotSerializable {
                conflict,
                minimal_subhistory,
                explored_states,
                budget_exhausted,
                ..
            } => (
                "not-serializable",
                CheckerStats {
                    witness_order: Vec::new(),
                    conflict_ops: conflict
                        .map(|(left, right)| vec![left, right])
                        .unwrap_or_default(),
                    minimal_subhistory: minimal_subhistory.clone(),
                    explored_states: *explored_states,
                    budget_exhausted: *budget_exhausted,
                },
            ),
            SerializableResult::Inconclusive {
                explored_states,
                budget_exhausted,
            } => (
                "inconclusive",
                CheckerStats {
                    witness_order: Vec::new(),
                    conflict_ops: Vec::new(),
                    minimal_subhistory: Vec::new(),
                    explored_states: *explored_states,
                    budget_exhausted: *budget_exhausted,
                },
            ),
        };
        CheckerArtifact {
            model: model.into(),
            outcome: outcome.to_string(),
            stats,
            details: Vec::new(),
        }
    }
}

pub fn check_linearizable<M>(
    model: &M,
    history: &[OpRecord<M::Input, M::Output>],
    max_states: usize,
) -> LinearizabilityResult
where
    M: Model,
{
    check_linearizable_with_budget(
        model,
        history,
        CheckBudget {
            max_steps: max_states as u64,
        },
    )
}

pub fn check_linearizable_with_budget<M>(
    model: &M,
    history: &[OpRecord<M::Input, M::Output>],
    budget: CheckBudget,
) -> LinearizabilityResult
where
    M: Model,
{
    let ops: Vec<_> = history.iter().filter(|op| op.is_complete()).collect();
    let search = run_search(model, &ops, budget.max_steps);
    if search.inconclusive {
        return LinearizabilityResult::Inconclusive {
            explored_states: search.explored,
            budget_exhausted: true,
        };
    }
    if search.linearizable {
        return LinearizabilityResult::Linearizable {
            order: search.order,
        };
    }

    let minimal_subhistory = minimize_failure(model, &ops, budget.max_steps);
    let conflict = match minimal_subhistory.as_slice() {
        [] => None,
        [single] => Some((*single, *single)),
        [first, rest @ ..] => Some((*first, *rest.last().expect("slice is non-empty"))),
    };
    LinearizabilityResult::NotLinearizable {
        reason: "no legal sequential order matched the observed outputs".to_string(),
        conflict,
        minimal_subhistory,
        explored_states: search.explored,
        budget_exhausted: false,
    }
}

/// Check a compact Elle-lite transaction history for serializability.
///
/// This is intentionally small: it enumerates deterministic sequential orders
/// over completed committed transactions and verifies observed reads against a
/// key/value state. It is meant for small histories and benchmark artifacts, not
/// as a complete Elle replacement.
pub fn check_serializable<K, V>(
    initial: BTreeMap<K, V>,
    history: &[TxnOpRecord<K, V>],
    budget: CheckBudget,
) -> SerializableResult
where
    K: Clone + Eq + Ord,
    V: Clone + Eq,
{
    let txns: Vec<_> = history
        .iter()
        .filter(|txn| txn.committed && txn.complete.is_some())
        .collect();
    let predecessors = txn_predecessors(&txns);
    let mut used = vec![false; txns.len()];
    let mut order = Vec::with_capacity(txns.len());
    let mut search = TxnSearch {
        txns: &txns,
        predecessors: &predecessors,
        max_steps: budget.max_steps,
        explored: 0,
        inconclusive: false,
        seen: Vec::new(),
    };
    let serializable = search.dfs(&mut used, &mut order, initial);

    if search.inconclusive {
        return SerializableResult::Inconclusive {
            explored_states: search.explored,
            budget_exhausted: true,
        };
    }
    if serializable {
        return SerializableResult::Serializable { order };
    }
    let ids: Vec<u64> = txns.iter().map(|txn| txn.id).collect();
    let conflict = match ids.as_slice() {
        [] => None,
        [single] => Some((*single, *single)),
        [first, rest @ ..] => Some((*first, *rest.last().expect("slice is non-empty"))),
    };
    SerializableResult::NotSerializable {
        reason: "no serial transaction order matched observed reads".to_string(),
        conflict,
        minimal_subhistory: ids,
        explored_states: search.explored,
        budget_exhausted: false,
    }
}

struct TxnSearch<'a, K, V> {
    txns: &'a [&'a TxnOpRecord<K, V>],
    predecessors: &'a [Vec<usize>],
    max_steps: u64,
    explored: u64,
    inconclusive: bool,
    seen: Vec<(Vec<bool>, BTreeMap<K, V>)>,
}

impl<K, V> TxnSearch<'_, K, V>
where
    K: Clone + Eq + Ord,
    V: Clone + Eq,
{
    fn dfs(&mut self, used: &mut [bool], order: &mut Vec<u64>, state: BTreeMap<K, V>) -> bool {
        if self.explored >= self.max_steps {
            self.inconclusive = true;
            return false;
        }
        if used.iter().all(|used| *used) {
            return true;
        }
        if self
            .seen
            .iter()
            .any(|(seen_used, seen_state)| seen_used.as_slice() == used && seen_state == &state)
        {
            return false;
        }
        self.seen.push((used.to_vec(), state.clone()));

        for idx in 0..self.txns.len() {
            if used[idx] || self.predecessors[idx].iter().any(|pred| !used[*pred]) {
                continue;
            }
            self.explored += 1;
            if let Some(next_state) = apply_txn(&state, self.txns[idx]) {
                used[idx] = true;
                order.push(self.txns[idx].id);
                if self.dfs(used, order, next_state) {
                    return true;
                }
                order.pop();
                used[idx] = false;
                if self.inconclusive {
                    return false;
                }
            }
        }
        false
    }
}

fn apply_txn<K, V>(state: &BTreeMap<K, V>, txn: &TxnOpRecord<K, V>) -> Option<BTreeMap<K, V>>
where
    K: Clone + Eq + Ord,
    V: Clone + Eq,
{
    let mut local = state.clone();
    for action in &txn.actions {
        match action {
            TxnAction::Read { key, value } => {
                if local.get(key) != value.as_ref() {
                    return None;
                }
            }
            TxnAction::Write { key, value } => {
                local.insert(key.clone(), value.clone());
            }
        }
    }
    Some(local)
}

fn txn_predecessors<K, V>(txns: &[&TxnOpRecord<K, V>]) -> Vec<Vec<usize>> {
    let mut preds = vec![Vec::new(); txns.len()];
    for (i, a) in txns.iter().enumerate() {
        let Some(a_complete) = a.complete else {
            continue;
        };
        for (j, b) in txns.iter().enumerate() {
            if i != j && a_complete <= b.invoke {
                preds[j].push(i);
            }
        }
    }
    preds
}

struct SearchResult {
    linearizable: bool,
    inconclusive: bool,
    explored: u64,
    order: Vec<u64>,
}

fn run_search<M>(model: &M, ops: &[&OpRecord<M::Input, M::Output>], max_steps: u64) -> SearchResult
where
    M: Model,
{
    let predecessors = predecessors(ops);
    let mut used = vec![false; ops.len()];
    let mut order = Vec::with_capacity(ops.len());
    let mut explored = 0u64;
    let mut inconclusive = false;
    let mut seen = Vec::<(Vec<bool>, M::State)>::new();
    let linearizable = dfs(
        model,
        ops,
        &predecessors,
        &mut used,
        &mut order,
        model.init(),
        max_steps,
        &mut explored,
        &mut inconclusive,
        &mut seen,
    );
    SearchResult {
        linearizable,
        inconclusive,
        explored,
        order,
    }
}

fn predecessors<I, O>(ops: &[&OpRecord<I, O>]) -> Vec<Vec<usize>> {
    let mut predecessors = vec![Vec::<usize>::new(); ops.len()];
    for (i, left) in ops.iter().enumerate() {
        let Some(left_complete) = left.complete else {
            continue;
        };
        for (j, right) in ops.iter().enumerate() {
            if i != j && left_complete <= right.invoke {
                predecessors[j].push(i);
            }
        }
    }
    predecessors
}

#[allow(clippy::too_many_arguments)]
fn dfs<M>(
    model: &M,
    ops: &[&OpRecord<M::Input, M::Output>],
    predecessors: &[Vec<usize>],
    used: &mut [bool],
    order: &mut Vec<u64>,
    state: M::State,
    max_steps: u64,
    explored: &mut u64,
    inconclusive: &mut bool,
    seen: &mut Vec<(Vec<bool>, M::State)>,
) -> bool
where
    M: Model,
{
    if order.len() == ops.len() {
        return true;
    }
    if *explored >= max_steps {
        *inconclusive = true;
        return false;
    }
    if seen
        .iter()
        .any(|(seen_used, seen_state)| seen_used.as_slice() == used && seen_state == &state)
    {
        return false;
    }
    seen.push((used.to_vec(), state.clone()));
    *explored += 1;

    for idx in 0..ops.len() {
        if used[idx] || predecessors[idx].iter().any(|pred| !used[*pred]) {
            continue;
        }
        let op = ops[idx];
        let Some(observed) = op.output.as_ref() else {
            continue;
        };
        let Some((next_state, expected)) = model.step(&state, &op.input) else {
            continue;
        };
        if &expected != observed {
            continue;
        }

        used[idx] = true;
        order.push(op.id);
        if dfs(
            model,
            ops,
            predecessors,
            used,
            order,
            next_state,
            max_steps,
            explored,
            inconclusive,
            seen,
        ) {
            return true;
        }
        order.pop();
        used[idx] = false;
    }
    false
}

fn minimize_failure<M>(
    model: &M,
    ops: &[&OpRecord<M::Input, M::Output>],
    max_steps: u64,
) -> Vec<u64>
where
    M: Model,
{
    let mut keep: Vec<usize> = (0..ops.len()).collect();
    let mut pos = 0usize;
    while pos < keep.len() {
        let mut candidate = keep.clone();
        candidate.remove(pos);
        if candidate.is_empty() {
            pos += 1;
            continue;
        }
        let candidate_ops: Vec<_> = candidate.iter().map(|idx| ops[*idx]).collect();
        let search = run_search(model, &candidate_ops, max_steps);
        if !search.linearizable && !search.inconclusive {
            keep = candidate;
        } else {
            pos += 1;
        }
    }
    keep.into_iter().map(|idx| ops[idx].id).collect()
}

/// Cheap online invariant helper.
pub fn check_invariant(name: &str, ok: bool) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(format!("invariant failed: {name}"))
    }
}

pub mod models {
    use std::collections::BTreeMap;

    use super::Model;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum RegisterInput<T> {
        Read,
        Write(T),
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum RegisterOutput<T> {
        Value(T),
        Ok,
    }

    #[derive(Clone, Debug)]
    pub struct Register<T> {
        initial: T,
    }

    impl<T> Register<T> {
        pub fn new(initial: T) -> Self {
            Self { initial }
        }
    }

    impl<T> Model for Register<T>
    where
        T: Clone + Eq,
    {
        type State = T;
        type Input = RegisterInput<T>;
        type Output = RegisterOutput<T>;

        fn init(&self) -> Self::State {
            self.initial.clone()
        }

        fn step(
            &self,
            state: &Self::State,
            input: &Self::Input,
        ) -> Option<(Self::State, Self::Output)> {
            match input {
                RegisterInput::Read => Some((state.clone(), RegisterOutput::Value(state.clone()))),
                RegisterInput::Write(value) => Some((value.clone(), RegisterOutput::Ok)),
            }
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum SingleKeyInput<T> {
        Get,
        Put(T),
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum SingleKeyOutput<T> {
        Value(Option<T>),
        Ok,
    }

    #[derive(Clone, Debug)]
    pub struct SingleKeyKv<T> {
        initial: Option<T>,
    }

    impl<T> SingleKeyKv<T> {
        pub fn new(initial: Option<T>) -> Self {
            Self { initial }
        }
    }

    impl<T> Model for SingleKeyKv<T>
    where
        T: Clone + Eq,
    {
        type State = Option<T>;
        type Input = SingleKeyInput<T>;
        type Output = SingleKeyOutput<T>;

        fn init(&self) -> Self::State {
            self.initial.clone()
        }

        fn step(
            &self,
            state: &Self::State,
            input: &Self::Input,
        ) -> Option<(Self::State, Self::Output)> {
            match input {
                SingleKeyInput::Get => Some((state.clone(), SingleKeyOutput::Value(state.clone()))),
                SingleKeyInput::Put(value) => Some((Some(value.clone()), SingleKeyOutput::Ok)),
            }
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum KvInput<K, V> {
        Get(K),
        Put(K, V),
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum KvOutput<V> {
        Value(Option<V>),
        Ok,
    }

    #[derive(Clone, Debug)]
    pub struct MultiKeyKv<K, V> {
        initial: BTreeMap<K, V>,
    }

    impl<K, V> MultiKeyKv<K, V> {
        pub fn new(initial: BTreeMap<K, V>) -> Self {
            Self { initial }
        }
    }

    impl<K, V> Model for MultiKeyKv<K, V>
    where
        K: Clone + Eq + Ord,
        V: Clone + Eq,
    {
        type State = BTreeMap<K, V>;
        type Input = KvInput<K, V>;
        type Output = KvOutput<V>;

        fn init(&self) -> Self::State {
            self.initial.clone()
        }

        fn step(
            &self,
            state: &Self::State,
            input: &Self::Input,
        ) -> Option<(Self::State, Self::Output)> {
            match input {
                KvInput::Get(key) => {
                    Some((state.clone(), KvOutput::Value(state.get(key).cloned())))
                }
                KvInput::Put(key, value) => {
                    let mut next = state.clone();
                    next.insert(key.clone(), value.clone());
                    Some((next, KvOutput::Ok))
                }
            }
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum AppendLogInput<T> {
        Append(T),
        ReadAll,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum AppendLogOutput<T> {
        Index(usize),
        Entries(Vec<T>),
    }

    #[derive(Clone, Debug)]
    pub struct AppendOnlyLog<T> {
        initial: Vec<T>,
    }

    impl<T> AppendOnlyLog<T> {
        pub fn new(initial: Vec<T>) -> Self {
            Self { initial }
        }
    }

    impl<T> Model for AppendOnlyLog<T>
    where
        T: Clone + Eq,
    {
        type State = Vec<T>;
        type Input = AppendLogInput<T>;
        type Output = AppendLogOutput<T>;

        fn init(&self) -> Self::State {
            self.initial.clone()
        }

        fn step(
            &self,
            state: &Self::State,
            input: &Self::Input,
        ) -> Option<(Self::State, Self::Output)> {
            match input {
                AppendLogInput::Append(value) => {
                    let mut next = state.clone();
                    let idx = next.len();
                    next.push(value.clone());
                    Some((next, AppendLogOutput::Index(idx)))
                }
                AppendLogInput::ReadAll => {
                    Some((state.clone(), AppendLogOutput::Entries(state.clone())))
                }
            }
        }
    }
}

fn u64_array(values: &[u64]) -> String {
    let items: Vec<String> = values.iter().map(u64::to_string).collect();
    format!("[{}]", items.join(","))
}

fn string_array(values: &[String]) -> String {
    let items: Vec<String> = values
        .iter()
        .map(|value| format!("\"{}\"", escape_json(value)))
        .collect();
    format!("[{}]", items.join(","))
}

fn escape_json(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::models::{
        AppendLogInput, AppendLogOutput, AppendOnlyLog, KvInput, KvOutput, MultiKeyKv, Register,
        RegisterInput, RegisterOutput, SingleKeyInput, SingleKeyKv, SingleKeyOutput,
    };
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn correct_register_history_is_linearizable() {
        let history = vec![
            OpRecord::completed(1, 0, RegisterInput::Write(7), RegisterOutput::Ok, 0, 1),
            OpRecord::completed(2, 1, RegisterInput::Read, RegisterOutput::Value(7), 2, 3),
        ];
        assert_eq!(
            check_linearizable(&Register::new(0), &history, 100),
            LinearizabilityResult::Linearizable { order: vec![1, 2] }
        );
    }

    #[test]
    fn stale_read_after_completed_write_is_rejected() {
        let history = vec![
            OpRecord::completed(1, 0, RegisterInput::Write(7), RegisterOutput::Ok, 0, 1),
            OpRecord::completed(2, 1, RegisterInput::Read, RegisterOutput::Value(0), 2, 3),
        ];
        match check_linearizable(&Register::new(0), &history, 100) {
            LinearizabilityResult::NotLinearizable {
                conflict,
                minimal_subhistory,
                ..
            } => {
                assert_eq!(conflict, Some((1, 2)));
                assert_eq!(minimal_subhistory, vec![1, 2]);
            }
            other => panic!("expected non-linearizable history, got {other:?}"),
        }
    }

    #[test]
    fn in_flight_operations_are_ignored() {
        let history = vec![
            OpRecord::in_flight(1, 0, RegisterInput::Write(7), 0),
            OpRecord::completed(2, 1, RegisterInput::Read, RegisterOutput::Value(0), 1, 2),
        ];
        assert_eq!(
            check_linearizable(&Register::new(0), &history, 100),
            LinearizabilityResult::Linearizable { order: vec![2] }
        );
    }

    #[test]
    fn deterministic_budget_returns_inconclusive() {
        let history = vec![
            OpRecord::completed(1, 0, RegisterInput::Write(1), RegisterOutput::Ok, 0, 10),
            OpRecord::completed(2, 1, RegisterInput::Write(2), RegisterOutput::Ok, 0, 10),
            OpRecord::completed(3, 2, RegisterInput::Read, RegisterOutput::Value(2), 0, 10),
        ];
        assert!(matches!(
            check_linearizable_with_budget(
                &Register::new(0),
                &history,
                CheckBudget { max_steps: 0 }
            ),
            LinearizabilityResult::Inconclusive {
                explored_states: 0,
                budget_exhausted: true
            }
        ));
    }

    #[test]
    fn single_key_kv_detects_stale_read() {
        let history = vec![
            OpRecord::completed(1, 0, SingleKeyInput::Put(7), SingleKeyOutput::Ok, 0, 1),
            OpRecord::completed(
                2,
                1,
                SingleKeyInput::Get,
                SingleKeyOutput::Value(None),
                2,
                3,
            ),
        ];
        assert!(matches!(
            check_linearizable(&SingleKeyKv::new(None), &history, 100),
            LinearizabilityResult::NotLinearizable {
                conflict: Some((1, 2)),
                budget_exhausted: false,
                ..
            }
        ));
    }

    #[test]
    fn concurrent_kv_reorder_can_be_legal() {
        let history = vec![
            OpRecord::completed(1, 0, KvInput::Put("x", 1), KvOutput::Ok, 0, 10),
            OpRecord::completed(2, 1, KvInput::Put("x", 2), KvOutput::Ok, 0, 10),
            OpRecord::completed(3, 2, KvInput::Get("x"), KvOutput::Value(Some(1)), 11, 12),
        ];
        assert!(matches!(
            check_linearizable(&MultiKeyKv::new(BTreeMap::new()), &history, 100),
            LinearizabilityResult::Linearizable { .. }
        ));
    }

    #[test]
    fn append_log_detects_lost_update() {
        let history = vec![
            OpRecord::completed(
                1,
                0,
                AppendLogInput::Append("a"),
                AppendLogOutput::Index(0),
                0,
                1,
            ),
            OpRecord::completed(
                2,
                1,
                AppendLogInput::Append("b"),
                AppendLogOutput::Index(1),
                2,
                3,
            ),
            OpRecord::completed(
                3,
                2,
                AppendLogInput::ReadAll,
                AppendLogOutput::Entries(vec!["a"]),
                4,
                5,
            ),
        ];
        assert!(matches!(
            check_linearizable(&AppendOnlyLog::new(Vec::new()), &history, 100),
            LinearizabilityResult::NotLinearizable {
                budget_exhausted: false,
                ..
            }
        ));
    }

    #[test]
    fn checker_stats_preserve_artifact_fields() {
        let history = vec![
            OpRecord::completed(1, 0, RegisterInput::Write(7), RegisterOutput::Ok, 0, 1),
            OpRecord::completed(2, 1, RegisterInput::Read, RegisterOutput::Value(0), 2, 3),
        ];
        let stats = check_linearizable(&Register::new(0), &history, 100).checker_stats();
        assert_eq!(stats.conflict_ops, vec![1, 2]);
        assert_eq!(stats.minimal_subhistory, vec![1, 2]);
        assert!(!stats.budget_exhausted);
    }
}
