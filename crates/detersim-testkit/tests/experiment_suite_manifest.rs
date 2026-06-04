use detersim_shrink::ShrinkConfig;
use detersim_sim::RunReport;
use detersim_testkit::{
    experiment_suite_manifest_to_json, ArtifactPolicy, ControlKind, EvidenceClass,
    ExperimentBudget, ExperimentCase, ExperimentCaseManifest, ExperimentSuite,
    ExperimentSuiteManifest, FailureSignature, OracleKind, RecallPolicy,
};

fn empty_run(seed: u64) -> RunReport {
    RunReport {
        seed,
        trace: Vec::new(),
        nemesis_trace: Vec::new(),
        history: Vec::new(),
        coverage_signals: Vec::new(),
        tape_log: Vec::new(),
        tape_events: Vec::new(),
        tape_replaying: false,
        tape_input_len: None,
        tape_cursor: 0,
        tape_consumed_all: true,
        tape_exhausted: false,
        dispatched: 0,
        aborted: false,
        deadlocked: false,
        parked_tasks: 0,
        tape_log_len: 0,
    }
}

fn empty_replay(seed: u64, _tape: Vec<u64>) -> RunReport {
    empty_run(seed)
}

fn no_failure(_report: &RunReport) -> Option<FailureSignature> {
    None
}

#[test]
fn suite_manifest_serializes_policy_oracle_and_artifact_metadata() {
    let manifest = ExperimentSuiteManifest {
        name: "manifest-smoke",
        cases: vec![
            ExperimentCaseManifest {
                name: "negative-control",
                recall_policy: RecallPolicy::MustNotRecall,
                oracle: OracleKind::Linearizability,
                expected_signature: None,
                seed_count: 10,
                artifact_policy: ArtifactPolicy::Never,
                case_family: "manifest",
                bug_variant: "correct",
                control_kind: ControlKind::NegativeControl,
                expected_recall_rate: Some(0.0),
                evidence_class: EvidenceClass::Reporting,
            },
            ExperimentCaseManifest {
                name: "plant-bug",
                recall_policy: RecallPolicy::MustRecall,
                oracle: OracleKind::Invariant,
                expected_signature: Some(FailureSignature::InvariantViolated(
                    "plant-bug".to_string(),
                )),
                seed_count: 10,
                artifact_policy: ArtifactPolicy::OnFailure,
                case_family: "manifest",
                bug_variant: "plant-bug",
                control_kind: ControlKind::PlantBug,
                expected_recall_rate: Some(1.0),
                evidence_class: EvidenceClass::Shrink,
            },
        ],
    };

    let json = experiment_suite_manifest_to_json(&manifest);
    assert!(json.contains("\"schema_version\":3"));
    assert!(json.contains("\"suite\":\"manifest-smoke\""));
    assert!(json.contains("\"recall_policy\":\"must_not_recall\""));
    assert!(json.contains("\"oracle\":\"linearizability\""));
    assert!(json.contains("\"artifact_policy\":\"on_failure\""));
    assert!(json.contains("\"type\":\"InvariantViolated\""));
    assert!(json.contains("\"case_family\":\"manifest\""));
    assert!(json.contains("\"control_kind\":\"plant_bug\""));
    assert!(json.contains("\"evidence_class\":\"shrink\""));
}

#[test]
fn runnable_suite_stays_separate_from_manifest_metadata() {
    let suite = ExperimentSuite {
        name: "runnable",
        cases: vec![(
            ExperimentCase {
                name: "case",
                budget: ExperimentBudget {
                    seed_count: 1,
                    shrink: ShrinkConfig::default(),
                },
                generate: empty_run,
                replay: empty_replay,
                oracle: no_failure,
            },
            RecallPolicy::MustNotRecall,
        )],
    };

    assert_eq!(suite.name, "runnable");
    assert_eq!(suite.cases.len(), 1);
}
