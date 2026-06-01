//! Seed and tape search helpers for DeterSim experiments.
//!
//! This crate sits above the deterministic runtime. It never schedules work
//! itself; it repeatedly runs public `ExperimentCase` entry points and records
//! stable coverage/failure signals.

use std::collections::BTreeSet;

use detersim_sim::RunReport;
use detersim_testkit::{ExperimentCase, FailureSignature};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchStrategy {
    Random,
    CoverageGuided,
    FailureDirected,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CoverageSignal {
    pub name: String,
}

#[derive(Clone, Debug, Default)]
pub struct SeedCorpus {
    pub candidates: Vec<SearchCandidate>,
    pub unique_coverage: Vec<CoverageSignal>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchBudget {
    pub seed_count: u64,
    pub retain_candidates: usize,
}

impl Default for SearchBudget {
    fn default() -> Self {
        Self {
            seed_count: 100,
            retain_candidates: 32,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SearchCandidate {
    pub seed: u64,
    pub rank: u64,
    pub score: u64,
    pub new_coverage: usize,
    pub coverage: Vec<CoverageSignal>,
    pub failure_signature: Option<FailureSignature>,
    pub tape_len: usize,
}

#[derive(Clone, Debug)]
pub struct SearchReport {
    pub strategy: SearchStrategy,
    pub seeds_attempted: u64,
    pub first_failing_seed: Option<u64>,
    pub first_failing_rank: Option<u64>,
    pub failures_observed: u64,
    pub corpus: SeedCorpus,
}

pub fn run_search(
    case: &ExperimentCase,
    strategy: SearchStrategy,
    budget: SearchBudget,
) -> SearchReport {
    let mut seen_coverage = BTreeSet::<CoverageSignal>::new();
    let mut candidates = Vec::<SearchCandidate>::new();
    let mut first_failing_seed = None;
    let mut first_failing_rank = None;
    let mut failures_observed = 0u64;

    for (rank, seed) in seed_order(strategy, budget.seed_count)
        .into_iter()
        .enumerate()
    {
        let report = (case.generate)(seed);
        let signature = (case.oracle)(&report);
        if signature.is_some() {
            failures_observed += 1;
            if first_failing_seed.is_none() {
                first_failing_seed = Some(seed);
                first_failing_rank = Some(rank as u64);
            }
        }

        let coverage = coverage_from_run(&report);
        let new_coverage = coverage
            .iter()
            .filter(|signal| !seen_coverage.contains(*signal))
            .count();
        for signal in &coverage {
            seen_coverage.insert(signal.clone());
        }

        let score = candidate_score(strategy, new_coverage, signature.as_ref(), &report);
        candidates.push(SearchCandidate {
            seed,
            rank: rank as u64,
            score,
            new_coverage,
            coverage,
            failure_signature: signature,
            tape_len: report.tape_log.len(),
        });
        retain_best(&mut candidates, budget.retain_candidates);
    }

    SearchReport {
        strategy,
        seeds_attempted: budget.seed_count,
        first_failing_seed,
        first_failing_rank,
        failures_observed,
        corpus: SeedCorpus {
            candidates,
            unique_coverage: seen_coverage.into_iter().collect(),
        },
    }
}

pub fn coverage_from_run(report: &RunReport) -> Vec<CoverageSignal> {
    let mut signals = BTreeSet::<CoverageSignal>::new();
    for signal in &report.coverage_signals {
        signals.insert(CoverageSignal {
            name: signal.clone(),
        });
    }
    for event in &report.tape_events {
        signals.insert(CoverageSignal {
            name: format!("tape:{}", event.label.as_str()),
        });
    }
    for entry in &report.history {
        signals.insert(CoverageSignal {
            name: format!("history:{entry}"),
        });
    }
    signals.into_iter().collect()
}

pub fn search_report_to_json(report: &SearchReport) -> String {
    let candidates: Vec<String> = report
        .corpus
        .candidates
        .iter()
        .map(|candidate| {
            format!(
                "{{\"seed\":{},\"rank\":{},\"score\":{},\"new_coverage\":{},\"coverage\":{},\"failure_signature\":{},\"tape_len\":{}}}",
                candidate.seed,
                candidate.rank,
                candidate.score,
                candidate.new_coverage,
                coverage_json(&candidate.coverage),
                option_signature_json(candidate.failure_signature.as_ref()),
                candidate.tape_len
            )
        })
        .collect();
    format!(
        "{{\"schema_version\":3,\"strategy\":\"{:?}\",\"seeds_attempted\":{},\"first_failing_seed\":{},\"first_failing_rank\":{},\"failures_observed\":{},\"unique_coverage\":{},\"candidates\":[{}]}}",
        report.strategy,
        report.seeds_attempted,
        option_u64(report.first_failing_seed),
        option_u64(report.first_failing_rank),
        report.failures_observed,
        coverage_json(&report.corpus.unique_coverage),
        candidates.join(",")
    )
}

fn seed_order(strategy: SearchStrategy, seed_count: u64) -> Vec<u64> {
    match strategy {
        SearchStrategy::Random => (0..seed_count).collect(),
        SearchStrategy::CoverageGuided => {
            let mut odds: Vec<u64> = (0..seed_count).filter(|seed| seed % 2 == 1).collect();
            odds.extend((0..seed_count).filter(|seed| seed % 2 == 0));
            odds
        }
        SearchStrategy::FailureDirected => {
            let mut seeds: Vec<u64> = (0..seed_count).collect();
            seeds.sort_by_key(|seed| (stress_score(*seed), *seed));
            seeds.reverse();
            seeds
        }
    }
}

fn candidate_score(
    strategy: SearchStrategy,
    new_coverage: usize,
    signature: Option<&FailureSignature>,
    report: &RunReport,
) -> u64 {
    let failure = u64::from(signature.is_some());
    let base = match strategy {
        SearchStrategy::Random => 0,
        SearchStrategy::CoverageGuided => new_coverage as u64 * 10,
        SearchStrategy::FailureDirected => failure * 1_000 + new_coverage as u64 * 10,
    };
    base + report.tape_log.len() as u64
}

fn stress_score(seed: u64) -> u64 {
    let mut x = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn retain_best(candidates: &mut Vec<SearchCandidate>, retain: usize) {
    if retain == 0 {
        candidates.clear();
        return;
    }
    candidates.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.rank.cmp(&b.rank))
            .then_with(|| a.seed.cmp(&b.seed))
    });
    candidates.truncate(retain);
}

fn coverage_json(signals: &[CoverageSignal]) -> String {
    let items: Vec<String> = signals
        .iter()
        .map(|signal| format!("\"{}\"", escape_json(&signal.name)))
        .collect();
    format!("[{}]", items.join(","))
}

fn option_signature_json(signature: Option<&FailureSignature>) -> String {
    signature
        .map(signature_json)
        .unwrap_or_else(|| "null".to_string())
}

fn signature_json(signature: &FailureSignature) -> String {
    match signature {
        FailureSignature::InvariantViolated(name) => format!(
            "{{\"type\":\"InvariantViolated\",\"name\":\"{}\"}}",
            escape_json(name)
        ),
        FailureSignature::NotLinearizable { conflict, model } => format!(
            "{{\"type\":\"NotLinearizable\",\"model\":\"{}\",\"conflict\":{}}}",
            escape_json(model),
            conflict
                .map(|(left, right)| format!("[{left},{right}]"))
                .unwrap_or_else(|| "null".to_string())
        ),
        FailureSignature::Deadlock => "{\"type\":\"Deadlock\"}".to_string(),
        FailureSignature::Panic(label) => {
            format!(
                "{{\"type\":\"Panic\",\"label\":\"{}\"}}",
                escape_json(label)
            )
        }
        FailureSignature::StorageCorruption(label) => format!(
            "{{\"type\":\"StorageCorruption\",\"label\":\"{}\"}}",
            escape_json(label)
        ),
    }
}

fn option_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string())
}

fn escape_json(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
