fn main() {
    let seed: u64 = std::env::var("DST_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(42);

    let durable = detersim_sim::scenarios::wal_recovery_world(seed, true);
    let lost = detersim_sim::scenarios::wal_recovery_world(seed, false);

    println!("seed={seed}");
    println!("flush-before-ack history={:?}", durable.history);
    println!("no-flush-before-ack history={:?}", lost.history);
}
