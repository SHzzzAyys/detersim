//! Minimal Raft-shaped reference protocol for v1.0 recall experiments.
//!
//! This is not a production Raft implementation. It is deliberately small and
//! fixed to three replicas so DeterSim can prove recall against specific
//! safety bugs: persistence omissions, stale reads, bad commit/log matching,
//! duplicate client request handling, and partition-induced dual leadership.

use std::time::Duration;

use detersim_core::{Clock, Env, Network, Storage};

use crate::client::notify_observer;

/// Observer node used by Mini-Raft experiments to collect protocol labels.
pub const RAFT_OBSERVER_NODE: u32 = 9;

/// Mini-Raft behavior variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaftBugVariant {
    /// Correct minimal three-node Raft-shaped behavior.
    Correct,
    TermNotPersisted,
    VoteNotPersisted,
    WrongCommitRule,
    WrongLogMatching,
    DualLeaderUnderPartition,
    FollowerStaleRead,
    DuplicateClientRequest,
    ApplyBeforeCommit,
    OldTermLeaderCommitsEntry,
}

/// Configuration for the Mini-Raft reference SUT.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MiniRaftConfig {
    pub bug: RaftBugVariant,
    pub persistence_probe: bool,
}

impl MiniRaftConfig {
    /// A correct Mini-Raft reference configuration.
    pub const fn correct() -> Self {
        Self {
            bug: RaftBugVariant::Correct,
            persistence_probe: false,
        }
    }

    /// A plant-a-bug Mini-Raft configuration.
    pub const fn with_bug(bug: RaftBugVariant) -> Self {
        Self {
            bug,
            persistence_probe: false,
        }
    }

    /// A persistence-focused run that writes term/vote/log, crashes, and
    /// reports the recovered state after restart.
    pub const fn persistence_probe(bug: RaftBugVariant) -> Self {
        Self {
            bug,
            persistence_probe: true,
        }
    }
}

/// Run one Mini-Raft node.
pub async fn run_mini_raft<E: Env>(env: E, config: MiniRaftConfig) {
    match env.node_id() {
        0 => run_node_zero(env, config).await,
        1 => run_node_one(env, config).await,
        2 => run_node_two(env, config).await,
        _ => {}
    }
}

async fn run_node_zero<E: Env>(env: E, config: MiniRaftConfig) {
    if run_persistence_probe(&env, config).await {
        return;
    }

    match config.bug {
        RaftBugVariant::WrongCommitRule => {
            env.net().send(1, b"append:x".to_vec()).await;
            notify_observer(&env, "raft-bug:wrong-commit-rule").await;
        }
        RaftBugVariant::WrongLogMatching => {
            let net = env.net();
            net.send(1, b"append:prev=2:x".to_vec()).await;
            let (_from, msg) = net.recv().await;
            if msg == b"accepted-conflict" {
                notify_observer(&env, "raft-bug:wrong-log-matching").await;
            }
        }
        RaftBugVariant::DualLeaderUnderPartition => {
            notify_observer(&env, "leader:0").await;
        }
        RaftBugVariant::ApplyBeforeCommit => {
            notify_observer(&env, "raft-bug:apply-before-commit").await;
        }
        RaftBugVariant::OldTermLeaderCommitsEntry => {
            notify_observer(&env, "raft-bug:old-term-leader-commits-entry").await;
        }
        RaftBugVariant::FollowerStaleRead => run_leader_for_stale_read(env).await,
        RaftBugVariant::DuplicateClientRequest => run_duplicate_request_leader(env).await,
        _ => run_correct_leader(env).await,
    }
}

async fn run_node_one<E: Env>(env: E, config: MiniRaftConfig) {
    match config.bug {
        RaftBugVariant::WrongLogMatching => {
            let (leader, msg) = env.net().recv().await;
            if msg == b"append:prev=2:x" {
                env.net().send(leader, b"accepted-conflict".to_vec()).await;
            }
        }
        RaftBugVariant::DualLeaderUnderPartition => {
            notify_observer(&env, "leader:1").await;
        }
        RaftBugVariant::FollowerStaleRead => {
            let (client, msg) = env.net().recv().await;
            if msg == b"read" {
                env.net().send(client, b"value:none".to_vec()).await;
            }
        }
        _ => run_correct_follower(env).await,
    }
}

