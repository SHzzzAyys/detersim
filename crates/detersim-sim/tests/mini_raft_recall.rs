use detersim_core::SimTime;
use detersim_nemesis::NemesisAction;
use detersim_protocols::{
    collect_protocol_events, run_mini_raft, run_mini_raft_client, MiniRaftConfig, RaftBugVariant,
    RAFT_OBSERVER_NODE,
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
        assert!(a.history.contains(&"raft:commit:x=1".to_string()));
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
        ("dual-leader", dual_leader_world, &["leader:0", "leader:1"]),
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
            &["raft-bug:apply-before-commit"],
        ),
        (
            "old-term-leader-commits-entry",
            old_term_leader_commits_entry_world,
            &["raft-bug:old-term-leader-commits-entry"],
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
        for label in run_mini_raft_client(env.clone(), raft_config).await {
            env.record(label);
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
    mini_raft_observed_world(seed, RaftBugVariant::DualLeaderUnderPartition, 2)
}

fn apply_before_commit_world(seed: u64) -> RunReport {
    mini_raft_observed_world(seed, RaftBugVariant::ApplyBeforeCommit, 1)
}

fn old_term_leader_commits_entry_world(seed: u64) -> RunReport {
    mini_raft_observed_world(seed, RaftBugVariant::OldTermLeaderCommitsEntry, 1)
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
        for label in run_mini_raft_client(env.clone(), raft_config).await {
            env.record(label);
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
