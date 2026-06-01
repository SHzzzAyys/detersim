use std::env;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use detersim_core::{ClockExt, Env, Network};
use detersim_search::{run_search, search_report_to_json, SearchBudget, SearchStrategy};
use detersim_shrink::ShrinkConfig;
use detersim_sim::{RunReport, SimEnv, World, WorldConfig};
use detersim_testkit::{
    experiment_summary_to_json, run_experiment_suite, shrink_replay_failure, ExperimentBudget,
    ExperimentCase, ExperimentSuite, FailureSignature, RecallPolicy,
};
use detersim_viz::{
    debug_artifact_html, debug_artifact_to_json, debug_artifact_v3_html, debug_artifact_v3_to_json,
    DebugArtifact, DebugArtifactV3,
};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("doctor") => doctor(),
        Some("init-sut") => init_sut(args.get(2).map(String::as_str)),
        Some("run-suite") => run_suite(args.get(2).map(String::as_str)),
        Some("search") => search(&args[2..]),
        Some("replay") => replay(args.get(2), args.get(3)),
        Some("shrink") => shrink(args.get(2).map(String::as_str)),
        Some("render") => render(
            args.get(2).map(String::as_str),
            args.get(3).map(String::as_str),
        ),
        Some("explain") => explain(args.get(2).map(String::as_str)),
        _ => {
            print_help();
            ExitCode::SUCCESS
        }
    }
}

fn doctor() -> ExitCode {
    let report = dropped_message_world(0);
    let suite = smoke_suite();
    let summary = run_experiment_suite(suite);
    let ok = !report.deadlocked && summary.policy_failures == 0;
    println!(
        "{{\"schema_version\":3,\"ok\":{},\"workspace\":\"{}\",\"sample_deadlocked\":{},\"sample_policy_failures\":{},\"commands\":[\"run-suite\",\"search\",\"replay\",\"shrink\",\"render\",\"explain\"]}}",
        ok,
        escape_json(env!("CARGO_MANIFEST_DIR")),
        report.deadlocked,
        summary.policy_failures
    );
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn init_sut(out: Option<&str>) -> ExitCode {
    let Some(out) = out else {
        eprintln!("usage: detersim init-sut <directory>");
        return ExitCode::from(2);
    };
    let root = Path::new(out);
    let src = root.join("src");
    let tests = root.join("tests");
    for dir in [&src, &tests] {
        if let Err(err) = std::fs::create_dir_all(dir) {
            eprintln!("failed to create template directory: {err}");
            return ExitCode::from(1);
        }
    }
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| ".".to_string());
    let cargo = format!(
        "[package]\nname = \"detersim-sut-template\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ndetersim-core = {{ path = \"{workspace}/crates/detersim-core\" }}\ndetersim-sim = {{ path = \"{workspace}/crates/detersim-sim\" }}\ndetersim-testkit = {{ path = \"{workspace}/crates/detersim-testkit\" }}\n"
    );
    let lib = "use std::time::Duration;\n\nuse detersim_core::{ClockExt, Env, Network};\nuse detersim_sim::{RunReport, SimEnv, World, WorldConfig};\n\npub fn run(seed: u64, drop_percent: u32) -> RunReport {\n    let mut world = World::with_config(seed, WorldConfig { horizon_ns: 100_000_000, max_events: 10_000 });\n    world.set_drop_percent(drop_percent);\n    world.add_node(0, |env: SimEnv| async move {\n        let result = env.clock().timeout(Duration::from_millis(20), env.net().recv()).await;\n        if result.is_err() { env.record(\"missing-message\"); }\n    });\n    world.add_node(1, |env: SimEnv| async move { env.net().send(0, b\"hello\".to_vec()).await; });\n    world.run()\n}\n";
    let test = "use detersim_sut_template::run;\nuse detersim_testkit::assert_deterministic;\n\n#[test]\nfn negative_control_is_deterministic() {\n    let report = assert_deterministic(0, |seed| run(seed, 0));\n    assert!(!report.history.contains(&\"missing-message\".to_string()));\n}\n\n#[test]\nfn plant_bug_is_visible() {\n    let report = run(0, 100);\n    assert!(report.history.contains(&\"missing-message\".to_string()));\n}\n";
    if let Err(err) = std::fs::write(root.join("Cargo.toml"), cargo)
        .and_then(|_| std::fs::write(src.join("lib.rs"), lib))
        .and_then(|_| std::fs::write(tests.join("detersim_sut.rs"), test))
    {
        eprintln!("failed to write template: {err}");
        return ExitCode::from(1);
    }
    println!(
        "{{\"schema_version\":3,\"created\":\"{}\",\"files\":[\"Cargo.toml\",\"src/lib.rs\",\"tests/detersim_sut.rs\"]}}",
        escape_json(&root.display().to_string())
    );
    ExitCode::SUCCESS
}

fn run_suite(out: Option<&str>) -> ExitCode {
    let summary = run_experiment_suite(smoke_suite());
    let json = experiment_summary_to_json(&summary);
    write_or_print(out, &json)
}

