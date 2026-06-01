use std::env;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use detersim_core::{ClockExt, Env, Network};
use detersim_shrink::ShrinkConfig;
use detersim_sim::{RunReport, SimEnv, World, WorldConfig};
use detersim_testkit::{
    experiment_summary_to_json, run_experiment_suite, shrink_replay_failure, ExperimentBudget,
    ExperimentCase, ExperimentSuite, FailureSignature, RecallPolicy,
};
use detersim_viz::{debug_artifact_html, debug_artifact_to_json, DebugArtifact};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("run-suite") => run_suite(args.get(2).map(String::as_str)),
        Some("replay") => replay(args.get(2), args.get(3)),
        Some("shrink") => shrink(args.get(2).map(String::as_str)),
        Some("render") => render(
            args.get(2).map(String::as_str),
            args.get(3).map(String::as_str),
        ),
        _ => {
            print_help();
            ExitCode::SUCCESS
        }
    }
}

fn run_suite(out: Option<&str>) -> ExitCode {
    let suite = ExperimentSuite {
        name: "cli-smoke-suite",
        cases: vec![(
            ExperimentCase {
                name: "missing-message",
                budget: ExperimentBudget {
                    seed_count: 10,
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
        )],
    };
    let summary = run_experiment_suite(suite);
    let json = experiment_summary_to_json(&summary);
    write_or_print(out, &json)
}

fn replay(seed: Option<&String>, tape: Option<&String>) -> ExitCode {
    let Some(seed) = seed.and_then(|seed| seed.parse::<u64>().ok()) else {
        eprintln!("usage: detersim replay <seed> <comma-separated-tape>");
        return ExitCode::from(2);
    };
    let Some(tape) = tape else {
        eprintln!("usage: detersim replay <seed> <comma-separated-tape>");
        return ExitCode::from(2);
    };
    let tape = parse_tape(tape);
    let report = dropped_message_world_replay(seed, tape);
    println!("{}", detersim_viz::run_report_to_json(&report));
    ExitCode::SUCCESS
}

fn shrink(out: Option<&str>) -> ExitCode {
    let artifact = shrink_replay_failure(
        0,
        dropped_message_world,
        dropped_message_world_replay,
        |report| missing_message_signature(report).is_some(),
        ShrinkConfig {
            max_attempts: 100,
            min_chunk_len: 2,
        },
    );
    let debug = DebugArtifact {
        title: "missing-message shrink".to_string(),
        run: artifact.minimized_replay,
        experiment_json: None,
        checker_json: None,
        shrink_json: Some(format!(
            "{{\"original_len\":{},\"minimized_len\":{},\"attempts\":{},\"accepted_removals\":{}}}",
            artifact.shrink.original_len,
            artifact.shrink.minimized.len(),
            artifact.shrink.attempts,
            artifact.shrink.accepted_removals
        )),
        failure_signature_json: Some(
            "{\"type\":\"InvariantViolated\",\"name\":\"missing-message\"}".to_string(),
        ),
    };
    write_or_print(out, &debug_artifact_to_json(&debug))
}

fn render(seed: Option<&str>, out: Option<&str>) -> ExitCode {
    let seed = seed.and_then(|seed| seed.parse::<u64>().ok()).unwrap_or(0);
    let report = dropped_message_world(seed);
    let artifact = DebugArtifact {
        title: format!("missing-message seed {seed}"),
        run: report,
        experiment_json: None,
        checker_json: None,
        shrink_json: None,
        failure_signature_json: missing_message_signature(&dropped_message_world(seed))
            .map(|_| "{\"type\":\"InvariantViolated\",\"name\":\"missing-message\"}".to_string()),
    };
    write_or_print(out, &debug_artifact_html(&artifact))
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

fn config() -> WorldConfig {
    WorldConfig {
        horizon_ns: 100_000_000,
        max_events: 10_000,
    }
}

fn missing_message_signature(report: &RunReport) -> Option<FailureSignature> {
    report
        .history
        .iter()
        .any(|entry| entry == "missing-message")
        .then(|| FailureSignature::InvariantViolated("missing-message".to_string()))
}

fn parse_tape(value: &str) -> Vec<u64> {
    if value.trim().is_empty() {
        Vec::new()
    } else {
        value
            .split(',')
            .filter_map(|item| item.trim().parse::<u64>().ok())
            .collect()
    }
}

fn write_or_print(out: Option<&str>, contents: &str) -> ExitCode {
    if let Some(path) = out {
        if let Some(parent) = Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(err) = std::fs::create_dir_all(parent) {
                    eprintln!("failed to create output directory: {err}");
                    return ExitCode::from(1);
                }
            }
        }
        if let Err(err) = std::fs::write(path, contents) {
            eprintln!("failed to write output: {err}");
            return ExitCode::from(1);
        }
    } else {
        println!("{contents}");
    }
    ExitCode::SUCCESS
}

fn print_help() {
    eprintln!(
        "usage:
  detersim run-suite [out.json]
  detersim replay <seed> <comma-separated-tape>
  detersim shrink [out.json]
  detersim render [seed] [out.html]"
    );
}
