use std::time::Duration;

use detersim_check::models::SingleKeyKv;
use detersim_check::{check_linearizable_with_budget, CheckBudget};
use detersim_core::{ClockExt, Env, Network, SimTime, Storage};
use detersim_nemesis::NemesisAction;
use detersim_net::{connect_pair, ConnectionId, StreamFault};
use detersim_protocols::{
    run_mini_raft, run_mini_raft_kv_client, run_primary_backup_kv, run_primary_backup_kv_client,
    single_key_kv_history, KvBugVariant, KvConfig, MiniRaftConfig, RaftBugVariant,
};
use detersim_sim::{RunReport, SimEnv, World, WorldConfig};
use detersim_testkit::{
    experiment_report_to_json, linearizability_signature, run_experiment_case, ExperimentBudget,
    ExperimentCase, FailureSignature,
};
use detersim_viz::{
    debug_artifact_v3_html, debug_artifact_v3_to_json, CausalGraph, DebugArtifactV3,
};

fn main() {
    emit("missing-message", missing_message_artifact());
    emit("replicated-kv-stale-read", kv_stale_read_artifact());
    emit("mini-raft-stale-read", mini_raft_stale_read_artifact());
    emit("storage-bitrot", storage_bitrot_artifact());
    emit("stream-transcript", stream_transcript_artifact());
}

fn emit(name: &str, artifact: DebugArtifactV3) {
    let json = debug_artifact_v3_to_json(&artifact);
    let html = debug_artifact_v3_html(&artifact);
    println!("{name}-json-bytes={}", json.len());
    println!("{name}-html-bytes={}", html.len());
}

fn missing_message_artifact() -> DebugArtifactV3 {
    let run = missing_message_world(0);
    let causal_graph = CausalGraph::from_run(&run).to_json();
    DebugArtifactV3 {
        title: "missing-message".to_string(),
        run,
        experiment_json: Some(experiment_report_to_json(
            run_experiment_case(&ExperimentCase {
                name: "missing-message-artifact",
                budget: ExperimentBudget {
                    seed_count: 5,
                    shrink: detersim_shrink::ShrinkConfig {
                        max_attempts: 100,
                        min_chunk_len: 2,
                    },
                },
                generate: missing_message_world,
                replay: missing_message_world_replay,
                oracle: missing_message_signature,
            })
            .report(),
        )),
        search_json: None,
        checker_json: None,
        shrink_json: Some("{\"kind\":\"signature-preserving\"}".to_string()),
        failure_signature_json: Some(
            "{\"type\":\"InvariantViolated\",\"name\":\"missing-message\"}".to_string(),
        ),
        coverage_json: None,
        causal_graph_json: Some(causal_graph),
        environment_json: Some("{\"example\":\"v3_artifacts\"}".to_string()),
    }
}

fn kv_stale_read_artifact() -> DebugArtifactV3 {
    let run = kv_read_from_stale_follower(0);
    let causal_graph = CausalGraph::from_run(&run).to_json();
    let check = check_linearizable_with_budget(
        &SingleKeyKv::new(None),
        &single_key_kv_history(&run.history),
        CheckBudget { max_steps: 10_000 },
    );
    DebugArtifactV3 {
        title: "replicated-kv stale follower read".to_string(),
        run,
        experiment_json: None,
        search_json: None,
        checker_json: Some(check.checker_artifact("single-key-kv").to_json()),
        shrink_json: None,
        failure_signature_json: linearizability_signature("single-key-kv", &check)
            .map(|_| "{\"type\":\"NotLinearizable\",\"model\":\"single-key-kv\"}".to_string()),
        coverage_json: None,
        causal_graph_json: Some(causal_graph),
        environment_json: Some("{\"example\":\"v3_artifacts\"}".to_string()),
    }
}

fn mini_raft_stale_read_artifact() -> DebugArtifactV3 {
    let run = mini_raft_stale_read(0);
    let causal_graph = CausalGraph::from_run(&run).to_json();
    let check = check_linearizable_with_budget(
        &SingleKeyKv::new(None),
        &single_key_kv_history(&run.history),
        CheckBudget { max_steps: 10_000 },
    );
    DebugArtifactV3 {
        title: "mini-raft stale follower read".to_string(),
        run,
        experiment_json: None,
        search_json: None,
        checker_json: Some(check.checker_artifact("mini-raft-single-key-kv").to_json()),
        shrink_json: None,
        failure_signature_json: linearizability_signature("mini-raft-single-key-kv", &check).map(
            |_| "{\"type\":\"NotLinearizable\",\"model\":\"mini-raft-single-key-kv\"}".to_string(),
        ),
        coverage_json: None,
        causal_graph_json: Some(causal_graph),
        environment_json: Some("{\"example\":\"v3_artifacts\"}".to_string()),
    }
}

