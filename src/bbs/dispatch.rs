//! Central message dispatch scheduler (Phase 1)
//!
//! This module introduces an initial scheduling layer between higher‑level BBS
//! command logic and the Meshtastic writer channel. The immediate scope is the
//! HELP public notice (broadcast) which previously used an ad‑hoc `tokio::spawn`
//! + `sleep` to defer sending after a DM. By centralizing enqueue logic we open
//! the path toward richer fairness and pacing features.
//!
//! Phase 1 (initial):
//! * Envelope abstraction with category + priority.
//! * Time‑based delay (earliest send) + global min gap enforcement.
//! * Help broadcast scheduled through this dispatcher.
//!
//! Phase 2 (active):
//! * All direct messages and broadcasts now enqueue through this scheduler (server helpers wrap usage).
//! * Bounded queue with overflow drop policy (drops lowest priority oldest on overflow).
//! * Priority aging (escalates messages waiting beyond aging threshold).
//! * Expanded priority levels (High/Normal/Low/Background) + richer categories for future phases.
//! * Basic runtime stats (dispatch counts, drops, escalations) with periodic logging.
//! * Tests: DM preemption over queued broadcasts and overflow drop policy.
//!
//! Planned Phases:
//! * Migrate all DM + broadcast sends through scheduler.
//! * Per‑category pacing (e.g. system vs user vs maintenance).
//! * Retry / ACK re‑enqueue integration (remove scattered timers).
//! * Metrics export (queue length, deferrals, latency).
//! * Optional token bucket or weighted fairness.
//! * Cancellation / priority aging.
//!
//! Design Notes:
//! * Keeps implementation intentionally simple (Vec + sort) due to tiny queue sizes now.
//! * Writer retains its own gating; once confident, inner gating can be slimmed.
//! * Public API kept minimal (`SchedulerHandle::enqueue`) to evolve internals safely.

use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

