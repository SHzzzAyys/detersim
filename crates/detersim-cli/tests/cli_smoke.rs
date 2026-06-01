use std::process::Command;

#[test]
fn cli_doctor_search_and_explain_emit_v3_json() {
    for args in [
        vec!["doctor"],
        vec!["search", "--budget", "4", "--strategy", "coverage-guided"],
        vec!["explain"],
    ] {
        let output = Command::new(cli_bin())
            .args(args)
            .output()
            .expect("run detersim cli");
        assert!(
            output.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("\"schema_version\":3"));
    }
}

#[test]
fn cli_init_sut_and_render_examples_write_files() {
    let root = std::env::temp_dir().join(format!("detersim-cli-smoke-{}", std::process::id()));
    let template = root.join("template");
    let artifacts = root.join("artifacts");
    let _ = std::fs::remove_dir_all(&root);

    let init = Command::new(cli_bin())
        .args(["init-sut", &template.display().to_string()])
        .output()
        .expect("run init-sut");
    assert!(init.status.success());
    assert!(template.join("Cargo.toml").exists());
    assert!(template.join("src").join("lib.rs").exists());

    let render = Command::new(cli_bin())
        .args(["render", "--examples", &artifacts.display().to_string()])
        .output()
        .expect("run render examples");
    assert!(render.status.success());
    assert!(artifacts.join("missing-message.json").exists());
    assert!(artifacts.join("missing-message.html").exists());

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
