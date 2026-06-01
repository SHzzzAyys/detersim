//! The entropy tape: the single source of *control-plane* randomness.
//!
//! Every control-plane decision (network delay, drop/keep, partition flip,
//! crash point, scheduling jitter) goes through [`EntropyTape::draw`]. In
//! `Generate` mode draws come from a seeded PRNG and are logged; in `Replay`
//! mode they are read back from a recorded tape. This single seam ties together
//! determinism, seed-based reproduction, and (later) shrinking.

use detersim_core::rng::SplitMix64;
use detersim_core::Rng;

/// A human-readable label for a draw, so traces and the future shrinker can
/// locate each decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TapeLabel {
    NetDelay,
    DropDecision,
    DuplicateDecision,
    ExtraDelay,
    CrashPoint,
    Partition,
    Restart,
    ClockSkew,
    Jitter,
    Nemesis,
    Other,
}

impl TapeLabel {
    pub fn as_str(self) -> &'static str {
        match self {
            TapeLabel::NetDelay => "net-delay",
            TapeLabel::DropDecision => "drop-decision",
            TapeLabel::DuplicateDecision => "duplicate-decision",
            TapeLabel::ExtraDelay => "extra-delay",
            TapeLabel::CrashPoint => "crash-point",
            TapeLabel::Partition => "partition",
            TapeLabel::Restart => "restart",
            TapeLabel::ClockSkew => "clock-skew",
            TapeLabel::Jitter => "jitter",
            TapeLabel::Nemesis => "nemesis",
            TapeLabel::Other => "other",
        }
    }
}

/// One control-plane tape draw with the stable label used by shrink/viz.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TapeEvent {
    pub index: usize,
    pub label: TapeLabel,
    pub value: u64,
}

/// An append-only log of control-plane random draws.
pub enum EntropyTape {
    Generate {
        rng: SplitMix64,
        log: Vec<u64>,
        events: Vec<TapeEvent>,
    },
    Replay {
        tape: Vec<u64>,
        cursor: usize,
        log: Vec<u64>,
        events: Vec<TapeEvent>,
    },
}

/// Replay/generate status captured at the end of a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TapeDiagnostics {
    pub replaying: bool,
    pub input_len: Option<usize>,
    pub cursor: usize,
    pub consumed_all: bool,
    pub exhausted: bool,
}

impl EntropyTape {
    /// Fresh tape seeded from `seed`. The control plane uses a derivation of the
    /// world seed that is disjoint from the SUT's own RNG streams.
    pub fn generate(seed: u64) -> Self {
        EntropyTape::Generate {
            rng: SplitMix64::new(seed ^ 0xA5A5_5A5A_DEAD_BEEF),
            log: Vec::new(),
            events: Vec::new(),
        }
    }

    /// Replay a previously recorded tape (e.g. a minimized failing case).
    pub fn replay(tape: Vec<u64>) -> Self {
        EntropyTape::Replay {
            tape,
            cursor: 0,
            log: Vec::new(),
            events: Vec::new(),
        }
    }

    /// Draw the next control-plane value. `label` is recorded for tooling.
    pub fn draw(&mut self, label: TapeLabel) -> u64 {
        match self {
            EntropyTape::Generate { rng, log, events } => {
                let v = rng.next_u64();
                let index = log.len();
                log.push(v);
                events.push(TapeEvent {
                    index,
                    label,
                    value: v,
                });
                v
            }
            EntropyTape::Replay {
                tape,
                cursor,
                log,
                events,
            } => {
                // Past the end of a (possibly shrunk) tape we yield 0 — a
                // deterministic, "do the calm thing" default.
                let v = tape.get(*cursor).copied().unwrap_or(0);
                let index = *cursor;
                *cursor += 1;
                log.push(v);
                events.push(TapeEvent {
                    index,
                    label,
                    value: v,
                });
                v
            }
        }
    }

    /// The sequence of values drawn so far (the recording for this run).
    pub fn log(&self) -> &[u64] {
        match self {
            EntropyTape::Generate { log, .. } | EntropyTape::Replay { log, .. } => log,
        }
    }

    /// Labeled control-plane draws made during this run.
    pub fn events(&self) -> &[TapeEvent] {
        match self {
            EntropyTape::Generate { events, .. } | EntropyTape::Replay { events, .. } => events,
        }
    }

    pub fn diagnostics(&self) -> TapeDiagnostics {
        match self {
            EntropyTape::Generate { log, .. } => TapeDiagnostics {
                replaying: false,
                input_len: None,
                cursor: log.len(),
                consumed_all: true,
                exhausted: false,
            },
            EntropyTape::Replay { tape, cursor, .. } => TapeDiagnostics {
                replaying: true,
                input_len: Some(tape.len()),
                cursor: *cursor,
                consumed_all: *cursor >= tape.len(),
                exhausted: *cursor > tape.len(),
            },
        }
    }
}
