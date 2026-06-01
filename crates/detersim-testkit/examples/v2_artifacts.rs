use detersim_check::models::SingleKeyKv;
use detersim_check::{check_linearizable_with_budget, CheckBudget};
use detersim_core::SimTime;
use detersim_nemesis::NemesisAction;
use detersim_protocols::{
    run_mini_raft, run_mini_raft_kv_client, run_primary_backup_kv, run_primary_backup_kv_client,
    single_key_kv_history, KvBugVariant, KvConfig, MiniRaftConfig, RaftBugVariant,
};
use detersim_sim::{RunReport, SimEnv, World, WorldConfig};
use detersim_testkit::{
    experiment_report_to_json, linearizability_signature, ExperimentBudget, ExperimentCase,
};
use detersim_viz::{debug_artifact_html, debug_artifact_to_json, DebugArtifact};

fn main() {
    let kv_report = kv_ack_before_replicate(0);
    let kv_check = check_linearizable_with_budget(
        &SingleKeyKv::new(None),
        &single_key_kv_history(&kv_report.history),
        CheckBudget { max_steps: 10_000 },
    );
    let kv_artifact = DebugArtifact {
        title: "replicated-kv ack-before-replicate".to_string(),
        run: kv_report.clone(),
        experiment_json: Some(experiment_report_to_json(
            detersim_testkit::run_experiment_case(&ExperimentCase {
                name: "kv-ack-before-replicate-artifact",
                budget: ExperimentBudget {
                    seed_count: 5,
                    shrink: detersim_shrink::ShrinkConfig {
                        max_attempts: 100,
                        min_chunk_len: 2,
                    },
                },
                generate: kv_ack_before_replicate,
                replay: kv_ack_before_replicate_replay,
                oracle: kv_signature,
            })
            .report(),
        )),
        checker_json: Some(checker_stats_json(&kv_check.checker_stats())),
        shrink_json: None,
        failure_signature_json: linearizability_signature("single-key-kv", &kv_check)
            .map(|_| "{\"type\":\"NotLinearizable\",\"model\":\"single-key-kv\"}".to_string()),
    };

    let raft_report = mini_raft_stale_read(0);
    let raft_check = check_linearizable_with_budget(
        &SingleKeyKv::new(None),
        &single_key_kv_history(&raft_report.history),
        CheckBudget { max_steps: 10_000 },
    );
    let raft_artifact = DebugArtifact {
        title: "mini-raft follower stale read".to_string(),
        run: raft_report,
        experiment_json: None,
        checker_json: Some(checker_stats_json(&raft_check.checker_stats())),
        shrink_json: None,
        failure_signature_json: linearizability_signature("mini-raft-single-key-kv", &raft_check)
            .map(|_| {
                "{\"type\":\"NotLinearizable\",\"model\":\"mini-raft-single-key-kv\"}".to_string()
            }),
    };

    let kv_json = debug_artifact_to_json(&kv_artifact);
    let kv_html = debug_artifact_html(&kv_artifact);
    let raft_json = debug_artifact_to_json(&raft_artifact);
    let raft_html = debug_artifact_html(&raft_artifact);

    println!("kv-json-bytes={}", kv_json.len());
    println!("kv-html-bytes={}", kv_html.len());
    println!("mini-raft-json-bytes={}", raft_json.len());
    println!("mini-raft-html-bytes={}", raft_html.len());
}

fn config() -> WorldConfig {
    WorldConfig {
        horizon_ns: 2_000_000_000,
        max_events: 50_000,
    }
}

fn kv_ack_before_replicate(seed: u64) -> RunReport {
    run_kv(
        World::with_config(seed, config()),
        KvBugVariant::AckBeforeReplicate,
    )
}

fn kv_ack_before_replicate_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_kv(
        World::replay(seed, tape, config()),
        KvBugVariant::AckBeforeReplicate,
    )
}

fn run_kv(mut world: World, variant: KvBugVariant) -> RunReport {
    world.schedule_nemesis(
        SimTime::ZERO,
        NemesisAction::AsymmetricPartition { from: 0, to: 1 },
    );
    let config = KvConfig::with_bug(variant);
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

fn kv_signature(report: &RunReport) -> Option<detersim_testkit::FailureSignature> {
    let result = check_linearizable_with_budget(
        &SingleKeyKv::new(None),
        &single_key_kv_history(&report.history),
        CheckBudget { max_steps: 10_000 },
    );
    linearizability_signature("single-key-kv", &result)
}

fn checker_stats_json(stats: &detersim_check::CheckerStats) -> String {
    format!(
        "{{\"witness_order\":{},\"conflict_ops\":{},\"minimal_subhistory\":{},\"explored_states\":{},\"budget_exhausted\":{}}}",
        u64_array(&stats.witness_order),
        u64_array(&stats.conflict_ops),
        u64_array(&stats.minimal_subhistory),
        stats.explored_states,
        stats.budget_exhausted
    )
}

fn u64_array(values: &[u64]) -> String {
    let items: Vec<String> = values.iter().map(u64::to_string).collect();
    format!("[{}]", items.join(","))
}
