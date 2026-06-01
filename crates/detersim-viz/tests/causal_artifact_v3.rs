use detersim_sim::scenarios::pingpong_world;
use detersim_viz::{
    debug_artifact_v3_html, debug_artifact_v3_to_json, CausalGraph, DebugArtifactV3,
};

#[test]
fn causal_graph_uses_stable_ids_and_embeds_in_v3_artifact() {
    let run = pingpong_world(11);
    let graph = CausalGraph::from_run(&run);
    assert!(graph.nodes.iter().any(|node| node.id.starts_with("trace-")));
    assert!(graph
        .nodes
        .iter()
        .all(|node| !node.id.contains("0x") && !node.label.contains("0x")));
    assert!(graph.edges.iter().any(|edge| edge.kind == "trace-order"));

    let artifact = DebugArtifactV3 {
        title: "causal graph".to_string(),
        run,
        experiment_json: None,
        search_json: None,
        checker_json: None,
        shrink_json: None,
        failure_signature_json: None,
        coverage_json: None,
        causal_graph_json: Some(graph.to_json()),
        environment_json: None,
    };

    let json = debug_artifact_v3_to_json(&artifact);
    assert!(json.contains("\"causal_graph\""));
    assert!(json.contains("\"nodes\""));
    let html = debug_artifact_v3_html(&artifact);
    assert!(html.contains("causal graph"));
    assert!(!html.contains("https://"));
}
