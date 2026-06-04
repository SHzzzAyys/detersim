use std::env;
use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::Duration;

use detersim_check::models::{AppendOnlyLog, SingleKeyKv};
use detersim_check::{check_linearizable_with_budget, CheckBudget};
use detersim_core::{Clock, ClockExt, Env, Network, SimTime, Storage};
use detersim_nemesis::NemesisAction;
use detersim_net::{connect_pair, ConnectionId, StreamFault};
use detersim_protocols::{
    append_log_history, collect_protocol_events, run_mini_raft, run_mini_raft_kv_client,
    run_primary_backup_kv, run_primary_backup_kv_client, single_key_kv_history, KvBugVariant,
    KvConfig, MiniRaftConfig, RaftBugVariant, RaftInvariant, RAFT_OBSERVER_NODE,
};
use detersim_search::{
    compare_search_suite, run_search, search_report_to_json,
    suite_search_comparison_report_to_json, SearchBudget, SearchStrategy,
};
use detersim_shrink::{RemovedLabel, ShrinkConfig};
use detersim_sim::{RunReport, SimEnv, World, WorldConfig};
use detersim_testkit::{
    experiment_suite_manifest_to_json, experiment_summary_to_json, linearizability_signature,
    run_experiment_suite, shrink_replay_failure, ArtifactPolicy, ExperimentBudget, ExperimentCase,
    ExperimentCaseManifest, ExperimentSuite, ExperimentSuiteManifest, FailureSignature, OracleKind,
    RecallPolicy,
};
use detersim_viz::{
    debug_artifact_html, debug_artifact_v3_html, debug_artifact_v3_to_json,
    raw_debug_artifact_html, CausalGraph, DebugArtifact, DebugArtifactV3,
};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("doctor") => doctor(),
        Some("init-sut") => init_sut(&args[2..]),
        Some("run-suite") => run_suite(&args[2..]),
        Some("search") => search(&args[2..]),
        Some("replay") => replay(args.get(2), args.get(3)),
        Some("shrink") => shrink(&args[2..]),
        Some("render") => render(&args[2..]),
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
    let sample_suite_ok = summary.policy_failures == 0;
    let artifact_render_ok = {
        let artifact = missing_message_v3_artifact("doctor artifact", 0);
        debug_artifact_v3_to_json(&artifact).contains("\"schema_version\":3")
            && debug_artifact_v3_html(&artifact).contains("<!doctype html>")
    };
    let template_smoke_ok = template_generation_smoke();
    let rustc =
        command_output("rustc", &["--version"]).unwrap_or_else(|| "unavailable".to_string());
    let ok = !report.deadlocked && sample_suite_ok && artifact_render_ok && template_smoke_ok;
    println!(
        "{{\"schema_version\":3,\"ok\":{},\"workspace\":\"{}\",\"rustc\":\"{}\",\"sample_suite_ok\":{},\"artifact_render_ok\":{},\"template_smoke_ok\":{},\"sample_deadlocked\":{},\"sample_policy_failures\":{},\"determinism_lint_hint\":\"bash scripts/lint_determinism.sh\",\"commands\":[\"run-suite\",\"search\",\"replay\",\"shrink\",\"render\",\"explain\"]}}",
        ok,
        escape_json(&workspace_root()),
        escape_json(&rustc),
        sample_suite_ok,
        artifact_render_ok,
        template_smoke_ok,
        report.deadlocked,
        summary.policy_failures
    );
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn init_sut(args: &[String]) -> ExitCode {
    let mut name = "detersim-sut-template".to_string();
    let mut template = "message".to_string();
    let mut out = None::<String>;
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--name" => {
                idx += 1;
                if let Some(value) = args.get(idx) {
                    name = value.clone();
                }
            }
            "--template" => {
                idx += 1;
                if let Some(value) = args.get(idx) {
                    template = value.clone();
                }
            }
            value => out = Some(value.to_string()),
        }
        idx += 1;
    }
    let Some(out) = out else {
        eprintln!("usage: detersim init-sut [--name name] [--template message|stream] <directory>");
        return ExitCode::from(2);
    };
    if template != "message" && template != "stream" {
        eprintln!("unsupported template '{template}', expected message or stream");
        return ExitCode::from(2);
    }
    let root = Path::new(&out);
    let src = root.join("src");
    let tests = root.join("tests");
    for dir in [&src, &tests] {
        if let Err(err) = std::fs::create_dir_all(dir) {
            eprintln!("failed to create template directory: {err}");
            return ExitCode::from(1);
        }
    }
    let workspace = path_for_toml(&workspace_root());
    let crate_name = crate_ident(&name);
    let cargo = template_cargo(&name, &workspace, &template);
    let lib = if template == "stream" {
        stream_template_lib()
    } else {
        message_template_lib()
    };
    let test = template_test(&crate_name, &template);
    let readme = template_readme(&name, &template);
    if let Err(err) = std::fs::write(root.join("Cargo.toml"), cargo)
        .and_then(|_| std::fs::write(src.join("lib.rs"), lib))
        .and_then(|_| std::fs::write(tests.join("detersim_sut.rs"), test))
        .and_then(|_| std::fs::write(root.join("README.md"), readme))
    {
        eprintln!("failed to write template: {err}");
        return ExitCode::from(1);
    }
    println!(
        "{{\"schema_version\":3,\"created\":\"{}\",\"name\":\"{}\",\"template\":\"{}\",\"files\":[\"Cargo.toml\",\"README.md\",\"src/lib.rs\",\"tests/detersim_sut.rs\"]}}",
        escape_json(&root.display().to_string())
        , escape_json(&name), escape_json(&template)
    );
    ExitCode::SUCCESS
}

fn run_suite(args: &[String]) -> ExitCode {
    let (suite_name, out) = parse_suite_and_out(args, "smoke");
    let Some(suite) = suite_by_name(&suite_name) else {
        return unsupported_suite(out, &suite_name);
    };
    let manifest = suite_manifest_by_name(&suite_name);
    let summary = run_experiment_suite(suite);
    let summary_json = experiment_summary_to_json(&summary);
    let json = if let Some(manifest) = manifest {
        format!(
            "{{\"schema_version\":3,\"suite\":{},\"summary\":{}}}",
            experiment_suite_manifest_to_json(&manifest),
            summary_json
        )
    } else {
        summary_json
    };
    write_or_print(out.as_deref(), &json)
}

fn search(args: &[String]) -> ExitCode {
    let mut budget = 10u64;
    let mut strategy = SearchStrategy::CoverageGuided;
    let mut suite = "smoke".to_string();
    let mut compare = false;
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
                if let Some(value) = args.get(idx) {
                    suite = value.clone();
                }
            }
            "--compare" => compare = true,
            "--out" => {
                idx += 1;
                if let Some(value) = args.get(idx) {
                    out = Some(value.as_str());
                }
            }
            value => out = Some(value),
        }
        idx += 1;
    }
    let search_budget = SearchBudget {
        seed_count: budget,
        retain_candidates: 16,
    };
    if compare {
        let Some(suite_obj) = suite_by_name(&suite) else {
            return unsupported_suite(out.map(str::to_string), &suite);
        };
        let report = compare_search_suite(
            &suite_obj,
            &[
                SearchStrategy::Random,
                SearchStrategy::CoverageGuided,
                SearchStrategy::FailureDirected,
            ],
            search_budget,
        );
        write_or_print(out, &suite_search_comparison_report_to_json(&report))
    } else {
        let Some(case) = case_by_suite_name(&suite) else {
            return unsupported_suite(out.map(str::to_string), &suite);
        };
        let report = run_search(&case, strategy, search_budget);
        write_or_print(out, &search_report_to_json(&report))
    }
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

