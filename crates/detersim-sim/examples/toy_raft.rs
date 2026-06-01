fn main() {
    let seed: u64 = std::env::var("DST_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(42);

    let report = detersim_sim::scenarios::toy_raft_world(seed);
    println!(
        "seed={} dispatched={} deadlocked={} history={:?}",
        report.seed, report.dispatched, report.deadlocked, report.history
    );
}
