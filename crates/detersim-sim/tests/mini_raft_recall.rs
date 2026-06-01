use detersim_check::models::SingleKeyKv;
use detersim_check::{check_linearizable_with_budget, CheckBudget, LinearizabilityResult};
use detersim_core::SimTime;
use detersim_nemesis::NemesisAction;
use detersim_protocols::{
    collect_protocol_events, run_mini_raft, run_mini_raft_client, run_mini_raft_kv_client,
    single_key_kv_history, MiniRaftConfig, RaftBugVariant, RaftInvariant, RAFT_OBSERVER_NODE,
};
use detersim_sim::{RunReport, SimEnv, World, WorldConfig};

fn seed_count(default: u64) -> u64 {
    std::env::var("DST_SEED_COUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn config() -> WorldConfig {
    WorldConfig {
        horizon_ns: 500_000_000,
        max_events: 20_000,
    }
}

#[test]
fn mini_raft_reference_is_stable_under_seed_sweep() {
    for seed in 0..seed_count(100) {
        let a = mini_raft_correct_world(seed);
        let b = mini_raft_correct_world(seed);
        assert_eq!(a.trace, b.trace, "mini raft trace diverged at seed {seed}");
        assert!(!a.aborted, "mini raft aborted at seed {seed}");
        assert!(!a.deadlocked, "mini raft deadlocked at seed {seed}");
        assert!(matches!(
            check_linearizable_with_budget(
                &SingleKeyKv::new(None),
                &single_key_kv_history(&a.history),
                CheckBudget { max_steps: 10_000 }
            ),
            LinearizabilityResult::Linearizable { .. }
        ));
    }
}

#[test]
fn mini_raft_stale_read_is_checker_backed() {
    for seed in 0..seed_count(100) {
        let report = follower_stale_read_world(seed);
        assert!(matches!(
            check_linearizable_with_budget(
                &SingleKeyKv::new(None),
                &single_key_kv_history(&report.history),
                CheckBudget { max_steps: 10_000 }
            ),
            LinearizabilityResult::NotLinearizable { .. }
        ));
    }
}

#[test]
fn mini_raft_persists_term_vote_and_log_across_restart() {
    for seed in 0..seed_count(100) {
        let report = mini_raft_persistence_world(seed, RaftBugVariant::Correct);
        assert!(report
            .history
            .contains(&"raft:recovered:term=1:voted=0:log=x".to_string()));
        assert!(!report
            .history
            .iter()
            .any(|entry| entry.starts_with("raft-bug:")));
    }
}

#[test]
fn mini_raft_plant_a_bug_variants_are_recalled() {
    let variants: &[(&str, fn(u64) -> RunReport, &[&str])] = &[
        (
            "term-not-persisted",
            term_not_persisted_world,
            &["raft-bug:term-not-persisted"],
        ),
        (
            "vote-not-persisted",
            vote_not_persisted_world,
            &["raft-bug:vote-not-persisted"],
        ),
        (
            "wrong-commit-rule",
            wrong_commit_rule_world,
            &["raft-bug:wrong-commit-rule"],
        ),
        (
            "wrong-log-matching",
            wrong_log_matching_world,
            &["raft-bug:wrong-log-matching"],
        ),
        (
            "dual-leader",
            dual_leader_world,
            &[
                "leader:0",
                "leader:1",
                RaftInvariant::SingleLeaderPerTerm.as_label(),
            ],
        ),
        (
            "follower-stale-read",
            follower_stale_read_world,
            &["raft-bug:follower-stale-read"],
        ),
        (
            "duplicate-client-request",
            duplicate_client_request_world,
            &["raft-bug:duplicate-client-request"],
        ),
        (
            "apply-before-commit",
            apply_before_commit_world,
            &[
                "raft-bug:apply-before-commit",
                RaftInvariant::AppliedIndexNotPastCommit.as_label(),
            ],
        ),
        (
            "old-term-leader-commits-entry",
            old_term_leader_commits_entry_world,
            &[
                "raft-bug:old-term-leader-commits-entry",
                RaftInvariant::CommittedEntriesNotLost.as_label(),
            ],
        ),
    ];

    for (name, run, expected) in variants {
        let mut hits = 0u64;
        for seed in 0..100 {
            let report = run(seed);
            if expected
                .iter()
                .all(|label| report.history.contains(&label.to_string()))
            {
                hits += 1;
            }
        }
        assert_eq!(hits, 100, "{name} recall was not 100%");
    }
}

fn mini_raft_correct_world(seed: u64) -> RunReport {
    let mut world = World::with_config(seed, config());
    let raft_config = MiniRaftConfig::correct();
    world.add_nodes(3, move |env: SimEnv| run_mini_raft(env, raft_config));
    world.add_node(4, move |env: SimEnv| async move {
        for op in run_mini_raft_kv_client(env.clone(), raft_config).await {
            env.record(op.to_history_line());
        }
    });
    world.run()
}

fn mini_raft_persistence_world(seed: u64, variant: RaftBugVariant) -> RunReport {
    let mut world = World::with_config(seed, config());
    world.schedule_nemesis(
        SimTime::from_nanos(1_000_000),
        NemesisAction::Crash { node: 0 },
    );
    world.schedule_nemesis(
        SimTime::from_nanos(2_000_000),
        NemesisAction::Restart { node: 0 },
    );
    let raft_config = MiniRaftConfig::persistence_probe(variant);
    world.add_nodes(3, move |env: SimEnv| run_mini_raft(env, raft_config));
    add_observer(&mut world, 1);
    world.run()
}

fn term_not_persisted_world(seed: u64) -> RunReport {
    mini_raft_persistence_world(seed, RaftBugVariant::TermNotPersisted)
}

fn vote_not_persisted_world(seed: u64) -> RunReport {
    mini_raft_persistence_world(seed, RaftBugVariant::VoteNotPersisted)
}

fn wrong_commit_rule_world(seed: u64) -> RunReport {
    mini_raft_observed_world(seed, RaftBugVariant::WrongCommitRule, 1)
}

fn wrong_log_matching_world(seed: u64) -> RunReport {
    mini_raft_observed_world(seed, RaftBugVariant::WrongLogMatching, 1)
}

fn dual_leader_world(seed: u64) -> RunReport {
    mini_raft_observed_world(seed, RaftBugVariant::DualLeaderUnderPartition, 4)
}

fn apply_before_commit_world(seed: u64) -> RunReport {
    mini_raft_observed_world(seed, RaftBugVariant::ApplyBeforeCommit, 2)
}

fn old_term_leader_commits_entry_world(seed: u64) -> RunReport {
    mini_raft_observed_world(seed, RaftBugVariant::OldTermLeaderCommitsEntry, 2)
}

fn follower_stale_read_world(seed: u64) -> RunReport {
    mini_raft_client_bug_world(seed, RaftBugVariant::FollowerStaleRead)
}

fn duplicate_client_request_world(seed: u64) -> RunReport {
    mini_raft_client_bug_world(seed, RaftBugVariant::DuplicateClientRequest)
}

fn mini_raft_observed_world(seed: u64, variant: RaftBugVariant, expected: usize) -> RunReport {
    let mut world = World::with_config(seed, config());
    let raft_config = MiniRaftConfig::with_bug(variant);
    world.add_nodes(3, move |env: SimEnv| run_mini_raft(env, raft_config));
    add_observer(&mut world, expected);
    world.run()
}

fn mini_raft_client_bug_world(seed: u64, variant: RaftBugVariant) -> RunReport {
    let mut world = World::with_config(seed, config());
    let raft_config = MiniRaftConfig::with_bug(variant);
    world.add_nodes(3, move |env: SimEnv| run_mini_raft(env, raft_config));
    world.add_node(4, move |env: SimEnv| async move {
        if matches!(raft_config.bug, RaftBugVariant::FollowerStaleRead) {
            let ops = run_mini_raft_kv_client(env.clone(), raft_config).await;
            let stale_read = ops.iter().any(|op| {
                matches!(
                    op,
                    detersim_protocols::RecordedOp::KvGet { value: None, .. }
                )
            });
            for op in ops {
                env.record(op.to_history_line());
            }
            if stale_read {
                env.record("raft-bug:follower-stale-read");
            }
        } else {
            for label in run_mini_raft_client(env.clone(), raft_config).await {
                env.record(label);
            }
        }
    });
    world.run()
}

fn add_observer(world: &mut World, expected: usize) {
    world.add_node(RAFT_OBSERVER_NODE, move |env: SimEnv| async move {
        for label in collect_protocol_events(env.clone(), expected).await {
            env.record(label);
        }
    });
}