async fn run_node_two<E: Env>(env: E, config: MiniRaftConfig) {
    match config.bug {
        RaftBugVariant::FollowerStaleRead => {
            let (leader, msg) = env.net().recv().await;
            if msg == b"append:x" {
                env.net().send(leader, b"ack".to_vec()).await;
            }
        }
        _ => run_correct_follower(env).await,
    }
}

async fn run_persistence_probe<E: Env>(env: &E, config: MiniRaftConfig) -> bool {
    if !config.persistence_probe {
        return false;
    }

    let storage = env.storage();
    if storage.is_empty().await {
        match config.bug {
            RaftBugVariant::TermNotPersisted => {
                storage.write_at(1, b"0").await.ok();
                storage.write_at(2, b"x").await.ok();
            }
            RaftBugVariant::VoteNotPersisted => {
                storage.write_at(0, b"1").await.ok();
                storage.write_at(2, b"x").await.ok();
            }
            _ => {
                storage.write_at(0, b"1").await.ok();
                storage.write_at(1, b"0").await.ok();
                storage.write_at(2, b"x").await.ok();
            }
        }
        storage.flush().await.ok();
        env.clock().sleep(Duration::from_millis(100)).await;
        true
    } else {
        let mut bytes = [0u8; 3];
        let _ = storage.read_at(0, &mut bytes).await.ok();
        let label = if bytes == [b'1', b'0', b'x'] {
            "raft:recovered:term=1:voted=0:log=x"
        } else if bytes[0] != b'1' {
            "raft-bug:term-not-persisted"
        } else if bytes[1] != b'0' {
            "raft-bug:vote-not-persisted"
        } else {
            "raft-bug:log-not-persisted"
        };
        notify_observer(env, label).await;
        true
    }
}

async fn run_correct_leader<E: Env>(env: E) {
    let net = env.net();
    let mut value: Option<String> = None;
    loop {
        let (client, msg) = net.recv().await;
        match msg.as_slice() {
            b"client-write:x" => {
                net.send(1, b"append:x".to_vec()).await;
                net.send(2, b"append:x".to_vec()).await;
                wait_for_acks(&net, 2).await;
                value = Some("x".to_string());
                net.send(client, b"ok".to_vec()).await;
            }
            b"client-read" => {
                let response = match value.as_deref() {
                    Some("x") => b"value:x".to_vec(),
                    _ => b"value:none".to_vec(),
                };
                net.send(client, response).await;
                net.send(1, b"stop".to_vec()).await;
                net.send(2, b"stop".to_vec()).await;
                break;
            }
            _ => {}
        }
    }
}

async fn run_correct_follower<E: Env>(env: E) {
    let net = env.net();
    let mut value: Option<String> = None;
    loop {
        let (from, msg) = net.recv().await;
        match msg.as_slice() {
            b"append:x" => {
                value = Some("x".to_string());
                net.send(from, b"ack".to_vec()).await;
            }
            b"read" => {
                let response = match value.as_deref() {
                    Some("x") => b"value:x".to_vec(),
                    _ => b"value:none".to_vec(),
                };
                net.send(from, response).await;
                break;
            }
            b"stop" => break,
            _ => {}
        }
    }
}

async fn run_leader_for_stale_read<E: Env>(env: E) {
    let net = env.net();
    let (client, msg) = net.recv().await;
    if msg == b"write:x" {
        net.send(2, b"append:x".to_vec()).await;
        let _ = net.recv().await;
        net.send(client, b"ok".to_vec()).await;
    }
}

async fn run_duplicate_request_leader<E: Env>(env: E) {
    let net = env.net();
    let mut applied = 0u32;
    for _ in 0..2 {
        let (client, msg) = net.recv().await;
        if msg == b"client:req-1:append:x" {
            applied += 1;
            net.send(client, format!("applied:{applied}").into_bytes())
                .await;
        }
    }
}

async fn wait_for_acks<N: Network>(net: &N, needed_acks: u32) {
    let mut acks = 0u32;
    while acks < needed_acks {
        let (_from, msg) = net.recv().await;
        if msg == b"ack" {
            acks += 1;
        }
    }
}
