//! Run with: `cargo run -p detersim-sim --example pingpong`
//! Reproduce a specific run with: `DST_SEED=123 cargo run -p detersim-sim --example pingpong`

fn main() {
    let seed: u64 = std::env::var("DST_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(42);

    let report = detersim_sim::scenarios::pingpong_world(seed);

    println!(
        "seed={} dispatched={} deadlocked={} parked={} tape_draws={}",
        report.seed, report.dispatched, report.deadlocked, report.parked_tasks, report.tape_log_len,
    );
    println!("--- event trace ---");
    for line in &report.trace {
        println!("{line}");
    }
}
