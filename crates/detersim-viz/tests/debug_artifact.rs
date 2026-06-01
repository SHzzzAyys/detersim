use detersim_sim::scenarios::pingpong_world;
use detersim_viz::{debug_artifact_html, debug_artifact_to_json, DebugArtifact};

#[test]
fn debug_artifact_json_and_html_include_v2_sections() {
    let artifact = DebugArtifact {
        title: "example failure".to_string(),
        run: pingpong_world(3),
        experiment_json: Some("{\"case\":\"pingpong\"}".to_string()),
        checker_json: Some("{\"explored_states\":4}".to_string()),
        shrink_json: Some("{\"signature_preserved\":true}".to_string()),
        failure_signature_json: Some("{\"type\":\"InvariantViolated\"}".to_string()),
    };

    let json = debug_artifact_to_json(&artifact);
    assert!(json.contains("\"schema_version\":2"));
    assert!(json.contains("\"tape_events\""));
    assert!(json.contains("\"experiment\""));

    let html = debug_artifact_html(&artifact);
    assert!(html.contains("<!doctype html>"));
    assert!(html.contains("failure signature"));
    assert!(html.contains("tape events"));
    assert!(!html.contains("https://"));
}