fn shrink(args: &[String]) -> ExitCode {
    let mut case_name = "missing-message".to_string();
    let mut seed = 0u64;
    let mut out = None::<String>;
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--case" => {
                idx += 1;
                if let Some(value) = args.get(idx) {
                    case_name = value.clone();
                }
            }
            "--seed" => {
                idx += 1;
                seed = args
                    .get(idx)
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(seed);
            }
            "--out" => {
                idx += 1;
                if let Some(value) = args.get(idx) {
                    out = Some(value.clone());
                }
            }
            value => out = Some(value.to_string()),
        }
        idx += 1;
    }

    let Some(case) = case_by_name(&case_name) else {
        let json = format!(
            "{{\"schema_version\":3,\"ok\":false,\"unsupported_case\":\"{}\",\"supported_cases\":{}}}",
            escape_json(&case_name),
            supported_cases_json()
        );
        return write_or_print(out.as_deref(), &json);
    };
    let generated = (case.generate)(seed);
    let Some(signature) = (case.oracle)(&generated) else {
        let json = format!(
            "{{\"schema_version\":3,\"ok\":false,\"case\":\"{}\",\"seed\":{},\"reason\":\"case_did_not_fail\"}}",
            escape_json(case.name),
            seed
        );
        return write_or_print(out.as_deref(), &json);
    };
    let signature_for_predicate = signature.clone();
    let artifact = shrink_replay_failure(
        seed,
        case.generate,
        case.replay,
        |report| (case.oracle)(report).as_ref() == Some(&signature_for_predicate),
        case.budget.shrink,
    );
    let shrink_json = format!(
        "{{\"original_len\":{},\"minimized_len\":{},\"attempts\":{},\"accepted_removals\":{},\"signature_preserved\":true,\"removed_labels\":{}}}",
        artifact.shrink.original_len,
        artifact.shrink.minimized.len(),
        artifact.shrink.attempts,
        artifact.shrink.accepted_removals,
        removed_labels_json_cli(&artifact.removed_labels)
    );
    let signature_json = failure_signature_json_cli(&signature);
    let graph =
        CausalGraph::from_sections(&artifact.minimized_replay, None, Some(&shrink_json)).to_json();
    let debug = DebugArtifactV3 {
        title: format!("{} shrink seed {}", case.name, seed),
        run: artifact.minimized_replay,
        experiment_json: None,
        search_json: None,
        checker_json: None,
        shrink_json: Some(shrink_json),
        failure_signature_json: Some(signature_json),
        coverage_json: None,
        causal_graph_json: Some(graph),
        environment_json: Some(
            "{\"schema\":\"v3.2\",\"workflow\":\"signature-preserving-shrink\"}".to_string(),
        ),
    };
    write_or_print(out.as_deref(), &debug_artifact_v3_to_json(&debug))
}

fn render(args: &[String]) -> ExitCode {
    let mut seed = None::<u64>;
    let mut examples = None::<String>;
    let mut artifact = None::<String>;
    let mut out = None::<String>;
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--examples" => {
                idx += 1;
                examples = Some(
                    args.get(idx)
                        .cloned()
                        .unwrap_or_else(|| "target/detersim-artifacts/v3".to_string()),
                );
            }
            "--artifact" => {
                idx += 1;
                if let Some(value) = args.get(idx) {
                    artifact = Some(value.clone());
                }
            }
            "--out" => {
                idx += 1;
                if let Some(value) = args.get(idx) {
                    out = Some(value.clone());
                }
            }
            value => {
                if seed.is_none() {
                    seed = value.parse::<u64>().ok();
                } else {
                    out = Some(value.to_string());
                }
            }
        }
        idx += 1;
    }

    if let Some(dir) = examples {
        return render_examples(&dir);
    }
    if let Some(path) = artifact {
        let Ok(json) = std::fs::read_to_string(&path) else {
            eprintln!("failed to read artifact: {path}");
            return ExitCode::from(1);
        };
        let version = detersim_viz::debug_artifact_schema_version(&json).unwrap_or(0);
        let html = raw_debug_artifact_html(&format!("DeterSim artifact schema {version}"), &json);
        return write_or_print(out.as_deref(), &html);
    }

    let seed = seed.unwrap_or(0);
    let report = dropped_message_world(seed);
    let signature_json =
        missing_message_signature(&report).map(|signature| failure_signature_json_cli(&signature));
    let artifact = DebugArtifact {
        title: format!("missing-message seed {seed}"),
        run: report,
        experiment_json: None,
        checker_json: None,
        shrink_json: None,
        failure_signature_json: signature_json,
    };
    write_or_print(out.as_deref(), &debug_artifact_html(&artifact))
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
    let missing = missing_message_v3_artifact("missing-message example", 0);
    let replicated = replicated_kv_artifact();
    let mini_raft = mini_raft_artifact();
    let storage = storage_fault_artifact();
    let stream = stream_transcript_artifact();
    let artifacts = [
        ("missing-message", missing),
        ("replicated-kv-stale-read", replicated),
        ("mini-raft-stale-read", mini_raft),
        ("storage-bitrot", storage),
        ("stream-transcript", stream),
    ];
    let mut written = Vec::new();
    for (name, artifact) in artifacts {
        let json_path = root.join(format!("{name}.json"));
        let html_path = root.join(format!("{name}.html"));
        if let Err(err) = std::fs::write(&json_path, debug_artifact_v3_to_json(&artifact))
            .and_then(|_| std::fs::write(&html_path, debug_artifact_v3_html(&artifact)))
        {
            eprintln!("failed to write v3 examples: {err}");
            return ExitCode::from(1);
        }
        written.push(format!(
            "{{\"json\":\"{}\",\"html\":\"{}\"}}",
            escape_json(&json_path.display().to_string()),
            escape_json(&html_path.display().to_string())
        ));
    }
    let index_path = root.join("index.json");
    let index_json = format!(
        "{{\"schema_version\":3,\"artifact_count\":{},\"artifacts\":[{}]}}",
        written.len(),
        written.join(",")
    );
    if let Err(err) = std::fs::write(&index_path, &index_json) {
        eprintln!("failed to write v3 example index: {err}");
        return ExitCode::from(1);
    }
    println!(
        "{{\"schema_version\":3,\"index\":\"{}\",\"artifacts\":[{}]}}",
        escape_json(&index_path.display().to_string()),
        written.join(",")
    );
    ExitCode::SUCCESS
}

