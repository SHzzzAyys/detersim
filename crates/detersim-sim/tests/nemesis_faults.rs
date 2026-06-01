use std::time::Duration;

use detersim_core::{Clock, ClockExt, Env, Network, SimTime};
use detersim_nemesis::NemesisAction;
use detersim_sim::{SimEnv, World, WorldConfig};

fn config() -> WorldConfig {
    WorldConfig {
        horizon_ns: 2_000_000_000,
        max_events: 20_000,
    }
}

#[test]
fn asymmetric_partition_blocks_only_one_direction() {
    let mut world = World::with_config(7, config());
    world.schedule_nemesis(
        SimTime::ZERO,
        NemesisAction::AsymmetricPartition { from: 0, to: 1 },
    );

    world.add_node(0, |env: SimEnv| async move {
        let net = env.net();
        net.send(1, b"blocked".to_vec()).await;
        if let Ok((_from, msg)) = env
            .clock()
            .timeout(Duration::from_millis(500), net.recv())
            .await
        {
            env.record(format!("node0:{}", String::from_utf8_lossy(&msg)));
        }
    });
    world.add_node(1, |env: SimEnv| async move {
        env.clock().sleep(Duration::from_millis(1)).await;
        env.net().send(0, b"back".to_vec()).await;
    });

    let report = world.run();
    assert!(report.history.contains(&"node0:back".to_string()));
    assert!(report
        .trace
        .iter()
        .any(|line| line.contains("drop-partition 0->1")));
    assert!(report
        .trace
        .iter()
        .any(|line| line.contains("deliver 1->0")));
}

#[test]
fn partition_heals_and_later_messages_deliver() {
    let mut world = World::with_config(11, config());
    world.schedule_nemesis(
        SimTime::ZERO,
        NemesisAction::Partition {
            groups: vec![vec![0], vec![1]],
        },
    );
    world.schedule_nemesis(SimTime::from_nanos(200_000_000), NemesisAction::HealAll);

    world.add_node(0, |env: SimEnv| async move {
        let clock = env.clock();
        let net = env.net();
        clock.sleep(Duration::from_millis(1)).await;
        net.send(1, b"before-heal".to_vec()).await;
        clock.sleep(Duration::from_millis(250)).await;
        net.send(1, b"after-heal".to_vec()).await;
    });
    world.add_node(1, |env: SimEnv| async move {
        let (_from, msg) = env.net().recv().await;
        env.record(format!("node1:{}", String::from_utf8_lossy(&msg)));
    });

    let report = world.run();
    assert_eq!(report.history, vec!["node1:after-heal"]);
    assert!(report
        .trace
        .iter()
        .any(|line| line.contains("drop-partition 0->1")));
}

#[test]
fn drop_and_duplicate_faults_are_observable() {
    let mut dropped = World::with_config(0, config());
    dropped.set_drop_percent(100);
    dropped.add_node(0, |env: SimEnv| async move {
        let result = env
            .clock()
            .timeout(Duration::from_millis(150), env.net().recv())
            .await;
        if result.is_err() {
            env.record("dropped");
        }
    });
    dropped.add_node(1, |env: SimEnv| async move {
        env.net().send(0, b"gone".to_vec()).await;
    });
    let dropped = dropped.run();
    assert_eq!(dropped.history, vec!["dropped"]);
    assert!(dropped
        .trace
        .iter()
        .any(|line| line.contains("drop-random 1->0")));

    let mut duplicated = World::with_config(0, config());
    duplicated.set_duplicate_percent(100);
    duplicated.add_node(0, |env: SimEnv| async move {
        env.net().send(1, b"twice".to_vec()).await;
    });
    duplicated.add_node(1, |env: SimEnv| async move {
        for idx in 0..2 {
            let (_from, msg) = env.net().recv().await;
            env.record(format!("{idx}:{}", String::from_utf8_lossy(&msg)));
        }
    });
    let duplicated = duplicated.run();
    assert_eq!(duplicated.history.len(), 2);
    assert!(duplicated
        .history
        .iter()
        .all(|entry| entry.ends_with(":twice")));
}

#[test]
fn clock_skew_keeps_node_local_time_monotonic() {
    let mut world = World::with_config(5, config());
    world.schedule_nemesis(
        SimTime::from_nanos(2_000_000),
        NemesisAction::ClockSkew {
            node: 0,
            offset_ns: -10_000_000,
        },
    );
    world.add_node(0, |env: SimEnv| async move {
        let clock = env.clock();
        clock.sleep(Duration::from_millis(1)).await;
        env.record(format!("now:{}", clock.now().as_nanos()));
        clock.sleep(Duration::from_millis(5)).await;
        env.record(format!("now:{}", clock.now().as_nanos()));
    });

    let report = world.run();
    let values: Vec<u64> = report
        .history
        .iter()
        .map(|entry| {
            entry
                .strip_prefix("now:")
                .expect("clock record")
                .parse::<u64>()
                .expect("clock value")
        })
        .collect();
    assert_eq!(values.len(), 2);
    assert!(values[1] >= values[0]);
}
