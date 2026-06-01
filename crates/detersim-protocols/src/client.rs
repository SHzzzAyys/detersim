//! Deterministic client workloads for protocol experiments.

use std::time::Duration;

use detersim_core::{Clock, Env, Network, NodeId};

use crate::history::RecordedOp;
use crate::mini_raft::{MiniRaftConfig, RaftBugVariant, RAFT_OBSERVER_NODE};
use crate::primary_backup_kv::{KvBugVariant, KvConfig};

/// A protocol-level client operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientOp {
    Put(i32),
    Get { replica: NodeId },
    Append(String),
    ReadAll,
}

/// Run the primary-backup KV client workload and return structured operations.
pub async fn run_primary_backup_kv_client<E: Env>(env: E, config: KvConfig) -> Vec<RecordedOp> {
    match config.bug {
        KvBugVariant::DuplicateRequestReapplied => run_append_log_client(env).await,
        KvBugVariant::FollowerAppliesUncommitted => run_uncommitted_follower_client(env).await,
        _ => run_single_key_client(env, config).await,
    }
}

/// Run a small Mini-Raft client workload and return observer labels.
pub async fn run_mini_raft_client<E: Env>(env: E, config: MiniRaftConfig) -> Vec<String> {
    match config.bug {
        RaftBugVariant::FollowerStaleRead => run_raft_stale_read_client(env).await,
        RaftBugVariant::DuplicateClientRequest => run_raft_duplicate_client(env).await,
        RaftBugVariant::Correct => run_raft_correct_client(env).await,
        _ => Vec::new(),
    }
}

/// Collect protocol observer events from the network.
pub async fn collect_protocol_events<E: Env>(env: E, expected: usize) -> Vec<String> {
    let net = env.net();
    let mut events = Vec::new();
    while events.len() < expected {
        let (_from, msg) = net.recv().await;
        events.push(String::from_utf8_lossy(&msg).to_string());
    }
    events
}

async fn run_single_key_client<E: Env>(env: E, config: KvConfig) -> Vec<RecordedOp> {
    let clock = env.clock();
    let net = env.net();
    let mut ops = Vec::new();

    let invoke = clock.now();
    net.send(0, b"put:7".to_vec()).await;
    let (_from, ack) = net.recv().await;
    if ack == b"ok" {
        ops.push(RecordedOp::KvPut {
            id: 1,
            process: env.node_id(),
            value: 7,
            invoke,
            complete: clock.now(),
        });
    }

    if matches!(config.bug, KvBugVariant::LostUpdate) {
        let invoke = clock.now();
        net.send(0, b"put:9".to_vec()).await;
        let (_from, ack) = net.recv().await;
        if ack == b"ok" {
            ops.push(RecordedOp::KvPut {
                id: 2,
                process: env.node_id(),
                value: 9,
                invoke,
                complete: clock.now(),
            });
        }
    }

    let read_id = if matches!(config.bug, KvBugVariant::LostUpdate) {
        3
    } else {
        2
    };
    let invoke = clock.now();
    net.send(1, b"get".to_vec()).await;
    let (_from, msg) = net.recv().await;
    ops.push(RecordedOp::KvGet {
        id: read_id,
        process: env.node_id(),
        value: parse_optional_i32(&String::from_utf8_lossy(&msg)),
        invoke,
        complete: clock.now(),
    });
    ops
}

async fn run_uncommitted_follower_client<E: Env>(env: E) -> Vec<RecordedOp> {
    let clock = env.clock();
    let net = env.net();
    net.send(0, b"put:7".to_vec()).await;
    clock.sleep(Duration::from_millis(250)).await;

    let invoke = clock.now();
    net.send(1, b"get".to_vec()).await;
    let (_from, msg) = net.recv().await;
    vec![RecordedOp::KvGet {
        id: 2,
        process: env.node_id(),
        value: parse_optional_i32(&String::from_utf8_lossy(&msg)),
        invoke,
        complete: clock.now(),
    }]
}

async fn run_append_log_client<E: Env>(env: E) -> Vec<RecordedOp> {
    let clock = env.clock();
    let net = env.net();
    let mut ops = Vec::new();

    let invoke = clock.now();
    net.send(0, b"append:x".to_vec()).await;
    let (_from, msg) = net.recv().await;
    let index = parse_index(&String::from_utf8_lossy(&msg)).unwrap_or(0);
    ops.push(RecordedOp::LogAppend {
        id: 1,
        process: env.node_id(),
        value: "x".to_string(),
        index,
        invoke,
        complete: clock.now(),
    });

    let invoke = clock.now();
    net.send(0, b"read_all".to_vec()).await;
    let (_from, msg) = net.recv().await;
    ops.push(RecordedOp::LogRead {
        id: 2,
        process: env.node_id(),
        entries: parse_entries(&String::from_utf8_lossy(&msg)),
        invoke,
        complete: clock.now(),
    });
    ops
}

async fn run_raft_correct_client<E: Env>(env: E) -> Vec<String> {
    let net = env.net();
    net.send(0, b"client-write:x".to_vec()).await;
    let (_from, ack) = net.recv().await;
    if ack != b"ok" {
        return vec!["raft-bug:write-not-acked".to_string()];
    }
    net.send(0, b"client-read".to_vec()).await;
    let (_from, value) = net.recv().await;
    if value == b"value:x" {
        vec!["raft:commit:x=1".to_string()]
    } else {
        vec!["raft-bug:read-after-write".to_string()]
    }
}

async fn run_raft_stale_read_client<E: Env>(env: E) -> Vec<String> {
    let net = env.net();
    net.send(0, b"write:x".to_vec()).await;
    let (_from, ack) = net.recv().await;
    if ack != b"ok" {
        return vec!["raft-bug:follower-stale-read:write-not-acked".to_string()];
    }
    net.send(1, b"read".to_vec()).await;
    let (_from, value) = net.recv().await;
    if value == b"value:none" {
        vec!["raft-bug:follower-stale-read".to_string()]
    } else {
        Vec::new()
    }
}

async fn run_raft_duplicate_client<E: Env>(env: E) -> Vec<String> {
    let net = env.net();
    net.send(0, b"client:req-1:append:x".to_vec()).await;
    let _ = net.recv().await;
    net.send(0, b"client:req-1:append:x".to_vec()).await;
    let (_from, msg) = net.recv().await;
    if msg == b"applied:2" {
        vec!["raft-bug:duplicate-client-request".to_string()]
    } else {
        Vec::new()
    }
}

pub(crate) async fn notify_observer<E: Env>(env: &E, label: &str) {
    env.net()
        .send(RAFT_OBSERVER_NODE, label.as_bytes().to_vec())
        .await;
}

fn parse_optional_i32(value: &str) -> Option<i32> {
    (value != "none")
        .then(|| value.parse::<i32>().ok())
        .flatten()
}

fn parse_index(value: &str) -> Option<usize> {
    value.strip_prefix("index:")?.parse::<usize>().ok()
}

fn parse_entries(value: &str) -> Vec<String> {
    if value.is_empty() {
        Vec::new()
    } else {
        value.split(',').map(str::to_string).collect()
    }
}
