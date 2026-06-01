//! `detersim-sim` -- the deterministic simulation runtime.
//!
//! All concurrency is represented as events in a single priority queue keyed by
//! `(SimTime, seq)`. `seq` is assigned by insertion order, so dispatch is a
//! deterministic total order. Every control-plane random choice is drawn from
//! the seeded entropy tape.

#![allow(async_fn_in_trait)]

use std::cell::RefCell;
use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::time::Duration;

use detersim_core::rng::SplitMix64;
use detersim_core::{Clock, Env, Message, Network, NodeId, SimTime, Storage};
use detersim_nemesis::{ConnectivityMatrix, NemesisAction, NemesisDraw, NemesisPlan, WorldView};

pub mod scenarios;
pub mod tape;

pub use tape::{TapeEvent, TapeLabel};

use tape::{EntropyTape, TapeDiagnostics};

const DEFAULT_HORIZON_NS: u64 = 60_000_000_000;
const DEFAULT_MAX_EVENTS: u64 = 5_000_000;

type TaskId = u64;
type TimerId = u64;
type BoxFut = Pin<Box<dyn Future<Output = ()> + 'static>>;
type NodeFactory = Rc<dyn Fn(SimEnv) -> BoxFut>;

#[derive(Clone, Copy, Debug)]
pub struct WorldConfig {
    pub horizon_ns: u64,
    pub max_events: u64,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            horizon_ns: DEFAULT_HORIZON_NS,
            max_events: DEFAULT_MAX_EVENTS,
        }
    }
}

#[derive(Clone)]
enum EventKind {
    PollTask(TaskId),
    TimerFire(TimerId),
    DeliverMsg {
        from: NodeId,
        to: NodeId,
        msg: Message,
    },
    Nemesis(NemesisAction),
}

#[derive(Clone)]
struct Keyed {
    time: u64,
    seq: u64,
    kind: EventKind,
}

impl PartialEq for Keyed {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time && self.seq == other.seq
    }
}

impl Eq for Keyed {}

impl PartialOrd for Keyed {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Keyed {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.time, self.seq).cmp(&(other.time, other.seq))
    }
}

struct TaskSlot {
    node: NodeId,
    future: Option<BoxFut>,
}

#[derive(Clone, Debug, Default)]
struct PageStore {
    pending: Vec<u8>,
    committed: Vec<u8>,
    write_log: Vec<(u64, Vec<u8>)>,
}

impl PageStore {
    fn write_at(&mut self, offset: u64, data: &[u8], torn: bool) {
        let mut data_len = data.len();
        if torn && data_len > 1 {
            data_len = data_len.div_ceil(2);
        }
        let written = data[..data_len].to_vec();
        apply_bytes(&mut self.pending, offset, &written);
        self.write_log.push((offset, written));
    }

    fn read_at(&self, offset: u64, out: &mut [u8]) -> usize {
        let start = offset as usize;
        let avail = self.pending.len().saturating_sub(start);
        let n = out.len().min(avail);
        out[..n].copy_from_slice(&self.pending[start..start + n]);
        n
    }

    fn flush(&mut self, pre_fsync_reorder: bool) {
        if pre_fsync_reorder && self.write_log.len() > 1 {
            let mut committed = self.committed.clone();
            for (offset, data) in self.write_log.iter().rev() {
                apply_bytes(&mut committed, *offset, data);
            }
            self.committed = committed;
            self.pending = self.committed.clone();
        } else {
            self.committed = self.pending.clone();
        }
        self.write_log.clear();
    }

    fn len(&self) -> u64 {
        self.pending.len() as u64
    }

    fn crash_recover(&mut self, bit_rot: bool) {
        self.pending = self.committed.clone();
        self.write_log.clear();
        if bit_rot {
            if let Some(first) = self.pending.first_mut() {
                *first ^= 0x01;
            }
            self.committed = self.pending.clone();
        }
    }
}

fn apply_bytes(target: &mut Vec<u8>, offset: u64, data: &[u8]) {
    let start = offset as usize;
    let end = start + data.len();
    if target.len() < end {
        target.resize(end, 0);
    }
    target[start..end].copy_from_slice(data);
}

#[derive(Clone, Debug)]
struct TimerWaiter {
    task: TaskId,
}

#[derive(Clone, Debug)]
struct FaultConfig {
    connectivity: ConnectivityMatrix,
    drop_percent: u32,
    duplicate_percent: u32,
    extra_delay_ms: u64,
    clock_skew_ns: BTreeMap<NodeId, i64>,
    bit_rot_nodes: BTreeSet<NodeId>,
    torn_write_nodes: BTreeSet<NodeId>,
    lost_on_crash_nodes: BTreeSet<NodeId>,
    pre_fsync_reorder_nodes: BTreeSet<NodeId>,
}

