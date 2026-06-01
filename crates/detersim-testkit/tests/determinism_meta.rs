use detersim_sim::scenarios::{
    gossip_world, partitioned_dual_leader_world, pingpong_world, pingpong_world_replay,
    timeout_cancel_world, timeout_world, toy_raft_world, wal_recovery_world,
};
use detersim_sim::RunReport;
use detersim_testkit::{
    assert_deterministic, assert_replay_identical, run_seed_range, seed_count, PlantBugCase,
};

#[test]
fn testkit_covers_same_seed_and_replay_oracles() {
    run_seed_range(seed_count(100), pingpong_world, |seed, _report| {
        assert_deterministic(seed, pingpong_world);
        assert_replay_identical(seed, pingpong_world, pingpong_world_replay);
    });
}

#[test]
fn testkit_covers_existing_scenarios() {
    run_seed_range(
        seed_count(50),
        |seed| gossip_world(seed, 5),
        |seed, report| {
            let deterministic = assert_deterministic(seed, |s| gossip_world(s, 5));
            assert_eq!(report.trace, deterministic.trace);
        },
    );

    run_seed_range(seed_count(50), timeout_world, |_seed, report| {
        assert_eq!(report.history, vec!["timeout"]);
    });

    run_seed_range(seed_count(50), timeout_cancel_world, |_seed, report| {
        assert!(report.history.iter().any(|e| e == "done"));
    });

    run_seed_range(seed_count(50), toy_raft_world, |_seed, report| {
        assert!(report.history.contains(&"raft:commit:x=1".to_string()));
    });
}

#[test]
fn testkit_covers_plant_bug_cases() {
    PlantBugCase {
        name: "partition dual leader",
        seeds: seed_count(50),
        run: partitioned_dual_leader_world,
        check: |report: &RunReport| {
            report.history.contains(&"leader:0".to_string())
                && report.history.contains(&"leader:1".to_string())
        },
    }
    .assert_reproduced();

    PlantBugCase {
        name: "unflushed wal loses ack",
        seeds: seed_count(50),
        run: |seed| wal_recovery_world(seed, false),
        check: |report: &RunReport| !report.history.contains(&"recovered:OKAY".to_string()),
    }
    .assert_reproduced();
}
