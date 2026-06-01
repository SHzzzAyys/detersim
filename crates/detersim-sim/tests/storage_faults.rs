use detersim_core::{Env, SimTime, Storage};
use detersim_nemesis::NemesisAction;
use detersim_sim::scenarios::{bitrot_wal_world, wal_recovery_world};
use detersim_sim::{SimEnv, World, WorldConfig};

fn config() -> WorldConfig {
    WorldConfig {
        horizon_ns: 100_000_000,
        max_events: 10_000,
    }
}

#[test]
fn ack_before_flush_is_lost_across_crash_restart() {
    for seed in 0..50 {
        let durable = wal_recovery_world(seed, true);
        assert!(durable.history.contains(&"recovered:OKAY".to_string()));

        let lost = wal_recovery_world(seed, false);
        assert!(lost.history.contains(&"ack".to_string()));
        assert!(!lost.history.contains(&"recovered:OKAY".to_string()));
    }
}

#[test]
fn bitrot_corrupts_committed_storage_detectably() {
    for seed in 0..50 {
        let report = bitrot_wal_world(seed);
        assert!(report.history.contains(&"checksum:bad".to_string()));
        assert!(report
            .nemesis_trace
            .contains(&"BitRot { node: 0 }".to_string()));
    }
}

#[test]
fn torn_write_commits_partial_data() {
    let mut world = World::with_config(0, config());
    world.schedule_nemesis(SimTime::ZERO, NemesisAction::TornWrite { node: 0 });
    world.add_node(0, |env: SimEnv| async move {
        let storage = env.storage();
        storage.write_at(0, b"ABCD").await.expect("write");
        storage.flush().await.expect("flush");
        let mut buf = [0u8; 4];
        let n = storage.read_at(0, &mut buf).await.expect("read");
        env.record(format!("torn:{}", String::from_utf8_lossy(&buf[..n])));
    });

    let report = world.run();
    assert_eq!(report.history, vec!["torn:AB"]);
}

#[test]
fn pre_fsync_reorder_changes_overlapping_commit_order() {
    let mut world = World::with_config(0, config());
    world.schedule_nemesis(SimTime::ZERO, NemesisAction::PreFsyncReorder { node: 0 });
    world.add_node(0, |env: SimEnv| async move {
        let storage = env.storage();
        storage.write_at(0, b"A").await.expect("write A");
        storage.write_at(0, b"B").await.expect("write B");
        let mut before = [0u8; 1];
        let n = storage.read_at(0, &mut before).await.expect("read before");
        env.record(format!(
            "before-flush:{}",
            String::from_utf8_lossy(&before[..n])
        ));

        storage.flush().await.expect("flush");
        let mut after = [0u8; 1];
        let n = storage.read_at(0, &mut after).await.expect("read after");
        env.record(format!(
            "after-flush:{}",
            String::from_utf8_lossy(&after[..n])
        ));
    });

    let report = world.run();
    assert_eq!(
        report.history,
        vec!["before-flush:B".to_string(), "after-flush:A".to_string()]
    );
}