impl Default for FaultConfig {
    fn default() -> Self {
        Self {
            connectivity: ConnectivityMatrix::new(),
            drop_percent: 0,
            duplicate_percent: 0,
            extra_delay_ms: 0,
            clock_skew_ns: BTreeMap::new(),
            bit_rot_nodes: BTreeSet::new(),
            torn_write_nodes: BTreeSet::new(),
            lost_on_crash_nodes: BTreeSet::new(),
            pre_fsync_reorder_nodes: BTreeSet::new(),
        }
    }
}

pub struct Inner {
    now: SimTime,
    seq: u64,
    queue: BinaryHeap<Reverse<Keyed>>,
    tasks: BTreeMap<TaskId, TaskSlot>,
    next_task_id: TaskId,
    current_task: Option<TaskId>,

    nodes: BTreeSet<NodeId>,
    factories: BTreeMap<NodeId, NodeFactory>,
    crashed: BTreeSet<NodeId>,

    inboxes: BTreeMap<NodeId, VecDeque<(NodeId, Message)>>,
    recv_waiters: BTreeMap<NodeId, Vec<TaskId>>,
    timers: BTreeMap<TimerId, TimerWaiter>,
    next_timer_id: TimerId,
    storage: BTreeMap<NodeId, PageStore>,

    faults: FaultConfig,
    tape: EntropyTape,
    rng_counter: BTreeMap<NodeId, u64>,
    last_local_now: BTreeMap<NodeId, SimTime>,
    config: WorldConfig,
    seed: u64,

    trace: Vec<String>,
    nemesis_trace: Vec<String>,
    history: Vec<String>,
    dispatched: u64,
}

impl Inner {
    fn push_event(&mut self, time: SimTime, kind: EventKind) {
        let seq = self.seq;
        self.seq += 1;
        self.queue.push(Reverse(Keyed {
            time: time.as_nanos(),
            seq,
            kind,
        }));
    }

    fn enqueue_poll_now(&mut self, task: TaskId) {
        self.push_event(self.now, EventKind::PollTask(task));
    }

    fn add_task(&mut self, node: NodeId, fut: BoxFut) -> TaskId {
        let id = self.next_task_id;
        self.next_task_id += 1;
        self.tasks.insert(
            id,
            TaskSlot {
                node,
                future: Some(fut),
            },
        );
        id
    }

    fn draw_net_delay_ms(&mut self) -> u64 {
        let base = 1 + self.tape.draw(TapeLabel::NetDelay) % 100;
        if self.faults.extra_delay_ms == 0 {
            base
        } else {
            base + self.tape.draw(TapeLabel::ExtraDelay) % (self.faults.extra_delay_ms + 1)
        }
    }

    fn draw_percent(&mut self, percent: u32, label: TapeLabel) -> bool {
        percent > 0 && self.tape.draw(label) % 100 < u64::from(percent.min(100))
    }

    fn register_recv_waiter(&mut self, node: NodeId, task: TaskId) {
        let waiters = self.recv_waiters.entry(node).or_default();
        if !waiters.contains(&task) {
            waiters.push(task);
        }
    }

    fn unregister_recv_waiter(&mut self, node: NodeId, task: TaskId) {
        if let Some(waiters) = self.recv_waiters.get_mut(&node) {
            waiters.retain(|t| *t != task);
        }
    }

    fn register_timer(&mut self, task: TaskId, deadline: SimTime) -> TimerId {
        let id = self.next_timer_id;
        self.next_timer_id += 1;
        self.timers.insert(id, TimerWaiter { task });
        self.push_event(deadline, EventKind::TimerFire(id));
        id
    }

    fn cancel_timer(&mut self, timer: TimerId) {
        self.timers.remove(&timer);
    }

    fn next_sut_rng(&mut self, node: NodeId) -> SplitMix64 {
        let counter = self.rng_counter.entry(node).or_insert(0);
        let n = *counter;
        *counter += 1;
        let mix = self.seed.wrapping_mul(0xD1B5_4A32_D192_ED03)
            ^ (node as u64).rotate_left(32)
            ^ n.wrapping_mul(0x2545_F491_4F6C_DD1D);
        SplitMix64::new(mix)
    }

    fn local_now(&mut self, node: NodeId) -> SimTime {
        let skew = i128::from(*self.faults.clock_skew_ns.get(&node).unwrap_or(&0));
        let raw = i128::from(self.now.as_nanos()) + skew;
        let clamped = raw.clamp(0, i128::from(u64::MAX)) as u64;
        let candidate = SimTime::from_nanos(clamped);
        let last = self.last_local_now.entry(node).or_insert(SimTime::ZERO);
        if candidate > *last {
            *last = candidate;
        }
        *last
    }

    fn global_from_local_deadline(&self, node: NodeId, local: SimTime) -> SimTime {
        let skew = i128::from(*self.faults.clock_skew_ns.get(&node).unwrap_or(&0));
        let raw = i128::from(local.as_nanos()) - skew;
        let clamped = raw.clamp(0, i128::from(u64::MAX)) as u64;
        SimTime::from_nanos(clamped.max(self.now.as_nanos()))
    }

