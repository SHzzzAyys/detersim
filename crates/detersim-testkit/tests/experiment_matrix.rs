use std::time::Duration;

use detersim_core::{ClockExt, Env, Network};
use detersim_shrink::ShrinkConfig;
use detersim_sim::{RunReport, SimEnv, World, WorldConfig};
use detersim_testkit::{
    assert_recall, experiment_matrix_report_to_json, experiment_report_to_json,
    run_experiment_case, run_experiment_matrix, summarize_experiment_matrix, ExperimentBudget,
    ExperimentCase, FailureSignature, RecallResult,
};

fn config() -> WorldConfig {
    WorldConfig {
        horizon_ns: 100_000_000,
        max_events: 10_000,
    }
}

fn dropped_message_world(seed: u64) -> RunReport {
    let world = World::with_config(seed, config());
    run_dropped_message(world)
}

fn dropped_message_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    let world = World::replay(seed, tape, config());
    run_dropped_message(world)
}

fn run_dropped_message(mut world: World) -> RunReport {
    world.set_drop_percent(50);
    world.add_node(0, |env: SimEnv| async move {
        let result = env
            .clock()
            .timeout(Duration::from_millis(20), env.net().recv())
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

fn never_fails(_report: &RunReport) -> Option<FailureSignature> {
    None
}

#[test]
fn experiment_case_records_recall_and_shrink_stats() {
    let case = ExperimentCase {
        name: "dropped-message",
        budget: ExperimentBudget {
            seed_count: 20,
            shrink: ShrinkConfig {
                max_attempts: 100,
                min_chunk_len: 2,
            },
        },
        generate: dropped_message_world,
        replay: dropped_message_world_replay,
        oracle: missing_message_signature,
    };

    let report = assert_recall(&case);
    assert_eq!(report.name, "dropped-message");
    assert_eq!(report.seeds_attempted, 20);
    assert!(report.failures_observed > 0);
    assert!(report.recall_rate > 0.0);
    assert_eq!(
        report.failure_signature,
        Some(FailureSignature::InvariantViolated(
            "missing-message".to_string()
        ))
    );
    assert!(report.replay_trace_identical);
    assert!(report.replay_byte_identical);
    assert!(report.minimized_matched_signature);
    assert!(report.artifact_json_bytes.unwrap_or_default() > 0);

    let json = experiment_report_to_json(&report);
    assert!(json.contains("\"failure_signature\""));
    assert!(json.contains("\"shrink_ratio\""));
}

#[test]
fn experiment_matrix_preserves_negative_control() {
    let failing = ExperimentCase {
        name: "dropped-message",
        budget: ExperimentBudget {
            seed_count: 20,
            shrink: ShrinkConfig::default(),
        },
        generate: dropped_message_world,
        replay: dropped_message_world_replay,
        oracle: missing_message_signature,
    };
    let negative_for_matrix = ExperimentCase {
        name: "negative-control",
        budget: ExperimentBudget {
            seed_count: 5,
            shrink: ShrinkConfig::default(),
        },
        generate: dropped_message_world,
        replay: dropped_message_world_replay,
        oracle: never_fails,
    };

    let reports = run_experiment_matrix(&[failing, negative_for_matrix]);
    assert!(matches!(reports[0], RecallResult::Recalled(_)));
    assert!(matches!(reports[1], RecallResult::NotRecalled(_)));
    assert_eq!(reports[1].report().failures_observed, 0);

    let summary = summarize_experiment_matrix(&reports);
    assert_eq!(summary.total_cases, 2);
    assert_eq!(summary.recalled_cases, 1);
    assert_eq!(summary.failed_cases, 1);
    assert_eq!(summary.signatures.len(), 1);
    let json = experiment_matrix_report_to_json(&summary);
    assert!(json.contains("\"total_cases\":2"));
    assert!(json.contains("\"signatures\""));

    let negative_standalone = ExperimentCase {
        name: "negative-control",
        budget: ExperimentBudget {
            seed_count: 5,
            shrink: ShrinkConfig::default(),
        },
        generate: dropped_message_world,
        replay: dropped_message_world_replay,
        oracle: never_fails,
    };
    let standalone = run_experiment_case(&negative_standalone);
    assert!(matches!(standalone, RecallResult::NotRecalled(_)));
}