fn template_cargo(name: &str, workspace: &str, template: &str) -> String {
    let net_dep = if template == "stream" {
        format!(
            "detersim-net = {{ path = \"{workspace}/crates/detersim-net\", version = \"0.1.0\" }}\n"
        )
    } else {
        String::new()
    };
    format!(
        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ndetersim-core = {{ path = \"{workspace}/crates/detersim-core\", version = \"0.1.0\" }}\ndetersim-sim = {{ path = \"{workspace}/crates/detersim-sim\", version = \"0.1.0\" }}\ndetersim-testkit = {{ path = \"{workspace}/crates/detersim-testkit\", version = \"0.1.0\" }}\ndetersim-viz = {{ path = \"{workspace}/crates/detersim-viz\", version = \"0.1.0\" }}\n{net_dep}",
        escape_toml(name)
    )
}

fn message_template_lib() -> &'static str {
    r#"use std::time::Duration;

use detersim_core::{ClockExt, Env, Network};
use detersim_sim::{RunReport, SimEnv, World, WorldConfig};
use detersim_testkit::{ExperimentBudget, ExperimentCase, FailureSignature};

pub fn run(seed: u64, drop_percent: u32) -> RunReport {
    let mut world = World::with_config(
        seed,
        WorldConfig {
            horizon_ns: 1_000_000_000,
            max_events: 10_000,
        },
    );
    world.set_drop_percent(drop_percent);
    world.add_node(0, |env: SimEnv| async move {
        let result = env
            .clock()
            .timeout(Duration::from_millis(200), env.net().recv())
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

pub fn plant_bug_case() -> ExperimentCase {
    ExperimentCase {
        name: "missing-message",
        budget: ExperimentBudget::default(),
        generate: plant_bug_run,
        replay: plant_bug_replay,
        oracle: missing_message_signature,
    }
}

fn plant_bug_run(seed: u64) -> RunReport {
    run(seed, 100)
}

fn plant_bug_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    let mut world = World::replay(
        seed,
        tape,
        WorldConfig {
            horizon_ns: 1_000_000_000,
            max_events: 10_000,
        },
    );
    world.set_drop_percent(100);
    world.add_node(0, |env: SimEnv| async move {
        let result = env
            .clock()
            .timeout(Duration::from_millis(200), env.net().recv())
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

pub fn missing_message_signature(report: &RunReport) -> Option<FailureSignature> {
    report
        .history
        .iter()
        .any(|entry| entry == "missing-message")
        .then(|| FailureSignature::InvariantViolated("missing-message".to_string()))
}
"#
}

fn stream_template_lib() -> &'static str {
    r#"use detersim_net::{connect_pair, ConnectionId, StreamFault};

pub fn transcript_lines() -> Vec<String> {
    let mut stream = connect_pair(0, 1, ConnectionId(1));
    stream.send(b"hello".to_vec(), &[]);
    stream.send(b"again".to_vec(), &[StreamFault::Duplicate { seq: 1 }]);
    stream.into_transcript().to_history_lines()
}
"#
}

fn template_test(crate_name: &str, template: &str) -> String {
    if template == "stream" {
        format!(
            "use {crate_name}::transcript_lines;\n\n#[test]\nfn stream_transcript_is_deterministic() {{\n    let a = transcript_lines();\n    let b = transcript_lines();\n    assert_eq!(a, b);\n    assert!(a.iter().any(|line| line == \"stream:duplicate:seq=1\"));\n}}\n"
        )
    } else {
        format!(
            "use {crate_name}::{{missing_message_signature, plant_bug_case, run}};\nuse detersim_testkit::{{assert_deterministic, run_experiment_case, RecallResult}};\nuse detersim_viz::{{debug_artifact_to_json, DebugArtifact}};\n\n#[test]\nfn negative_control_is_deterministic() {{\n    let report = assert_deterministic(0, |seed| run(seed, 0));\n    assert!(!report.history.contains(&\"missing-message\".to_string()));\n}}\n\n#[test]\nfn plant_bug_case_recalls_failure() {{\n    let case = plant_bug_case();\n    let result = run_experiment_case(&case);\n    assert!(matches!(result, RecallResult::Recalled(_)));\n}}\n\n#[test]\nfn artifact_render_test() {{\n    let report = run(0, 100);\n    let artifact = DebugArtifact {{\n        title: \"generated-template\".to_string(),\n        run: report.clone(),\n        experiment_json: None,\n        checker_json: None,\n        shrink_json: None,\n        failure_signature_json: missing_message_signature(&report).map(|_| \"{{\\\"type\\\":\\\"InvariantViolated\\\",\\\"name\\\":\\\"missing-message\\\"}}\".to_string()),\n    }};\n    let json = debug_artifact_to_json(&artifact);\n    assert!(json.contains(\"missing-message\"));\n}}\n"
        )
    }
}

fn template_readme(name: &str, template: &str) -> String {
    format!(
        "# {name}\n\nGenerated by `detersim init-sut --template {template}`.\n\nRun:\n\n```powershell\ncargo test\n```\n\nThis template is intentionally local-first and uses DeterSim path dependencies from the source checkout.\n"
    )
}

fn workspace_root() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| ".".to_string())
}

fn path_for_toml(path: &str) -> String {
    path.replace('\\', "/")
}

fn crate_ident(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn escape_toml(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn command_output(command: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn template_generation_smoke() -> bool {
    let workspace = path_for_toml(&workspace_root());
    let cargo = template_cargo("doctor-template", &workspace, "message");
    let lib = message_template_lib();
    let test = template_test("doctor_template", "message");
    cargo.contains("detersim-testkit")
        && cargo.contains("detersim-viz")
        && lib.contains("plant_bug_case")
        && lib.contains("ExperimentCase")
        && test.contains("artifact_render_test")
}

fn missing_message_v3_artifact(title: &str, seed: u64) -> DebugArtifactV3 {
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
    let run = dropped_message_world(seed);
    let graph = CausalGraph::from_run(&run).to_json();
    DebugArtifactV3 {
        title: title.to_string(),
        run,
        experiment_json: Some(experiment_summary_to_json(&summary)),
        search_json: Some(search_report_to_json(&search)),
        checker_json: None,
        shrink_json: Some("{\"kind\":\"signature-preserving\"}".to_string()),
        failure_signature_json: Some(
            "{\"type\":\"InvariantViolated\",\"name\":\"missing-message\"}".to_string(),
        ),
        coverage_json: None,
        causal_graph_json: Some(graph),
        environment_json: Some(
            "{\"schema\":\"v3\",\"determinism\":\"same-binary-same-platform\"}".to_string(),
        ),
    }
}

fn replicated_kv_artifact() -> DebugArtifactV3 {
    let run = replicated_kv_world(0);
    let checker = check_linearizable_with_budget(
        &SingleKeyKv::new(None),
        &single_key_kv_history(&run.history),
        CheckBudget { max_steps: 10_000 },
    )
    .checker_artifact("single-key-kv")
    .to_json();
    let shrink = "{\"kind\":\"signature-preserving\"}".to_string();
    let graph = CausalGraph::from_sections(&run, Some(&checker), Some(&shrink)).to_json();
    DebugArtifactV3 {
        title: "replicated-kv stale read".to_string(),
        causal_graph_json: Some(graph),
        run,
        experiment_json: Some(experiment_summary_to_json(&run_experiment_suite(
            replicated_kv_suite(),
        ))),
        search_json: None,
        checker_json: Some(checker),
        shrink_json: Some(shrink),
        failure_signature_json: Some(
            "{\"type\":\"NotLinearizable\",\"model\":\"single-key-kv\"}".to_string(),
        ),
        coverage_json: None,
        environment_json: Some("{\"kind\":\"cli-replicated-kv-example\"}".to_string()),
    }
}

fn mini_raft_artifact() -> DebugArtifactV3 {
    let run = mini_raft_smoke_world(0);
    let checker = check_linearizable_with_budget(
        &SingleKeyKv::new(None),
        &single_key_kv_history(&run.history),
        CheckBudget { max_steps: 10_000 },
    )
    .checker_artifact("single-key-kv")
    .to_json();
    let shrink = "{\"kind\":\"signature-preserving\"}".to_string();
    let graph = CausalGraph::from_sections(&run, Some(&checker), Some(&shrink)).to_json();
    DebugArtifactV3 {
        title: "mini-raft stale read".to_string(),
        causal_graph_json: Some(graph),
        run,
        experiment_json: Some(experiment_summary_to_json(&run_experiment_suite(
            mini_raft_smoke_suite(),
        ))),
        search_json: None,
        checker_json: Some(checker),
        shrink_json: Some(shrink),
        failure_signature_json: Some(
            "{\"type\":\"NotLinearizable\",\"model\":\"single-key-kv\"}".to_string(),
        ),
        coverage_json: None,
        environment_json: Some("{\"kind\":\"cli-mini-raft-example\"}".to_string()),
    }
}

fn storage_fault_artifact() -> DebugArtifactV3 {
    let run = storage_fault_world(0);
    let shrink = "{\"kind\":\"storage-fault\"}".to_string();
    let graph = CausalGraph::from_sections(&run, None, Some(&shrink)).to_json();
    DebugArtifactV3 {
        title: "storage bitrot".to_string(),
        causal_graph_json: Some(graph),
        run,
        experiment_json: Some(experiment_summary_to_json(&run_experiment_suite(
            storage_faults_suite(),
        ))),
        search_json: None,
        checker_json: None,
        shrink_json: Some(shrink),
        failure_signature_json: Some(
            "{\"type\":\"StorageCorruption\",\"label\":\"bitrot\"}".to_string(),
        ),
        coverage_json: None,
        environment_json: Some("{\"kind\":\"cli-storage-example\"}".to_string()),
    }
}

fn stream_transcript_artifact() -> DebugArtifactV3 {
    let report = stream_transcript_report(0);
    let graph = CausalGraph::from_run(&report).to_json();
    DebugArtifactV3 {
        title: "stream-transcript example".to_string(),
        run: report,
        experiment_json: None,
        search_json: None,
        checker_json: None,
        shrink_json: Some("{\"example\":\"stream\"}".to_string()),
        failure_signature_json: Some(
            "{\"type\":\"InvariantViolated\",\"name\":\"stream-transcript\"}".to_string(),
        ),
        coverage_json: Some("[\"stream:deliver\",\"stream:duplicate\"]".to_string()),
        causal_graph_json: Some(graph),
        environment_json: Some("{\"kind\":\"cli-stream-example\"}".to_string()),
    }
}

fn stream_transcript_report(seed: u64) -> RunReport {
    let mut stream = connect_pair(0, 1, ConnectionId(1));
    stream.send(b"hello".to_vec(), &[]);
    stream.send(b"again".to_vec(), &[StreamFault::Duplicate { seq: 1 }]);
    let transcript = stream.into_transcript();
    let history = transcript.to_history_lines();
    RunReport {
        seed,
        trace: history.clone(),
        nemesis_trace: Vec::new(),
        history,
        coverage_signals: vec![
            "stream:enqueue".to_string(),
            "stream:duplicate".to_string(),
            "stream:deliver".to_string(),
        ],
        tape_log: Vec::new(),
        tape_events: Vec::new(),
        tape_replaying: false,
        tape_input_len: None,
        tape_cursor: 0,
        tape_consumed_all: true,
        tape_exhausted: false,
        dispatched: transcript.delivered.len() as u64,
        aborted: false,
        deadlocked: false,
        parked_tasks: 0,
        tape_log_len: 0,
    }
}

fn smoke_suite() -> ExperimentSuite {
    ExperimentSuite {
        name: "cli-smoke-suite",
        cases: vec![(smoke_case(), RecallPolicy::MustRecall)],
    }
}

fn replicated_kv_suite() -> ExperimentSuite {
    ExperimentSuite {
        name: "replicated-kv",
        cases: vec![
            (replicated_kv_correct_case(), RecallPolicy::MustNotRecall),
            (
                replicated_kv_ack_before_replicate_case(),
                RecallPolicy::MustRecall,
            ),
            (replicated_kv_case(), RecallPolicy::MustRecall),
            (replicated_kv_lost_update_case(), RecallPolicy::MustRecall),
            (
                replicated_kv_duplicate_request_case(),
                RecallPolicy::MustRecall,
            ),
            (
                replicated_kv_follower_applies_uncommitted_case(),
                RecallPolicy::MustRecall,
            ),
            (
                replicated_kv_quorum_count_off_by_one_case(),
                RecallPolicy::MustRecall,
            ),
        ],
    }
}

fn mini_raft_smoke_suite() -> ExperimentSuite {
    ExperimentSuite {
        name: "mini-raft-smoke",
        cases: vec![
            (mini_raft_wrong_commit_rule_case(), RecallPolicy::MustRecall),
            (
                mini_raft_wrong_log_matching_case(),
                RecallPolicy::MustRecall,
            ),
            (mini_raft_smoke_case(), RecallPolicy::MustRecall),
            (
                mini_raft_duplicate_client_request_case(),
                RecallPolicy::MustRecall,
            ),
            (
                mini_raft_apply_before_commit_case(),
                RecallPolicy::MustRecall,
            ),
            (
                mini_raft_old_term_leader_commits_entry_case(),
                RecallPolicy::MustRecall,
            ),
            (
                mini_raft_term_not_persisted_case(),
                RecallPolicy::MustRecall,
            ),
            (
                mini_raft_vote_not_persisted_case(),
                RecallPolicy::MustRecall,
            ),
            (mini_raft_dual_leader_case(), RecallPolicy::MustRecall),
        ],
    }
}

fn storage_faults_suite() -> ExperimentSuite {
    ExperimentSuite {
        name: "storage-faults",
        cases: vec![
            (storage_ack_before_flush_case(), RecallPolicy::MustRecall),
            (storage_fault_case(), RecallPolicy::MustRecall),
            (storage_torn_write_case(), RecallPolicy::MustRecall),
        ],
    }
}

fn suite_by_name(name: &str) -> Option<ExperimentSuite> {
    match name {
        "smoke" => Some(smoke_suite()),
        "replicated-kv" => Some(replicated_kv_suite()),
        "mini-raft-smoke" => Some(mini_raft_smoke_suite()),
        "storage-faults" => Some(storage_faults_suite()),
        _ => None,
    }
}

fn suite_manifest_by_name(name: &str) -> Option<ExperimentSuiteManifest> {
    let suite = suite_by_name(name)?;
    let suite_name = suite.name;
    let cases = suite
        .cases
        .into_iter()
        .map(|(case, policy)| ExperimentCaseManifest {
            name: case.name,
            recall_policy: policy,
            oracle: oracle_kind_for_case(case.name),
            expected_signature: expected_signature_for_case(case.name),
            seed_count: case.budget.seed_count,
            artifact_policy: if matches!(policy, RecallPolicy::MustRecall) {
                ArtifactPolicy::OnFailure
            } else {
                ArtifactPolicy::Never
            },
        })
        .collect();
    Some(ExperimentSuiteManifest {
        name: suite_name,
        cases,
    })
}

fn case_by_suite_name(name: &str) -> Option<ExperimentCase> {
    match name {
        "smoke" => Some(smoke_case()),
        "replicated-kv" => Some(replicated_kv_case()),
        "mini-raft-smoke" => Some(mini_raft_smoke_case()),
        "storage-faults" => Some(storage_fault_case()),
        _ => None,
    }
}

fn case_by_name(name: &str) -> Option<ExperimentCase> {
    for suite_name in [
        "smoke",
        "replicated-kv",
        "mini-raft-smoke",
        "storage-faults",
    ] {
        if let Some(suite) = suite_by_name(suite_name) {
            for (case, _policy) in suite.cases {
                if case.name == name {
                    return Some(case);
                }
            }
        }
    }
    None
}

fn supported_cases_json() -> String {
    let mut cases = Vec::new();
    for suite_name in [
        "smoke",
        "replicated-kv",
        "mini-raft-smoke",
        "storage-faults",
    ] {
        if let Some(suite) = suite_by_name(suite_name) {
            for (case, _policy) in suite.cases {
                cases.push(format!("\"{}\"", escape_json(case.name)));
            }
        }
    }
    format!("[{}]", cases.join(","))
}

fn oracle_kind_for_case(name: &str) -> OracleKind {
    if name.contains("storage") {
        OracleKind::Storage
    } else if name.contains("term-not")
        || name.contains("vote-not")
        || name.contains("dual-leader")
        || name == "missing-message"
    {
        OracleKind::Invariant
    } else {
        OracleKind::Linearizability
    }
}

fn expected_signature_for_case(name: &str) -> Option<FailureSignature> {
    if name.contains("correct-negative-control") {
        None
    } else if name.contains("duplicate") {
        Some(FailureSignature::NotLinearizable {
            conflict: None,
            model: "append-log".to_string(),
        })
    } else if name.contains("storage-bitrot") {
        Some(FailureSignature::StorageCorruption("bitrot".to_string()))
    } else if name.contains("ack-before-flush") {
        Some(FailureSignature::StorageCorruption(
            "ack-before-flush-lost".to_string(),
        ))
    } else if name.contains("torn-write") {
        Some(FailureSignature::StorageCorruption(
            "torn-write".to_string(),
        ))
    } else if name.contains("term-not") {
        Some(FailureSignature::InvariantViolated(
            "raft-term-not-persisted".to_string(),
        ))
    } else if name.contains("vote-not") {
        Some(FailureSignature::InvariantViolated(
            "raft-vote-not-persisted".to_string(),
        ))
    } else if name.contains("dual-leader") {
        Some(FailureSignature::InvariantViolated(
            "raft-single-leader-per-term".to_string(),
        ))
    } else if name == "missing-message" {
        Some(FailureSignature::InvariantViolated(
            "missing-message".to_string(),
        ))
    } else {
        Some(FailureSignature::NotLinearizable {
            conflict: None,
            model: "single-key-kv".to_string(),
        })
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

fn replicated_kv_case() -> ExperimentCase {
    ExperimentCase {
        name: "replicated-kv-read-from-stale-follower",
        budget: ExperimentBudget {
            seed_count: 32,
            shrink: ShrinkConfig {
                max_attempts: 100,
                min_chunk_len: 2,
            },
        },
        generate: replicated_kv_world,
        replay: replicated_kv_world_replay,
        oracle: single_key_signature,
    }
}

fn replicated_kv_correct_case() -> ExperimentCase {
    kv_case(
        "replicated-kv-correct-negative-control",
        replicated_kv_correct_world,
        replicated_kv_correct_world_replay,
        single_key_signature,
    )
}

fn replicated_kv_ack_before_replicate_case() -> ExperimentCase {
    kv_case(
        "replicated-kv-ack-before-replicate",
        replicated_kv_ack_before_replicate_world,
        replicated_kv_ack_before_replicate_world_replay,
        single_key_signature,
    )
}

fn replicated_kv_lost_update_case() -> ExperimentCase {
    kv_case(
        "replicated-kv-lost-update",
        replicated_kv_lost_update_world,
        replicated_kv_lost_update_world_replay,
        single_key_signature,
    )
}

fn replicated_kv_duplicate_request_case() -> ExperimentCase {
    kv_case(
        "replicated-kv-duplicate-request-reapplied",
        replicated_kv_duplicate_request_world,
        replicated_kv_duplicate_request_world_replay,
        append_log_signature,
    )
}

fn replicated_kv_follower_applies_uncommitted_case() -> ExperimentCase {
    kv_case(
        "replicated-kv-follower-applies-uncommitted",
        replicated_kv_follower_applies_uncommitted_world,
        replicated_kv_follower_applies_uncommitted_world_replay,
        single_key_signature,
    )
}

fn replicated_kv_quorum_count_off_by_one_case() -> ExperimentCase {
    kv_case(
        "replicated-kv-quorum-count-off-by-one",
        replicated_kv_quorum_count_off_by_one_world,
        replicated_kv_quorum_count_off_by_one_world_replay,
        single_key_signature,
    )
}

fn kv_case(
    name: &'static str,
    generate: fn(u64) -> RunReport,
    replay: fn(u64, Vec<u64>) -> RunReport,
    oracle: fn(&RunReport) -> Option<FailureSignature>,
) -> ExperimentCase {
    ExperimentCase {
        name,
        budget: ExperimentBudget {
            seed_count: 32,
            shrink: ShrinkConfig {
                max_attempts: 100,
                min_chunk_len: 2,
            },
        },
        generate,
        replay,
        oracle,
    }
}

fn mini_raft_smoke_case() -> ExperimentCase {
    ExperimentCase {
        name: "mini-raft-follower-stale-read",
        budget: ExperimentBudget {
            seed_count: 32,
            shrink: ShrinkConfig {
                max_attempts: 100,
                min_chunk_len: 2,
            },
        },
        generate: mini_raft_smoke_world,
        replay: mini_raft_smoke_world_replay,
        oracle: protocol_history_signature,
    }
}

fn mini_raft_wrong_commit_rule_case() -> ExperimentCase {
    mini_raft_checker_case(
        "mini-raft-wrong-commit-rule",
        mini_raft_wrong_commit_rule_world,
        mini_raft_wrong_commit_rule_world_replay,
    )
}

fn mini_raft_wrong_log_matching_case() -> ExperimentCase {
    mini_raft_checker_case(
        "mini-raft-wrong-log-matching",
        mini_raft_wrong_log_matching_world,
        mini_raft_wrong_log_matching_world_replay,
    )
}

fn mini_raft_duplicate_client_request_case() -> ExperimentCase {
    mini_raft_checker_case(
        "mini-raft-duplicate-client-request",
        mini_raft_duplicate_client_request_world,
        mini_raft_duplicate_client_request_world_replay,
    )
}

fn mini_raft_apply_before_commit_case() -> ExperimentCase {
    mini_raft_checker_case(
        "mini-raft-apply-before-commit",
        mini_raft_apply_before_commit_world,
        mini_raft_apply_before_commit_world_replay,
    )
}

fn mini_raft_old_term_leader_commits_entry_case() -> ExperimentCase {
    mini_raft_checker_case(
        "mini-raft-old-term-leader-commits-entry",
        mini_raft_old_term_leader_commits_entry_world,
        mini_raft_old_term_leader_commits_entry_world_replay,
    )
}

fn mini_raft_term_not_persisted_case() -> ExperimentCase {
    mini_raft_invariant_case(
        "mini-raft-term-not-persisted",
        mini_raft_term_not_persisted_world,
        mini_raft_term_not_persisted_world_replay,
        mini_raft_term_not_persisted_signature,
    )
}

fn mini_raft_vote_not_persisted_case() -> ExperimentCase {
    mini_raft_invariant_case(
        "mini-raft-vote-not-persisted",
        mini_raft_vote_not_persisted_world,
        mini_raft_vote_not_persisted_world_replay,
        mini_raft_vote_not_persisted_signature,
    )
}

fn mini_raft_dual_leader_case() -> ExperimentCase {
    mini_raft_invariant_case(
        "mini-raft-dual-leader-under-partition",
        mini_raft_dual_leader_world,
        mini_raft_dual_leader_world_replay,
        mini_raft_dual_leader_signature,
    )
}

fn mini_raft_checker_case(
    name: &'static str,
    generate: fn(u64) -> RunReport,
    replay: fn(u64, Vec<u64>) -> RunReport,
) -> ExperimentCase {
    ExperimentCase {
        name,
        budget: ExperimentBudget {
            seed_count: 32,
            shrink: ShrinkConfig {
                max_attempts: 100,
                min_chunk_len: 2,
            },
        },
        generate,
        replay,
        oracle: protocol_history_signature,
    }
}

fn mini_raft_invariant_case(
    name: &'static str,
    generate: fn(u64) -> RunReport,
    replay: fn(u64, Vec<u64>) -> RunReport,
    oracle: fn(&RunReport) -> Option<FailureSignature>,
) -> ExperimentCase {
    ExperimentCase {
        name,
        budget: ExperimentBudget {
            seed_count: 16,
            shrink: ShrinkConfig {
                max_attempts: 100,
                min_chunk_len: 2,
            },
        },
        generate,
        replay,
        oracle,
    }
}

fn storage_fault_case() -> ExperimentCase {
    ExperimentCase {
        name: "storage-bitrot-smoke",
        budget: ExperimentBudget {
            seed_count: 4,
            shrink: ShrinkConfig {
                max_attempts: 10,
                min_chunk_len: 2,
            },
        },
        generate: storage_fault_world,
        replay: storage_fault_world_replay,
        oracle: storage_fault_signature,
    }
}

fn storage_ack_before_flush_case() -> ExperimentCase {
    ExperimentCase {
        name: "storage-ack-before-flush-lost-on-crash",
        budget: ExperimentBudget {
            seed_count: 4,
            shrink: ShrinkConfig {
                max_attempts: 10,
                min_chunk_len: 2,
            },
        },
        generate: storage_ack_before_flush_world,
        replay: storage_ack_before_flush_world_replay,
        oracle: storage_ack_before_flush_signature,
    }
}

fn storage_torn_write_case() -> ExperimentCase {
    ExperimentCase {
        name: "storage-torn-write",
        budget: ExperimentBudget {
            seed_count: 4,
            shrink: ShrinkConfig {
                max_attempts: 10,
                min_chunk_len: 2,
            },
        },
        generate: storage_torn_write_world,
        replay: storage_torn_write_world_replay,
        oracle: storage_torn_write_signature,
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

fn replicated_kv_world(seed: u64) -> RunReport {
    run_replicated_kv_variant(
        World::with_config(seed, protocol_config()),
        KvBugVariant::ReadFromStaleFollower,
    )
}

fn replicated_kv_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_replicated_kv_variant(
        World::replay(seed, tape, protocol_config()),
        KvBugVariant::ReadFromStaleFollower,
    )
}

fn replicated_kv_correct_world(seed: u64) -> RunReport {
    run_replicated_kv_variant(
        World::with_config(seed, protocol_config()),
        KvBugVariant::Correct,
    )
}

fn replicated_kv_correct_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_replicated_kv_variant(
        World::replay(seed, tape, protocol_config()),
        KvBugVariant::Correct,
    )
}

fn replicated_kv_ack_before_replicate_world(seed: u64) -> RunReport {
    run_replicated_kv_variant(
        World::with_config(seed, protocol_config()),
        KvBugVariant::AckBeforeReplicate,
    )
}

fn replicated_kv_ack_before_replicate_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_replicated_kv_variant(
        World::replay(seed, tape, protocol_config()),
        KvBugVariant::AckBeforeReplicate,
    )
}

fn replicated_kv_lost_update_world(seed: u64) -> RunReport {
    run_replicated_kv_variant(
        World::with_config(seed, protocol_config()),
        KvBugVariant::LostUpdate,
    )
}

fn replicated_kv_lost_update_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_replicated_kv_variant(
        World::replay(seed, tape, protocol_config()),
        KvBugVariant::LostUpdate,
    )
}

fn replicated_kv_duplicate_request_world(seed: u64) -> RunReport {
    run_replicated_kv_variant(
        World::with_config(seed, protocol_config()),
        KvBugVariant::DuplicateRequestReapplied,
    )
}

fn replicated_kv_duplicate_request_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_replicated_kv_variant(
        World::replay(seed, tape, protocol_config()),
        KvBugVariant::DuplicateRequestReapplied,
    )
}

fn replicated_kv_follower_applies_uncommitted_world(seed: u64) -> RunReport {
    run_replicated_kv_variant(
        World::with_config(seed, protocol_config()),
        KvBugVariant::FollowerAppliesUncommitted,
    )
}

fn replicated_kv_follower_applies_uncommitted_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_replicated_kv_variant(
        World::replay(seed, tape, protocol_config()),
        KvBugVariant::FollowerAppliesUncommitted,
    )
}

fn replicated_kv_quorum_count_off_by_one_world(seed: u64) -> RunReport {
    run_replicated_kv_variant(
        World::with_config(seed, protocol_config()),
        KvBugVariant::QuorumCountOffByOne,
    )
}

fn replicated_kv_quorum_count_off_by_one_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_replicated_kv_variant(
        World::replay(seed, tape, protocol_config()),
        KvBugVariant::QuorumCountOffByOne,
    )
}

fn run_replicated_kv_variant(mut world: World, variant: KvBugVariant) -> RunReport {
    if matches!(variant, KvBugVariant::AckBeforeReplicate) {
        world.schedule_nemesis(
            SimTime::ZERO,
            NemesisAction::AsymmetricPartition { from: 0, to: 1 },
        );
    }
    let config = KvConfig::with_bug(variant);
    world.add_nodes(3, move |env: SimEnv| run_primary_backup_kv(env, config));
    world.add_node(4, move |env: SimEnv| async move {
        for op in run_primary_backup_kv_client(env.clone(), config).await {
            env.record(op.to_history_line());
        }
    });
    world.run()
}

fn mini_raft_smoke_world(seed: u64) -> RunReport {
    run_mini_raft_client_variant(
        World::with_config(seed, protocol_config()),
        RaftBugVariant::FollowerStaleRead,
    )
}

fn mini_raft_smoke_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_mini_raft_client_variant(
        World::replay(seed, tape, protocol_config()),
        RaftBugVariant::FollowerStaleRead,
    )
}

fn mini_raft_wrong_commit_rule_world(seed: u64) -> RunReport {
    run_mini_raft_client_variant(
        World::with_config(seed, protocol_config()),
        RaftBugVariant::WrongCommitRule,
    )
}

fn mini_raft_wrong_commit_rule_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_mini_raft_client_variant(
        World::replay(seed, tape, protocol_config()),
        RaftBugVariant::WrongCommitRule,
    )
}

fn mini_raft_wrong_log_matching_world(seed: u64) -> RunReport {
    run_mini_raft_client_variant(
        World::with_config(seed, protocol_config()),
        RaftBugVariant::WrongLogMatching,
    )
}

fn mini_raft_wrong_log_matching_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_mini_raft_client_variant(
        World::replay(seed, tape, protocol_config()),
        RaftBugVariant::WrongLogMatching,
    )
}