    fn world_view(&self) -> WorldView {
        WorldView::new(
            self.now,
            self.nodes.iter().copied().collect(),
            self.crashed.clone(),
            self.faults.connectivity.clone(),
        )
    }

    fn apply_nemesis(&mut self, action: NemesisAction) -> Option<RestartOutcome> {
        self.nemesis_trace.push(format!("{action:?}"));
        match action {
            NemesisAction::Partition { groups } => self.faults.connectivity.partition(&groups),
            NemesisAction::AsymmetricPartition { from, to } => {
                self.faults.connectivity.block(from, to);
            }
            NemesisAction::SetLink { from, to, up } => {
                self.faults.connectivity.set_link(from, to, up);
            }
            NemesisAction::HealAll => self.faults.connectivity.heal_all(),
            NemesisAction::Crash { node } => self.crash_node(node),
            NemesisAction::Restart { node } => return Some(self.restart_precheck(node)),
            NemesisAction::ClockSkew { node, offset_ns } => {
                self.faults.clock_skew_ns.insert(node, offset_ns);
            }
            NemesisAction::BitRot { node } => {
                if let Some(store) = self.storage.get_mut(&node) {
                    store.crash_recover(true);
                }
            }
            NemesisAction::TornWrite { node } => {
                self.faults.torn_write_nodes.insert(node);
            }
            NemesisAction::LostOnCrash { node } => {
                self.faults.lost_on_crash_nodes.insert(node);
            }
            NemesisAction::PreFsyncReorder { node } => {
                self.faults.pre_fsync_reorder_nodes.insert(node);
            }
        }
        None
    }

    fn crash_node(&mut self, node: NodeId) {
        if !self.nodes.contains(&node) {
            return;
        }
        self.crashed.insert(node);
        self.inboxes.remove(&node);
        self.recv_waiters.remove(&node);

        let removed: BTreeSet<TaskId> = self
            .tasks
            .iter()
            .filter_map(|(task, slot)| (slot.node == node).then_some(*task))
            .collect();
        self.tasks.retain(|_, slot| slot.node != node);
        for waiters in self.recv_waiters.values_mut() {
            waiters.retain(|task| !removed.contains(task));
        }
        self.timers
            .retain(|_, waiter| !removed.contains(&waiter.task));

        let bit_rot = self.faults.bit_rot_nodes.contains(&node);
        if self.faults.lost_on_crash_nodes.contains(&node) || bit_rot {
            self.storage.entry(node).or_default().crash_recover(bit_rot);
        }

        let old_queue = std::mem::take(&mut self.queue);
        for Reverse(event) in old_queue {
            if event_is_live_after_crash(&event.kind, node, &removed, &self.timers) {
                self.queue.push(Reverse(event));
            }
        }
    }

    fn restart_precheck(&self, node: NodeId) -> RestartOutcome {
        if !self.nodes.contains(&node) {
            RestartOutcome::NodeMissing { node }
        } else if !self.crashed.contains(&node) {
            RestartOutcome::NodeNotCrashed { node }
        } else if !self.factories.contains_key(&node) {
            RestartOutcome::MissingFactory { node }
        } else {
            RestartOutcome::Restarted { node }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestartOutcome {
    Restarted { node: NodeId },
    NodeMissing { node: NodeId },
    NodeNotCrashed { node: NodeId },
    MissingFactory { node: NodeId },
}

fn event_is_live_after_crash(
    kind: &EventKind,
    crashed_node: NodeId,
    removed_tasks: &BTreeSet<TaskId>,
    timers: &BTreeMap<TimerId, TimerWaiter>,
) -> bool {
    match kind {
        EventKind::PollTask(task) => !removed_tasks.contains(task),
        EventKind::TimerFire(timer) => timers.contains_key(timer),
        EventKind::DeliverMsg { from, to, .. } => *from != crashed_node && *to != crashed_node,
        EventKind::Nemesis(_) => true,
    }
}

fn noop_waker() -> Waker {
    fn no_op(_: *const ()) {}
    fn clone(_: *const ()) -> RawWaker {
        RawWaker::new(std::ptr::null(), &VTABLE)
    }
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, no_op, no_op, no_op);
    // SAFETY: the vtable ignores its data pointer, so a null data pointer is
    // valid and has no resources to release.
    unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
}

#[derive(Clone)]
pub struct SimEnv {
    inner: Rc<RefCell<Inner>>,
    node: NodeId,
}

#[derive(Clone)]
pub struct SimClock {
    inner: Rc<RefCell<Inner>>,
    node: NodeId,
}

#[derive(Clone)]
pub struct SimNet {
    inner: Rc<RefCell<Inner>>,
    node: NodeId,
}

#[derive(Clone)]
pub struct SimStorage {
    inner: Rc<RefCell<Inner>>,
    node: NodeId,
}

pub struct SimJoinHandle<T> {
    slot: Rc<RefCell<JoinSlot<T>>>,
    inner: Rc<RefCell<Inner>>,
    waiting_task: Option<TaskId>,
}

impl<T> Unpin for SimJoinHandle<T> {}

struct JoinSlot<T> {
    done: bool,
    output: Option<T>,
    joiners: Vec<TaskId>,
}

struct SleepFuture {
    inner: Rc<RefCell<Inner>>,
    node: NodeId,
    deadline: SimTime,
    timer: Option<TimerId>,
    done: bool,
}

impl Future for SleepFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        let mut g = this.inner.borrow_mut();
        if g.local_now(this.node) >= this.deadline {
            if let Some(timer) = this.timer.take() {
                g.cancel_timer(timer);
            }
            this.done = true;
            return Poll::Ready(());
        }
        if this
            .timer
            .is_some_and(|timer| !g.timers.contains_key(&timer))
        {
            this.timer = None;
        }
        if this.timer.is_none() {
            let task = g.current_task.expect("sleep called outside of a task");
            let global_deadline = g.global_from_local_deadline(this.node, this.deadline);
            this.timer = Some(g.register_timer(task, global_deadline));
        }
        Poll::Pending
    }
}

