use std::time::Duration;

use detersim_core::{ClockExt, Env, Network};
use detersim_shrink::ShrinkConfig;
use detersim_sim::{RunReport, SimEnv, World, WorldConfig};
use detersim_testkit::shrink_replay_failure;

fn build(seed: u64, replay_tape: Option<Vec<u64>>) -> World {
    let config = WorldConfig {
        horizon_ns: 100_000_000,
        max_events: 10_000,
    };
    match replay_tape {
        Some(tape) => World::replay(seed, tape, config),
        None => World::with_config(seed, config),
    }
}

fn run(seed: u64) -> RunReport {
    run_inner(seed, None)
}

fn replay(seed: u64, tape: Vec<u64>) -> RunReport {
    run_inner(seed, Some(tape))
}

fn run_inner(seed: u64, replay_tape: Option<Vec<u64>>) -> RunReport {
    let mut world = build(seed, replay_tape);
    world.set_drop_percent(50);
    world.add_node(0, |env: SimEnv| async move {
        let result = env
            .clock()
            .timeout(Duration::from_millis(20), env.net().recv())
            .await;
        if result.is_err() {
            env.record("missing-message");
        }
    });
    world.add_node(1, |env: SimEnv| async move {
        env.net().send(0, b"hello".to_vec()).await;
    });
    world.run()
}

fn main() {
    let mut seed = 0u64;
    while !run(seed).history.contains(&"missing-message".to_string()) {
        seed += 1;
    }

    let artifact = shrink_replay_failure(
        seed,
        run,
        replay,
        |report| report.history.contains(&"missing-message".to_string()),
        ShrinkConfig {
            max_attempts: 200,
            min_chunk_len: 2,
        },
    );

    println!("seed={}", artifact.seed);
    println!(
        "tape: {} -> {} draws in {} attempts",
        artifact.shrink.original_len,
        artifact.shrink.minimized.len(),
        artifact.shrink.attempts
    );
    println!("history={:?}", artifact.minimized_replay.history);
    println!("json-bytes={}", artifact.minimized_json.len());
    println!("html-bytes={}", artifact.minimized_html.len());
}