fn mini_raft_duplicate_client_request_world(seed: u64) -> RunReport {
    run_mini_raft_client_variant(
        World::with_config(seed, protocol_config()),
        RaftBugVariant::DuplicateClientRequest,
    )
}

fn mini_raft_duplicate_client_request_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_mini_raft_client_variant(
        World::replay(seed, tape, protocol_config()),
        RaftBugVariant::DuplicateClientRequest,
    )
}

fn mini_raft_apply_before_commit_world(seed: u64) -> RunReport {
    run_mini_raft_client_variant(
        World::with_config(seed, protocol_config()),
        RaftBugVariant::ApplyBeforeCommit,
    )
}

fn mini_raft_apply_before_commit_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_mini_raft_client_variant(
        World::replay(seed, tape, protocol_config()),
        RaftBugVariant::ApplyBeforeCommit,
    )
}

fn mini_raft_old_term_leader_commits_entry_world(seed: u64) -> RunReport {
    run_mini_raft_client_variant(
        World::with_config(seed, protocol_config()),
        RaftBugVariant::OldTermLeaderCommitsEntry,
    )
}

fn mini_raft_old_term_leader_commits_entry_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_mini_raft_client_variant(
        World::replay(seed, tape, protocol_config()),
        RaftBugVariant::OldTermLeaderCommitsEntry,
    )
}