impl Drop for SleepFuture {
    fn drop(&mut self) {
        if self.done {
            return;
        }
        if let Some(timer) = self.timer.take() {
            if let Ok(mut g) = self.inner.try_borrow_mut() {
                g.cancel_timer(timer);
            }
        }
    }
}

struct RecvFuture {
    inner: Rc<RefCell<Inner>>,
    node: NodeId,
    registered_task: Option<TaskId>,
}

impl Future for RecvFuture {
    type Output = (NodeId, Message);

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let mut g = this.inner.borrow_mut();
        if let Some(inbox) = g.inboxes.get_mut(&this.node) {
            if let Some(msg) = inbox.pop_front() {
                if let Some(task) = this.registered_task.take() {
                    g.unregister_recv_waiter(this.node, task);
                }
                return Poll::Ready(msg);
            }
        }
        let task = g.current_task.expect("recv called outside of a task");
        g.register_recv_waiter(this.node, task);
        this.registered_task = Some(task);
        Poll::Pending
    }
}

impl Drop for RecvFuture {
    fn drop(&mut self) {
        if let Some(task) = self.registered_task.take() {
            if let Ok(mut g) = self.inner.try_borrow_mut() {
                g.unregister_recv_waiter(self.node, task);
            }
        }
    }
}

impl SimEnv {
    /// Record a deterministic, user-visible event for scenarios and checkers.
    pub fn record(&self, event: impl Into<String>) {
        self.inner.borrow_mut().history.push(event.into());
    }
}

impl Clock for SimClock {
    fn now(&self) -> SimTime {
        self.inner.borrow_mut().local_now(self.node)
    }

    async fn sleep(&self, dur: Duration) {
        let deadline = self
            .inner
            .borrow_mut()
            .local_now(self.node)
            .saturating_add(dur);
        self.sleep_until(deadline).await;
    }

    async fn sleep_until(&self, deadline: SimTime) {
        SleepFuture {
            inner: self.inner.clone(),
            node: self.node,
            deadline,
            timer: None,
            done: false,
        }
        .await;
    }
}

impl Network for SimNet {
    async fn send(&self, to: NodeId, msg: Message) {
        let mut g = self.inner.borrow_mut();
        let from = self.node;
        if g.crashed.contains(&from) {
            g.trace.push(format!("send-from-crashed:{from}->{to}"));
            return;
        }
        let delay = g.draw_net_delay_ms();
        let at = g.now.saturating_add(Duration::from_millis(delay));
        g.push_event(
            at,
            EventKind::DeliverMsg {
                from,
                to,
                msg: msg.clone(),
            },
        );
        let duplicate_percent = g.faults.duplicate_percent;
        let duplicate = g.draw_percent(duplicate_percent, TapeLabel::DuplicateDecision);
        if duplicate {
            let extra = 1 + g.tape.draw(TapeLabel::ExtraDelay) % 10;
            let at = at.saturating_add(Duration::from_millis(extra));
            g.push_event(at, EventKind::DeliverMsg { from, to, msg });
        }
    }

    async fn recv(&self) -> (NodeId, Message) {
        RecvFuture {
            inner: self.inner.clone(),
            node: self.node,
            registered_task: None,
        }
        .await
    }
}

impl Storage for SimStorage {
    async fn write_at(&self, offset: u64, data: &[u8]) -> std::io::Result<()> {
        let mut g = self.inner.borrow_mut();
        let torn = g.faults.torn_write_nodes.contains(&self.node);
        g.storage
            .entry(self.node)
            .or_default()
            .write_at(offset, data, torn);
        Ok(())
    }

    async fn read_at(&self, offset: u64, out: &mut [u8]) -> std::io::Result<usize> {
        let g = self.inner.borrow();
        let n = g
            .storage
            .get(&self.node)
            .map(|store| store.read_at(offset, out))
            .unwrap_or(0);
        Ok(n)
    }