fn search(args: &[String]) -> ExitCode {
    let mut budget = 10u64;
    let mut strategy = SearchStrategy::CoverageGuided;
    let mut out = None;
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--budget" => {
                idx += 1;
                budget = args
                    .get(idx)
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(budget);
            }
            "--strategy" => {
                idx += 1;
                strategy = match args.get(idx).map(String::as_str) {
                    Some("random") => SearchStrategy::Random,
                    Some("failure-directed") => SearchStrategy::FailureDirected,
                    _ => SearchStrategy::CoverageGuided,
                };
            }
            "--suite" => {
                idx += 1;
            }
            value => out = Some(value),
        }
        idx += 1;
    }
    let case = smoke_case();
    let report = run_search(
        &case,
        strategy,
        SearchBudget {
            seed_count: budget,
            retain_candidates: 16,
        },
    );
    write_or_print(out, &search_report_to_json(&report))
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
    if seed == Some("--examples") {
        return render_examples(out.unwrap_or("target/detersim-artifacts/v3"));
    }
    let seed = seed.and_then(|seed| seed.parse::<u64>().ok()).unwrap_or(0);
    let report = dropped_message_world(seed);
    let signature_json = missing_message_signature(&report)
        .map(|_| "{\"type\":\"InvariantViolated\",\"name\":\"missing-message\"}".to_string());
    let artifact = DebugArtifact {
        title: format!("missing-message seed {seed}"),
        run: report,
        experiment_json: None,
        checker_json: None,
        shrink_json: None,
        failure_signature_json: signature_json,
    };
    write_or_print(out, &debug_artifact_html(&artifact))
}

fn explain(out: Option<&str>) -> ExitCode {
    let case = smoke_case();
    let summary = run_experiment_suite(smoke_suite());
    let search = run_search(
        &case,
        SearchStrategy::CoverageGuided,
        SearchBudget {
            seed_count: 10,
            retain_candidates: 8,
        },
    );
    let run = dropped_message_world(0);
    let artifact = DebugArtifactV3 {
        title: "missing-message explanation".to_string(),
        run,
        experiment_json: Some(experiment_summary_to_json(&summary)),
        search_json: Some(search_report_to_json(&search)),
        checker_json: None,
        shrink_json: Some("{\"kind\":\"signature-preserving\"}".to_string()),
        failure_signature_json: Some(
            "{\"type\":\"InvariantViolated\",\"name\":\"missing-message\"}".to_string(),
        ),
        coverage_json: None,
        causal_graph_json: Some("{\"nodes\":[\"send\",\"drop\",\"timeout\"],\"edges\":[[\"send\",\"drop\"],[\"drop\",\"timeout\"]]}".to_string()),
        environment_json: Some("{\"schema\":\"v3\",\"determinism\":\"same-binary-same-platform\"}".to_string()),
    };
    write_or_print(out, &debug_artifact_v3_to_json(&artifact))
}

fn render_examples(dir: &str) -> ExitCode {
    let root = Path::new(dir);
    if let Err(err) = std::fs::create_dir_all(root) {
        eprintln!("failed to create examples directory: {err}");
        return ExitCode::from(1);
    }
    let artifact = DebugArtifactV3 {
        title: "missing-message example".to_string(),
        run: dropped_message_world(0),
        experiment_json: Some(experiment_summary_to_json(&run_experiment_suite(
            smoke_suite(),
        ))),
        search_json: Some(search_report_to_json(&run_search(
            &smoke_case(),
            SearchStrategy::CoverageGuided,
            SearchBudget {
                seed_count: 10,
                retain_candidates: 8,
            },
        ))),
        checker_json: None,
        shrink_json: Some("{\"example\":\"v3\"}".to_string()),
        failure_signature_json: Some(
            "{\"type\":\"InvariantViolated\",\"name\":\"missing-message\"}".to_string(),
        ),
        coverage_json: None,
        causal_graph_json: Some("{\"nodes\":[],\"edges\":[]}".to_string()),
        environment_json: Some("{\"kind\":\"cli-example\"}".to_string()),
    };
    let json = root.join("missing-message.json");
    let html = root.join("missing-message.html");
    if let Err(err) = std::fs::write(&json, debug_artifact_v3_to_json(&artifact))
        .and_then(|_| std::fs::write(&html, debug_artifact_v3_html(&artifact)))
    {
        eprintln!("failed to write v3 examples: {err}");
        return ExitCode::from(1);
    }
    println!(
        "{{\"schema_version\":3,\"json\":\"{}\",\"html\":\"{}\"}}",
        escape_json(&json.display().to_string()),
        escape_json(&html.display().to_string())
    );
    ExitCode::SUCCESS
}

fn smoke_suite() -> ExperimentSuite {
    ExperimentSuite {
        name: "cli-smoke-suite",
        cases: vec![(smoke_case(), RecallPolicy::MustRecall)],
    }
}

fn smoke_case() -> ExperimentCase {
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
    }
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

fn print_help() {
    eprintln!(
        "usage:
  detersim doctor
  detersim init-sut <directory>
  detersim run-suite [out.json]
  detersim search [--suite smoke] [--budget n] [--strategy random|coverage-guided|failure-directed] [out.json]
  detersim replay <seed> <comma-separated-tape>
  detersim shrink [out.json]
  detersim render [seed] [out.html]
  detersim render --examples <directory>
  detersim explain [out.json]"
    );
}