fn run_mini_raft_client_variant(mut world: World, variant: RaftBugVariant) -> RunReport {
    let config = MiniRaftConfig::with_bug(variant);
    world.add_nodes(3, move |env: SimEnv| run_mini_raft(env, config));
    world.add_node(4, move |env: SimEnv| async move {
        for op in run_mini_raft_kv_client(env.clone(), config).await {
            env.record(op.to_history_line());
        }
    });
    world.run()
}

fn mini_raft_term_not_persisted_world(seed: u64) -> RunReport {
    run_mini_raft_persistence_variant(
        World::with_config(seed, protocol_config()),
        RaftBugVariant::TermNotPersisted,
    )
}

fn mini_raft_term_not_persisted_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_mini_raft_persistence_variant(
        World::replay(seed, tape, protocol_config()),
        RaftBugVariant::TermNotPersisted,
    )
}

fn mini_raft_vote_not_persisted_world(seed: u64) -> RunReport {
    run_mini_raft_persistence_variant(
        World::with_config(seed, protocol_config()),
        RaftBugVariant::VoteNotPersisted,
    )
}

fn mini_raft_vote_not_persisted_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_mini_raft_persistence_variant(
        World::replay(seed, tape, protocol_config()),
        RaftBugVariant::VoteNotPersisted,
    )
}

