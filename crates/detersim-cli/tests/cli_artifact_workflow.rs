use std::process::Command;

#[test]
fn cli_artifact_path_workflow_chains_suite_search_shrink_and_render() {
    let root = std::env::temp_dir().join(format!(
        "detersim-cli-artifact-workflow-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create temp root");

    let suite_json = root.join("suite.json");
    let suite = Command::new(cli_bin())
        .args([
            "run-suite",
            "--suite",
            "replicated-kv",
            "--out",
            &suite_json.display().to_string(),
        ])
        .output()
        .expect("run suite");
    assert!(
        suite.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&suite.stderr)
    );
    let suite_contents = std::fs::read_to_string(&suite_json).expect("read suite json");
    assert!(suite_contents.contains("\"schema_version\":3"));
    assert!(suite_contents.contains("\"policy_failures\":0"));

    let search_json = root.join("search.json");
    let search = Command::new(cli_bin())
        .args([
            "search",
            "--suite",
            "replicated-kv",
            "--compare",
            "--budget",
            "16",
            "--out",
            &search_json.display().to_string(),
        ])
        .output()
        .expect("run search");
    assert!(search.status.success());
    let search_contents = std::fs::read_to_string(&search_json).expect("read search json");
    assert!(search_contents.contains("\"strategy_wins\""));

    let shrink_json = root.join("shrink.json");
    let shrink = Command::new(cli_bin())
        .args([
            "shrink",
            "--case",
            "missing-message",
            "--seed",
            "0",
            "--out",
            &shrink_json.display().to_string(),
        ])
        .output()
        .expect("run shrink");
    assert!(shrink.status.success());
    let shrink_contents = std::fs::read_to_string(&shrink_json).expect("read shrink json");
    assert!(shrink_contents.contains("\"signature_preserved\":true"));
    assert!(shrink_contents.contains("\"causal_graph\""));

    let html = root.join("shrink.html");
    let render = Command::new(cli_bin())
        .args([
            "render",
            "--artifact",
            &shrink_json.display().to_string(),
            "--out",
            &html.display().to_string(),
        ])
        .output()
        .expect("render artifact");
    assert!(render.status.success());
    let html_contents = std::fs::read_to_string(&html).expect("read html");
    assert!(html_contents.contains("<!doctype html>"));
    assert!(html_contents.contains("failure signature"));

    let examples = root.join("examples");
    let render_examples = Command::new(cli_bin())
        .args(["render", "--examples", &examples.display().to_string()])
        .output()
        .expect("render examples");
    assert!(render_examples.status.success());
    assert!(examples.join("index.json").exists());
    assert!(examples.join("mini-raft-stale-read.html").exists());

    let _ = std::fs::remove_dir_all(&root);
}

fn cli_bin() -> String {
    std::env::var("CARGO_BIN_EXE_detersim-cli").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("current test exe path");
        while path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name != "debug" && name != "release")
        {
            path.pop();
        }
        path.push(if cfg!(windows) {
            "detersim-cli.exe"
        } else {
            "detersim-cli"
        });
        path.display().to_string()
    })
}
