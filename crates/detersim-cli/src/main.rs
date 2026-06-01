use std::env;
use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::Duration;

use detersim_check::models::{AppendOnlyLog, SingleKeyKv};
use detersim_check::{check_linearizable_with_budget, CheckBudget};
use detersim_core::{ClockExt, Env, Network, Storage};
use detersim_net::{connect_pair, ConnectionId, StreamFault};
use detersim_protocols::{
    append_log_history, run_mini_raft, run_mini_raft_kv_client, run_primary_backup_kv,
    run_primary_backup_kv_client, single_key_kv_history, KvBugVariant, KvConfig, MiniRaftConfig,
    RaftBugVariant,
};
use detersim_search::{
    compare_search_strategies, run_search, search_comparison_report_to_json, search_report_to_json,
    SearchBudget, SearchStrategy,
};
use detersim_shrink::ShrinkConfig;
use detersim_sim::{RunReport, SimEnv, World, WorldConfig};
use detersim_testkit::{
    experiment_summary_to_json, linearizability_signature, run_experiment_suite,
    shrink_replay_failure, ExperimentBudget, ExperimentCase, ExperimentSuite, FailureSignature,
    RecallPolicy,
};
use detersim_viz::{
    debug_artifact_html, debug_artifact_to_json, debug_artifact_v3_html, debug_artifact_v3_to_json,
    CausalGraph, DebugArtifact, DebugArtifactV3,
};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("doctor") => doctor(),
        Some("init-sut") => init_sut(&args[2..]),
        Some("run-suite") => run_suite(&args[2..]),
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
    let sample_suite_ok = summary.policy_failures == 0;
    let artifact_render_ok = {
        let artifact = missing_message_v3_artifact("doctor artifact", 0);
        debug_artifact_v3_to_json(&artifact).contains("\"schema_version\":3")
            && debug_artifact_v3_html(&artifact).contains("<!doctype html>")
    };
    let rustc =
        command_output("rustc", &["--version"]).unwrap_or_else(|| "unavailable".to_string());
    let ok = !report.deadlocked && sample_suite_ok && artifact_render_ok;
    println!(
        "{{\"schema_version\":3,\"ok\":{},\"workspace\":\"{}\",\"rustc\":\"{}\",\"sample_suite_ok\":{},\"artifact_render_ok\":{},\"sample_deadlocked\":{},\"sample_policy_failures\":{},\"determinism_lint_hint\":\"bash scripts/lint_determinism.sh\",\"commands\":[\"run-suite\",\"search\",\"replay\",\"shrink\",\"render\",\"explain\"]}}",
        ok,
        escape_json(&workspace_root()),
        escape_json(&rustc),
        sample_suite_ok,
        artifact_render_ok,
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
    let summary = run_experiment_suite(suite);
    let json = experiment_summary_to_json(&summary);
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
            value => out = Some(value),
        }
        idx += 1;
    }
    let Some(case) = case_by_suite_name(&suite) else {
        return unsupported_suite(out.map(str::to_string), &suite);
    };
    let search_budget = SearchBudget {
        seed_count: budget,
        retain_candidates: 16,
    };
    if compare {
        let report = compare_search_strategies(
            &case,
            &[SearchStrategy::Random, SearchStrategy::CoverageGuided],
            search_budget,
        );
        write_or_print(out, &search_comparison_report_to_json(&report))
    } else {
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
    println!(
        "{{\"schema_version\":3,\"artifacts\":[{}]}}",
        written.join(",")
    );
    ExitCode::SUCCESS
}

