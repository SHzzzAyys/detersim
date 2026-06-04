use detersim_search::{
    compare_search_suite, suite_search_comparison_report_to_json, SearchBudget, SearchStrategy,
};
use detersim_shrink::ShrinkConfig;
use detersim_sim::RunReport;
use detersim_testkit::{
    ExperimentBudget, ExperimentCase, ExperimentSuite, FailureSignature, RecallPolicy,
};

fn run_even_failure(seed: u64) -> RunReport {
    report(seed, seed % 2 == 0, "even")
}

fn replay_even_failure(seed: u64, _tape: Vec<u64>) -> RunReport {
    run_even_failure(seed)
}

fn run_odd_failure(seed: u64) -> RunReport {
    report(seed, seed % 2 == 1, "odd")
}

fn replay_odd_failure(seed: u64, _tape: Vec<u64>) -> RunReport {
    run_odd_failure(seed)
}

fn report(seed: u64, fail: bool, label: &str) -> RunReport {
    let history = if fail {
        vec![format!("bug:{label}")]
    } else {
        vec![format!("ok:{label}")]
    };
    RunReport {
        seed,
        trace: vec![format!("seed:{seed}:{label}")],
        nemesis_trace: Vec::new(),
        history,
        coverage_signals: vec![format!("coverage:{label}:{}", seed % 3)],
        tape_log: vec![seed],
        tape_events: Vec::new(),
        tape_replaying: false,
        tape_input_len: None,
        tape_cursor: 0,
        tape_consumed_all: true,
        tape_exhausted: false,
        dispatched: 1,
        aborted: false,
        deadlocked: false,
        parked_tasks: 0,
        tape_log_len: 1,
    }
}

fn signature(report: &RunReport) -> Option<FailureSignature> {
    report
        .history
        .iter()
        .find(|entry| entry.starts_with("bug:"))
        .map(|entry| FailureSignature::InvariantViolated(entry.clone()))
}

fn case(
    name: &'static str,
    generate: fn(u64) -> RunReport,
    replay: fn(u64, Vec<u64>) -> RunReport,
) -> ExperimentCase {
    ExperimentCase {
        name,
        budget: ExperimentBudget {
            seed_count: 4,
            shrink: ShrinkConfig::default(),
        },
        generate,
        replay,
        oracle: signature,
    }
}

#[test]
fn suite_search_comparison_aggregates_cases_and_winners() {
    let suite = ExperimentSuite {
        name: "search-suite",
        cases: vec![
            (
                case("even-case", run_even_failure, replay_even_failure),
                RecallPolicy::MustRecall,
            ),
            (
                case("odd-case", run_odd_failure, replay_odd_failure),
                RecallPolicy::MustRecall,
            ),
        ],
    };

    let report = compare_search_suite(
        &suite,
        &[
            SearchStrategy::Random,
            SearchStrategy::CoverageGuided,
            SearchStrategy::FailureDirected,
        ],
        SearchBudget {
            seed_count: 4,
            retain_candidates: 4,
        },
    );

    assert_eq!(report.suite_name, "search-suite");
    assert_eq!(report.case_reports.len(), 2);
    assert!(!report.strategy_wins.is_empty());
    assert!(report
        .case_reports
        .iter()
        .all(|case| case.strategy_winner.is_some()));
    assert!(report
        .case_reports
        .iter()
        .any(|case| case.sparse_case && !case.dense_case));

    let json = suite_search_comparison_report_to_json(&report);
    assert!(json.contains("\"schema_version\":3"));
    assert!(json.contains("\"suite\":\"search-suite\""));
    assert!(json.contains("\"case_count\":2"));
    assert!(json.contains("\"strategy_wins\""));
    assert!(json.contains("\"median_first_failing_rank\""));
    assert!(json.contains("\"sparse_case\":true"));
}
