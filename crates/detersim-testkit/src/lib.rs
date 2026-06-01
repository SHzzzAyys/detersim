//! User-facing deterministic test helpers.
//!
//! This crate is intentionally a thin harness over `detersim-sim`; it does not
//! introduce a second execution path.

use std::collections::BTreeMap;

use detersim_check::LinearizabilityResult;
use detersim_shrink::{shrink_tape_with_config, ShrinkConfig, ShrinkReport};
use detersim_sim::RunReport;
use detersim_viz::{run_report_to_json, timeline_html};

pub fn seed_count(default: u64) -> u64 {
    std::env::var("DST_SEED_COUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

pub fn assert_deterministic<F>(seed: u64, run: F) -> RunReport
where
    F: Fn(u64) -> RunReport,
{
    let a = run(seed);
    let b = run(seed);
    assert_reports_equal(seed, "same-seed", &a, &b);
    a
}

pub fn run_seed_range<F, C>(count: u64, run: F, mut check: C)
where
    F: Fn(u64) -> RunReport,
    C: FnMut(u64, &RunReport),
{
    for seed in 0..count {
        let report = run(seed);
        check(seed, &report);
    }
}

pub fn assert_replay_identical<G, R>(seed: u64, generate: G, replay: R) -> (RunReport, RunReport)
where
    G: Fn(u64) -> RunReport,
    R: Fn(u64, Vec<u64>) -> RunReport,
{
    let generated = generate(seed);
    let replayed = replay(seed, generated.tape_log.clone());

    assert!(
        replayed.tape_replaying,
        "replay run did not report replay mode for seed {seed}"
    );
    assert_eq!(
        replayed.tape_input_len,
        Some(generated.tape_log.len()),
        "replay input length mismatch for seed {seed}"
    );
    assert!(
        replayed.tape_consumed_all,
        "replay did not consume the full tape for seed {seed}"
    );
    assert!(
        !replayed.tape_exhausted,
        "replay exhausted its input tape for seed {seed}"
    );

    assert_reports_equal(seed, "generate-vs-replay", &generated, &replayed);
    (generated, replayed)
}

pub struct PlantBugCase<F, C> {
    pub name: &'static str,
    pub seeds: u64,
    pub run: F,
    pub check: C,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// A normalized description of what failed.
///
/// Keep signatures free of incidental scheduler details. They are used to
/// decide whether replay and shrinking preserved the same underlying bug.
pub enum FailureSignature {
    InvariantViolated(String),
    NotLinearizable {
        conflict: Option<(u64, u64)>,
        model: String,
    },
    Deadlock,
    Panic(String),
    StorageCorruption(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Seed and shrink budget for one experiment case.
pub struct ExperimentBudget {
    pub seed_count: u64,
    pub shrink: ShrinkConfig,
}

impl Default for ExperimentBudget {
    fn default() -> Self {
        Self {
            seed_count: 100,
            shrink: ShrinkConfig::default(),
        }
    }
}

/// A runnable plant-a-bug or negative-control experiment.
///
/// The oracle returns `Some(signature)` only when the run exhibits the failure
/// this case is trying to recall.
pub struct ExperimentCase {
    pub name: &'static str,
    pub budget: ExperimentBudget,
    pub generate: fn(u64) -> RunReport,
    pub replay: fn(u64, Vec<u64>) -> RunReport,
    pub oracle: fn(&RunReport) -> Option<FailureSignature>,
}

#[derive(Clone, Debug)]
/// Summary statistics for one experiment case.
pub struct ExperimentReport {
    pub name: &'static str,
    pub seeds_attempted: u64,
    pub first_failing_seed: Option<u64>,
    pub failures_observed: u64,
    pub recall_rate: f64,
    pub recalled: bool,
    pub failure_signature: Option<FailureSignature>,
    pub original_tape_len: Option<usize>,
    pub minimized_tape_len: Option<usize>,
    pub shrink_ratio: Option<f64>,
    pub shrink_attempts: Option<usize>,
    pub accepted_removals: Option<usize>,
    pub artifact_json_bytes: Option<usize>,
    pub artifact_html_bytes: Option<usize>,
    pub replay_tape_input_len: Option<usize>,
    pub replay_tape_cursor: Option<usize>,
    pub replay_tape_consumed_all: Option<bool>,
    pub replay_tape_exhausted: Option<bool>,
    pub replay_trace_identical: bool,
    pub replay_history_identical: bool,
    pub replay_nemesis_identical: bool,
    pub replay_byte_identical: bool,
    pub replay_matched_signature: bool,
    pub minimized_matched_signature: bool,
}

#[derive(Clone, Debug)]
/// Recall outcome for one experiment case.
pub enum RecallResult {
    Recalled(ExperimentReport),
    NotRecalled(ExperimentReport),
}

impl RecallResult {
    pub fn report(&self) -> &ExperimentReport {
        match self {
            RecallResult::Recalled(report) | RecallResult::NotRecalled(report) => report,
        }
    }
}

#[derive(Clone, Debug)]
/// Matrix-level summary over a deterministic list of experiment cases.
pub struct ExperimentMatrixReport {
    pub total_cases: usize,
    pub recalled_cases: usize,
    pub failed_cases: usize,
    pub average_recall_rate: f64,
    pub average_shrink_ratio: Option<f64>,
    pub min_original_tape_len: Option<usize>,
    pub max_original_tape_len: Option<usize>,
    pub min_minimized_tape_len: Option<usize>,
    pub max_minimized_tape_len: Option<usize>,
    pub signatures: Vec<(FailureSignature, u64)>,
}

#[derive(Clone, Debug)]
pub struct FailureDebugArtifact {
    pub seed: u64,
    pub generated: RunReport,
    pub minimized_replay: RunReport,
    pub shrink: ShrinkReport,
    pub minimized_json: String,
    pub minimized_html: String,
}

pub fn shrink_replay_failure<G, R, C>(
    seed: u64,
    generate: G,
    replay: R,
    still_fails: C,
    config: ShrinkConfig,
) -> FailureDebugArtifact
where
    G: Fn(u64) -> RunReport,
    R: Fn(u64, Vec<u64>) -> RunReport,
    C: Fn(&RunReport) -> bool,
{
    let generated = generate(seed);
    assert!(
        still_fails(&generated),
        "cannot shrink seed {seed}: generated run does not fail"
    );

    let original_replay = replay(seed, generated.tape_log.clone());
    assert!(
        still_fails(&original_replay),
        "cannot shrink seed {seed}: original tape replay does not reproduce the failure"
    );

    let shrink = shrink_tape_with_config(
        &generated.tape_log,
        |candidate| {
            let report = replay(seed, candidate.to_vec());
            still_fails(&report)
        },
        config,
    );
    assert!(
        shrink.reproduced,
        "shrink reported non-reproducing original tape at seed {seed}"
    );

    let minimized_replay = replay(seed, shrink.minimized.clone());
    assert!(
        still_fails(&minimized_replay),
        "minimized tape did not reproduce the failure at seed {seed}"
    );

    let minimized_json = run_report_to_json(&minimized_replay);
    let minimized_html = timeline_html(&minimized_replay);

    FailureDebugArtifact {
        seed,
        generated,
        minimized_replay,
        shrink,
        minimized_json,
        minimized_html,
    }
}

/// Convert a checker result into a stable failure signature.
pub fn linearizability_signature(
    model: impl Into<String>,
    result: &LinearizabilityResult,
) -> Option<FailureSignature> {
    match result {
        LinearizabilityResult::NotLinearizable { conflict, .. } => {
            Some(FailureSignature::NotLinearizable {
                conflict: *conflict,
                model: model.into(),
            })
        }
        _ => None,
    }
}

/// Run one experiment case, including first-failure replay and shrink.
pub fn run_experiment_case(case: &ExperimentCase) -> RecallResult {
    let mut first: Option<(u64, RunReport, FailureSignature)> = None;
    let mut failures_observed = 0u64;

    for seed in 0..case.budget.seed_count {
        let report = (case.generate)(seed);
        if let Some(signature) = (case.oracle)(&report) {
            failures_observed += 1;
            if first.is_none() {
                first = Some((seed, report, signature));
            }
        }
    }

    let recall_rate = if case.budget.seed_count == 0 {
        0.0
    } else {
        failures_observed as f64 / case.budget.seed_count as f64
    };

    let Some((seed, generated, signature)) = first else {
        return RecallResult::NotRecalled(ExperimentReport {
            name: case.name,
            seeds_attempted: case.budget.seed_count,
            first_failing_seed: None,
            failures_observed,
            recall_rate,
            recalled: false,
            failure_signature: None,
            original_tape_len: None,
            minimized_tape_len: None,
            shrink_ratio: None,
            shrink_attempts: None,
            accepted_removals: None,
            artifact_json_bytes: None,
            artifact_html_bytes: None,
            replay_tape_input_len: None,
            replay_tape_cursor: None,
            replay_tape_consumed_all: None,
            replay_tape_exhausted: None,
            replay_trace_identical: false,
            replay_history_identical: false,
            replay_nemesis_identical: false,
            replay_byte_identical: false,
            replay_matched_signature: false,
            minimized_matched_signature: false,
        });
    };

    let replayed = (case.replay)(seed, generated.tape_log.clone());
    let replay_matched_signature = (case.oracle)(&replayed).as_ref() == Some(&signature);
    let replay_trace_identical = generated.trace == replayed.trace;
    let replay_history_identical = generated.history == replayed.history;
    let replay_nemesis_identical = generated.nemesis_trace == replayed.nemesis_trace;
    let replay_byte_identical =
        generated.trace.join("\n").into_bytes() == replayed.trace.join("\n").into_bytes();
    let mut report = ExperimentReport {
        name: case.name,
        seeds_attempted: case.budget.seed_count,
        first_failing_seed: Some(seed),
        failures_observed,
        recall_rate,
        recalled: false,
        failure_signature: Some(signature.clone()),
        original_tape_len: Some(generated.tape_log.len()),
        minimized_tape_len: None,
        shrink_ratio: None,
        shrink_attempts: None,
        accepted_removals: None,
        artifact_json_bytes: None,
        artifact_html_bytes: None,
        replay_tape_input_len: replayed.tape_input_len,
        replay_tape_cursor: Some(replayed.tape_cursor),
        replay_tape_consumed_all: Some(replayed.tape_consumed_all),
        replay_tape_exhausted: Some(replayed.tape_exhausted),
        replay_trace_identical,
        replay_history_identical,
        replay_nemesis_identical,
        replay_byte_identical,
        replay_matched_signature,
        minimized_matched_signature: false,
    };

    if !replay_matched_signature
        || !replay_trace_identical
        || !replay_history_identical
        || !replay_nemesis_identical
        || !replay_byte_identical
    {
        return RecallResult::NotRecalled(report);
    }

    let shrink = shrink_tape_with_config(
        &generated.tape_log,
        |candidate| {
            let candidate_report = (case.replay)(seed, candidate.to_vec());
            (case.oracle)(&candidate_report).as_ref() == Some(&signature)
        },
        case.budget.shrink,
    );
    let minimized = (case.replay)(seed, shrink.minimized.clone());
    let minimized_matched_signature = (case.oracle)(&minimized).as_ref() == Some(&signature);
    let minimized_json = run_report_to_json(&minimized);
    let minimized_html = timeline_html(&minimized);

    report.minimized_tape_len = Some(shrink.minimized.len());
    report.shrink_ratio = Some(if generated.tape_log.is_empty() {
        1.0
    } else {
        shrink.minimized.len() as f64 / generated.tape_log.len() as f64
    });
    report.shrink_attempts = Some(shrink.attempts);
    report.accepted_removals = Some(shrink.accepted_removals);
    report.artifact_json_bytes = Some(minimized_json.len());
    report.artifact_html_bytes = Some(minimized_html.len());
    report.minimized_matched_signature = minimized_matched_signature;
    report.recalled = minimized_matched_signature;

    if minimized_matched_signature {
        RecallResult::Recalled(report)
    } else {
        RecallResult::NotRecalled(report)
    }
}

/// Run a fixed matrix of experiment cases in input order.
pub fn run_experiment_matrix(cases: &[ExperimentCase]) -> Vec<RecallResult> {
    cases.iter().map(run_experiment_case).collect()
}

/// Summarize experiment recall and shrink effectiveness in deterministic order.
pub fn summarize_experiment_matrix(results: &[RecallResult]) -> ExperimentMatrixReport {
    let total_cases = results.len();
    let recalled_cases = results
        .iter()
        .filter(|result| matches!(result, RecallResult::Recalled(_)))
        .count();
    let failed_cases = total_cases - recalled_cases;
    let average_recall_rate = if total_cases == 0 {
        0.0
    } else {
        results
            .iter()
            .map(|result| result.report().recall_rate)
            .sum::<f64>()
            / total_cases as f64
    };

    let shrink_ratios: Vec<f64> = results
        .iter()
        .filter_map(|result| result.report().shrink_ratio)
        .collect();
    let average_shrink_ratio = if shrink_ratios.is_empty() {
        None
    } else {
        Some(shrink_ratios.iter().sum::<f64>() / shrink_ratios.len() as f64)
    };

    let original_lengths: Vec<usize> = results
        .iter()
        .filter_map(|result| result.report().original_tape_len)
        .collect();
    let minimized_lengths: Vec<usize> = results
        .iter()
        .filter_map(|result| result.report().minimized_tape_len)
        .collect();
    let mut signatures = BTreeMap::<FailureSignature, u64>::new();
    for result in results {
        if let Some(signature) = result.report().failure_signature.clone() {
            *signatures.entry(signature).or_insert(0) += 1;
        }
    }

    ExperimentMatrixReport {
        total_cases,
        recalled_cases,
        failed_cases,
        average_recall_rate,
        average_shrink_ratio,
        min_original_tape_len: original_lengths.iter().min().copied(),
        max_original_tape_len: original_lengths.iter().max().copied(),
        min_minimized_tape_len: minimized_lengths.iter().min().copied(),
        max_minimized_tape_len: minimized_lengths.iter().max().copied(),
        signatures: signatures.into_iter().collect(),
    }
}

/// Run an experiment and panic unless replay and minimized replay preserve the signature.
pub fn assert_recall(case: &ExperimentCase) -> ExperimentReport {
    match run_experiment_case(case) {
        RecallResult::Recalled(report) => report,
        RecallResult::NotRecalled(report) => {
            panic!(
                "experiment '{}' did not recall its expected failure: {:?}",
                case.name, report
            )
        }
    }
}

/// Export one experiment report as deterministic JSON.
pub fn experiment_report_to_json(report: &ExperimentReport) -> String {
    format!(
        "{{\"name\":\"{}\",\"seeds_attempted\":{},\"first_failing_seed\":{},\"failures_observed\":{},\"recall_rate\":{},\"recalled\":{},\"failure_signature\":{},\"original_tape_len\":{},\"minimized_tape_len\":{},\"shrink_ratio\":{},\"shrink_attempts\":{},\"accepted_removals\":{},\"artifact_json_bytes\":{},\"artifact_html_bytes\":{},\"replay_tape_input_len\":{},\"replay_tape_cursor\":{},\"replay_tape_consumed_all\":{},\"replay_tape_exhausted\":{},\"replay_trace_identical\":{},\"replay_history_identical\":{},\"replay_nemesis_identical\":{},\"replay_byte_identical\":{},\"replay_matched_signature\":{},\"minimized_matched_signature\":{}}}",
        escape_json(report.name),
        report.seeds_attempted,
        option_u64(report.first_failing_seed),
        report.failures_observed,
        f64_json(report.recall_rate),
        report.recalled,
        option_signature_json(report.failure_signature.as_ref()),
        option_usize(report.original_tape_len),
        option_usize(report.minimized_tape_len),
        option_f64(report.shrink_ratio),
        option_usize(report.shrink_attempts),
        option_usize(report.accepted_removals),
        option_usize(report.artifact_json_bytes),
        option_usize(report.artifact_html_bytes),
        option_usize(report.replay_tape_input_len),
        option_usize(report.replay_tape_cursor),
        option_bool(report.replay_tape_consumed_all),
        option_bool(report.replay_tape_exhausted),
        report.replay_trace_identical,
        report.replay_history_identical,
        report.replay_nemesis_identical,
        report.replay_byte_identical,
        report.replay_matched_signature,
        report.minimized_matched_signature,
    )
}

/// Export a matrix report as deterministic JSON.
pub fn experiment_matrix_report_to_json(report: &ExperimentMatrixReport) -> String {
    let signatures: Vec<String> = report
        .signatures
        .iter()
        .map(|(signature, count)| {
            format!(
                "{{\"signature\":{},\"count\":{}}}",
                signature_json(signature),
                count
            )
        })
        .collect();
    format!(
        "{{\"total_cases\":{},\"recalled_cases\":{},\"failed_cases\":{},\"average_recall_rate\":{},\"average_shrink_ratio\":{},\"min_original_tape_len\":{},\"max_original_tape_len\":{},\"min_minimized_tape_len\":{},\"max_minimized_tape_len\":{},\"signatures\":[{}]}}",
        report.total_cases,
        report.recalled_cases,
        report.failed_cases,
        f64_json(report.average_recall_rate),
        option_f64(report.average_shrink_ratio),
        option_usize(report.min_original_tape_len),
        option_usize(report.max_original_tape_len),
        option_usize(report.min_minimized_tape_len),
        option_usize(report.max_minimized_tape_len),
        signatures.join(","),
    )
}

fn option_signature_json(signature: Option<&FailureSignature>) -> String {
    signature
        .map(signature_json)
        .unwrap_or_else(|| "null".to_string())
}

fn signature_json(signature: &FailureSignature) -> String {
    match signature {
        FailureSignature::InvariantViolated(name) => format!(
            "{{\"type\":\"InvariantViolated\",\"name\":\"{}\"}}",
            escape_json(name)
        ),
        FailureSignature::NotLinearizable { conflict, model } => format!(
            "{{\"type\":\"NotLinearizable\",\"model\":\"{}\",\"conflict\":{}}}",
            escape_json(model),
            conflict
                .map(|(left, right)| format!("[{left},{right}]"))
                .unwrap_or_else(|| "null".to_string())
        ),
        FailureSignature::Deadlock => "{\"type\":\"Deadlock\"}".to_string(),
        FailureSignature::Panic(label) => {
            format!(
                "{{\"type\":\"Panic\",\"label\":\"{}\"}}",
                escape_json(label)
            )
        }
        FailureSignature::StorageCorruption(label) => format!(
            "{{\"type\":\"StorageCorruption\",\"label\":\"{}\"}}",
            escape_json(label)
        ),
    }
}

fn option_u64(value: Option<u64>) -> String {
    value
        .map(|n| n.to_string())
        .unwrap_or_else(|| "null".to_string())
}

fn option_usize(value: Option<usize>) -> String {
    value
        .map(|n| n.to_string())
        .unwrap_or_else(|| "null".to_string())
}

fn option_bool(value: Option<bool>) -> String {
    value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "null".to_string())
}

fn option_f64(value: Option<f64>) -> String {
    value.map(f64_json).unwrap_or_else(|| "null".to_string())
}

fn f64_json(value: f64) -> String {
    if value.is_finite() {
        value.to_string()
    } else {
        "null".to_string()
    }
}

fn escape_json(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

impl<F, C> PlantBugCase<F, C>
where
    F: Fn(u64) -> RunReport,
    C: Fn(&RunReport) -> bool,
{
    pub fn assert_reproduced(&self) {
        for seed in 0..self.seeds {
            let report = (self.run)(seed);
            assert!(
                (self.check)(&report),
                "plant-a-bug case '{}' was not reproduced at seed {seed}",
                self.name
            );
        }
    }
}

fn assert_reports_equal(seed: u64, label: &str, a: &RunReport, b: &RunReport) {
    assert_eq!(
        a.trace, b.trace,
        "{label} trace diverged for seed {seed} (reproduce with DST_SEED={seed})"
    );
    assert_eq!(
        a.trace.join("\n").into_bytes(),
        b.trace.join("\n").into_bytes(),
        "{label} byte trace diverged for seed {seed} (reproduce with DST_SEED={seed})"
    );
    assert_eq!(
        a.history, b.history,
        "{label} history diverged for seed {seed}"
    );
    assert_eq!(
        a.nemesis_trace, b.nemesis_trace,
        "{label} nemesis trace diverged for seed {seed}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use detersim_core::{ClockExt, Env, Network};
    use detersim_sim::scenarios::{
        partitioned_dual_leader_world, pingpong_world, pingpong_world_replay,
    };
    use detersim_sim::{SimEnv, World, WorldConfig};
    use std::time::Duration;

    #[test]
    fn same_seed_helper_accepts_pingpong() {
        let report = assert_deterministic(7, pingpong_world);
        assert!(!report.deadlocked);
    }

    #[test]
    fn replay_helper_accepts_pingpong() {
        let (_generated, replayed) =
            assert_replay_identical(7, pingpong_world, pingpong_world_replay);
        assert!(replayed.tape_replaying);
    }

    #[test]
    fn plant_bug_case_reports_dual_leader() {
        let case = PlantBugCase {
            name: "dual leader",
            seeds: 5,
            run: partitioned_dual_leader_world,
            check: |report: &RunReport| {
                report.history.contains(&"leader:0".to_string())
                    && report.history.contains(&"leader:1".to_string())
            },
        };
        case.assert_reproduced();
    }

    #[test]
    fn shrink_replay_failure_exports_artifacts() {
        fn run(seed: u64) -> RunReport {
            let mut world = World::with_config(
                seed,
                WorldConfig {
                    horizon_ns: 100_000_000,
                    max_events: 10_000,
                },
            );
            world.set_drop_percent(50);
            world.add_node(0, |env: SimEnv| async move {
                let net = env.net();
                let clock = env.clock();
                let result = clock.timeout(Duration::from_millis(20), net.recv()).await;
                if result.is_err() {
                    env.record("missing-message");
                }
            });
            world.add_node(1, |env: SimEnv| async move {
                env.net().send(0, b"hello".to_vec()).await;
            });
            world.run()
        }

        fn replay(seed: u64, tape: Vec<u64>) -> RunReport {
            let mut world = World::replay(
                seed,
                tape,
                WorldConfig {
                    horizon_ns: 100_000_000,
                    max_events: 10_000,
                },
            );
            world.set_drop_percent(50);
            world.add_node(0, |env: SimEnv| async move {
                let net = env.net();
                let clock = env.clock();
                let result = clock.timeout(Duration::from_millis(20), net.recv()).await;
                if result.is_err() {
                    env.record("missing-message");
                }
            });
            world.add_node(1, |env: SimEnv| async move {
                env.net().send(0, b"hello".to_vec()).await;
            });
            world.run()
        }

        let mut seed = 0;
        while !run(seed).history.contains(&"missing-message".to_string()) {
            seed += 1;
            assert!(seed < 100, "test setup could not find a failing seed");
        }

        let artifact = shrink_replay_failure(
            seed,
            run,
            replay,
            |report| report.history.contains(&"missing-message".to_string()),
            ShrinkConfig {
                max_attempts: 100,
                min_chunk_len: 2,
            },
        );
        assert!(artifact.shrink.reproduced);
        assert!(artifact.minimized_json.contains("\"history\""));
        assert!(artifact.minimized_html.contains("<!doctype html>"));
    }
}
