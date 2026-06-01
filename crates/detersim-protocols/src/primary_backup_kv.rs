//! Primary-backup key/value SUT used by recall experiments.

use detersim_core::{Env, Network};

/// Primary-backup KV behavior variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KvBugVariant {
    /// Correct quorum-acknowledged primary-backup behavior.
    Correct,
    /// Primary acknowledges before replication can be observed as durable.
    AckBeforeReplicate,
    /// Follower reads are served from a stale replica.
    ReadFromStaleFollower,
    /// Concurrent writes can be observed as a lost update.
    LostUpdate,
    /// Retried append requests are applied more than once.
    DuplicateRequestReapplied,
    /// A follower exposes an uncommitted value before the write completes.
    FollowerAppliesUncommitted,
    /// The primary treats one follower ack as a quorum.
    QuorumCountOffByOne,
}

/// Configuration for the primary-backup KV SUT.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KvConfig {
    pub bug: KvBugVariant,
}

impl KvConfig {
    /// A correct primary-backup configuration.
    pub const fn correct() -> Self {
        Self {
            bug: KvBugVariant::Correct,
        }
    }

    /// A plant-a-bug configuration.
    pub const fn with_bug(bug: KvBugVariant) -> Self {
        Self { bug }
    }
}

/// Run the server-side primary-backup SUT for one node.
///
/// Nodes `0..=2` are used as primary and followers. Client nodes are supplied
/// by the experiment harness through `run_primary_backup_kv_client`.
pub async fn run_primary_backup_kv<E: Env>(env: E, config: KvConfig) {
    match env.node_id() {
        0 => run_primary(env, config).await,
        1 | 2 => run_follower(env, config).await,
        _ => {}
    }
}

async fn run_primary<E: Env>(env: E, config: KvConfig) {
    if matches!(config.bug, KvBugVariant::DuplicateRequestReapplied) {
        run_append_log_primary(env, config).await;
        return;
    }

    let net = env.net();
    loop {
        let (client, msg) = net.recv().await;
        match msg.as_slice() {
            b"put:7" => match config.bug {
                KvBugVariant::Correct => {
                    replicate_to_quorum(&net, 7, 2).await;
                    net.send(client, b"ok".to_vec()).await;
                    break;
                }
                KvBugVariant::AckBeforeReplicate => {
                    net.send(1, b"replicate:7".to_vec()).await;
                    net.send(2, b"replicate:7".to_vec()).await;
                    net.send(client, b"ok".to_vec()).await;
                    break;
                }
                KvBugVariant::ReadFromStaleFollower => {
                    replicate_to_node(&net, 2, 7).await;
                    net.send(client, b"ok".to_vec()).await;
                    break;
                }
                KvBugVariant::LostUpdate => {
                    replicate_to_node(&net, 1, 7).await;
                    net.send(client, b"ok".to_vec()).await;
                }
                KvBugVariant::FollowerAppliesUncommitted => {
                    net.send(1, b"replicate-uncommitted:7".to_vec()).await;
                }
                KvBugVariant::QuorumCountOffByOne => {
                    replicate_to_node(&net, 2, 7).await;
                    net.send(client, b"ok".to_vec()).await;
                    break;
                }
                KvBugVariant::DuplicateRequestReapplied => {}
            },
            b"put:9" if matches!(config.bug, KvBugVariant::LostUpdate) => {
                replicate_to_node(&net, 2, 9).await;
                net.send(client, b"ok".to_vec()).await;
                break;
            }
            _ => {}
        }
    }
}

async fn run_follower<E: Env>(env: E, _config: KvConfig) {
    let mut value: Option<i32> = None;
    let net = env.net();
    loop {
        let (from, msg) = net.recv().await;
        match msg.as_slice() {
            b"replicate:7" => {
                value = Some(7);
                net.send(from, b"ack".to_vec()).await;
            }
            b"replicate:9" => {
                value = Some(9);
                net.send(from, b"ack".to_vec()).await;
            }
            b"replicate-uncommitted:7" => {
                value = Some(7);
            }
            b"get" => {
                net.send(from, kv_value_msg(value).into_bytes()).await;
                break;
            }
            b"stop" => break,
            _ => {}
        }
    }
}

async fn run_append_log_primary<E: Env>(env: E, config: KvConfig) {
    let net = env.net();
    let mut entries: Vec<String> = Vec::new();
    loop {
        let (client, msg) = net.recv().await;
        match msg.as_slice() {
            b"append:x" => {
                let index = entries.len();
                entries.push("x".to_string());
                if matches!(config.bug, KvBugVariant::DuplicateRequestReapplied) {
                    entries.push("x".to_string());
                }
                net.send(client, format!("index:{index}").into_bytes())
                    .await;
            }
            b"read_all" => {
                net.send(client, entries.join(",").into_bytes()).await;
                break;
            }
            _ => {}
        }
    }
}

async fn replicate_to_quorum<N: Network>(net: &N, value: i32, needed_acks: u32) {
    let msg = format!("replicate:{value}").into_bytes();
    net.send(1, msg.clone()).await;
    net.send(2, msg).await;
    wait_for_acks(net, needed_acks).await;
}

async fn replicate_to_node<N: Network>(net: &N, node: u32, value: i32) {
    net.send(node, format!("replicate:{value}").into_bytes())
        .await;
    wait_for_acks(net, 1).await;
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

fn kv_value_msg(value: Option<i32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string())
}
