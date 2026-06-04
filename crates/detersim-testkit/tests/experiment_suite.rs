use std::time::Duration;

use detersim_core::{ClockExt, Env, Network};
use detersim_shrink::ShrinkConfig;
use detersim_sim::{RunReport, SimEnv, World, WorldConfig};
use detersim_testkit::{
    experiment_summary_to_json, run_experiment_suite, ExperimentBudget, ExperimentCase,
    ExperimentSuite, FailureSignature, RecallPolicy,
};

fn config() -> WorldConfig {
    WorldConfig {
        horizon_ns: 100_000_000,
        max_events: 10_000,
    }
}

fn ok_world(seed: u64) -> RunReport {
    let mut world = World::with_config(seed, config());
    world.add_node(0, |env: SimEnv| async move {
        env.record("ok");
    });
    world.run()
}

fn ok_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    let mut world = World::replay(seed, tape, config());
    world.add_node(0, |env: SimEnv| async move {
        env.record("ok");
    });
    world.run()
}

fn dropped_message_world(seed: u64) -> RunReport {
    run_dropped_message(World::with_config(seed, config()))
}

fn dropped_message_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_dropped_message(World::replay(seed, tape, config()))
}

fn run_dropped_message(mut world: World) -> RunReport {
    world.set_drop_percent(100);
    world.add_node(0, |env: SimEnv| async move {
        let net = env.net();
        let result = env
            .clock()
            .timeout(Duration::from_millis(20), net.recv())
            .await;
        if result.is_err() {
            env.record("missing-message");
        }
    });
    world.add_node(1, |env: SimEnv| async move {
        env.net().send(0, b"hello".to_vec()).await;
    });
    world.run()
}

fn missing_message_signature(report: &RunReport) -> Option<FailureSignature> {
    report
        .history
        .contains(&"missing-message".to_string())
        .then(|| FailureSignature::InvariantViolated("missing-message".to_string()))
}

fn no_failure(_report: &RunReport) -> Option<FailureSignature> {
    None
}

#[test]
fn experiment_suite_enforces_recall_policies_and_json() {
    let suite = ExperimentSuite {
        name: "v2-suite-smoke",
        cases: vec![
            (
                ExperimentCase {
                    name: "dropped-message",
                    budget: ExperimentBudget {
                        seed_count: 5,
                        shrink: ShrinkConfig {
                            max_attempts: 100,
                            min_chunk_len: 2,
                        },
                    },
                    generate: dropped_message_world,
                    replay: dropped_message_world_replay,
                    oracle: missing_message_signature,
                },
                RecallPolicy::MustRecall,
            ),
            (
                ExperimentCase {
                    name: "ok-negative-control",
                    budget: ExperimentBudget {
                        seed_count: 5,
                        shrink: ShrinkConfig::default(),
                    },
                    generate: ok_world,
                    replay: ok_world_replay,
                    oracle: no_failure,
                },
                RecallPolicy::MustNotRecall,
            ),
        ],
    };

    let summary = run_experiment_suite(suite);
    assert_eq!(summary.total_cases, 2);
    assert_eq!(summary.policy_failures, 0);
    assert_eq!(summary.control_failures, 0);
    assert_eq!(summary.oracle_inconclusive_count, 0);
    assert_eq!(summary.replay_mismatch_count, 0);
    assert_eq!(summary.shrink_mismatch_count, 0);
    assert_eq!(summary.required_recalled, 1);
    assert_eq!(summary.required_not_recalled, 1);
    assert_eq!(summary.matrix.recalled_cases, 1);

    let json = experiment_summary_to_json(&summary);
    assert!(json.contains("\"schema_version\":2"));
    assert!(json.contains("\"v2-suite-smoke\""));
    assert!(json.contains("\"artifacts\""));
    assert!(json.contains("\"control_failures\":0"));
    assert!(json.contains("\"replay_mismatch_count\":0"));
}
