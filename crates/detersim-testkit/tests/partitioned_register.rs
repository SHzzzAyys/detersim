use std::time::Duration;

use detersim_check::models::{Register, RegisterInput, RegisterOutput};
use detersim_check::{
    check_linearizable_with_budget, CheckBudget, LinearizabilityResult, OpRecord,
};
use detersim_core::{Clock, Env, Network, SimTime};
use detersim_nemesis::RandomLinkFault;
use detersim_shrink::ShrinkConfig;
use detersim_sim::{RunReport, SimEnv, World, WorldConfig};
use detersim_testkit::{
    assert_recall, linearizability_signature, ExperimentBudget, ExperimentCase, FailureSignature,
};

fn partitioned_register_bug_world(seed: u64) -> RunReport {
    let world = World::with_config(seed, config());
    run_partitioned_register_bug(world)
}

fn partitioned_register_bug_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    let world = World::replay(seed, tape, config());
    run_partitioned_register_bug(world)
}

fn config() -> WorldConfig {
    WorldConfig {
        horizon_ns: 2_000_000_000,
        max_events: 20_000,
    }
}

fn run_partitioned_register_bug(mut world: World) -> RunReport {
    world.add_node(0, |env: SimEnv| async move {
        let net = env.net();
        let (client, msg) = net.recv().await;
        if msg == b"write:7" {
            net.send(1, b"replicate:7".to_vec()).await;
            net.send(client, b"ok".to_vec()).await;
        }
    });

    world.add_node(1, |env: SimEnv| async move {
        let net = env.net();
        let mut value = 0i32;
        loop {
            let (from, msg) = net.recv().await;
            match msg.as_slice() {
                b"replicate:7" => value = 7,
                b"read" => {
                    net.send(from, format!("value:{value}").into_bytes()).await;
                    break;
                }
                _ => {}
            }
        }
    });

    world.add_node(2, |env: SimEnv| async move {
        let clock = env.clock();
        let net = env.net();

        clock.sleep(Duration::from_millis(1)).await;
        let write_invoke = clock.now();
        net.send(0, b"write:7".to_vec()).await;
        let (_from, ack) = net.recv().await;
        assert_eq!(ack, b"ok");
        let write_complete = clock.now();
        env.record(format!(
            "lin:1:write:7:ok:{}:{}",
            write_invoke.as_nanos(),
            write_complete.as_nanos()
        ));

        clock.sleep(Duration::from_millis(200)).await;
        let read_invoke = clock.now();
        net.send(1, b"read".to_vec()).await;
        let (_from, value) = net.recv().await;
        let read_complete = clock.now();
        let value = String::from_utf8(value).expect("register response is utf8");
        let value = value
            .strip_prefix("value:")
            .expect("register response shape")
            .parse::<i32>()
            .expect("register value");
        env.record(format!(
            "lin:2:read:{value}:value:{}:{}",
            read_invoke.as_nanos(),
            read_complete.as_nanos()
        ));
    });

    let mut plan = RandomLinkFault::new(SimTime::ZERO, vec![(1, 0), (0, 1)], false);
    assert_eq!(world.schedule_nemesis_plan(&mut plan, 1), 1);
    world.run()
}

fn parsed_history(report: &RunReport) -> Vec<OpRecord<RegisterInput<i32>, RegisterOutput<i32>>> {
    report
        .history
        .iter()
        .filter_map(|entry| {
            let parts: Vec<_> = entry.split(':').collect();
            if parts.len() != 7 || parts[0] != "lin" {
                return None;
            }
            let id = parts[1].parse::<u64>().expect("operation id");
            let invoke = SimTime::from_nanos(parts[5].parse::<u64>().expect("invoke time"));
            let complete = SimTime::from_nanos(parts[6].parse::<u64>().expect("complete time"));
            match parts[2] {
                "write" => Some(OpRecord::completed_at(
                    id,
                    2,
                    RegisterInput::Write(parts[3].parse::<i32>().expect("write value")),
                    RegisterOutput::Ok,
                    invoke,
                    complete,
                )),
                "read" => Some(OpRecord::completed_at(
                    id,
                    2,
                    RegisterInput::Read,
                    RegisterOutput::Value(parts[3].parse::<i32>().expect("read value")),
                    invoke,
                    complete,
                )),
                _ => None,
            }
        })
        .collect()
}

fn lin_result(report: &RunReport) -> LinearizabilityResult {
    check_linearizable_with_budget(
        &Register::new(0),
        &parsed_history(report),
        CheckBudget { max_steps: 10_000 },
    )
}

fn is_not_linearizable(report: &RunReport) -> bool {
    matches!(
        lin_result(report),
        LinearizabilityResult::NotLinearizable { .. }
    )
}

fn failure_signature(report: &RunReport) -> Option<FailureSignature> {
    linearizability_signature("register", &lin_result(report))
}

#[test]
fn partitioned_register_bug_replays_and_shrinks() {
    let case = ExperimentCase {
        name: "partitioned-register-ack-before-replication",
        budget: ExperimentBudget {
            seed_count: 200,
            shrink: ShrinkConfig {
                max_attempts: 200,
                min_chunk_len: 2,
            },
        },
        generate: partitioned_register_bug_world,
        replay: partitioned_register_bug_world_replay,
        oracle: failure_signature,
    };
    let experiment = assert_recall(&case);
    assert_eq!(experiment.seeds_attempted, 200);
    assert!(experiment.first_failing_seed.is_some());
    assert_eq!(
        experiment.failure_signature,
        Some(FailureSignature::NotLinearizable {
            conflict: Some((1, 2)),
            model: "register".to_string()
        })
    );
    assert!(experiment.replay_trace_identical);
    assert!(experiment.replay_history_identical);
    assert!(experiment.replay_nemesis_identical);
    assert!(experiment.replay_matched_signature);
    assert!(experiment.minimized_matched_signature);
    assert!(experiment.artifact_json_bytes.unwrap_or_default() > 0);

    let seed = experiment.first_failing_seed.expect("recalled seed");
    let generated = partitioned_register_bug_world(seed);
    assert!(is_not_linearizable(&generated));
    assert!(
        generated
            .nemesis_trace
            .iter()
            .any(|entry| entry.contains("SetLink")
                && entry.contains("from: 0")
                && entry.contains("to: 1")),
        "failing seed must cut the replication link"
    );
}