    async fn flush(&self) -> std::io::Result<()> {
        let mut g = self.inner.borrow_mut();
        let pre_fsync_reorder = g.faults.pre_fsync_reorder_nodes.contains(&self.node);
        g.storage
            .entry(self.node)
            .or_default()
            .flush(pre_fsync_reorder);
        Ok(())
    }

    async fn len(&self) -> u64 {
        self.inner
            .borrow()
            .storage
            .get(&self.node)
            .map(PageStore::len)
            .unwrap_or(0)
    }
}

impl<T> Future for SimJoinHandle<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<T> {
        let this = self.get_mut();
        let mut slot = this.slot.borrow_mut();
        if slot.done {
            this.waiting_task = None;
            return Poll::Ready(
                slot.output
                    .take()
                    .expect("join output is taken exactly once"),
            );
        }
        let task = this
            .inner
            .borrow()
            .current_task
            .expect("join awaited outside of a task");
        if !slot.joiners.contains(&task) {
            slot.joiners.push(task);
        }
        this.waiting_task = Some(task);
        Poll::Pending
    }
}

impl<T> Drop for SimJoinHandle<T> {
    fn drop(&mut self) {
        if let Some(task) = self.waiting_task.take() {
            if let Ok(mut slot) = self.slot.try_borrow_mut() {
                slot.joiners.retain(|t| *t != task);
            }
        }
    }
}

impl Env for SimEnv {
    type Clock = SimClock;
    type Net = SimNet;
    type Storage = SimStorage;
    type Rng = SplitMix64;
    type JoinHandle<T> = SimJoinHandle<T>;

    fn node_id(&self) -> NodeId {
        self.node
    }

    fn clock(&self) -> SimClock {
        SimClock {
            inner: self.inner.clone(),
            node: self.node,
        }
    }

    fn net(&self) -> SimNet {
        SimNet {
            inner: self.inner.clone(),
            node: self.node,
        }
    }

    fn storage(&self) -> SimStorage {
        SimStorage {
            inner: self.inner.clone(),
            node: self.node,
        }
    }

    fn rng(&self) -> SplitMix64 {
        self.inner.borrow_mut().next_sut_rng(self.node)
    }

    fn spawn<F>(&self, fut: F) -> SimJoinHandle<F::Output>
    where
        F: Future + 'static,
    {
        let slot = Rc::new(RefCell::new(JoinSlot {
            done: false,
            output: None,
            joiners: Vec::new(),
        }));
        let slot_for_task = slot.clone();
        let inner_for_task = self.inner.clone();

        let wrapper = async move {
            let out = fut.await;
            let joiners = {
                let mut slot = slot_for_task.borrow_mut();
                slot.output = Some(out);
                slot.done = true;
                std::mem::take(&mut slot.joiners)
            };
            let mut g = inner_for_task.borrow_mut();
            for task in joiners {
                g.enqueue_poll_now(task);
            }
        };

        let mut g = self.inner.borrow_mut();
        let task = g.add_task(self.node, Box::pin(wrapper));
        g.enqueue_poll_now(task);
        SimJoinHandle {
            slot,
            inner: self.inner.clone(),
            waiting_task: None,
        }
    }
}

pub struct World {
    inner: Rc<RefCell<Inner>>,
}

struct TapeDraw<'a> {
    tape: &'a mut EntropyTape,
}

impl NemesisDraw for TapeDraw<'_> {
    fn draw(&mut self, label: &'static str) -> u64 {
        let tape_label = match label {
            "nemesis.partition" | "nemesis.link" => TapeLabel::Partition,
            "nemesis.restart" => TapeLabel::Restart,
            "nemesis.clock-skew" => TapeLabel::ClockSkew,
            _ => TapeLabel::Nemesis,
        };
        self.tape.draw(tape_label)
    }
}

#[derive(Clone, Debug)]
pub struct RunReport {
    pub seed: u64,
    pub trace: Vec<String>,
    pub nemesis_trace: Vec<String>,
    pub history: Vec<String>,
    pub coverage_signals: Vec<String>,
    pub tape_log: Vec<u64>,
    pub tape_events: Vec<TapeEvent>,
    pub tape_replaying: bool,
    pub tape_input_len: Option<usize>,
    pub tape_cursor: usize,
    pub tape_consumed_all: bool,
    pub tape_exhausted: bool,
    pub dispatched: u64,
    pub aborted: bool,
    pub deadlocked: bool,
    pub parked_tasks: usize,
    pub tape_log_len: usize,
}

impl World {
    pub fn new(seed: u64) -> Self {
        Self::with_config(seed, WorldConfig::default())
    }

    pub fn with_config(seed: u64, config: WorldConfig) -> Self {
        Self::from_tape(seed, config, EntropyTape::generate(seed))
    }