fn run_mini_raft_persistence_variant(mut world: World, variant: RaftBugVariant) -> RunReport {
    world.schedule_nemesis(
        SimTime::from_nanos(1_000_000),
        NemesisAction::Crash { node: 0 },
    );
    world.schedule_nemesis(
        SimTime::from_nanos(2_000_000),
        NemesisAction::Restart { node: 0 },
    );
    let config = MiniRaftConfig::persistence_probe(variant);
    world.add_nodes(3, move |env: SimEnv| run_mini_raft(env, config));
    add_raft_observer(&mut world, 1);
    world.run()
}

fn mini_raft_dual_leader_world(seed: u64) -> RunReport {
    run_mini_raft_observed_variant(
        World::with_config(seed, protocol_config()),
        RaftBugVariant::DualLeaderUnderPartition,
        4,
    )
}

fn mini_raft_dual_leader_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_mini_raft_observed_variant(
        World::replay(seed, tape, protocol_config()),
        RaftBugVariant::DualLeaderUnderPartition,
        4,
    )
}

fn run_mini_raft_observed_variant(
    mut world: World,
    variant: RaftBugVariant,
    expected: usize,
) -> RunReport {
    let config = MiniRaftConfig::with_bug(variant);
    world.add_nodes(3, move |env: SimEnv| run_mini_raft(env, config));
    add_raft_observer(&mut world, expected);
    world.run()
}

