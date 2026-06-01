//! Consistency checkers for `detersim`.
//!
//! This crate is deliberately small but strict: histories are structured,
//! search budgets are deterministic step counts, and candidate enumeration uses
//! input order rather than hash iteration.

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