fn storage_bitrot_artifact() -> DebugArtifactV3 {
    let run = storage_bitrot(0);
    DebugArtifactV3 {
        title: "storage bitrot".to_string(),
        causal_graph_json: Some(CausalGraph::from_run(&run).to_json()),
        run,
        experiment_json: None,
        search_json: None,
        checker_json: None,
        shrink_json: Some("{\"kind\":\"storage-fault\"}".to_string()),
        failure_signature_json: Some(
            "{\"type\":\"StorageCorruption\",\"label\":\"bitrot\"}".to_string(),
        ),
        coverage_json: None,
        environment_json: Some("{\"example\":\"v3_artifacts\"}".to_string()),
    }
}

fn stream_transcript_artifact() -> DebugArtifactV3 {
    let run = stream_transcript_report(0);
    DebugArtifactV3 {
        title: "stream transcript".to_string(),
        causal_graph_json: Some(CausalGraph::from_run(&run).to_json()),
        run,
        experiment_json: None,
        search_json: None,
        checker_json: None,
        shrink_json: Some("{\"example\":\"stream\"}".to_string()),
        failure_signature_json: Some(
            "{\"type\":\"InvariantViolated\",\"name\":\"stream-transcript\"}".to_string(),
        ),
        coverage_json: Some("[\"stream:deliver\",\"stream:duplicate\"]".to_string()),
        environment_json: Some("{\"example\":\"v3_artifacts\"}".to_string()),
    }
}

fn config() -> WorldConfig {
    WorldConfig {
        horizon_ns: 2_000_000_000,
        max_events: 50_000,
    }
}

fn missing_message_world(seed: u64) -> RunReport {
    run_missing_message(World::with_config(seed, config()))
}

fn missing_message_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_missing_message(World::replay(seed, tape, config()))
}

fn run_missing_message(mut world: World) -> RunReport {
    world.set_drop_percent(100);
    world.add_node(0, |env: SimEnv| async move {
        let result = env
            .clock()
            .timeout(Duration::from_millis(20), env.net().recv())
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

fn missing_message_signature(report: &RunReport) -> Option<FailureSignature> {
    report
        .history
        .iter()
        .any(|entry| entry == "missing-message")
        .then(|| FailureSignature::InvariantViolated("missing-message".to_string()))
}

fn kv_read_from_stale_follower(seed: u64) -> RunReport {
    let mut world = World::with_config(seed, config());
    world.schedule_nemesis(
        SimTime::ZERO,
        NemesisAction::AsymmetricPartition { from: 0, to: 1 },
    );
    let config = KvConfig::with_bug(KvBugVariant::ReadFromStaleFollower);
    world.add_nodes(3, move |env: SimEnv| run_primary_backup_kv(env, config));
    world.add_node(3, move |env: SimEnv| async move {
        for op in run_primary_backup_kv_client(env.clone(), config).await {
            env.record(op.to_history_line());
        }
    });
    world.run()
}

fn mini_raft_stale_read(seed: u64) -> RunReport {
    let mut world = World::with_config(seed, config());
    let config = MiniRaftConfig::with_bug(RaftBugVariant::FollowerStaleRead);
    world.add_nodes(3, move |env: SimEnv| run_mini_raft(env, config));
    world.add_node(4, move |env: SimEnv| async move {
        for op in run_mini_raft_kv_client(env.clone(), config).await {
            env.record(op.to_history_line());
        }
    });
    world.run()
}

fn storage_bitrot(seed: u64) -> RunReport {
    let mut world = World::with_config(seed, config());
    world.add_node(0, |env: SimEnv| async move {
        let storage = env.storage();
        let _ = storage.write_at(0, b"ok").await;
        let _ = storage.flush().await;
        env.record("storage-corruption:bitrot");
    });
    world.run()
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
