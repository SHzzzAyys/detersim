use detersim_check::models::{AppendOnlyLog, SingleKeyKv};
use detersim_check::{check_linearizable_with_budget, CheckBudget, LinearizabilityResult};
use detersim_core::SimTime;
use detersim_nemesis::NemesisAction;
use detersim_protocols::{
    append_log_history, run_primary_backup_kv, run_primary_backup_kv_client, single_key_kv_history,
    KvBugVariant, KvConfig,
};
use detersim_shrink::ShrinkConfig;
use detersim_sim::{RunReport, SimEnv, World, WorldConfig};
use detersim_testkit::{
    assert_recall, linearizability_signature, run_experiment_case, ExperimentBudget,
    ExperimentCase, FailureSignature, RecallResult,
};

const KV_SEEDS: u64 = 500;

fn config() -> WorldConfig {
    WorldConfig {
        horizon_ns: 2_000_000_000,
        max_events: 50_000,
    }
}

fn kv_correct(seed: u64) -> RunReport {
    run_variant(World::with_config(seed, config()), KvBugVariant::Correct)
}

fn kv_correct_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_variant(World::replay(seed, tape, config()), KvBugVariant::Correct)
}

fn kv_ack_before_replicate(seed: u64) -> RunReport {
    run_variant(
        World::with_config(seed, config()),
        KvBugVariant::AckBeforeReplicate,
    )
}

fn kv_ack_before_replicate_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_variant(
        World::replay(seed, tape, config()),
        KvBugVariant::AckBeforeReplicate,
    )
}

fn kv_read_from_stale_follower(seed: u64) -> RunReport {
    run_variant(
        World::with_config(seed, config()),
        KvBugVariant::ReadFromStaleFollower,
    )
}

fn kv_read_from_stale_follower_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_variant(
        World::replay(seed, tape, config()),
        KvBugVariant::ReadFromStaleFollower,
    )
}

fn kv_lost_update(seed: u64) -> RunReport {
    run_variant(World::with_config(seed, config()), KvBugVariant::LostUpdate)
}

fn kv_lost_update_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_variant(
        World::replay(seed, tape, config()),
        KvBugVariant::LostUpdate,
    )
}

fn kv_duplicate_request(seed: u64) -> RunReport {
    run_variant(
        World::with_config(seed, config()),
        KvBugVariant::DuplicateRequestReapplied,
    )
}

fn kv_duplicate_request_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_variant(
        World::replay(seed, tape, config()),
        KvBugVariant::DuplicateRequestReapplied,
    )
}

fn kv_follower_applies_uncommitted(seed: u64) -> RunReport {
    run_variant(
        World::with_config(seed, config()),
        KvBugVariant::FollowerAppliesUncommitted,
    )
}

fn kv_follower_applies_uncommitted_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_variant(
        World::replay(seed, tape, config()),
        KvBugVariant::FollowerAppliesUncommitted,
    )
}

fn kv_quorum_count_off_by_one(seed: u64) -> RunReport {
    run_variant(
        World::with_config(seed, config()),
        KvBugVariant::QuorumCountOffByOne,
    )
}

fn kv_quorum_count_off_by_one_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_variant(
        World::replay(seed, tape, config()),
        KvBugVariant::QuorumCountOffByOne,
    )
}

fn run_variant(mut world: World, variant: KvBugVariant) -> RunReport {
    if matches!(variant, KvBugVariant::AckBeforeReplicate) {
        world.schedule_nemesis(
            SimTime::ZERO,
            NemesisAction::AsymmetricPartition { from: 0, to: 1 },
        );
    }

    let kv_config = KvConfig::with_bug(variant);
    world.add_nodes(3, move |env: SimEnv| run_primary_backup_kv(env, kv_config));
    world.add_node(3, move |env: SimEnv| async move {
        for op in run_primary_backup_kv_client(env.clone(), kv_config).await {
            env.record(op.to_history_line());
        }
    });
    world.run()
}

fn kv_signature(report: &RunReport) -> Option<FailureSignature> {
    let result = check_linearizable_with_budget(
        &SingleKeyKv::new(None),
        &single_key_kv_history(&report.history),
        CheckBudget { max_steps: 10_000 },
    );
    linearizability_signature("single-key-kv", &result)
}

