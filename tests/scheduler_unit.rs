#![cfg(feature = "meshtastic-proto")]
use meshbbs::bbs::dispatch::{start_scheduler, SchedulerConfig, MessageEnvelope, MessageCategory, Priority};
use meshbbs::meshtastic::OutgoingMessage;
use tokio::sync::mpsc;
use std::time::{Duration, Instant};

fn base_cfg() -> SchedulerConfig {
    SchedulerConfig {
        min_send_gap_ms: 10,
        post_dm_broadcast_gap_ms: 5,
        help_broadcast_delay_ms: 20,
        max_queue: 16,
        aging_threshold_ms: 30,
        stats_interval_ms: 0,
    }
}

fn mk_msg(content: &str) -> OutgoingMessage {
    OutgoingMessage { to_node: None, channel: 0, content: content.to_string(), priority: meshbbs::meshtastic::MessagePriority::Normal, kind: meshbbs::meshtastic::OutgoingKind::Normal }
}

#[tokio::test]
async fn priority_preemption() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let cfg = base_cfg();
    let handle = start_scheduler(cfg, tx);
    // Queue multiple low priority broadcasts
    for i in 0..3 {
        handle.enqueue(MessageEnvelope::new(MessageCategory::Broadcast, Priority::Low, Duration::from_millis(20), mk_msg(&format!("b{i}"))));
    }
    // Queue a high priority direct with no delay
    handle.enqueue(MessageEnvelope::new(MessageCategory::Direct, Priority::High, Duration::from_millis(0), mk_msg("dm")));

    // First dispatched should be dm (after min gap interval tick)
    let first = tokio::time::timeout(Duration::from_millis(200), async {
        rx.recv().await
    }).await.expect("timeout waiting first").expect("chan closed");
    assert!(first.content == "dm", "expected dm first got {}", first.content);
}

#[tokio::test]
async fn aging_escalation() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut cfg = base_cfg();
    cfg.aging_threshold_ms = 30; // fast aging
    let handle = start_scheduler(cfg, tx);
    handle.enqueue(MessageEnvelope::new(MessageCategory::Maintenance, Priority::Background, Duration::from_millis(0), mk_msg("bg")));
    // Wait enough for aging to escalate to Low/Normal/High sequence; dispatch will occur after min gap
    let received = tokio::time::timeout(Duration::from_millis(300), async { rx.recv().await }).await.expect("timeout").expect("closed");
    assert_eq!(received.content, "bg");
}

#[tokio::test]
async fn overflow_drops() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut cfg = base_cfg();
    cfg.max_queue = 3;
    let handle = start_scheduler(cfg, tx);
    for i in 0..5 { handle.enqueue(MessageEnvelope::new(MessageCategory::Broadcast, Priority::Low, Duration::from_millis(50), mk_msg(&format!("msg{i}")))); }
    // Snapshot after enqueues
    let stats = handle.snapshot().await.expect("snapshot");
    assert!(stats.dropped_overflow >= 2, "expected overflow drops stats={:?}", stats);
}

#[tokio::test]
async fn min_gap_enforced() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut cfg = base_cfg();
    cfg.min_send_gap_ms = 40;
    let min_gap = cfg.min_send_gap_ms;
    let handle = start_scheduler(cfg, tx);
    for i in 0..3 { handle.enqueue(MessageEnvelope::new(MessageCategory::Direct, Priority::High, Duration::from_millis(0), mk_msg(&format!("d{i}")))); }
    let first_time = Instant::now();
    let mut times = Vec::new();
    for _ in 0..3 { let msg = tokio::time::timeout(Duration::from_millis(500), async { rx.recv().await }).await.expect("timeout").expect("closed"); times.push((Instant::now(), msg.content)); }
    // Check gaps at least min_send_gap_ms (minus small scheduler tick tolerance ~5ms)
    for w in times.windows(2) {
        let delta = w[1].0.duration_since(w[0].0).as_millis() as i64;
        assert!(delta >= 30, "dispatch gap too small: {}ms times={:?}", delta, times);
    }
    assert!(first_time.elapsed().as_millis() as u64 >= min_gap as u64, "total time should reflect min gaps");
}
