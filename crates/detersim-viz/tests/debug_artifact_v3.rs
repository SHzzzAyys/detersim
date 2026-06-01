use detersim_sim::scenarios::pingpong_world;
use detersim_viz::{
    debug_artifact_schema_version, debug_artifact_v3_html, debug_artifact_v3_to_json,
    DebugArtifactV3,
};

#[test]
fn debug_artifact_v3_contains_search_coverage_and_causal_graph() {
    let artifact = DebugArtifactV3 {
        title: "v3 artifact".to_string(),
        run: pingpong_world(3),
        experiment_json: Some("{\"case\":\"pingpong\"}".to_string()),
        search_json: Some("{\"strategy\":\"CoverageGuided\"}".to_string()),
        checker_json: Some("{\"outcome\":\"linearizable\"}".to_string()),
        shrink_json: Some("{\"signature_preserved\":true}".to_string()),
        failure_signature_json: Some("{\"type\":\"InvariantViolated\"}".to_string()),
        coverage_json: Some("[\"message-edge:0->1\"]".to_string()),
        causal_graph_json: Some(
            "{\"nodes\":[\"send\",\"deliver\"],\"edges\":[[\"send\",\"deliver\"]]}".to_string(),
        ),
        environment_json: Some("{\"platform\":\"test\"}".to_string()),
    };

    let json = debug_artifact_v3_to_json(&artifact);
    assert_eq!(debug_artifact_schema_version(&json), Some(3));
    assert!(json.contains("\"causal_graph\""));
    assert!(json.contains("\"search\""));

    let html = debug_artifact_v3_html(&artifact);
    assert!(html.contains("coverage"));
    assert!(html.contains("causal graph"));
    assert!(!html.contains("https://"));
}