fn log_signature(report: &RunReport) -> Option<FailureSignature> {
    let result = check_linearizable_with_budget(
        &AppendOnlyLog::new(Vec::<String>::new()),
        &append_log_history(&report.history),
        CheckBudget { max_steps: 10_000 },
    );
    linearizability_signature("append-log", &result)
}

fn assert_negative_control(generate: fn(u64) -> RunReport, replay: fn(u64, Vec<u64>) -> RunReport) {
    let case = ExperimentCase {
        name: "replicated-kv-correct-negative-control",
        budget: ExperimentBudget {
            seed_count: KV_SEEDS,
            shrink: ShrinkConfig::default(),
        },
        generate,
        replay,
        oracle: kv_signature,
    };
    let result = run_experiment_case(&case);
    assert!(
        matches!(result, RecallResult::NotRecalled(_)),
        "correct replicated KV produced a failure: {result:?}"
    );
    assert_eq!(result.report().failures_observed, 0);
}

fn assert_bug_case(
    name: &'static str,
    generate: fn(u64) -> RunReport,
    replay: fn(u64, Vec<u64>) -> RunReport,
    oracle: fn(&RunReport) -> Option<FailureSignature>,
    expected_model: &str,
) {
    let case = ExperimentCase {
        name,
        budget: ExperimentBudget {
            seed_count: KV_SEEDS,
            shrink: ShrinkConfig {
                max_attempts: 200,
                min_chunk_len: 2,
            },
        },
        generate,
        replay,
        oracle,
    };
    let report = assert_recall(&case);
    assert_eq!(report.seeds_attempted, KV_SEEDS);
    assert_eq!(report.failures_observed, KV_SEEDS);
    assert!(report.replay_byte_identical);
    assert!(report.minimized_matched_signature);
    assert!(matches!(
        report.failure_signature,
        Some(FailureSignature::NotLinearizable { ref model, .. }) if model == expected_model
    ));
}

#[test]
fn correct_primary_backup_kv_is_negative_control() {
    assert_negative_control(kv_correct, kv_correct_replay);
}

#[test]
fn ack_before_replicate_is_recalled() {
    assert_bug_case(
        "kv-ack-before-replicate",
        kv_ack_before_replicate,
        kv_ack_before_replicate_replay,
        kv_signature,
        "single-key-kv",
    );
}

#[test]
fn read_from_stale_follower_is_recalled() {
    assert_bug_case(
        "kv-read-from-stale-follower",
        kv_read_from_stale_follower,
        kv_read_from_stale_follower_replay,
        kv_signature,
        "single-key-kv",
    );
}

#[test]
fn lost_update_is_recalled() {
    assert_bug_case(
        "kv-lost-update",
        kv_lost_update,
        kv_lost_update_replay,
        kv_signature,
        "single-key-kv",
    );
}

#[test]
fn duplicate_request_reapplied_is_recalled() {
    assert_bug_case(
        "kv-duplicate-request-reapplied",
        kv_duplicate_request,
        kv_duplicate_request_replay,
        log_signature,
        "append-log",
    );
}

#[test]
fn follower_applies_uncommitted_is_recalled() {
    assert_bug_case(
        "kv-follower-applies-uncommitted",
        kv_follower_applies_uncommitted,
        kv_follower_applies_uncommitted_replay,
        kv_signature,
        "single-key-kv",
    );
}

#[test]
fn quorum_count_off_by_one_is_recalled() {
    assert_bug_case(
        "kv-quorum-count-off-by-one",
        kv_quorum_count_off_by_one,
        kv_quorum_count_off_by_one_replay,
        kv_signature,
        "single-key-kv",
    );
}

#[test]
fn kv_replay_trace_is_byte_identical_for_control() {
    let generated = kv_correct(7);
    let replayed = kv_correct_replay(7, generated.tape_log.clone());
    assert_eq!(generated.trace, replayed.trace);
    assert_eq!(generated.history, replayed.history);
    assert_eq!(generated.nemesis_trace, replayed.nemesis_trace);
    assert!(matches!(
        check_linearizable_with_budget(
            &SingleKeyKv::new(None),
            &single_key_kv_history(&generated.history),
            CheckBudget { max_steps: 10_000 }
        ),
        LinearizabilityResult::Linearizable { .. }
    ));
}
