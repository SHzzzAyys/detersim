fn main() {
    let seed: u64 = std::env::var("DST_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(42);

    let report = detersim_sim::scenarios::partitioned_dual_leader_world(seed);

    println!(
        "seed={} dispatched={} deadlocked={}",
        report.seed, report.dispatched, report.deadlocked
    );
    println!("history={:?}", report.history);
    println!("--- trace ---");
    for line in &report.trace {
        println!("{line}");
    }
}
