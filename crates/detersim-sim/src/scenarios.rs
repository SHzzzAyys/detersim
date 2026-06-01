//! Reference scenarios used by the example binary and the determinism meta-test.
//!
//! These double as the smallest possible SUTs: each is written generically
//! against `Env` (here instantiated with `SimEnv`), exercising messaging,
//! timers, and task spawning.

use std::time::Duration;

use detersim_core::{Clock, ClockExt, Env, Network, NodeId, SimTime, Storage};
use detersim_nemesis::NemesisAction;

use crate::{RunReport, SimEnv, World, WorldConfig};

const ROUNDS: u32 = 4;

/// Two nodes bounce a ping/pong back and forth `ROUNDS` times, then stop.
///
/// Node 0 kicks things off, then for each round waits for a pong, pauses, and
/// (except on the last round) sends the next ping. Node 1 waits for a ping,
/// pauses, and replies with a pong. Both loops terminate cleanly, so the world
/// reaches quiescence with no parked tasks.
pub fn pingpong_world(seed: u64) -> RunReport {
    let world = World::new(seed);
    run_pingpong(world)
}

pub fn pingpong_world_replay(seed: u64, tape: Vec<u64>) -> RunReport {
    let world = World::replay(seed, tape, WorldConfig::default());
    run_pingpong(world)
}

fn run_pingpong(mut world: World) -> RunReport {
    world.add_node(0, |env: SimEnv| async move {
        let clock = env.clock();
        let net = env.net();
        net.send(1, b"ping 0".to_vec()).await;
        for r in 0..ROUNDS {
            let (_from, _pong) = net.recv().await;
            clock.sleep(Duration::from_millis(10)).await;
            if r + 1 < ROUNDS {
                net.send(1, format!("ping {}", r + 1).into_bytes()).await;
            }
        }
    });

    world.add_node(1, |env: SimEnv| async move {
        let clock = env.clock();
        let net = env.net();
        for r in 0..ROUNDS {
            let (from, _ping) = net.recv().await;
            clock.sleep(Duration::from_millis(5)).await;
            net.send(from, format!("pong {r}").into_bytes()).await;
        }
    });

    world.run()
}

/// A single node spawns a child task that computes a value after a delay; the
/// parent joins it, asserts the value, then sleeps. Exercises `spawn` + join.
pub fn spawn_demo_world(seed: u64) -> RunReport {
    let mut world = World::new(seed);

    world.add_node(0, |env: SimEnv| async move {
        let clock = env.clock();
        let child = env.spawn({
            let clock = clock.clone();
            async move {
                clock.sleep(Duration::from_millis(20)).await;
                7u32
            }
        });
        let value = child.await;
        assert_eq!(value, 7, "joined child returned the wrong value");
        clock.sleep(Duration::from_millis(5)).await;
    });

    world.run()
}

/// A small multi-node gossip/echo run. Node 0 broadcasts to every other node;
/// every peer replies once. Delivery order varies across seeds but remains
/// byte-identical for the same seed.
pub fn gossip_world(seed: u64, nodes: NodeId) -> RunReport {
    let mut world = World::new(seed);

    world.add_nodes(nodes, move |env: SimEnv| async move {
        let id = env.node_id();
        let net = env.net();
        if id == 0 {
            for peer in 1..nodes {
                net.send(peer, format!("hello {peer}").into_bytes()).await;
            }
            for _ in 1..nodes {
                let (from, msg) = net.recv().await;
                env.record(format!("reply:{from}:{}", String::from_utf8_lossy(&msg)));
            }
        } else {
            let (from, msg) = net.recv().await;
            env.record(format!("seen:{id}:{}", String::from_utf8_lossy(&msg)));
            net.send(from, format!("ack {id}").into_bytes()).await;
        }
    });

    world.run()
}

/// A timeout with no incoming message should return `Err(Timeout)` and then
/// allow the node to terminate cleanly.
pub fn timeout_world(seed: u64) -> RunReport {
    let mut world = World::new(seed);

    world.add_node(0, |env: SimEnv| async move {
        let clock = env.clock();
        let net = env.net();
        if clock
            .timeout(Duration::from_millis(10), net.recv())
            .await
            .is_err()
        {
            env.record("timeout");
        }
    });

    world.run()
}

/// The receive branch wins before the timeout. The cancelled timer must not
/// later re-poll the task.
pub fn timeout_cancel_world(seed: u64) -> RunReport {
    let mut world = World::new(seed);

    world.add_node(0, |env: SimEnv| async move {
        let clock = env.clock();
        let net = env.net();
        let result = clock.timeout(Duration::from_millis(200), net.recv()).await;
        if let Ok((from, msg)) = result {
            env.record(format!("got:{from}:{}", String::from_utf8_lossy(&msg)));
        }
        clock.sleep(Duration::from_millis(250)).await;
        env.record("done");
    });

    world.add_node(1, |env: SimEnv| async move {
        env.net().send(0, b"fast".to_vec()).await;
    });

    world.run()
}