    pub fn replay(seed: u64, tape: Vec<u64>, config: WorldConfig) -> Self {
        Self::from_tape(seed, config, EntropyTape::replay(tape))
    }

    fn from_tape(seed: u64, config: WorldConfig, tape: EntropyTape) -> Self {
        let inner = Inner {
            now: SimTime::ZERO,
            seq: 0,
            queue: BinaryHeap::new(),
            tasks: BTreeMap::new(),
            next_task_id: 0,
            current_task: None,
            nodes: BTreeSet::new(),
            factories: BTreeMap::new(),
            crashed: BTreeSet::new(),
            inboxes: BTreeMap::new(),
            recv_waiters: BTreeMap::new(),
            timers: BTreeMap::new(),
            next_timer_id: 0,
            storage: BTreeMap::new(),
            faults: FaultConfig::default(),
            tape,
            rng_counter: BTreeMap::new(),
            last_local_now: BTreeMap::new(),
            config,
            seed,
            trace: Vec::new(),
            nemesis_trace: Vec::new(),
            history: Vec::new(),
            dispatched: 0,
        };
        Self {
            inner: Rc::new(RefCell::new(inner)),
        }
    }

    pub fn env_for(&self, node: NodeId) -> SimEnv {
        SimEnv {
            inner: self.inner.clone(),
            node,
        }
    }

    pub fn add_node<F, Fut>(&mut self, node: NodeId, f: F) -> &mut Self
    where
        F: Fn(SimEnv) -> Fut + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        let factory: NodeFactory = Rc::new(move |env| Box::pin(f(env)));
        self.add_node_factory(node, factory);
        self
    }

    pub fn add_nodes<F, Fut>(&mut self, count: NodeId, f: F) -> &mut Self
    where
        F: Fn(SimEnv) -> Fut + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        let f = Rc::new(f);
        for node in 0..count {
            let f_node = f.clone();
            let factory: NodeFactory = Rc::new(move |env| Box::pin((*f_node)(env)));
            self.add_node_factory(node, factory);
        }
        self
    }

    fn add_node_factory(&mut self, node: NodeId, factory: NodeFactory) {
        {
            let mut g = self.inner.borrow_mut();
            if !g.nodes.insert(node) {
                panic!("node {node} registered twice");
            }
            g.faults.connectivity.add_node(node);
            g.inboxes.entry(node).or_default();
            g.storage.entry(node).or_default();
            g.factories.insert(node, factory.clone());
        }
        self.spawn_registered_node(node, factory);
    }

    fn spawn_registered_node(&self, node: NodeId, factory: NodeFactory) {
        let env = self.env_for(node);
        let fut = factory(env);
        let mut g = self.inner.borrow_mut();
        let task = g.add_task(node, fut);
        g.enqueue_poll_now(task);
    }

    pub fn schedule_nemesis(&mut self, at: SimTime, action: NemesisAction) -> &mut Self {
        self.inner
            .borrow_mut()
            .push_event(at, EventKind::Nemesis(action));
        self
    }

    /// Ask a deterministic nemesis plan for up to `max_actions` future actions
    /// and enqueue them as normal simulator events. Any plan randomness is drawn
    /// through the world's entropy tape, so generated tapes can be replayed and
    /// minimized by the testkit.
    pub fn schedule_nemesis_plan<P: NemesisPlan>(
        &mut self,
        plan: &mut P,
        max_actions: usize,
    ) -> usize {
        let mut scheduled = 0usize;
        while scheduled < max_actions {
            let next = {
                let mut g = self.inner.borrow_mut();
                let view = g.world_view();
                let mut draw = TapeDraw { tape: &mut g.tape };
                plan.next_action(&view, &mut draw)
            };
            let Some((at, action)) = next else {
                break;
            };
            self.inner
                .borrow_mut()
                .push_event(at, EventKind::Nemesis(action));
            scheduled += 1;
        }
        scheduled
    }

    pub fn partition(&mut self, groups: Vec<Vec<NodeId>>) -> &mut Self {
        let _ = self
            .inner
            .borrow_mut()
            .apply_nemesis(NemesisAction::Partition { groups });
        self
    }

    pub fn heal_all(&mut self) -> &mut Self {
        let _ = self
            .inner
            .borrow_mut()
            .apply_nemesis(NemesisAction::HealAll);
        self
    }

    pub fn set_drop_percent(&mut self, percent: u32) -> &mut Self {
        self.inner.borrow_mut().faults.drop_percent = percent.min(100);
        self
    }

    pub fn set_duplicate_percent(&mut self, percent: u32) -> &mut Self {
        self.inner.borrow_mut().faults.duplicate_percent = percent.min(100);
        self
    }

    pub fn set_extra_delay_ms(&mut self, max_extra_delay_ms: u64) -> &mut Self {
        self.inner.borrow_mut().faults.extra_delay_ms = max_extra_delay_ms;
        self
    }

    pub fn set_clock_skew(&mut self, node: NodeId, offset_ns: i64) -> &mut Self {
        let _ = self
            .inner
            .borrow_mut()
            .apply_nemesis(NemesisAction::ClockSkew { node, offset_ns });
        self
    }

    pub fn crash_node(&mut self, node: NodeId) -> &mut Self {
        let _ = self
            .inner
            .borrow_mut()
            .apply_nemesis(NemesisAction::Crash { node });
        self
    }

    pub fn restart_node(&mut self, node: NodeId) -> &mut Self {
        let outcome = self.restart_node_outcome(node);
        self.inner
            .borrow_mut()
            .history
            .push(format_restart_outcome(outcome));
        self
    }

    pub fn restart_node_outcome(&mut self, node: NodeId) -> RestartOutcome {
        let factory = {
            let mut g = self.inner.borrow_mut();
            if !g.nodes.contains(&node) {
                return RestartOutcome::NodeMissing { node };
            }
            if !g.crashed.contains(&node) {
                return RestartOutcome::NodeNotCrashed { node };
            }
            let Some(factory) = g.factories.get(&node).cloned() else {
                return RestartOutcome::MissingFactory { node };
            };
            if !g.crashed.remove(&node) {
                return RestartOutcome::NodeNotCrashed { node };
            }
            factory
        };
        self.spawn_registered_node(node, factory);
        RestartOutcome::Restarted { node }
    }

    pub fn run(&mut self) -> RunReport {
        let waker = noop_waker();
        loop {
            let action = {
                let mut g = self.inner.borrow_mut();
                pop_until_poll(&mut g)
            };

            match action {
                PopResult::Quiesce | PopResult::Abort => break,
                PopResult::Restart(node) => {
                    let outcome = self.restart_node_outcome(node);
                    self.inner
                        .borrow_mut()
                        .history
                        .push(format_restart_outcome(outcome));
                }
                PopResult::Poll(task_id, mut fut) => {
                    let mut cx = Context::from_waker(&waker);
                    let polled = fut.as_mut().poll(&mut cx);

                    let mut g = self.inner.borrow_mut();
                    g.current_task = None;
                    match polled {
                        Poll::Pending => {
                            if let Some(slot) = g.tasks.get_mut(&task_id) {
                                slot.future = Some(fut);
                            }
                        }
                        Poll::Ready(()) => {
                            g.tasks.remove(&task_id);
                        }
                    }
                }
            }
        }

        let g = self.inner.borrow();
        let deadlocked = g.queue.is_empty() && !g.tasks.is_empty();
        let aborted = g.dispatched >= g.config.max_events || g.now.as_nanos() > g.config.horizon_ns;
        let tape_log = g.tape.log().to_vec();
        let tape_events = g.tape.events().to_vec();
        let coverage_signals = semantic_coverage(&g);
        let tape: TapeDiagnostics = g.tape.diagnostics();
        RunReport {
            seed: g.seed,
            trace: g.trace.clone(),
            nemesis_trace: g.nemesis_trace.clone(),
            history: g.history.clone(),
            coverage_signals,
            dispatched: g.dispatched,
            aborted,
            deadlocked,
            parked_tasks: g.tasks.len(),
            tape_log_len: tape_log.len(),
            tape_log,
            tape_events,
            tape_replaying: tape.replaying,
            tape_input_len: tape.input_len,
            tape_cursor: tape.cursor,
            tape_consumed_all: tape.consumed_all,
            tape_exhausted: tape.exhausted,
        }
    }
}