fn template_cargo(name: &str, workspace: &str, template: &str) -> String {
    let net_dep = if template == "stream" {
        format!("detersim-net = {{ path = \"{workspace}/crates/detersim-net\" }}\n")
    } else {
        String::new()
    };
    format!(
        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ndetersim-core = {{ path = \"{workspace}/crates/detersim-core\" }}\ndetersim-sim = {{ path = \"{workspace}/crates/detersim-sim\" }}\ndetersim-testkit = {{ path = \"{workspace}/crates/detersim-testkit\" }}\n{net_dep}",
        escape_toml(name)
    )
}

fn message_template_lib() -> &'static str {
    r#"use std::time::Duration;

use detersim_core::{ClockExt, Env, Network};
use detersim_sim::{RunReport, SimEnv, World, WorldConfig};

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
            "use {crate_name}::run;\nuse detersim_testkit::assert_deterministic;\n\n#[test]\nfn negative_control_is_deterministic() {{\n    let report = assert_deterministic(0, |seed| run(seed, 0));\n    assert!(!report.history.contains(&\"missing-message\".to_string()));\n}}\n\n#[test]\nfn plant_bug_is_visible() {{\n    let report = run(0, 100);\n    assert!(report.history.contains(&\"missing-message\".to_string()));\n}}\n"
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
    DebugArtifactV3 {
        title: "replicated-kv stale read".to_string(),
        causal_graph_json: Some(CausalGraph::from_run(&run).to_json()),
        run,
        experiment_json: Some(experiment_summary_to_json(&run_experiment_suite(
            replicated_kv_suite(),
        ))),
        search_json: None,
        checker_json: Some(checker),
        shrink_json: Some("{\"kind\":\"signature-preserving\"}".to_string()),
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
    DebugArtifactV3 {
        title: "mini-raft stale read".to_string(),
        causal_graph_json: Some(CausalGraph::from_run(&run).to_json()),
        run,
        experiment_json: Some(experiment_summary_to_json(&run_experiment_suite(
            mini_raft_smoke_suite(),
        ))),
        search_json: None,
        checker_json: Some(checker),
        shrink_json: Some("{\"kind\":\"signature-preserving\"}".to_string()),
        failure_signature_json: Some(
            "{\"type\":\"NotLinearizable\",\"model\":\"single-key-kv\"}".to_string(),
        ),
        coverage_json: None,
        environment_json: Some("{\"kind\":\"cli-mini-raft-example\"}".to_string()),
    }
}

fn storage_fault_artifact() -> DebugArtifactV3 {
    let run = storage_fault_world(0);
    DebugArtifactV3 {
        title: "storage bitrot".to_string(),
        causal_graph_json: Some(CausalGraph::from_run(&run).to_json()),
        run,
        experiment_json: Some(experiment_summary_to_json(&run_experiment_suite(
            storage_faults_suite(),
        ))),
        search_json: None,
        checker_json: None,
        shrink_json: Some("{\"kind\":\"storage-fault\"}".to_string()),
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
        cases: vec![(replicated_kv_case(), RecallPolicy::MustRecall)],
    }
}

fn mini_raft_smoke_suite() -> ExperimentSuite {
    ExperimentSuite {
        name: "mini-raft-smoke",
        cases: vec![(mini_raft_smoke_case(), RecallPolicy::MustRecall)],
    }
}

fn storage_faults_suite() -> ExperimentSuite {
    ExperimentSuite {
        name: "storage-faults",
        cases: vec![(storage_fault_case(), RecallPolicy::MustRecall)],
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

fn case_by_suite_name(name: &str) -> Option<ExperimentCase> {
    match name {
        "smoke" => Some(smoke_case()),
        "replicated-kv" => Some(replicated_kv_case()),
        "mini-raft-smoke" => Some(mini_raft_smoke_case()),
        "storage-faults" => Some(storage_fault_case()),
        _ => None,
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
    run_replicated_kv(World::with_config(seed, protocol_config()))
}

fn replicated_kv_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_replicated_kv(World::replay(seed, tape, protocol_config()))
}

fn run_replicated_kv(mut world: World) -> RunReport {
    let config = KvConfig::with_bug(KvBugVariant::ReadFromStaleFollower);
    world.add_nodes(3, move |env: SimEnv| run_primary_backup_kv(env, config));
    world.add_node(4, move |env: SimEnv| async move {
        for op in run_primary_backup_kv_client(env.clone(), config).await {
            env.record(op.to_history_line());
        }
    });
    world.run()
}

fn mini_raft_smoke_world(seed: u64) -> RunReport {
    run_mini_raft_smoke(World::with_config(seed, protocol_config()))
}

fn mini_raft_smoke_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_mini_raft_smoke(World::replay(seed, tape, protocol_config()))
}

fn run_mini_raft_smoke(mut world: World) -> RunReport {
    let config = MiniRaftConfig::with_bug(RaftBugVariant::FollowerStaleRead);
    world.add_nodes(3, move |env: SimEnv| run_mini_raft(env, config));
    world.add_node(4, move |env: SimEnv| async move {
        for op in run_mini_raft_kv_client(env.clone(), config).await {
            env.record(op.to_history_line());
        }
    });
    world.run()
}

fn storage_fault_world(seed: u64) -> RunReport {
    run_storage_fault(World::with_config(seed, protocol_config()))
}

fn storage_fault_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_storage_fault(World::replay(seed, tape, protocol_config()))
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

fn storage_fault_signature(report: &RunReport) -> Option<FailureSignature> {
    report
        .history
        .iter()
        .any(|entry| entry == "storage-corruption:bitrot")
        .then(|| FailureSignature::StorageCorruption("bitrot".to_string()))
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