/// A deliberately buggy leader detector: if two nodes cannot hear one another,
/// both self-elect after a timeout. This is the first plant-a-bug scenario.
pub fn partitioned_dual_leader_world(seed: u64) -> RunReport {
    let mut world = World::new(seed);

    world.add_nodes(2, |env: SimEnv| async move {
        let id = env.node_id();
        let peer = 1 - id;
        let clock = env.clock();
        let net = env.net();
        net.send(peer, b"heartbeat".to_vec()).await;
        if clock
            .timeout(Duration::from_millis(30), net.recv())
            .await
            .is_err()
        {
            env.record(format!("leader:{id}"));
        }
    });

    world.partition(vec![vec![0], vec![1]]);
    world.run()
}

/// A tiny WAL-like scenario. On first start it writes an acknowledged record and
/// sleeps. A scheduled crash/restart then tests whether the acknowledged record
/// survived; without `flush`, it is lost under `LostOnCrash`.
pub fn wal_recovery_world(seed: u64, flush_before_ack: bool) -> RunReport {
    let mut world = World::new(seed);
    world.schedule_nemesis(SimTime::ZERO, NemesisAction::LostOnCrash { node: 0 });
    world.schedule_nemesis(
        SimTime::from_nanos(1_000_000),
        NemesisAction::Crash { node: 0 },
    );
    world.schedule_nemesis(
        SimTime::from_nanos(2_000_000),
        NemesisAction::Restart { node: 0 },
    );

    world.add_node(0, move |env: SimEnv| async move {
        let storage = env.storage();
        if storage.len().await == 0 {
            storage.write_at(0, b"OKAY").await.expect("write WAL");
            if flush_before_ack {
                storage.flush().await.expect("flush WAL");
            }
            env.record("ack");
            env.clock().sleep(Duration::from_millis(100)).await;
        } else {
            let mut buf = [0u8; 4];
            let n = storage.read_at(0, &mut buf).await.expect("read WAL");
            env.record(format!("recovered:{}", String::from_utf8_lossy(&buf[..n])));
        }
    });

    world.run()
}

/// Bit rot flips committed data; the toy checksum is simply byte equality.
pub fn bitrot_wal_world(seed: u64) -> RunReport {
    let mut world = World::new(seed);
    world.schedule_nemesis(
        SimTime::from_nanos(1_000_000),
        NemesisAction::BitRot { node: 0 },
    );
    world.schedule_nemesis(
        SimTime::from_nanos(2_000_000),
        NemesisAction::Crash { node: 0 },
    );
    world.schedule_nemesis(
        SimTime::from_nanos(3_000_000),
        NemesisAction::Restart { node: 0 },
    );

    world.add_node(0, |env: SimEnv| async move {
        let storage = env.storage();
        if storage.len().await == 0 {
            storage.write_at(0, b"OKAY").await.expect("write WAL");
            storage.flush().await.expect("flush WAL");
            env.record("stored");
            env.clock().sleep(Duration::from_millis(100)).await;
        } else {
            let mut buf = [0u8; 4];
            let n = storage.read_at(0, &mut buf).await.expect("read WAL");
            if &buf[..n] == b"OKAY" {
                env.record("checksum:ok");
            } else {
                env.record("checksum:bad");
            }
        }
    });

    world.run()
}

/// A tiny Raft-shaped replication smoke test: node 0 sends one append entry to
/// two followers and commits after both acknowledgements. It is intentionally
/// not a full consensus algorithm; it is the Phase 6 reference-entry scaffold.
pub fn toy_raft_world(seed: u64) -> RunReport {
    let mut world = World::new(seed);

    world.add_nodes(3, |env: SimEnv| async move {
        let id = env.node_id();
        let net = env.net();
        if id == 0 {
            net.send(1, b"append:x=1".to_vec()).await;
            net.send(2, b"append:x=1".to_vec()).await;
            let mut acks = 0u32;
            while acks < 2 {
                let (_from, msg) = net.recv().await;
                if msg == b"ack:x=1" {
                    acks += 1;
                }
            }
            env.record("raft:commit:x=1");
        } else {
            let (leader, msg) = net.recv().await;
            if msg == b"append:x=1" {
                env.record(format!("raft:append:{id}:x=1"));
                net.send(leader, b"ack:x=1".to_vec()).await;
            }
        }
    });

    world.run()
}