fn semantic_coverage(g: &Inner) -> Vec<String> {
    let mut signals = BTreeSet::new();
    signals.insert(
        if g.dispatched >= g.config.max_events || g.now.as_nanos() > g.config.horizon_ns {
            "outcome:aborted".to_string()
        } else if g.queue.is_empty() && !g.tasks.is_empty() {
            "outcome:deadlocked".to_string()
        } else {
            "outcome:quiesced".to_string()
        },
    );

    for event in g.tape.events() {
        signals.insert(format!("tape:{}", event.label.as_str()));
    }
    for entry in &g.nemesis_trace {
        if let Some(kind) = entry
            .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .find(|part| !part.is_empty())
        {
            signals.insert(format!("nemesis:{kind}"));
        }
    }
    for entry in &g.history {
        if let Some((kind, _rest)) = entry.split_once(':') {
            signals.insert(format!("history:{kind}"));
        } else {
            signals.insert(format!("history:{entry}"));
        }
    }
    for line in &g.trace {
        if let Some(edge) = trace_edge(line) {
            signals.insert(format!("message-edge:{edge}"));
        }
        if let Some(drop) = trace_drop_kind(line) {
            signals.insert(format!("network:{drop}"));
        }
        if line.contains("timer fire=") {
            signals.insert("timer:fire".to_string());
        }
        if line.contains("poll task=") {
            signals.insert("task:poll".to_string());
        }
        if line.contains(" nemesis ") {
            signals.insert("nemesis:event".to_string());
        }
    }

    signals.into_iter().collect()
}

