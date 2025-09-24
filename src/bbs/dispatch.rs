//! Central message dispatch scheduler (Phase 1)
//!
//! This module introduces an initial scheduling layer between higher‑level BBS
//! command logic and the Meshtastic writer channel. The immediate scope is the
//! HELP public notice (broadcast) which previously used an ad‑hoc `tokio::spawn`
//! + `sleep` to defer sending after a DM. By centralizing enqueue logic we open
//! the path toward richer fairness and pacing features.
//!
//! Phase 1 (current):
//! * Envelope abstraction with category + priority.
//! * Time‑based delay (earliest send) + global min gap enforcement.
//! * Help broadcast scheduled through this dispatcher.
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
pub enum MessageCategory {
    DirectHelp,
    HelpBroadcast,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Priority { High, Normal }

#[derive(Debug)]
pub struct MessageEnvelope {
    pub category: MessageCategory,
    pub priority: Priority,
    pub earliest: Instant,
    pub msg: OutgoingMessage,
}

impl MessageEnvelope {
    pub fn new(category: MessageCategory, priority: Priority, delay: Duration, msg: OutgoingMessage) -> Self {
        Self { category, priority, earliest: Instant::now() + delay, msg }
    }
}

pub struct SchedulerConfig {
    pub min_send_gap_ms: u64,
    pub post_dm_broadcast_gap_ms: u64,
    pub help_broadcast_delay_ms: u64,
}

impl SchedulerConfig {
    pub fn effective_help_delay(&self) -> Duration {
        let composite = self.min_send_gap_ms + self.post_dm_broadcast_gap_ms;
        Duration::from_millis(self.help_broadcast_delay_ms.max(composite))
    }
}

pub enum ScheduleCommand {
    Enqueue(MessageEnvelope),
    Shutdown(oneshot::Sender<()>),
}

pub struct SchedulerHandle {
    tx: mpsc::UnboundedSender<ScheduleCommand>,
}

impl SchedulerHandle {
    pub fn enqueue(&self, env: MessageEnvelope) { let _ = self.tx.send(ScheduleCommand::Enqueue(env)); }
    pub async fn shutdown(&self) { let (tx, rx) = oneshot::channel(); let _ = self.tx.send(ScheduleCommand::Shutdown(tx)); let _ = rx.await; }
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
        const TICK: Duration = Duration::from_millis(50);
        loop {
            tokio::select! {
                Some(cmd) = rx.recv() => {
                    match cmd {
                        ScheduleCommand::Enqueue(env) => { queue.push(env); },
                        ScheduleCommand::Shutdown(done) => { let _ = done.send(()); break; }
                    }
                }
                _ = tokio::time::sleep(TICK) => {}
            }
            if queue.is_empty() { continue; }
            // Find next eligible by priority then earliest time
            let now = Instant::now();
            queue.sort_by(|a,b| a.priority.cmp(&b.priority).then(a.earliest.cmp(&b.earliest))); // small n expected phase 1
            if let Some(pos) = queue.iter().position(|e| e.earliest <= now) {
                let ready = queue.remove(pos);
                // Enforce min gap here (belt + suspenders with writer)
                if let Some(last) = last_sent {
                    let needed = Duration::from_millis(cfg.min_send_gap_ms);
                    if now < last + needed { continue; }
                }
                log::trace!("dispatching scheduled message category={:?}", ready.category);
                if outgoing.send(ready.msg).is_err() {
                    log::warn!("outgoing channel closed; dropping message");
                } else {
                    last_sent = Some(now);
                }
            }
        }
        log::debug!("scheduler loop terminated");
    });

    handle
}
