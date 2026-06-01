use std::time::Duration;

use detersim_core::{ClockExt, Env, Network};
use detersim_search::{run_search, search_report_to_json, SearchBudget, SearchStrategy};
use detersim_shrink::ShrinkConfig;
use detersim_sim::{RunReport, SimEnv, World, WorldConfig};
use detersim_testkit::{ExperimentBudget, ExperimentCase, FailureSignature};

#[test]
fn coverage_guided_search_prioritizes_signal_rich_seeds() {
    let case = ExperimentCase {
        name: "odd-seed-missing-message",
        budget: ExperimentBudget {
            seed_count: 8,
            shrink: ShrinkConfig::default(),
        },
        generate: odd_seed_failure,
        replay: odd_seed_failure_replay,
        oracle,
    };
    let budget = SearchBudget {
        seed_count: 8,
        retain_candidates: 8,
    };

    let random = run_search(&case, SearchStrategy::Random, budget);
    let guided = run_search(&case, SearchStrategy::CoverageGuided, budget);

    assert_eq!(random.first_failing_rank, Some(1));
    assert_eq!(guided.first_failing_rank, Some(0));
    assert!(guided.corpus.unique_coverage.len() >= random.corpus.unique_coverage.len());
    let json = search_report_to_json(&guided);
    assert!(json.contains("\"schema_version\":3"));
    assert!(json.contains("odd-seed-failure"));
}

fn odd_seed_failure(seed: u64) -> RunReport {
    run_inner(seed, None)
}

fn odd_seed_failure_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_inner(seed, Some(tape))
}

fn run_inner(seed: u64, replay: Option<Vec<u64>>) -> RunReport {
    let config = WorldConfig {
        horizon_ns: 1_000_000_000,
        max_events: 10_000,
    };
    let mut world = match replay {
        Some(tape) => World::replay(seed, tape, config),
        None => World::with_config(seed, config),
    };
    world.set_drop_percent(if seed % 2 == 1 { 100 } else { 0 });
    world.add_node(0, |env: SimEnv| async move {
        let result = env
            .clock()
            .timeout(Duration::from_millis(200), env.net().recv())
            .await;
        if result.is_err() {
            env.record("odd-seed-failure");
        }
    });
    world.add_node(1, |env: SimEnv| async move {
        env.net().send(0, b"hello".to_vec()).await;
    });
    world.run()
}

fn oracle(report: &RunReport) -> Option<FailureSignature> {
    report
        .history
        .iter()
        .any(|entry| entry == "odd-seed-failure")
        .then(|| FailureSignature::InvariantViolated("odd-seed-failure".to_string()))
}
