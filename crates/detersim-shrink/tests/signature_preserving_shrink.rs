use detersim_shrink::{ShrinkConfig, SignaturePreservingShrink};

#[test]
fn label_aware_signature_shrink_reports_removed_labels() {
    let tape = [0, 42, 7, 0];
    let labels = ["jitter", "partition", "drop-decision", "jitter"];
    let outcome = SignaturePreservingShrink::run_label_aware(
        &tape,
        &labels,
        |candidate| candidate.contains(&42).then_some("partition-failure"),
        ShrinkConfig {
            max_attempts: 100,
            min_chunk_len: 2,
        },
    );

    assert!(outcome.signature_preserved);
    assert_eq!(outcome.original_signature, Some("partition-failure"));
    assert_eq!(outcome.minimized_signature, Some("partition-failure"));
    assert_eq!(outcome.report.minimized, vec![42]);
    assert!(outcome
        .removed_labels
        .iter()
        .any(|label| label.label == "jitter" && label.count == 2));
    assert!(outcome.effectiveness.ratio < 1.0);
}