fn add_raft_observer(world: &mut World, expected: usize) {
    world.add_node(RAFT_OBSERVER_NODE, move |env: SimEnv| async move {
        for label in collect_protocol_events(env.clone(), expected).await {
            env.record(label);
        }
    });
}

fn storage_fault_world(seed: u64) -> RunReport {
    run_storage_fault(World::with_config(seed, protocol_config()))
}

fn storage_fault_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_storage_fault(World::replay(seed, tape, protocol_config()))
}

fn storage_ack_before_flush_world(seed: u64) -> RunReport {
    run_storage_ack_before_flush(World::with_config(seed, protocol_config()))
}

fn storage_ack_before_flush_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_storage_ack_before_flush(World::replay(seed, tape, protocol_config()))
}

fn run_storage_ack_before_flush(mut world: World) -> RunReport {
    world.schedule_nemesis(SimTime::ZERO, NemesisAction::LostOnCrash { node: 0 });
    world.schedule_nemesis(
        SimTime::from_nanos(1_000_000),
        NemesisAction::Crash { node: 0 },
    );
    world.schedule_nemesis(
        SimTime::from_nanos(2_000_000),
        NemesisAction::Restart { node: 0 },
    );
    world.add_node(0, |env: SimEnv| async move {
        let storage = env.storage();
        if storage.len().await == 0 {
            storage.write_at(0, b"OKAY").await.expect("write WAL");
            env.record("ack-before-flush");
            env.clock().sleep(Duration::from_millis(100)).await;
        } else {
            let mut buf = [0u8; 4];
            let n = storage.read_at(0, &mut buf).await.expect("read WAL");
            env.record(format!("recovered:{}", String::from_utf8_lossy(&buf[..n])));
        }
    });
    world.run()
}

