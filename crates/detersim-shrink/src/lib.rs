//! Entropy-tape shrinking.
//!
//! The shrinker is conservative: a candidate tape is accepted only after the
//! caller's failure predicate says it still reproduces. It starts with chunk
//! deletion, then finishes with single-draw deletion under the same budget.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShrinkReport {
    pub original_len: usize,
    pub minimized: Vec<u64>,
    pub kept_indices: Vec<usize>,
    pub attempts: usize,
    pub reproduced: bool,
    pub accepted_removals: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemovedLabel {
    pub label: String,
    pub count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShrinkEffectiveness {
    pub original_len: usize,
    pub minimized_len: usize,
    pub ratio: f64,
    pub attempts: usize,
    pub accepted_removals: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LabelAwareShrinkReport {
    pub shrink: ShrinkReport,
    pub removed_labels: Vec<RemovedLabel>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShrinkOutcome<S> {
    pub original_signature: Option<S>,
    pub minimized_signature: Option<S>,
    pub signature_preserved: bool,
    pub report: ShrinkReport,
    pub effectiveness: ShrinkEffectiveness,
}

pub struct SignaturePreservingShrink;

impl SignaturePreservingShrink {
    pub fn run<S, F>(tape: &[u64], mut classify: F, config: ShrinkConfig) -> ShrinkOutcome<S>
    where
        S: Clone + Eq,
        F: FnMut(&[u64]) -> Option<S>,
    {
        let original_signature = classify(tape);
        let report = if let Some(signature) = original_signature.clone() {
            shrink_tape_with_config(
                tape,
                |candidate| classify(candidate) == Some(signature.clone()),
                config,
            )
        } else {
            ShrinkReport {
                original_len: tape.len(),
                minimized: tape.to_vec(),
                kept_indices: (0..tape.len()).collect(),
                attempts: 1,
                reproduced: false,
                accepted_removals: 0,
            }
        };
        let minimized_signature = classify(&report.minimized);
        let signature_preserved =
            original_signature.is_some() && minimized_signature == original_signature;
        let effectiveness = ShrinkEffectiveness::from_report(&report);
        ShrinkOutcome {
            original_signature,
            minimized_signature,
            signature_preserved,
            report,
            effectiveness,
        }
    }
}

impl ShrinkEffectiveness {
    pub fn from_report(report: &ShrinkReport) -> Self {
        let minimized_len = report.minimized.len();
        let ratio = if report.original_len == 0 {
            1.0
        } else {
            minimized_len as f64 / report.original_len as f64
        };
        Self {
            original_len: report.original_len,
            minimized_len,
            ratio,
            attempts: report.attempts,
            accepted_removals: report.accepted_removals,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShrinkConfig {
    pub max_attempts: usize,
    pub min_chunk_len: usize,
}

impl Default for ShrinkConfig {
    fn default() -> Self {
        Self {
            max_attempts: 1_000,
            min_chunk_len: 2,
        }
    }
}

pub fn shrink_tape<F>(tape: &[u64], still_fails: F, max_attempts: usize) -> ShrinkReport
where
    F: FnMut(&[u64]) -> bool,
{
    shrink_tape_with_config(
        tape,
        still_fails,
        ShrinkConfig {
            max_attempts,
            ..ShrinkConfig::default()
        },
    )
}

pub fn shrink_tape_with_config<F>(
    tape: &[u64],
    mut still_fails: F,
    config: ShrinkConfig,
) -> ShrinkReport
where
    F: FnMut(&[u64]) -> bool,
{
    if !still_fails(tape) {
        return ShrinkReport {
            original_len: tape.len(),
            minimized: tape.to_vec(),
            kept_indices: (0..tape.len()).collect(),
            attempts: 1,
            reproduced: false,
            accepted_removals: 0,
        };
    }

    let mut attempts = 1usize;
    let mut kept_indices: Vec<usize> = (0..tape.len()).collect();
    let mut current = tape.to_vec();
    let mut accepted_removals = 0usize;
    let mut chunk_len = current.len() / 2;

    while chunk_len >= config.min_chunk_len && attempts < config.max_attempts {
        let mut idx = 0usize;
        let mut accepted_this_round = false;
        while idx + chunk_len <= current.len() && attempts < config.max_attempts {
            let mut candidate = current.clone();
            candidate.drain(idx..idx + chunk_len);
            attempts += 1;
            if still_fails(&candidate) {
                current = candidate;
                kept_indices.drain(idx..idx + chunk_len);
                accepted_removals += 1;
                accepted_this_round = true;
            } else {
                idx += chunk_len;
            }
        }
        if !accepted_this_round {
            chunk_len /= 2;
        }
    }

    let mut idx = 0usize;
    while idx < current.len() && attempts < config.max_attempts {
        let mut candidate = current.clone();
        candidate.remove(idx);
        attempts += 1;
        if still_fails(&candidate) {
            current = candidate;
            kept_indices.remove(idx);
            accepted_removals += 1;
        } else {
            idx += 1;
        }
    }

    ShrinkReport {
        original_len: tape.len(),
        minimized: current,
        kept_indices,
        attempts,
        reproduced: true,
        accepted_removals,
    }
}

pub fn shrink_tape_label_aware_with_config<S, F>(
    tape: &[u64],
    labels: &[S],
    mut still_fails: F,
    config: ShrinkConfig,
) -> LabelAwareShrinkReport
where
    S: AsRef<str>,
    F: FnMut(&[u64]) -> bool,
{
    if labels.len() != tape.len() {
        return LabelAwareShrinkReport {
            shrink: shrink_tape_with_config(tape, still_fails, config),
            removed_labels: Vec::new(),
        };
    }

    let mut attempts = 0usize;
    let mut current = tape.to_vec();
    let mut kept_indices: Vec<usize> = (0..tape.len()).collect();
    let mut accepted_removals = 0usize;
    let mut reproduced = false;

    attempts += 1;
    if still_fails(&current) {
        reproduced = true;
        for tier in 0..=3 {
            let mut pos = 0usize;
            while pos < current.len() && attempts < config.max_attempts {
                let original_idx = kept_indices[pos];
                if label_tier(labels[original_idx].as_ref()) != tier {
                    pos += 1;
                    continue;
                }
                let mut candidate = current.clone();
                candidate.remove(pos);
                attempts += 1;
                if still_fails(&candidate) {
                    current = candidate;
                    kept_indices.remove(pos);
                    accepted_removals += 1;
                } else {
                    pos += 1;
                }
            }
        }
    }

    let shrink = ShrinkReport {
        original_len: tape.len(),
        minimized: current,
        kept_indices,
        attempts,
        reproduced,
        accepted_removals,
    };
    let removed_labels = summarize_removed_labels(labels, &shrink.kept_indices);
    LabelAwareShrinkReport {
        shrink,
        removed_labels,
    }
}

fn label_tier(label: &str) -> usize {
    match label {
        "net-delay" | "extra-delay" | "jitter" => 0,
        "drop-decision" | "duplicate-decision" => 1,
        "partition" | "crash-point" | "restart" | "clock-skew" | "nemesis" => 2,
        _ => 3,
    }
}

fn summarize_removed_labels<S: AsRef<str>>(
    labels: &[S],
    kept_indices: &[usize],
) -> Vec<RemovedLabel> {
    let mut out = Vec::<RemovedLabel>::new();
    for (idx, label) in labels.iter().enumerate() {
        if kept_indices.contains(&idx) {
            continue;
        }
        let label = label.as_ref();
        if let Some(existing) = out.iter_mut().find(|entry| entry.label == label) {
            existing.count += 1;
        } else {
            out.push(RemovedLabel {
                label: label.to_string(),
                count: 1,
            });
        }
    }
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shrink_keeps_failure_reproducing() {
        let report = shrink_tape(&[1, 42, 2, 3], |tape| tape.contains(&42), 100);
        assert!(report.reproduced);
        assert_eq!(report.minimized, vec![42]);
        assert_eq!(report.kept_indices, vec![1]);
        assert!(report.accepted_removals > 0);
    }

    #[test]
    fn non_reproducing_input_is_reported() {
        let report = shrink_tape(&[1, 2, 3], |tape| tape.contains(&42), 100);
        assert!(!report.reproduced);
        assert_eq!(report.minimized, vec![1, 2, 3]);
        assert_eq!(report.kept_indices, vec![0, 1, 2]);
        assert_eq!(report.accepted_removals, 0);
    }

    #[test]
    fn chunk_pass_removes_large_irrelevant_regions() {
        let tape = [0, 0, 0, 42, 0, 0, 0, 0];
        let report = shrink_tape_with_config(
            &tape,
            |candidate| candidate.contains(&42),
            ShrinkConfig {
                max_attempts: 100,
                min_chunk_len: 2,
            },
        );
        assert_eq!(report.minimized, vec![42]);
        assert_eq!(report.kept_indices, vec![3]);
        assert!(report.attempts < tape.len() * 2);
    }

    #[test]
    fn signature_preserving_shrink_tracks_same_failure() {
        let outcome = SignaturePreservingShrink::run(
            &[0, 7, 0, 42, 0],
            |candidate| {
                candidate
                    .contains(&42)
                    .then(|| "meaningful-failure".to_string())
            },
            ShrinkConfig {
                max_attempts: 100,
                min_chunk_len: 2,
            },
        );
        assert!(outcome.signature_preserved);
        assert_eq!(outcome.report.minimized, vec![42]);
        assert!(outcome.effectiveness.ratio < 1.0);
    }

    #[test]
    fn label_aware_shrink_removes_low_priority_draws_first() {
        let labels = ["net-delay", "partition", "drop-decision", "net-delay"];
        let report = shrink_tape_label_aware_with_config(
            &[1, 42, 7, 8],
            &labels,
            |candidate| candidate.contains(&42),
            ShrinkConfig {
                max_attempts: 100,
                min_chunk_len: 2,
            },
        );
        assert!(report.shrink.reproduced);
        assert_eq!(report.shrink.minimized, vec![42]);
        assert_eq!(report.shrink.kept_indices, vec![1]);
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
}
