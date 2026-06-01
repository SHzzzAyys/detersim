use detersim_shrink::{shrink_tape_label_aware_with_config, RemovedLabel, ShrinkConfig};

#[test]
fn label_aware_shrink_reports_removed_label_summary() {
    let labels = ["net-delay", "drop-decision", "partition", "net-delay"];
    let report = shrink_tape_label_aware_with_config(
        &[10, 20, 99, 30],
        &labels,
        |candidate| candidate.contains(&99),
        ShrinkConfig {
            max_attempts: 100,
            min_chunk_len: 2,
        },
    );

    assert!(report.shrink.reproduced);
    assert_eq!(report.shrink.minimized, vec![99]);
    assert_eq!(report.shrink.kept_indices, vec![2]);
    assert_eq!(
        report.removed_labels,
        vec![
            RemovedLabel {
                label: "drop-decision".to_string(),
                count: 1,
            },
            RemovedLabel {
                label: "net-delay".to_string(),
                count: 2,
            }
        ]
    );
}
