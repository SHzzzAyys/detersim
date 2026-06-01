//! Deterministic stream helpers.
//!
//! This crate models socket-style frame delivery without using real sockets.
//! Runtime integration still belongs in `detersim-sim`; the types here are
//! pure data plus a small ordered-delivery state machine.

use std::collections::{BTreeMap, BTreeSet};

use detersim_core::NodeId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConnectionId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StreamEndpoint {
    pub node: NodeId,
    pub connection: ConnectionId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub seq: u64,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamFault {
    Drop { seq: u64 },
    Duplicate { seq: u64 },
    Delay { seq: u64, after_seq: u64 },
    Disconnect,
    Reconnect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamEvent {
    pub label: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StreamTranscript {
    pub events: Vec<StreamEvent>,
    pub delivered: Vec<Frame>,
}

impl StreamTranscript {
    pub fn record(&mut self, label: impl Into<String>) {
        self.events.push(StreamEvent {
            label: label.into(),
        });
    }

    pub fn to_history_lines(&self) -> Vec<String> {
        self.events
            .iter()
            .map(|event| event.label.clone())
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct DeterministicStream {
    source: StreamEndpoint,
    target: StreamEndpoint,
    next_send: u64,
    next_deliver: u64,
    connected: bool,
    buffered: BTreeMap<u64, Frame>,
    delivered_once: BTreeSet<u64>,
    transcript: StreamTranscript,
}

impl DeterministicStream {
    pub fn new(source: StreamEndpoint, target: StreamEndpoint) -> Self {
        Self {
            source,
            target,
            next_send: 0,
            next_deliver: 0,
            connected: true,
            buffered: BTreeMap::new(),
            delivered_once: BTreeSet::new(),
            transcript: StreamTranscript::default(),
        }
    }

    pub fn send(&mut self, bytes: impl Into<Vec<u8>>, faults: &[StreamFault]) {
        let frame = Frame {
            seq: self.next_send,
            bytes: bytes.into(),
        };
        self.next_send += 1;

        if faults
            .iter()
            .any(|fault| matches!(fault, StreamFault::Disconnect))
        {
            self.connected = false;
            self.transcript.record("stream:disconnect");
        }
        if faults
            .iter()
            .any(|fault| matches!(fault, StreamFault::Reconnect))
        {
            self.connected = true;
            self.transcript.record("stream:reconnect");
        }
        if !self.connected {
            self.transcript
                .record(format!("stream:blocked:seq={}", frame.seq));
            return;
        }
        if faults
            .iter()
            .any(|fault| matches!(fault, StreamFault::Drop { seq } if *seq == frame.seq))
        {
            self.transcript
                .record(format!("stream:drop:seq={}", frame.seq));
            return;
        }

        let duplicate = faults
            .iter()
            .any(|fault| matches!(fault, StreamFault::Duplicate { seq } if *seq == frame.seq));
        let delayed_after = faults.iter().find_map(|fault| match fault {
            StreamFault::Delay { seq, after_seq } if *seq == frame.seq => Some(*after_seq),
            _ => None,
        });

        self.buffered.insert(frame.seq, frame.clone());
        self.transcript
            .record(format!("stream:enqueue:seq={}", frame.seq));
        if duplicate {
            self.buffered.insert(frame.seq, frame.clone());
            self.transcript
                .record(format!("stream:duplicate:seq={}", frame.seq));
        }
        if let Some(after_seq) = delayed_after {
            self.transcript
                .record(format!("stream:delay:seq={}:after={after_seq}", frame.seq));
            if self.next_deliver <= after_seq {
                return;
            }
        }
        self.drain_ordered();
    }

    pub fn flush(&mut self) {
        self.drain_ordered();
    }

    pub fn transcript(&self) -> &StreamTranscript {
        &self.transcript
    }

    pub fn into_transcript(mut self) -> StreamTranscript {
        self.flush();
        self.transcript
    }

    fn drain_ordered(&mut self) {
        while let Some(frame) = self.buffered.remove(&self.next_deliver) {
            if self.delivered_once.insert(frame.seq) {
                self.transcript.record(format!(
                    "stream:deliver:{}->{}:seq={}",
                    self.source.node, self.target.node, frame.seq
                ));
                self.transcript.delivered.push(frame);
            }
            self.next_deliver += 1;
        }
    }
}

pub fn connect_pair(left: NodeId, right: NodeId, connection: ConnectionId) -> DeterministicStream {
    DeterministicStream::new(
        StreamEndpoint {
            node: left,
            connection,
        },
        StreamEndpoint {
            node: right,
            connection,
        },
    )
}
