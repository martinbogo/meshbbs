//! Minimal metrics scaffolding (Phase 3)
//! This will later be extended with Prometheus exposition and histograms.
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

static RELIABLE_SENT: AtomicU64 = AtomicU64::new(0);
static RELIABLE_ACKED: AtomicU64 = AtomicU64::new(0);
static RELIABLE_FAILED: AtomicU64 = AtomicU64::new(0);
static RELIABLE_RETRIES: AtomicU64 = AtomicU64::new(0);
static ACK_LATENCY_SUM_MS: AtomicU64 = AtomicU64::new(0);
static ACK_LATENCY_COUNT: AtomicU64 = AtomicU64::new(0);

#[allow(dead_code)]
pub fn inc_reliable_sent() { RELIABLE_SENT.fetch_add(1, Ordering::Relaxed); }
#[allow(dead_code)]
pub fn inc_reliable_acked() { RELIABLE_ACKED.fetch_add(1, Ordering::Relaxed); }
#[allow(dead_code)]
pub fn inc_reliable_failed() { RELIABLE_FAILED.fetch_add(1, Ordering::Relaxed); }
#[allow(dead_code)]
pub fn inc_reliable_retries() { RELIABLE_RETRIES.fetch_add(1, Ordering::Relaxed); }
#[allow(dead_code)]
pub fn observe_ack_latency(sent_at: Instant) {
    let ms = sent_at.elapsed().as_millis() as u64;
    ACK_LATENCY_SUM_MS.fetch_add(ms, Ordering::Relaxed);
    ACK_LATENCY_COUNT.fetch_add(1, Ordering::Relaxed);
}

#[derive(Debug, Default, Clone)]
pub struct Snapshot {
    pub reliable_sent: u64,
    pub reliable_acked: u64,
    pub reliable_failed: u64,
    pub reliable_retries: u64,
    pub ack_latency_avg_ms: Option<u64>,
}

#[allow(dead_code)]
pub fn snapshot() -> Snapshot {
    let sent = RELIABLE_SENT.load(Ordering::Relaxed);
    let acked = RELIABLE_ACKED.load(Ordering::Relaxed);
    let failed = RELIABLE_FAILED.load(Ordering::Relaxed);
    let retries = RELIABLE_RETRIES.load(Ordering::Relaxed);
    let sum = ACK_LATENCY_SUM_MS.load(Ordering::Relaxed);
    let count = ACK_LATENCY_COUNT.load(Ordering::Relaxed);
    Snapshot {
        reliable_sent: sent,
        reliable_acked: acked,
        reliable_failed: failed,
        reliable_retries: retries,
        ack_latency_avg_ms: if count > 0 { Some(sum / count) } else { None },
    }
}