fn trace_edge(line: &str) -> Option<&str> {
    let (_before, after) = line.split_once("deliver ")?;
    after.split_whitespace().next()
}

fn trace_drop_kind(line: &str) -> Option<&str> {
    let (_before, after) = line.split_once("] ")?;
    let kind = after.split_whitespace().next()?;
    kind.strip_prefix("drop-")
}

fn format_restart_outcome(outcome: RestartOutcome) -> String {
    match outcome {
        RestartOutcome::Restarted { node } => format!("restart:{node}:restarted"),
        RestartOutcome::NodeMissing { node } => format!("restart:{node}:node-missing"),
        RestartOutcome::NodeNotCrashed { node } => format!("restart:{node}:node-not-crashed"),
        RestartOutcome::MissingFactory { node } => format!("restart:{node}:missing-factory"),
    }
}

enum PopResult {
    Quiesce,
    Abort,
    Restart(NodeId),
    Poll(TaskId, BoxFut),
}

fn pop_until_poll(g: &mut Inner) -> PopResult {
    loop {
        let Reverse(event) = match g.queue.pop() {
            Some(event) => event,
            None => return PopResult::Quiesce,
        };

        g.now = SimTime::from_nanos(event.time);
        if g.now.as_nanos() > g.config.horizon_ns || g.dispatched >= g.config.max_events {
            return PopResult::Abort;
        }

        match event.kind {
            EventKind::TimerFire(timer) => {
                if let Some(waiter) = g.timers.remove(&timer) {
                    g.dispatched += 1;
                    g.trace
                        .push(render(event.time, event.seq, &EventKind::TimerFire(timer)));
                    g.enqueue_poll_now(waiter.task);
                }
            }
            EventKind::DeliverMsg { from, to, msg } => {
                let kind = EventKind::DeliverMsg {
                    from,
                    to,
                    msg: msg.clone(),
                };
                if !g.nodes.contains(&to) {
                    g.dispatched += 1;
                    g.trace.push(format!(
                        "[t={:>10} #{:>6}] drop-unregistered {from}->{to}",
                        event.time, event.seq
                    ));
                    continue;
                }
                if g.crashed.contains(&to) || g.crashed.contains(&from) {
                    g.dispatched += 1;
                    g.trace.push(format!(
                        "[t={:>10} #{:>6}] drop-crashed {from}->{to}",
                        event.time, event.seq
                    ));
                    continue;
                }
                if !g.faults.connectivity.is_connected(from, to) {
                    g.dispatched += 1;
                    g.trace.push(format!(
                        "[t={:>10} #{:>6}] drop-partition {from}->{to}",
                        event.time, event.seq
                    ));
                    continue;
                }
                if g.draw_percent(g.faults.drop_percent, TapeLabel::DropDecision) {
                    g.dispatched += 1;
                    g.trace.push(format!(
                        "[t={:>10} #{:>6}] drop-random {from}->{to}",
                        event.time, event.seq
                    ));
                    continue;
                }
                g.dispatched += 1;
                g.trace.push(render(event.time, event.seq, &kind));
                g.inboxes.entry(to).or_default().push_back((from, msg));
                let waiters = g
                    .recv_waiters
                    .get_mut(&to)
                    .map(std::mem::take)
                    .unwrap_or_default();
                for task in waiters {
                    g.enqueue_poll_now(task);
                }
            }
            EventKind::Nemesis(action) => {
                let kind = EventKind::Nemesis(action.clone());
                g.dispatched += 1;
                g.trace.push(render(event.time, event.seq, &kind));
                if let NemesisAction::Restart { node } = action {
                    g.nemesis_trace
                        .push(format!("{:?}", NemesisAction::Restart { node }));
                    return PopResult::Restart(node);
                }
                let _ = g.apply_nemesis(action);
            }
            EventKind::PollTask(task) => {
                let Some(slot) = g.tasks.get_mut(&task) else {
                    continue;
                };
                let Some(fut) = slot.future.take() else {
                    continue;
                };
                if g.crashed.contains(&slot.node) {
                    continue;
                }
                g.dispatched += 1;
                g.trace
                    .push(render(event.time, event.seq, &EventKind::PollTask(task)));
                g.current_task = Some(task);
                return PopResult::Poll(task, fut);
            }
        }
    }
}

fn render(time: u64, seq: u64, kind: &EventKind) -> String {
    match kind {
        EventKind::PollTask(task) => format!("[t={time:>10} #{seq:>6}] poll task={task}"),
        EventKind::TimerFire(timer) => format!("[t={time:>10} #{seq:>6}] timer fire={timer}"),
        EventKind::DeliverMsg { from, to, msg } => format!(
            "[t={time:>10} #{seq:>6}] deliver {from}->{to} {:?}",
            String::from_utf8_lossy(msg)
        ),
        EventKind::Nemesis(action) => format!("[t={time:>10} #{seq:>6}] nemesis {action:?}"),
    }
}
