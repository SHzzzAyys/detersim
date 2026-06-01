//! Entropy-tape shrinking.
//!
//! The shrinker is conservative: a candidate tape is accepted only after the
//! caller's failure predicate says it still reproduces. It starts with chunk
//! deletion, then finishes with single-draw deletion under the same budget.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShrinkReport {
    pub original_len: usize,
    pub minimized: Vec<u64>,
    pub attempts: usize,
    pub reproduced: bool,
    pub accepted_removals: usize,
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
            attempts: 1,
            reproduced: false,
            accepted_removals: 0,
        };
    }

    let mut attempts = 1usize;
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
            accepted_removals += 1;
        } else {
            idx += 1;
        }
    }

    ShrinkReport {
        original_len: tape.len(),
        minimized: current,
        attempts,
        reproduced: true,
        accepted_removals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shrink_keeps_failure_reproducing() {
        let report = shrink_tape(&[1, 42, 2, 3], |tape| tape.contains(&42), 100);
        assert!(report.reproduced);
        assert_eq!(report.minimized, vec![42]);
        assert!(report.accepted_removals > 0);
    }

    #[test]
    fn non_reproducing_input_is_reported() {
        let report = shrink_tape(&[1, 2, 3], |tape| tape.contains(&42), 100);
        assert!(!report.reproduced);
        assert_eq!(report.minimized, vec![1, 2, 3]);
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
}
