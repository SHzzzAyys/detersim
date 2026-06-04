use detersim_sim::scenarios::{bitrot_wal_world, partitioned_dual_leader_world};
use detersim_viz::{
    debug_artifact_schema_version, debug_artifact_to_json, debug_artifact_v3_html,
    debug_artifact_v3_to_json, raw_debug_artifact_html, CausalGraph, DebugArtifact,
    DebugArtifactV3,
};

#[test]
fn schema_detection_accepts_v2_v3_and_v32_causal_artifacts() {
    let v2 = DebugArtifact {
        title: "v2".to_string(),
        run: partitioned_dual_leader_world(0),
        experiment_json: None,
        checker_json: None,
        shrink_json: None,
        failure_signature_json: None,
    };
    let v2_json = debug_artifact_to_json(&v2);
    assert_eq!(debug_artifact_schema_version(&v2_json), Some(2));

    let run = bitrot_wal_world(0);
    let checker = "{\"witness_order\":[0],\"minimal_subhistory\":[0]}";
    let shrink = "{\"removed_labels\":[{\"label\":\"jitter\",\"count\":1}]}";
    let graph = CausalGraph::from_sections(&run, Some(checker), Some(shrink));
    assert!(graph
        .edges
        .iter()
        .any(|edge| edge.kind == "checker-conflict-op"));
    assert!(graph
        .edges
        .iter()
        .any(|edge| edge.kind == "preserves-failure"));
    assert!(graph
        .edges
        .iter()
        .any(|edge| edge.kind == "nemesis-affects-history"));
    assert!(
        graph
            .edges
            .iter()
            .any(|edge| edge.kind == "delivery-wakes-next-trace")
            || graph
                .edges
                .iter()
                .any(|edge| edge.kind == "preserves-failure")
    );

    let v3 = DebugArtifactV3 {
        title: "v3.2".to_string(),
        run,
        experiment_json: None,
        search_json: None,
        checker_json: Some(checker.to_string()),
        shrink_json: Some(shrink.to_string()),
        failure_signature_json: Some(
            "{\"type\":\"StorageCorruption\",\"label\":\"bitrot\"}".to_string(),
        ),
        coverage_json: None,
        causal_graph_json: Some(graph.to_json()),
        environment_json: Some("{\"schema\":\"v3.2\"}".to_string()),
    };
    let v3_json = debug_artifact_v3_to_json(&v3);
    assert_eq!(debug_artifact_schema_version(&v3_json), Some(3));
    assert!(v3_json.contains("\"causal_graph\""));
    let v3_html = debug_artifact_v3_html(&v3);
    assert!(v3_html.contains("message table"));

    let html = raw_debug_artifact_html("compat", &v3_json);
    assert!(html.contains("<!doctype html>"));
    assert!(html.contains("failure signature"));
    assert!(!html.contains("https://"));
}