use crate::meshtastic::OutgoingMessage;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[allow(dead_code)] // Some categories not yet emitted in Phase 2; reserved for Phase 3+ (retries, system, maintenance)
pub enum MessageCategory {
    Direct,          // Direct user DM (interactive)
    Broadcast,       // Public/channel broadcast
    System,          // System/service messages (welcome, prompts)
    Admin,           // Administrative notices / moderation actions
    Retry,           // Retries / re-sends (future Phase 3)
    Maintenance,     // Background sync / housekeeping
    HelpBroadcast,   // Specific labelled variant (can be merged into Broadcast later)
    DirectHelp,      // Specific labelled variant for help DM (High priority labeling aid)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[allow(dead_code)] // Background priority reserved for future maintenance tasks
pub enum Priority { High, Normal, Low, Background }

#[derive(Debug)]
pub struct MessageEnvelope {
    pub category: MessageCategory,
    pub priority: Priority,
    pub earliest: Instant,
    pub enqueued_at: Instant,
    pub msg: OutgoingMessage,
}

impl MessageEnvelope {
    pub fn new(category: MessageCategory, priority: Priority, delay: Duration, msg: OutgoingMessage) -> Self {
        let now = Instant::now();
        Self { category, priority, earliest: now + delay, enqueued_at: now, msg }
    }
}

pub struct SchedulerConfig {
    pub min_send_gap_ms: u64,
    pub post_dm_broadcast_gap_ms: u64,
    pub help_broadcast_delay_ms: u64,
    pub max_queue: usize,
    pub aging_threshold_ms: u64,
    pub stats_interval_ms: u64,
}

impl SchedulerConfig {
    pub fn effective_help_delay(&self) -> Duration {
        let composite = self.min_send_gap_ms + self.post_dm_broadcast_gap_ms;
        Duration::from_millis(self.help_broadcast_delay_ms.max(composite))
    }
    pub fn aging_threshold(&self) -> Duration { Duration::from_millis(self.aging_threshold_ms) }
    pub fn stats_interval(&self) -> Duration { Duration::from_millis(self.stats_interval_ms) }
}

pub enum ScheduleCommand {
    Enqueue(MessageEnvelope),
    Snapshot(oneshot::Sender<SchedulerStats>),
    Shutdown(oneshot::Sender<()>),
}

#[derive(Debug, Clone, Default)]
pub struct SchedulerStats {
    pub queued: usize,
    pub dispatched_total: u64,
    pub dropped_total: u64,
    pub dropped_overflow: u64,
    pub escalations: u64,
}

#[derive(Clone)]
pub struct SchedulerHandle {
    tx: mpsc::UnboundedSender<ScheduleCommand>,
}

impl SchedulerHandle {
    pub fn enqueue(&self, env: MessageEnvelope) { let _ = self.tx.send(ScheduleCommand::Enqueue(env)); }
    pub async fn shutdown(&self) { let (tx, rx) = oneshot::channel(); let _ = self.tx.send(ScheduleCommand::Shutdown(tx)); let _ = rx.await; }
    pub async fn snapshot(&self) -> Option<SchedulerStats> { let (tx, rx) = oneshot::channel(); if self.tx.send(ScheduleCommand::Snapshot(tx)).is_ok() { rx.await.ok() } else { None } }
}

pub fn start_scheduler(
    cfg: SchedulerConfig,
    outgoing: mpsc::UnboundedSender<OutgoingMessage>,
) -> SchedulerHandle {
    let (tx, mut rx) = mpsc::unbounded_channel::<ScheduleCommand>();
    let handle = SchedulerHandle { tx: tx.clone() };

    tokio::spawn(async move {
        let mut last_sent: Option<Instant> = None;
        let mut queue: Vec<MessageEnvelope> = Vec::new();
        let mut stats = SchedulerStats::default();
        const TICK: Duration = Duration::from_millis(50);
        let mut last_stats_log = Instant::now();
        loop {
            tokio::select! {
                Some(cmd) = rx.recv() => {
                    match cmd {
                        ScheduleCommand::Enqueue(env) => {
                            // Enforce max queue: drop lowest priority oldest if overflow
                            if queue.len() >= cfg.max_queue {
                                // Find a victim: lowest priority, then oldest enqueued_at
                                if let Some(victim_pos) = queue.iter().enumerate().max_by(|a,b| {
                                    let (ai, av) = a; let (bi, bv) = b;
                                    av.priority.cmp(&bv.priority) // reversed by using max later we want worst
                                        .then(av.enqueued_at.cmp(&bv.enqueued_at))
                                        .then(ai.cmp(&bi))
                                }).map(|(i,_)| i) {
                                    queue.remove(victim_pos); // Drop victim
                                    stats.dropped_total += 1;
                                    stats.dropped_overflow += 1;
                                    log::warn!("scheduler overflow: dropped one message (queue_full={})", queue.len());
                                }
                            }
                            queue.push(env);
                        },
                        ScheduleCommand::Snapshot(resp) => { let _ = resp.send(SchedulerStats { queued: queue.len(), ..stats }); },
                        ScheduleCommand::Shutdown(done) => { let _ = done.send(()); break; }
                    }
                }
                _ = tokio::time::sleep(TICK) => {}
            }
            if queue.is_empty() { continue; }
            let now = Instant::now();

            // Periodic stats logging
            if cfg.stats_interval_ms > 0 && now.duration_since(last_stats_log) >= cfg.stats_interval() {
                log::debug!("scheduler stats: queued={} dispatched_total={} dropped_total={} overflow={} escalations={}", queue.len(), stats.dispatched_total, stats.dropped_total, stats.dropped_overflow, stats.escalations);
                last_stats_log = now;
            }

            // Adjust priorities for aging (copy-on-write approach to keep simple)
            for env in queue.iter_mut() {
                if env.priority != Priority::High { // can't escalate above High
                    if now.duration_since(env.enqueued_at) >= cfg.aging_threshold() {
                        // Single-step escalation
                        let old = env.priority;
                        env.priority = match env.priority { Priority::Background => Priority::Low, Priority::Low => Priority::Normal, Priority::Normal => Priority::High, Priority::High => Priority::High };
                        if env.priority != old { stats.escalations += 1; }
                    }
                }
            }

            // Sort after potential escalations
            queue.sort_by(|a,b| a.priority.cmp(&b.priority).then(a.earliest.cmp(&b.earliest)));

            // Find first eligible item (earliest <= now)
            if let Some(pos) = queue.iter().position(|e| e.earliest <= now) {
                // Enforce min gap
                if let Some(last) = last_sent { if now < last + Duration::from_millis(cfg.min_send_gap_ms) { continue; } }
                let ready = queue.remove(pos);
                if outgoing.send(ready.msg).is_err() { log::warn!("outgoing channel closed; dropping message"); stats.dropped_total += 1; }
                else { stats.dispatched_total += 1; last_sent = Some(now); }
            }
        }
        log::debug!("scheduler loop terminated");
    });

    handle
}
