//! The determinism meta-test — the master oracle for the whole project.
//!
//! If any non-determinism leaks into the runtime, `same_seed_byte_identical`
//! turns red. Treat a failure here as a real bug, never a flaky test to retry.
//!
//! Scale the seed range with `DST_SEED_COUNT` (default below). A nightly soak
//! should run this in `--release` with a large count.

use std::time::Duration;

use detersim_core::{Clock, Env, SimTime, Storage};
use detersim_nemesis::NemesisAction;
use detersim_sim::scenarios::{
    bitrot_wal_world, gossip_world, partitioned_dual_leader_world, pingpong_world,
    pingpong_world_replay, spawn_demo_world, timeout_cancel_world, timeout_world, toy_raft_world,
    wal_recovery_world,
};
use detersim_sim::{SimEnv, World, WorldConfig};

fn seed_count(default: u64) -> u64 {
    std::env::var("DST_SEED_COUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

#[test]
fn same_seed_byte_identical() {
    for seed in 0..seed_count(200) {
        let a = pingpong_world(seed);
        let b = pingpong_world(seed);
        assert_eq!(a.trace, b.trace, "trace diverged for seed {seed}");
        // Byte-level identity too (this is what would hold across process restarts).
        assert_eq!(
            a.trace.join("\n").into_bytes(),
            b.trace.join("\n").into_bytes(),
            "byte-level trace diverged for seed {seed}"
        );
    }
}

#[test]
fn replay_same_tape_is_byte_identical() {
    for seed in 0..seed_count(200) {
        let generated = pingpong_world(seed);
        let replayed = pingpong_world_replay(seed, generated.tape_log.clone());
        assert!(
            replayed.tape_replaying,
            "not in replay mode for seed {seed}"
        );
        assert_eq!(replayed.tape_input_len, Some(generated.tape_log.len()));
        assert!(
            replayed.tape_consumed_all,
            "replay left tape unread for seed {seed}"
        );
        assert!(
            !replayed.tape_exhausted,
            "replay exhausted tape for seed {seed}"
        );
        assert_eq!(
            generated.trace, replayed.trace,
            "replay trace diverged for seed {seed}"
        );
        assert_eq!(
            generated.history, replayed.history,
            "replay history diverged for seed {seed}"
        );
        assert_eq!(
            generated.nemesis_trace, replayed.nemesis_trace,
            "replay nemesis trace diverged for seed {seed}"
        );
    }
}

#[test]
fn no_false_deadlock_and_terminates() {
    for seed in 0..seed_count(200) {
        let r = pingpong_world(seed);
        assert!(!r.deadlocked, "false deadlock at seed {seed}");
        assert!(!r.aborted, "hit a safety ceiling at seed {seed}");
        assert_eq!(r.parked_tasks, 0, "leftover parked tasks at seed {seed}");
        assert!(r.dispatched > 0, "nothing happened at seed {seed}");
    }
}

#[test]
fn seeds_actually_vary() {
    // Guards against an "accidentally constant" simulation that ignores entropy:
    // distinct seeds should generally produce distinct traces.
    let mut distinct = std::collections::BTreeSet::new();
    for seed in 0..50u64 {
        distinct.insert(pingpong_world(seed).trace.join("\n"));
    }
    assert!(
        distinct.len() > 1,
        "all seeds produced identical traces — the sim isn't using its entropy"
    );
}

#[test]
fn spawn_and_join_works_and_deterministic() {
    for seed in 0..50u64 {
        let a = spawn_demo_world(seed);
        let b = spawn_demo_world(seed);
        assert_eq!(
            a.trace, b.trace,
            "spawn-demo trace diverged for seed {seed}"
        );
        assert!(!a.deadlocked, "spawn-demo deadlocked at seed {seed}");
        assert_eq!(
            a.parked_tasks, 0,
            "spawn-demo left parked tasks at seed {seed}"
        );
    }
}

#[test]
fn gossip_is_multinode_and_deterministic() {
    for seed in 0..seed_count(100) {
        let a = gossip_world(seed, 5);
        let b = gossip_world(seed, 5);
        assert_eq!(a.trace, b.trace, "gossip trace diverged for seed {seed}");
        assert!(!a.deadlocked, "gossip deadlocked at seed {seed}");
        assert_eq!(a.parked_tasks, 0, "gossip left parked tasks at seed {seed}");
        assert_eq!(
            a.history.iter().filter(|e| e.starts_with("reply:")).count(),
            4
        );
    }
}

#[test]
fn timeout_paths_terminate_and_cancel_losers() {
    for seed in 0..seed_count(100) {
        let timed_out = timeout_world(seed);
        assert_eq!(timed_out.history, vec!["timeout"]);
        assert!(!timed_out.deadlocked, "timeout deadlocked at seed {seed}");

        let cancelled = timeout_cancel_world(seed);
        assert!(cancelled
            .history
            .iter()
            .any(|e| e.starts_with("got:1:fast")));
        assert!(cancelled.history.iter().any(|e| e == "done"));
        assert!(
            !cancelled
                .trace
                .iter()
                .any(|line| line.contains("timer fire=0")),
            "cancelled timeout timer fired for seed {seed}"
        );
    }
}

#[test]
fn partition_plant_bug_finds_dual_leader() {
    for seed in 0..seed_count(50) {
        let report = partitioned_dual_leader_world(seed);
        assert!(report.history.contains(&"leader:0".to_string()));
        assert!(report.history.contains(&"leader:1".to_string()));
        assert!(
            report
                .trace
                .iter()
                .any(|line| line.contains("drop-partition")),
            "partition did not affect traffic at seed {seed}"
        );
    }
}

#[test]
fn wal_flush_controls_crash_recovery() {
    for seed in 0..seed_count(50) {
        let durable = wal_recovery_world(seed, true);
        assert!(durable.history.contains(&"recovered:OKAY".to_string()));
        assert!(durable.history.contains(&"restart:0:restarted".to_string()));
        assert!(
            !durable
                .history
                .iter()
                .any(|entry| entry.contains("deferred")),
            "restart used a deferred path at seed {seed}"
        );

        let lost = wal_recovery_world(seed, false);
        assert!(
            !lost.history.contains(&"recovered:OKAY".to_string()),
            "unflushed ack survived crash at seed {seed}"
        );
    }
}

#[test]
fn restart_from_nemesis_uses_real_restart_path() {
    let report = wal_recovery_world(7, true);
    assert!(report
        .nemesis_trace
        .contains(&"Restart { node: 0 }".to_string()));
    assert!(report.history.contains(&"restart:0:restarted".to_string()));
    assert!(
        !report
            .history
            .iter()
            .any(|entry| entry.contains("deferred")),
        "restart history still contains deferred marker"
    );
}

#[test]
fn restart_outcome_reports_invalid_requests() {
    let mut world = World::new(7);
    assert_eq!(
        world.restart_node_outcome(99),
        detersim_sim::RestartOutcome::NodeMissing { node: 99 }
    );
    world.add_node(0, |_env: SimEnv| async move {});
    assert_eq!(
        world.restart_node_outcome(0),
        detersim_sim::RestartOutcome::NodeNotCrashed { node: 0 }
    );
}

#[test]
fn crash_cleans_node_events() {
    let mut world = World::with_config(
        7,
        WorldConfig {
            horizon_ns: 20_000_000,
            max_events: 10_000,
        },
    );
    world.schedule_nemesis(
        SimTime::from_nanos(1_000_000),
        NemesisAction::Crash { node: 0 },
    );
    world.schedule_nemesis(
        SimTime::from_nanos(2_000_000),
        NemesisAction::Restart { node: 0 },
    );
    world.add_node(0, |env: SimEnv| async move {
        let storage = env.storage();
        if storage.len().await == 0 {
            storage.write_at(0, b"1").await.expect("write boot marker");
            storage.flush().await.expect("flush boot marker");
            env.record("boot:first");
            env.clock().sleep(Duration::from_millis(100)).await;
            env.record("after-sleep:first");
        } else {
            env.record("boot:second");
            env.clock().sleep(Duration::from_millis(3)).await;
            env.record("after-sleep:second");
        }
    });

    let report = world.run();
    assert!(report.history.contains(&"boot:first".to_string()));
    assert!(report.history.contains(&"restart:0:restarted".to_string()));
    assert!(report.history.contains(&"boot:second".to_string()));
    assert!(report.history.contains(&"after-sleep:second".to_string()));
    assert!(
        !report.history.contains(&"after-sleep:first".to_string()),
        "old pre-crash timer woke a removed task"
    );
}

#[test]
fn wal_bitrot_is_detectable() {
    for seed in 0..seed_count(50) {
        let report = bitrot_wal_world(seed);
        assert!(report.history.contains(&"checksum:bad".to_string()));
    }
}

#[test]
fn toy_raft_reference_commits_deterministically() {
    for seed in 0..seed_count(50) {
        let a = toy_raft_world(seed);
        let b = toy_raft_world(seed);
        assert_eq!(a.trace, b.trace, "toy raft trace diverged for seed {seed}");
        assert!(a.history.contains(&"raft:commit:x=1".to_string()));
        assert_eq!(
            a.history
                .iter()
                .filter(|entry| entry.starts_with("raft:append:"))
                .count(),
            2
        );
    }
}
