//! `detersim-core` — the capability traits a System Under Test (SUT) is written against.
//!
//! A SUT touches the outside world **only** through an [`Env`]. In production you
//! inject a real implementation (tokio + std); in tests you inject the deterministic
//! `SimEnv` from `detersim-sim`. This makes determinism a *structural* property:
//! if the SUT only uses `E`, and `SimEnv` is deterministic, the whole run is.
//!
//! See `AGENTS.md` for the rules that protect that property.

// We deliberately use `async fn` in public traits. With a single-threaded executor
// the returned futures are `!Send`, which is correct and desirable here, so we opt
// out of the lint that nudges toward `impl Future + Send`.
#![allow(async_fn_in_trait)]

pub mod rng;
pub mod time;

pub use rng::{Rng, SplitMix64};
pub use time::SimTime;

use std::future::Future;
use std::task::Poll;
use std::time::Duration;

/// Identifies a node in the simulated cluster.
pub type NodeId = u32;

/// A network/storage payload. (`Vec<u8>` keeps `core` dependency-free; swap for
/// `bytes::Bytes` later if zero-copy matters.)
pub type Message = Vec<u8>;

/// Access to (logical) time.
pub trait Clock: Clone {
    /// Current logical time. Monotonic within a run.
    fn now(&self) -> SimTime;

    /// Suspend the current task until at least `dur` of logical time has elapsed.
    async fn sleep(&self, dur: Duration);

    /// Suspend the current task until logical time reaches `deadline`
    /// (returns immediately if it already has).
    async fn sleep_until(&self, deadline: SimTime);
}

/// Returned when a [`ClockExt::timeout`] deadline wins the race.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timeout;

/// Convenience combinators built only from the logical [`Clock`] capability.
///
/// The implementation deliberately has no `Send` bound: the simulator is
/// single-threaded, and adding `Send` here would make valid simulated futures
/// harder to write for no semantic gain.
pub trait ClockExt: Clock {
    /// Run `fut` until it completes or `dur` of logical time elapses.
    async fn timeout<F>(&self, dur: Duration, fut: F) -> Result<F::Output, Timeout>
    where
        F: Future,
    {
        let mut fut = Box::pin(fut);
        let mut timer = Box::pin(self.sleep(dur));

        std::future::poll_fn(move |cx| {
            if let Poll::Ready(out) = fut.as_mut().poll(cx) {
                return Poll::Ready(Ok(out));
            }
            if timer.as_mut().poll(cx).is_ready() {
                return Poll::Ready(Err(Timeout));
            }
            Poll::Pending
        })
        .await
    }
}

impl<T: Clock> ClockExt for T {}

/// Node-addressed, **unreliable, unordered** datagram messaging.
///
/// This is the honest primitive: the medium may drop, duplicate, delay, or
/// reorder messages. Reliability and ordering are things the SUT builds on top
/// (and which fault injection then stresses).
pub trait Network: Clone {
    /// Best-effort send to `to`. Returns once the message is handed to the medium.
    async fn send(&self, to: NodeId, msg: Message);

    /// Receive the next datagram delivered to this node: `(from, payload)`.
    async fn recv(&self) -> (NodeId, Message);
}

/// A byte-addressable storage device with explicit durability (`flush`).
///
/// In the simulator, only flushed data survives a crash. Phase 3 layers fault
/// models (lost-on-crash, torn writes, bit rot, pre-fsync reorder) on top.
pub trait Storage: Clone {
    async fn write_at(&self, offset: u64, data: &[u8]) -> std::io::Result<()>;
    async fn read_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<usize>;
    /// Make all prior writes durable.
    async fn flush(&self) -> std::io::Result<()>;
    /// Current byte length of the device.
    async fn len(&self) -> u64;

    /// Whether the device currently has zero bytes.
    async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

/// The single entry point through which a SUT touches the world.
///
/// Write a SUT as `async fn run<E: Env>(env: E)`; instantiate `E` with `SimEnv`
/// in tests or a real environment in production.
pub trait Env: Clone + 'static {
    type Clock: Clock;
    type Net: Network;
    type Storage: Storage;
    type Rng: Rng;
    /// The handle returned by [`Env::spawn`]; awaiting it yields the task's output.
    type JoinHandle<T>: Future<Output = T>;

    /// The id of the node this `Env` belongs to.
    fn node_id(&self) -> NodeId;

    fn clock(&self) -> Self::Clock;
    fn net(&self) -> Self::Net;
    fn storage(&self) -> Self::Storage;

    /// An independent, deterministic RNG stream for SUT-internal randomness.
    fn rng(&self) -> Self::Rng;

    /// Spawn a concurrent task on this node. Note: **no `Send` bound** — the
    /// executor is single-threaded by design.
    fn spawn<F>(&self, fut: F) -> Self::JoinHandle<F::Output>
    where
        F: Future + 'static;
}
