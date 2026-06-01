use std::process::Command;

#[test]
fn cli_generates_and_exercises_message_template() {
    let root = std::env::temp_dir().join(format!("detersim-cli-e2e-{}", std::process::id()));
    let template = root.join("demo-message");
    let artifacts = root.join("artifacts");
    let _ = std::fs::remove_dir_all(&root);

    let init = Command::new(cli_bin())
        .args([
            "init-sut",
            "--name",
            "demo-message",
            "--template",
            "message",
            &template.display().to_string(),
        ])
        .output()
        .expect("run init-sut");
    assert!(
        init.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    assert!(template.join("Cargo.toml").exists());
    assert!(template.join("README.md").exists());
    assert!(template.join("src").join("lib.rs").exists());
    assert!(template.join("tests").join("detersim_sut.rs").exists());

    let generated_tests = Command::new("cargo")
        .arg("test")
        .current_dir(&template)
        .output()
        .expect("run generated cargo test");
    assert!(
        generated_tests.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&generated_tests.stdout),
        String::from_utf8_lossy(&generated_tests.stderr)
    );

    for args in [
        vec!["doctor"],
        vec!["search", "--suite", "smoke", "--budget", "8"],
        vec!["explain"],
    ] {
        let output = Command::new(cli_bin())
            .args(args)
            .output()
            .expect("run cli command");
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("\"schema_version\":3"));
    }

    let render = Command::new(cli_bin())
        .args(["render", "--examples", &artifacts.display().to_string()])
        .output()
        .expect("run render examples");
    assert!(render.status.success());
    assert!(artifacts.join("missing-message.html").exists());
    assert!(artifacts.join("stream-transcript.html").exists());

    let unsupported = Command::new(cli_bin())
        .args(["search", "--suite", "replicated-kv-placeholder"])
        .output()
        .expect("run unsupported suite");
    assert!(unsupported.status.success());
    let stdout = String::from_utf8_lossy(&unsupported.stdout);
    assert!(stdout.contains("\"unsupported_suite\""));
    assert!(stdout.contains("test targets are the source of truth"));

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