fn run_storage_fault(mut world: World) -> RunReport {
    world.add_node(0, |env: SimEnv| async move {
        let storage = env.storage();
        let _ = storage.write_at(0, b"ok").await;
        let _ = storage.flush().await;
        let mut bytes = [0u8; 2];
        let _ = storage.read_at(0, &mut bytes).await;
        env.record("storage-corruption:bitrot");
    });
    world.run()
}

fn storage_torn_write_world(seed: u64) -> RunReport {
    run_storage_torn_write(World::with_config(seed, protocol_config()))
}

fn storage_torn_write_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_storage_torn_write(World::replay(seed, tape, protocol_config()))
}

fn run_storage_torn_write(mut world: World) -> RunReport {
    world.schedule_nemesis(SimTime::ZERO, NemesisAction::TornWrite { node: 0 });
    world.add_node(0, |env: SimEnv| async move {
        let storage = env.storage();
        storage.write_at(0, b"ABCD").await.expect("write");
        storage.flush().await.expect("flush");
        let mut buf = [0u8; 4];
        let n = storage.read_at(0, &mut buf).await.expect("read");
        env.record(format!("torn:{}", String::from_utf8_lossy(&buf[..n])));
    });
    world.run()
}

fn config() -> WorldConfig {
    WorldConfig {
        horizon_ns: 100_000_000,
        max_events: 10_000,
    }
}

fn protocol_config() -> WorldConfig {
    WorldConfig {
        horizon_ns: 2_000_000_000,
        max_events: 20_000,
    }
}

fn missing_message_signature(report: &RunReport) -> Option<FailureSignature> {
    report
        .history
        .iter()
        .any(|entry| entry == "missing-message")
        .then(|| FailureSignature::InvariantViolated("missing-message".to_string()))
}

fn protocol_history_signature(report: &RunReport) -> Option<FailureSignature> {
    let log_history = append_log_history(&report.history);
    if !log_history.is_empty() {
        let result = check_linearizable_with_budget(
            &AppendOnlyLog::new(Vec::<String>::new()),
            &log_history,
            CheckBudget { max_steps: 10_000 },
        );
        return linearizability_signature("append-only-log", &result);
    }
    single_key_signature(report)
}

fn single_key_signature(report: &RunReport) -> Option<FailureSignature> {
    let result = check_linearizable_with_budget(
        &SingleKeyKv::new(None),
        &single_key_kv_history(&report.history),
        CheckBudget { max_steps: 10_000 },
    );
    linearizability_signature("single-key-kv", &result)
}

fn append_log_signature(report: &RunReport) -> Option<FailureSignature> {
    let result = check_linearizable_with_budget(
        &AppendOnlyLog::new(Vec::<String>::new()),
        &append_log_history(&report.history),
        CheckBudget { max_steps: 10_000 },
    );
    linearizability_signature("append-log", &result)
}

fn mini_raft_term_not_persisted_signature(report: &RunReport) -> Option<FailureSignature> {
    report
        .history
        .iter()
        .any(|entry| entry == "raft-bug:term-not-persisted")
        .then(|| FailureSignature::InvariantViolated("raft-term-not-persisted".to_string()))
}

fn mini_raft_vote_not_persisted_signature(report: &RunReport) -> Option<FailureSignature> {
    report
        .history
        .iter()
        .any(|entry| entry == "raft-bug:vote-not-persisted")
        .then(|| FailureSignature::InvariantViolated("raft-vote-not-persisted".to_string()))
}

fn mini_raft_dual_leader_signature(report: &RunReport) -> Option<FailureSignature> {
    let invariant = RaftInvariant::SingleLeaderPerTerm.as_label().to_string();
    report
        .history
        .iter()
        .any(|entry| entry == &invariant)
        .then(|| FailureSignature::InvariantViolated("raft-single-leader-per-term".to_string()))
}

fn storage_ack_before_flush_signature(report: &RunReport) -> Option<FailureSignature> {
    (report
        .history
        .iter()
        .any(|entry| entry == "ack-before-flush")
        && !report.history.iter().any(|entry| entry == "recovered:OKAY"))
    .then(|| FailureSignature::StorageCorruption("ack-before-flush-lost".to_string()))
}

fn storage_fault_signature(report: &RunReport) -> Option<FailureSignature> {
    report
        .history
        .iter()
        .any(|entry| entry == "storage-corruption:bitrot")
        .then(|| FailureSignature::StorageCorruption("bitrot".to_string()))
}

fn storage_torn_write_signature(report: &RunReport) -> Option<FailureSignature> {
    report
        .history
        .iter()
        .any(|entry| entry == "torn:AB")
        .then(|| FailureSignature::StorageCorruption("torn-write".to_string()))
}

fn parse_suite_and_out(args: &[String], default_suite: &str) -> (String, Option<String>) {
    let mut suite = default_suite.to_string();
    let mut out = None;
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--suite" => {
                idx += 1;
                if let Some(value) = args.get(idx) {
                    suite = value.clone();
                }
            }
            "--out" => {
                idx += 1;
                if let Some(value) = args.get(idx) {
                    out = Some(value.clone());
                }
            }
            value => out = Some(value.to_string()),
        }
        idx += 1;
    }
    (suite, out)
}

fn unsupported_suite(out: Option<String>, suite: &str) -> ExitCode {
    let json = format!(
        "{{\"schema_version\":3,\"ok\":false,\"unsupported_suite\":\"{}\",\"supported_suites\":[\"smoke\",\"replicated-kv\",\"mini-raft-smoke\",\"storage-faults\"],\"note\":\"test targets are the source of truth for full benchmark gates\"}}",
        escape_json(suite)
    );
    write_or_print(out.as_deref(), &json)
}

fn failure_signature_json_cli(signature: &FailureSignature) -> String {
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
        FailureSignature::Panic(label) => format!(
            "{{\"type\":\"Panic\",\"label\":\"{}\"}}",
            escape_json(label)
        ),
        FailureSignature::StorageCorruption(label) => format!(
            "{{\"type\":\"StorageCorruption\",\"label\":\"{}\"}}",
            escape_json(label)
        ),
    }
}

fn removed_labels_json_cli(labels: &[RemovedLabel]) -> String {
    let items: Vec<String> = labels
        .iter()
        .map(|label| {
            format!(
                "{{\"label\":\"{}\",\"count\":{}}}",
                escape_json(&label.label),
                label.count
            )
        })
        .collect();
    format!("[{}]", items.join(","))
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
  detersim run-suite [--suite smoke|replicated-kv|mini-raft-smoke|storage-faults] [out.json]
  detersim search [--suite smoke|replicated-kv|mini-raft-smoke|storage-faults] [--budget n] [--strategy random|coverage-guided|failure-directed] [--compare] [out.json]
  detersim replay <seed> <comma-separated-tape>
  detersim shrink [out.json]
  detersim render [seed] [out.html]
  detersim render --examples <directory>
  detersim explain [out.json]"
    );
}
