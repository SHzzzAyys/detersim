//! Deterministic reference protocols used by DeterSim experiments.
//!
//! The crate is intentionally below `detersim-sim`: protocol code is written
//! only against [`detersim_core::Env`]. Tests and examples choose whether to run
//! it in `SimEnv`, a future `RealEnv`, or another compatible environment.

#![allow(async_fn_in_trait)]

pub mod client;
pub mod history;
pub mod mini_raft;
pub mod primary_backup_kv;

pub use client::{
    collect_protocol_events, run_mini_raft_client, run_primary_backup_kv_client, ClientOp,
};
pub use history::{append_log_history, single_key_kv_history, RecordedOp};
pub use mini_raft::{run_mini_raft, MiniRaftConfig, RaftBugVariant, RAFT_OBSERVER_NODE};
pub use primary_backup_kv::{run_primary_backup_kv, KvBugVariant, KvConfig};
