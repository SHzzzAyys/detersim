use std::process::Command;

#[test]
fn cli_runs_real_benchmark_suites_and_search_comparison() {
    for suite in ["replicated-kv", "mini-raft-smoke", "storage-faults"] {
        let output = Command::new(cli_bin())
            .args(["run-suite", "--suite", suite])
            .output()
            .expect("run suite");
        assert!(
            output.status.success(),
            "suite {suite} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("\"schema_version\":3"));
        assert!(stdout.contains("\"summary\""));
        assert!(stdout.contains("\"policy_failures\":0"));
    }

    let search = Command::new(cli_bin())
        .args([
            "search",
            "--suite",
            "replicated-kv",
            "--budget",
            "16",
            "--strategy",
            "coverage-guided",
        ])
        .output()
        .expect("run search");
    assert!(search.status.success());
    assert!(String::from_utf8_lossy(&search.stdout).contains("\"schema_version\":3"));

    let compare = Command::new(cli_bin())
        .args([
            "search",
            "--suite",
            "replicated-kv",
            "--budget",
            "16",
            "--compare",
        ])
        .output()
        .expect("run comparison");
    assert!(compare.status.success());
    let stdout = String::from_utf8_lossy(&compare.stdout);
    assert!(stdout.contains("\"strategy_wins\""));
    assert!(stdout.contains("\"cases\""));
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
