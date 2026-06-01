use std::collections::BTreeMap;

use detersim_check::models::{
    AppendLogInput, AppendLogOutput, AppendOnlyLog, Register, RegisterInput, RegisterOutput,
};
use detersim_check::{
    check_linearizable, check_serializable, CheckBudget, LinearizabilityResult, OpRecord,
    SerializableResult, TxnAction, TxnOpRecord,
};

#[test]
fn checker_artifact_serializes_linearizability_witness() {
    let history = vec![
        OpRecord::completed(1, 0, RegisterInput::Write(7), RegisterOutput::Ok, 0, 1),
        OpRecord::completed(2, 1, RegisterInput::Read, RegisterOutput::Value(7), 2, 3),
    ];
    let result = check_linearizable(&Register::new(0), &history, 100);
    let artifact = result.checker_artifact("register").to_json();
    assert!(matches!(result, LinearizabilityResult::Linearizable { .. }));
    assert!(artifact.contains("\"schema_version\":3"));
    assert!(artifact.contains("\"witness_order\":[1,2]"));
}

#[test]
fn append_log_model_detects_lost_update() {
    let history = vec![
        OpRecord::completed(
            1,
            0,
            AppendLogInput::Append("a"),
            AppendLogOutput::Index(0),
            0,
            1,
        ),
        OpRecord::completed(
            2,
            1,
            AppendLogInput::Append("b"),
            AppendLogOutput::Index(1),
            2,
            3,
        ),
        OpRecord::completed(
            3,
            2,
            AppendLogInput::ReadAll,
            AppendLogOutput::Entries(vec!["a"]),
            4,
            5,
        ),
    ];
    assert!(matches!(
        check_linearizable(&AppendOnlyLog::new(Vec::new()), &history, 100),
        LinearizabilityResult::NotLinearizable { .. }
    ));
}

#[test]
fn elle_lite_transaction_checker_detects_write_skew() {
    let history = vec![
        TxnOpRecord::committed(
            1,
            0,
            vec![
                TxnAction::Read {
                    key: "x",
                    value: Some(false),
                },
                TxnAction::Read {
                    key: "y",
                    value: Some(false),
                },
                TxnAction::Write {
                    key: "x",
                    value: true,
                },
            ],
            0,
            10,
        ),
        TxnOpRecord::committed(
            2,
            1,
            vec![
                TxnAction::Read {
                    key: "x",
                    value: Some(false),
                },
                TxnAction::Read {
                    key: "y",
                    value: Some(false),
                },
                TxnAction::Write {
                    key: "y",
                    value: true,
                },
            ],
            0,
            10,
        ),
    ];
    let initial = BTreeMap::from([("x", false), ("y", false)]);
    let result = check_serializable(initial, &history, CheckBudget { max_steps: 100 });
    assert!(matches!(
        result,
        SerializableResult::NotSerializable {
            conflict: Some((1, 2)),
            ..
        }
    ));
    assert!(result
        .checker_artifact("elle-lite")
        .to_json()
        .contains("not-serializable"));
}

#[test]
fn transaction_checker_reports_inconclusive_on_low_budget() {
    let history = vec![TxnOpRecord::committed(
        1,
        0,
        vec![TxnAction::Read {
            key: "x",
            value: Some(0),
        }],
        0,
        1,
    )];
    let initial = BTreeMap::from([("x", 0)]);
    assert!(matches!(
        check_serializable(initial, &history, CheckBudget { max_steps: 0 }),
        SerializableResult::Inconclusive {
            budget_exhausted: true,
            ..
        }
    ));
}
