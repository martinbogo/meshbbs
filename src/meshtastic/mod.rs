//! # Meshtastic Device Communication Module
//! 
//! This module provides communication interfaces with Meshtastic devices, supporting both
//! text-based and protocol buffer communication modes. It handles device connection management,
//! message parsing, and event processing.
//!
//! ## Features
//!
//! - **Serial Communication**: Connect to Meshtastic devices via USB/UART
//! - **Protocol Support**: Both text parsing and protobuf decoding
//! - **Event Processing**: Convert raw device messages to structured events
//! - **SLIP Decoding**: Handle SLIP-encoded protocol buffer frames
//!
//! ## Communication Modes
//!
//! ### Text Mode (Default)
//! ```rust,no_run
//! # #[cfg(feature = "serial")]
//! # {
//! use meshbbs::meshtastic::MeshtasticDevice;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Create device connection
//!     let mut device = MeshtasticDevice::new("/dev/ttyUSB0", 115200).await?;
//!     
//!     // Receive text messages
//!     while let Some(message) = device.receive_message().await? {
//!         println!("Received: {}", message);
//!     }
//!     Ok(())
//! }
//! # }
//! ```
//!
//! ### Protocol Buffer Mode (with `meshtastic-proto` feature)
//! When enabled, provides rich packet decoding for positions, node info, telemetry, etc.
//!
//! ## Event Types
//!
//! The module produces [`TextEvent`] instances that represent different types of
//! communication from the mesh network:
//!
//! - **Messages**: Text communications between nodes
//! - **Node Info**: Device information and capabilities
//! - **Position**: GPS location data
//! - **Telemetry**: Device metrics and sensor data
//!
//! ## Error Handling
//!
//! The module provides robust error handling for:
//! - Device connection failures
//! - Serial communication errors
//! - Protocol parsing issues
//! - Timeout conditions
//!
//! ## Configuration
//!
//! Device parameters are typically configured via the main configuration system:
//!
//! ```toml
//! [meshtastic]
//! port = "/dev/ttyUSB0"
//! baud_rate = 115200
//! node_id = ""
//! channel = 0
//! ```

use anyhow::{Result, anyhow};
use log::{info, debug, error, trace, warn};
use tokio::time::{sleep, Duration};
use tokio::sync::mpsc;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Priority level for outgoing messages
#[derive(Debug, Clone)]
pub enum MessagePriority {
    High,    // Direct messages with want_ack for immediate transmission
    Normal,  // Regular broadcasts
}

/// Outgoing message structure for the writer task
#[derive(Debug, Clone)]
pub struct OutgoingMessage {
    pub to_node: Option<u32>,  // None for broadcast, Some(node_id) for direct
    pub channel: u32,          // Channel index (0 = primary)
    pub content: String,       // Message content
    pub priority: MessagePriority,
}

/// Writer tuning parameters, typically sourced from Config
#[derive(Debug, Clone)]
pub struct WriterTuning {
    /// Minimum gap between any text sends (ms). Enforced with a hard lower bound of 2000ms.
    pub min_send_gap_ms: u64,
    /// Retransmit backoff schedule in seconds, e.g. [4, 8, 16]. Must be non-empty; values <=0 ignored.
    pub dm_resend_backoff_seconds: Vec<u64>,
    /// Additional pacing delay for a broadcast sent immediately after a reliable DM (ms)
    pub post_dm_broadcast_gap_ms: u64,
    /// Minimum gap between two consecutive reliable DMs (ms)
    pub dm_to_dm_gap_ms: u64,
}

impl Default for WriterTuning {
    fn default() -> Self {
        Self {
            min_send_gap_ms: 2000,
            dm_resend_backoff_seconds: vec![4, 8, 16],
            post_dm_broadcast_gap_ms: 1200,
            dm_to_dm_gap_ms: 600,
        }
    }
}

/// Control messages for coordinating between tasks
#[derive(Debug)]
pub enum ControlMessage {
    Shutdown,
    #[allow(dead_code)]
    DeviceStatus,
    #[allow(dead_code)]
    ConfigRequest(u32),
    #[allow(dead_code)]
    Heartbeat,
    SetNodeId(u32),
    /// Correlated ACK from radio for a previously sent reliable packet (reply_id)
    AckReceived(u32),
    /// Routing error reported by radio for a particular request/reply id (value per proto::routing::Error)
    RoutingError { id: u32, reason: i32 },
}

#[cfg(feature = "meshtastic-proto")]
use bytes::BytesMut;
#[cfg(feature = "meshtastic-proto")]
use crate::protobuf::meshtastic_generated as proto;

// Provide hex_snippet early so it is in scope for receive_message (always available)
fn hex_snippet(data: &[u8], max: usize) -> String {
    use std::cmp::min;
    data.iter().take(min(max, data.len())).map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join("")
}

#[cfg(feature = "meshtastic-proto")]
fn fmt_percent(val: f32) -> String {
    if val.is_finite() {
        if val <= 1.0 { format!("{:.0}%", val * 100.0) } else { format!("{:.0}%", val) }
    } else {
        "na".to_string()
    }
}

#[cfg(feature = "meshtastic-proto")]
fn summarize_known_port_payload(port: proto::PortNum, payload: &[u8]) -> Option<String> {
    use bytes::BytesMut;
    use prost::Message;
    match port {
        proto::PortNum::TelemetryApp => {
            let mut b = BytesMut::from(payload).freeze();
            if let Ok(t) = proto::Telemetry::decode(&mut b) {
                if let Some(variant) = t.variant {
                    use proto::telemetry::Variant as TVar;
                    match variant {
                        TVar::DeviceMetrics(dm) => {
                            let mut parts: Vec<String> = Vec::new();
                            if let Some(batt) = dm.battery_level { parts.push(format!("batt={}{}", batt, "%")); }
                            if let Some(v) = dm.voltage { parts.push(format!("v={:.2}V", v)); }
                            if let Some(up) = dm.uptime_seconds { parts.push(format!("up={}s", up)); }
                            if let Some(util) = dm.channel_utilization { parts.push(format!("util={}", fmt_percent(util))); }
                            if let Some(tx) = dm.air_util_tx { parts.push(format!("tx={}", fmt_percent(tx))); }
                            if !parts.is_empty() { return Some(format!("telemetry/device {}", parts.join(" "))); }
                            return Some("telemetry/device".to_string());
                        }
                        TVar::EnvironmentMetrics(env) => {
                            let mut parts: Vec<String> = Vec::new();
                            if let Some(t) = env.temperature { parts.push(format!("temp={:.1}C", t)); }
                            if let Some(h) = env.relative_humidity { parts.push(format!("hum={:.0}%", h)); }
                            if let Some(p) = env.barometric_pressure { parts.push(format!("press={:.0}hPa", p)); }
                            if !parts.is_empty() { return Some(format!("telemetry/env {}", parts.join(" "))); }
                            return Some("telemetry/env".to_string());
                        }
                        TVar::LocalStats(ls) => {
                            let mut parts: Vec<String> = Vec::new();
                            parts.push(format!("up={}s", ls.uptime_seconds));
                            parts.push(format!("util={}", fmt_percent(ls.channel_utilization)));
                            parts.push(format!("tx={}", fmt_percent(ls.air_util_tx)));
                            parts.push(format!("rx={} bad={} dupe={}", ls.num_packets_rx, ls.num_packets_rx_bad, ls.num_rx_dupe));
                            return Some(format!("telemetry/local {}", parts.join(" ")));
                        }
                        TVar::PowerMetrics(_) => return Some("telemetry/power".to_string()),
                        TVar::AirQualityMetrics(_) => return Some("telemetry/air".to_string()),
                        TVar::HealthMetrics(_) => return Some("telemetry/health".to_string()),
                        TVar::HostMetrics(_) => return Some("telemetry/host".to_string()),
                    }
                }
                return Some("telemetry".to_string());
            }
            None
        }
        proto::PortNum::PositionApp => {
            let mut b = BytesMut::from(payload).freeze();
            if let Ok(pos) = proto::Position::decode(&mut b) {
                let lat = pos.latitude_i.map(|v| v as f64 * 1e-7);
                let lon = pos.longitude_i.map(|v| v as f64 * 1e-7);
                let alt = pos.altitude.or(pos.altitude_hae.map(|v| v as i32));
                let mut parts = Vec::new();
                if let (Some(la), Some(lo)) = (lat, lon) { parts.push(format!("lat={:.5} lon={:.5}", la, lo)); }
                if let Some(a) = alt { parts.push(format!("alt={}m", a)); }
                if parts.is_empty() { None } else { Some(format!("position {}", parts.join(" "))) }
            } else { None }
        }
        proto::PortNum::NodeinfoApp => {
            let mut b = BytesMut::from(payload).freeze();
            if let Ok(u) = proto::User::decode(&mut b) {
                let ln = u.long_name.trim();
                let sn = u.short_name.trim();
                if !ln.is_empty() || !sn.is_empty() {
                    if !ln.is_empty() && !sn.is_empty() { Some(format!("user {} ({})", ln, sn)) } else { Some(format!("user {}{}", ln, sn)) }
                } else { Some("user".to_string()) }
            } else { None }
        }
        _ => None,
    }
}

#[cfg(feature = "meshtastic-proto")]
pub mod slip; // restore SLIP decoder (Meshtastic uses SLIP over some transports)

#[cfg(feature = "serial")]
use serialport::SerialPort;

use serde::{Deserialize, Serialize};
use std::path::Path;
use chrono::{DateTime, Utc};

/// Cached node information with timestamp for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedNodeInfo {
    pub node_id: u32,
    pub long_name: String,
    pub short_name: String,
    pub last_seen: DateTime<Utc>,
    pub first_seen: DateTime<Utc>,
}

/// Node cache for persistent storage
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeCache {
    pub nodes: std::collections::HashMap<u32, CachedNodeInfo>,
    pub last_updated: DateTime<Utc>,
}

impl NodeCache {
    pub fn new() -> Self {
        Self {
            nodes: std::collections::HashMap::new(),
            last_updated: Utc::now(),
        }
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let cache: NodeCache = serde_json::from_str(&content)?;
        Ok(cache)
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn update_node(&mut self, node_id: u32, long_name: String, short_name: String) {
        let now = Utc::now();
        self.nodes.entry(node_id)
            .and_modify(|n| {
                n.long_name = long_name.clone();
                n.short_name = short_name.clone();
                n.last_seen = now;
            })
            .or_insert(CachedNodeInfo {
                node_id,
                long_name,
                short_name,
                last_seen: now,
                first_seen: now,
            });
        self.last_updated = now;
    }

    #[allow(dead_code)]
    pub fn remove_stale_nodes(&mut self, max_age_days: u32) -> usize {
        let cutoff = Utc::now() - chrono::Duration::days(max_age_days as i64);
        let initial_count = self.nodes.len();
        self.nodes.retain(|_, node| node.last_seen > cutoff);
        let removed = initial_count - self.nodes.len();
        if removed > 0 {
            self.last_updated = Utc::now();
        }
        removed
    }
}

/// Represents a connection to a Meshtastic device
pub struct MeshtasticDevice {
    #[allow(dead_code)]
    port_name: String,
    #[allow(dead_code)]
    baud_rate: u32,
    #[cfg(feature = "serial")]
    port: Option<Box<dyn SerialPort>>,
    #[cfg(feature = "meshtastic-proto")]
    slip: slip::SlipDecoder,
    #[cfg(feature = "meshtastic-proto")]
    config_request_id: Option<u32>,
    #[cfg(feature = "meshtastic-proto")]
    have_my_info: bool,
    #[cfg(feature = "meshtastic-proto")]
    have_radio_config: bool,
    #[cfg(feature = "meshtastic-proto")]
    have_module_config: bool,
    #[cfg(feature = "meshtastic-proto")]
    config_complete: bool,
    #[cfg(feature = "meshtastic-proto")]
    nodes: std::collections::HashMap<u32, proto::NodeInfo>,
    #[cfg(feature = "meshtastic-proto")]
    node_cache: NodeCache,
    #[cfg(feature = "meshtastic-proto")]
    cache_file_path: String,
    #[cfg(feature = "meshtastic-proto")]
    binary_frames_seen: bool,
    #[cfg(feature = "meshtastic-proto")]
    last_want_config_sent: Option<std::time::Instant>,
    #[cfg(feature = "meshtastic-proto")]
    rx_buf: Vec<u8>, // accumulation buffer for length-prefixed frames (0x94 0xC3 hdr)
    #[cfg(feature = "meshtastic-proto")]
    text_events: VecDeque<TextEvent>,
    #[cfg(feature = "meshtastic-proto")]
    our_node_id: Option<u32>,
}

/// Structured text event extracted from protobuf packets
#[cfg(feature = "meshtastic-proto")]
#[derive(Debug, Clone)]
pub struct TextEvent {
    pub source: u32,
    #[allow(dead_code)]
    pub dest: Option<u32>,
    pub is_direct: bool,
    pub channel: Option<u32>,
    pub content: String,
}

/// Reader task for continuous Meshtastic device reading
#[cfg(feature = "meshtastic-proto")]
pub struct MeshtasticReader {
    #[cfg(feature = "serial")]
    port: Arc<Mutex<Box<dyn SerialPort>>>,
    slip: slip::SlipDecoder,
    rx_buf: Vec<u8>,
    text_event_tx: mpsc::UnboundedSender<TextEvent>,
    control_rx: mpsc::UnboundedReceiver<ControlMessage>,
    writer_control_tx: mpsc::UnboundedSender<ControlMessage>,
    node_cache: NodeCache,
    cache_file_path: String,
    nodes: std::collections::HashMap<u32, proto::NodeInfo>,
    our_node_id: Option<u32>,
    binary_frames_seen: bool,
}

/// Writer task for Meshtastic device writing
#[cfg(feature = "meshtastic-proto")]
pub struct MeshtasticWriter {
    #[cfg(feature = "serial")]
    port: Arc<Mutex<Box<dyn SerialPort>>>,
    outgoing_rx: mpsc::UnboundedReceiver<OutgoingMessage>,
    control_rx: mpsc::UnboundedReceiver<ControlMessage>,
    our_node_id: Option<u32>,
    config_request_id: Option<u32>,
    last_want_config_sent: Option<std::time::Instant>,
    // Track pending reliable sends awaiting ACK
    pending: std::collections::HashMap<u32, PendingSend>,
    // Pacing: time of the last high-priority (reliable DM) send to avoid rate limiting
    last_high_priority_sent: Option<std::time::Instant>,
    // Gating: enforce a minimum interval between any text packet transmissions
    last_text_send: Option<std::time::Instant>,
    // Configuration tuning
    tuning: WriterTuning,
}

#[derive(Debug, Clone)]
struct PendingSend {
    to: u32,
    channel: u32,
    full_content: String,
    content_preview: String,
    attempts: u8,
    // When the next resend attempt is allowed (based on backoff schedule)
    next_due: std::time::Instant,
    // Index into BACKOFF_SECONDS for next scheduling step (capped at last)
    backoff_idx: u8,
}

impl MeshtasticDevice {
    #[cfg(feature = "meshtastic-proto")]
    pub fn format_node_label(&self, id: u32) -> String {
        if let Some(info) = self.nodes.get(&id) {
            if let Some(user) = &info.user {
                let ln = user.long_name.trim();
                if !ln.is_empty() { return ln.to_string(); }
            }
        }
        // Fallback to short uppercase hex (similar to Meshtastic short name style but simplified)
        format!("0x{:06X}", id & 0xFFFFFF)
    }

    #[cfg(feature = "meshtastic-proto")]
    pub fn format_node_short_label(&self, id: u32) -> String {
        if let Some(info) = self.nodes.get(&id) {
            if let Some(user) = &info.user {
                let sn = user.short_name.trim();
                if !sn.is_empty() { return sn.to_string(); }
            }
        }
        format!("0x{:06X}", id & 0xFFFFFF)
    }

    #[cfg(feature = "meshtastic-proto")]
    pub fn format_node_combined(&self, id: u32) -> (String, String) {
        let short = self.format_node_short_label(id);
        let long = self.format_node_label(id);
        (short, long)
    }

    #[cfg(feature = "meshtastic-proto")]
    #[allow(dead_code)]
    pub fn our_node_id(&self) -> Option<u32> { self.our_node_id }
    // ---------- Public state accessors (read-only) ----------
    #[cfg(feature = "meshtastic-proto")]
    pub fn binary_detected(&self) -> bool { self.binary_frames_seen }
    #[cfg(feature = "meshtastic-proto")]
    pub fn is_config_complete(&self) -> bool { self.config_complete }
    #[cfg(feature = "meshtastic-proto")]
    pub fn have_my_info(&self) -> bool { self.have_my_info }
    #[cfg(feature = "meshtastic-proto")]
    pub fn have_radio_config(&self) -> bool { self.have_radio_config }
    #[cfg(feature = "meshtastic-proto")]
    pub fn have_module_config(&self) -> bool { self.have_module_config }
    #[cfg(feature = "meshtastic-proto")]
    pub fn node_count(&self) -> usize { self.nodes.len() }
    #[cfg(feature = "meshtastic-proto")]
    pub fn config_request_id_hex(&self) -> Option<String> { self.config_request_id.map(|id| format!("0x{:08x}", id)) }

    /// Load node cache from persistent storage
    #[cfg(feature = "meshtastic-proto")]
    #[allow(dead_code)]
    pub fn load_node_cache(&mut self) -> anyhow::Result<()> {
        if Path::new(&self.cache_file_path).exists() {
            match NodeCache::load_from_file(&self.cache_file_path) {
                Ok(cache) => {
                    info!("Loaded {} cached nodes from {}", cache.nodes.len(), self.cache_file_path);
                    // Merge cached nodes into runtime nodes
                    for (node_id, cached_node) in &cache.nodes {
                        if !self.nodes.contains_key(node_id) {
                            let mut node_info = proto::NodeInfo {
                                num: *node_id,
                                ..Default::default()
                            };
                            node_info.user = Some(proto::User {
                                long_name: cached_node.long_name.clone(),
                                short_name: cached_node.short_name.clone(),
                                ..Default::default()
                            });
                            self.nodes.insert(*node_id, node_info);
                        }
                    }
                    self.node_cache = cache;
                    Ok(())
                },
                Err(e) => {
                    warn!("Failed to load node cache: {}", e);
                    Ok(()) // Continue without cache
                }
            }
        } else {
            info!("No node cache file found at {}, starting fresh", self.cache_file_path);
            Ok(())
        }
    }

    /// Save node cache to persistent storage
    #[cfg(feature = "meshtastic-proto")]
    pub fn save_node_cache(&self) -> anyhow::Result<()> {
        // Ensure directory exists
        if let Some(parent) = Path::new(&self.cache_file_path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        self.node_cache.save_to_file(&self.cache_file_path)
    }

    /// Clean up stale nodes from cache (nodes not seen for specified days)
    #[cfg(feature = "meshtastic-proto")]
    #[allow(dead_code)]
    pub fn cleanup_stale_nodes(&mut self, max_age_days: u32) -> anyhow::Result<usize> {
        let removed = self.node_cache.remove_stale_nodes(max_age_days);
        if removed > 0 {
            info!("Cleaned up {} stale nodes from cache", removed);
            self.save_node_cache()?;
        }
        Ok(removed)
    }

    /// Create a new Meshtastic device connection
    pub async fn new(port_name: &str, baud_rate: u32) -> Result<Self> {
        info!("Initializing Meshtastic device on {} at {} baud", port_name, baud_rate);
        
        #[cfg(feature = "serial")]
        {
            let mut builder = serialport::new(port_name, baud_rate)
                .timeout(Duration::from_millis(500));
            // Some USB serial adapters need explicit settings
            #[cfg(unix)] { builder = builder.data_bits(serialport::DataBits::Eight).stop_bits(serialport::StopBits::One).parity(serialport::Parity::None); }
            let mut port = builder
                .open()
                .map_err(|e| anyhow!("Failed to open serial port {}: {}", port_name, e))?;
            // Toggle DTR/RTS to reset/ensure device wakes (common for ESP32 based boards)
            let _ = port.write_data_terminal_ready(true);
            let _ = port.write_request_to_send(true);
            // Small settle delay
            sleep(Duration::from_millis(150)).await;
            // Clear any existing buffered startup text
            let mut purge_buf = [0u8; 512];
            if let Ok(available) = port.bytes_to_read() { if available > 0 { let _ = port.read(&mut purge_buf); } }
            debug!("Serial port initialized, flushed existing {} bytes", purge_buf.len());
            Ok(MeshtasticDevice {
                    port_name: port_name.to_string(),
                    baud_rate,
                    port: Some(port),
            #[cfg(feature = "meshtastic-proto")]
            slip: slip::SlipDecoder::new(),
            #[cfg(feature = "meshtastic-proto")]
            config_request_id: None,
            #[cfg(feature = "meshtastic-proto")]
            have_my_info: false,
            #[cfg(feature = "meshtastic-proto")]
            have_radio_config: false,
            #[cfg(feature = "meshtastic-proto")]
            have_module_config: false,
            #[cfg(feature = "meshtastic-proto")]
            config_complete: false,
            #[cfg(feature = "meshtastic-proto")]
            nodes: std::collections::HashMap::new(),
            #[cfg(feature = "meshtastic-proto")]
            node_cache: NodeCache::new(),
            #[cfg(feature = "meshtastic-proto")]
            cache_file_path: "data/node_cache.json".to_string(),
            #[cfg(feature = "meshtastic-proto")]
            binary_frames_seen: false,
            #[cfg(feature = "meshtastic-proto")]
            last_want_config_sent: None,
            #[cfg(feature = "meshtastic-proto")]
            rx_buf: Vec::new(),
            #[cfg(feature = "meshtastic-proto")]
            text_events: VecDeque::new(),
            #[cfg(feature = "meshtastic-proto")]
            our_node_id: None,
                })
        }
        
        #[cfg(not(feature = "serial"))]
        {
            warn!("Serial support not compiled in, using mock device");
            Ok(MeshtasticDevice {
                port_name: port_name.to_string(),
                baud_rate,
                #[cfg(feature = "meshtastic-proto")]
                slip: slip::SlipDecoder::new(),
                #[cfg(feature = "meshtastic-proto")]
                config_request_id: None,
                #[cfg(feature = "meshtastic-proto")]
                have_my_info: false,
                #[cfg(feature = "meshtastic-proto")]
                have_radio_config: false,
                #[cfg(feature = "meshtastic-proto")]
                have_module_config: false,
                #[cfg(feature = "meshtastic-proto")]
                config_complete: false,
                #[cfg(feature = "meshtastic-proto")]
                nodes: std::collections::HashMap::new(),
                #[cfg(feature = "meshtastic-proto")]
                node_cache: NodeCache::new(),
                #[cfg(feature = "meshtastic-proto")]
                cache_file_path: "data/node_cache.json".to_string(),
                #[cfg(feature = "meshtastic-proto")]
                binary_frames_seen: false,
                #[cfg(feature = "meshtastic-proto")]
                last_want_config_sent: None,
                #[cfg(feature = "meshtastic-proto")]
                rx_buf: Vec::new(),
                #[cfg(feature = "meshtastic-proto")]
                text_events: VecDeque::new(),
                #[cfg(feature = "meshtastic-proto")]
                our_node_id: None,
            })
        }
    }

    /// Receive a message from the device
    pub async fn receive_message(&mut self) -> Result<Option<String>> {
        #[cfg(feature = "serial")]
        {
            if let Some(ref mut port) = self.port {
                let mut buffer = [0; 1024];
                match port.read(&mut buffer) {
                    Ok(bytes_read) if bytes_read > 0 => {
                        let raw_slice = &buffer[..bytes_read];
                        trace!("RAW {} bytes: {}", bytes_read, hex_snippet(raw_slice, 64));
                        // Heuristic: if first byte looks like ASCII '{' or '[' we might be seeing JSON debug output - log it fully
                        if raw_slice[0] == b'{' || raw_slice[0] == b'[' { debug!("ASCII/JSON chunk: {}", String::from_utf8_lossy(raw_slice)); }
                        // First, try to interpret as protobuf (framed). Meshtastic typically uses
                        // a length-delimited protobuf framing. Here we do a heuristic attempt.
                        #[cfg(feature = "meshtastic-proto")]
                        // First try Meshtastic wired serial length-prefixed framing: 0x94 0xC3 len_hi len_lo
                        if cfg!(feature = "meshtastic-proto") {
                            self.rx_buf.extend_from_slice(raw_slice);
                            // Attempt to extract as many frames as present
                            'outer: loop {
                                if self.rx_buf.len() < 4 { break; }
                                // Realign to header if needed
                                if !(self.rx_buf[0] == 0x94 && self.rx_buf[1] == 0xC3) {
                                    // discard until possible header (avoid huge scans)
                                    if let Some(pos) = self.rx_buf.iter().position(|&b| b == 0x94) { if pos > 0 { self.rx_buf.drain(0..pos); } }
                                    else { self.rx_buf.clear(); break; }
                                    if self.rx_buf.len() < 4 { break; }
                                    if !(self.rx_buf[0]==0x94 && self.rx_buf[1]==0xC3) { continue; }
                                }
                                let declared = ((self.rx_buf[2] as usize) << 8) | (self.rx_buf[3] as usize);
                                // Basic sanity cap (avoid absurd lengths)
                                if declared == 0 || declared > 8192 { // unreasonable, shift one byte
                                    self.rx_buf.drain(0..1); continue; }
                                if self.rx_buf.len() < 4 + declared { break; }
                                let frame: Vec<u8> = self.rx_buf[4..4+declared].to_vec();
                                self.rx_buf.drain(0..4+declared);
                                if let Some(summary) = self.try_parse_protobuf_frame(&frame) {
                                    self.binary_frames_seen = true;
                                    self.update_state_from_summary(&summary);
                                    return Ok(Some(summary));
                                } else {
                                    // Not a FromRadio; ignore and continue (could be other message type)
                                    continue 'outer;
                                }
                            }
                        }
                        // SLIP framing path: some firmwares emit SLIP encoded protobuf frames
                        #[cfg(feature = "meshtastic-proto")]
                        {
                            let frames = self.slip.push(raw_slice);
                            if !frames.is_empty() { self.binary_frames_seen = true; }
                            for frame in frames {
                                trace!("SLIP frame {} bytes", frame.len());
                                if let Some(summary) = self.try_parse_protobuf_frame(&frame) {
                                    self.update_state_from_summary(&summary);
                                    return Ok(Some(summary));
                                }
                            }
                        }

                        // Fallback: treat as UTF-8 / ANSI diagnostic or simplified text frame
                        let message = String::from_utf8_lossy(raw_slice);
                        debug!("Received text: {}", message.trim());
                        if let Some(parsed) = self.parse_meshtastic_message(&message) {
                            return Ok(Some(parsed));
                        }
                    }
                    Ok(_) => {
                        // No data available
                        sleep(Duration::from_millis(10)).await;
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                        // Timeout is normal
                        sleep(Duration::from_millis(10)).await;
                    }
                    Err(e) => {
                        // Treat EINTR (Interrupted system call) gracefully: occurs during CTRL-C/shutdown signals.
                        if e.kind() == std::io::ErrorKind::Interrupted {
                            debug!("Serial read interrupted (EINTR), likely shutdown in progress");
                            sleep(Duration::from_millis(5)).await;
                            // Return None so outer logic can decide to continue loop without spamming errors
                            return Ok(None);
                        }
                        error!("Serial read error: {}", e);
                        return Err(anyhow!("Serial read error: {}", e));
                    }
                }
            }
        }
        
        #[cfg(not(feature = "serial"))]
        {
            // Mock implementation for testing
            sleep(Duration::from_millis(100)).await;
        }
        
        Ok(None)
    }

    /// Send a message to a specific node
    #[allow(dead_code)]
    pub async fn send_message(&mut self, to_node: &str, message: &str) -> Result<()> {
        // When protobuf support is enabled we send a proper MeshPacket so real
        // Meshtastic nodes/app clients will display the reply. Fallback to the
        // legacy ASCII stub otherwise.
        #[cfg(feature = "meshtastic-proto")]
        {
            if let Ok(numeric) = u32::from_str_radix(to_node.trim_start_matches("0x"), 16) {
                // Treat to_node as hex node id; channel 0 (primary) for now.
                self.send_text_packet(Some(numeric), 0, message)?;
                return Ok(());
            } else if let Ok(numeric_dec) = to_node.parse::<u32>() {
                self.send_text_packet(Some(numeric_dec), 0, message)?;
                return Ok(());
            }
            // If parsing fails, fall back to legacy path below.
        }

        let formatted_message = format!("TO:{} MSG:{}\n", to_node, message);

        #[cfg(feature = "serial")]
        {
            if let Some(ref mut port) = self.port {
                port.write_all(formatted_message.as_bytes())
                    .map_err(|e| anyhow!("Failed to write to serial port: {}", e))?;
                port.flush()
                    .map_err(|e| anyhow!("Failed to flush serial port: {}", e))?;

                debug!("(legacy) Sent to {}: {}", to_node, message);
            }
        }

        #[cfg(not(feature = "serial"))]
        {
            debug!("(legacy mock) send to {}: {}", to_node, message);
        }

        Ok(())
    }

    /// Parse a Meshtastic message into our format
    fn parse_meshtastic_message(&self, raw_message: &str) -> Option<String> {
        // This is a simplified parser for demonstration
        // Real implementation would parse actual Meshtastic protobuf messages
        
        let message = raw_message.trim();
        
        // Look for text message format: FROM:1234567890 MSG:Hello World
        if let Some(from_start) = message.find("FROM:") {
            if let Some(msg_start) = message.find("MSG:") {
                let from_end = message[from_start + 5..].find(' ').unwrap_or(message.len() - from_start - 5);
                let from_node = &message[from_start + 5..from_start + 5 + from_end];
                let msg_content = &message[msg_start + 4..];
                
                return Some(format!("{}:{}", from_node, msg_content));
            }
        }
        
        None
    }

    #[cfg(feature = "meshtastic-proto")]
    fn try_parse_protobuf_frame(&mut self, data: &[u8]) -> Option<String> {
        use proto::{FromRadio, PortNum};
        use proto::from_radio::PayloadVariant as FRPayload;
        use proto::mesh_packet::PayloadVariant as MPPayload;
        use prost::Message;
    let bytes = BytesMut::from(data);
        if let Ok(msg) = FromRadio::decode(&mut bytes.freeze()) {
            match msg.payload_variant.as_ref()? {
                FRPayload::ConfigCompleteId(id) => { return Some(format!("CONFIG_COMPLETE:{id}")); }
                FRPayload::Packet(pkt) => {
                    if let Some(MPPayload::Decoded(data_msg)) = &pkt.payload_variant {
                        let port = PortNum::try_from(data_msg.portnum).unwrap_or(PortNum::UnknownApp);
                        // Correlate explicit ACKs
                        if pkt.priority == 120 && data_msg.reply_id != 0 {
                            return Some(format!(
                                "ACK:id={} from=0x{:08x} to=0x{:08x} port={:?}",
                                data_msg.reply_id, pkt.from, pkt.to, port
                            ));
                        }
                        // Decode routing control messages and report errors tied to a specific id
                        if matches!(port, PortNum::RoutingApp) {
                            let mut b = BytesMut::from(&data_msg.payload[..]).freeze();
                            if let Ok(routing) = proto::Routing::decode(&mut b) {
                                use proto::routing::{Variant as RVar, Error as RErr};
                                if let Some(RVar::ErrorReason(e)) = routing.variant {
                                    if let Some(err) = RErr::try_from(e).ok() {
                                        let corr_id = if data_msg.request_id != 0 { data_msg.request_id } else { data_msg.reply_id };
                                        return Some(format!("ROUTING_ERROR:{:?}:id={}", err, corr_id));
                                    }
                                }
                            }
                        }
                        match port {
                            PortNum::TextMessageApp => {
                                if let Ok(text) = std::str::from_utf8(&data_msg.payload) {
                                    let dest = if pkt.to != 0 { Some(pkt.to) } else { None };
                                    // is_direct if destination equals our node id (when known) and not broadcast
                                    let is_direct = match (dest, self.our_node_id) { (Some(d), Some(our)) if d == our => true, _ => false };
                                    // In current Meshtastic proto, channel is a u32 field (0 = primary). Treat 0 as Some(0) for uniformity.
                                    let channel = Some(pkt.channel as u32);
                                    self.text_events.push_back(TextEvent { source: pkt.from, dest, is_direct, channel, content: text.to_string() });
                                    return Some(format!("TEXT:{}:{}", pkt.from, text));
                                }
                            }
                            PortNum::TextMessageCompressedApp => {
                                // Attempt naive decompression: if payload seems ASCII printable treat directly; else hex summarize.
                                let maybe_text = if data_msg.payload.iter().all(|b| b.is_ascii() && !b.is_ascii_control()) {
                                    Some(String::from_utf8_lossy(&data_msg.payload).to_string())
                                } else { None };
                                if let Some(text) = maybe_text {
                                    let dest = if pkt.to != 0 { Some(pkt.to) } else { None };
                                    let is_direct = match (dest, self.our_node_id) { (Some(d), Some(our)) if d == our => true, _ => false };
                                    let channel = Some(pkt.channel as u32);
                                    self.text_events.push_back(TextEvent { source: pkt.from, dest, is_direct, channel, content: text.clone() });
                                    return Some(format!("TEXT:{}:{}", pkt.from, text));
                                } else {
                                    let hex = data_msg.payload.iter().map(|b| format!("{:02x}", b)).collect::<String>();
                                    return Some(format!("CTEXT:{}:{}", pkt.from, hex));
                                }
                            }
                            _ => {
                                if let Some(s) = summarize_known_port_payload(port, &data_msg.payload) {
                                    return Some(format!("PKT:{}:port={:?}:{}", pkt.from, port, s));
                                } else {
                                    return Some(format!("PKT:{}:port={:?}:len={} hex={}...", pkt.from, port, data_msg.payload.len(), hex_snippet(&data_msg.payload, 12)));
                                }
                            },
                        }
                    }
                }
                FRPayload::MyInfo(info) => return Some(format!("MYINFO:{}", info.my_node_num)),
                FRPayload::NodeInfo(n) => {
                    if let Some(user) = &n.user {
                        return Some(format!("NODE:{}:{}:{}", n.num, user.long_name, user.short_name));
                    } else {
                        return Some(format!("NODE:{}:", n.num));
                    }
                },
                FRPayload::Config(_) => return Some("CONFIG".to_string()),
                FRPayload::ModuleConfig(_) => return Some("MODULE_CONFIG".to_string()),
                FRPayload::FileInfo(_) => return Some("FILE_INFO".to_string()),
                FRPayload::QueueStatus(qs) => {
                    if qs.res != 0 {
                        return Some(format!(
                            "QUEUE_STATUS:res={} FREE={}/{} id={} (non-zero res)", qs.res, qs.free, qs.maxlen, qs.mesh_packet_id
                        ));
                    } else {
                        return Some(format!(
                            "QUEUE_STATUS:res={} free={}/{} id={}", qs.res, qs.free, qs.maxlen, qs.mesh_packet_id
                        ));
                    }
                },
                FRPayload::XmodemPacket(_) => return Some("XMODEM_PACKET".to_string()),
                FRPayload::Metadata(_) => return Some("METADATA".to_string()),
                FRPayload::MqttClientProxyMessage(_) => return Some("MQTT_PROXY".to_string()),
                _ => {}
            }
        }
        None
    }

    #[cfg(feature = "meshtastic-proto")]
    pub fn update_state_from_summary(&mut self, summary: &str) {
        if summary.starts_with("MYINFO:") { self.have_my_info = true; }
        else if summary.starts_with("NODE:") {
            let parts: Vec<&str> = summary.split(':').collect();
            if parts.len() >= 2 { if let Ok(id) = parts[1].parse::<u32>() {
                let long_name = if parts.len() >= 3 { parts[2].to_string() } else { String::new() };
                let network_short_name = if parts.len() >= 4 { parts[3].to_string() } else { String::new() };
                
                // Prefer network short name if available and non-empty
                let short_name = if !network_short_name.trim().is_empty() {
                    debug!("Using network short name '{}' for node {} ({})", network_short_name.trim(), id, long_name);
                    network_short_name.trim().to_string()
                } else if !long_name.is_empty() { 
                    // Generate short name from long name (first 4 chars or similar)
                    let generated = long_name.chars().take(4).collect::<String>().to_uppercase();
                    info!("Generated short name '{}' from long name '{}' for node {}", generated, long_name, id);
                    generated
                } else { 
                    format!("{:04X}", id & 0xFFFF) 
                };
                
                let mut ni = proto::NodeInfo { num: id, ..Default::default() };
                if !long_name.is_empty() { 
                    ni.user = Some(proto::User{ 
                        long_name: long_name.clone(), 
                        short_name: short_name.clone(),
                        ..Default::default()
                    }); 
                }
                self.nodes.insert(id, ni);
                
                // Update cache
                self.node_cache.update_node(id, long_name, short_name);
                // Save cache asynchronously (best effort, don't block on failure)
                if let Err(e) = self.save_node_cache() {
                    debug!("Failed to save node cache: {}", e);
                }
            }}
        }
    else if summary == "CONFIG" { self.have_radio_config = true; }
        else if summary == "MODULE_CONFIG" { self.have_module_config = true; }
        else if summary.starts_with("CONFIG_COMPLETE:") {
            if let Some(id_str) = summary.split(':').nth(1) { if let Ok(id_val) = id_str.parse::<u32>() { if self.config_request_id == Some(id_val) { self.config_complete = true; }}}
        }
        if summary.starts_with("MYINFO:") {
            if let Some(id_str) = summary.split(':').nth(1) { if let Ok(id_val) = id_str.parse::<u32>() { self.our_node_id = Some(id_val); }}
        }
    }

    #[cfg(feature = "meshtastic-proto")]
    pub fn initial_sync_complete(&self) -> bool { self.config_complete && self.have_my_info && self.have_radio_config }
    /// Disconnect from the device
    pub async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from Meshtastic device");
        #[cfg(feature = "serial")]
        { self.port = None; }
        Ok(())
    }

}

#[cfg(feature = "meshtastic-proto")]
impl MeshtasticDevice {
    /// Retrieve next structured text event if available
    #[allow(dead_code)]
    pub fn next_text_event(&mut self) -> Option<TextEvent> { self.text_events.pop_front() }
    /// Build and send a text message MeshPacket via ToRadio (feature gated).
    /// to: Some(node_id) for direct, None for broadcast
    /// channel: channel index (0 primary)
    #[cfg(feature = "meshtastic-proto")]
    #[allow(dead_code)]
    pub fn send_text_packet(&mut self, to: Option<u32>, channel: u32, text: &str) -> Result<()> {
        use proto::{ToRadio, MeshPacket, Data, PortNum};
        use proto::to_radio::PayloadVariant as TRPayload;
        use proto::mesh_packet::PayloadVariant as MPPayload;
        use prost::Message;
        // Determine destination: broadcast if None
        let dest = to.unwrap_or(0xffffffff);
        // Populate Data payload
        let data_msg = Data {
            portnum: PortNum::TextMessageApp as i32,
            payload: text.as_bytes().to_vec().into(),
            want_response: false,
            dest: 0, // filled by firmware
            source: 0,
            request_id: 0,
            reply_id: 0,
            emoji: 0,
            bitfield: None,
            ..Default::default()
        };
        let from_node = self.our_node_id.ok_or_else(|| 
            anyhow!("Cannot send message: our_node_id not yet known (device may not have provided MYINFO)")
        )?;
        
        // For direct messages (DMs), use reliable delivery to try to force immediate transmission
        let is_dm = to.is_some() && dest != 0xffffffff;
        let packet_id = if is_dm { 
            // Generate unique ID for reliable packets (required for want_ack)
            use std::time::{SystemTime, UNIX_EPOCH};
            let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
            (since_epoch.as_secs() as u32) ^ (since_epoch.subsec_nanos()) // Simple ID generation
        } else { 
            0 // Broadcast packets don't need ID
        };
        
        let pkt = MeshPacket {
            from: from_node,
            to: dest,
            channel,
            payload_variant: Some(MPPayload::Decoded(data_msg)),
            id: packet_id,
            rx_time: 0,
            rx_snr: 0.0,
            hop_limit: 3, // Allow routing through mesh
            want_ack: is_dm, // Request ACK for DMs to trigger immediate transmission
            priority: if is_dm { 70 } else { 0 }, // Use RELIABLE priority (70) for DMs, DEFAULT (0) for broadcasts
            ..Default::default()
        };
        

        
        let toradio = ToRadio { payload_variant: Some(TRPayload::Packet(pkt)) };
        // Encode and send using existing framing helper
        #[cfg(feature = "serial")]
        if let Some(ref mut port) = self.port {
            let mut payload = Vec::with_capacity(128);
            toradio.encode(&mut payload)?;
            let mut hdr = [0u8;4]; hdr[0]=0x94; hdr[1]=0xC3; hdr[2]=((payload.len()>>8)&0xFF) as u8; hdr[3]=(payload.len()&0xFF) as u8;
            port.write_all(&hdr)?; port.write_all(&payload)?; port.flush()?;
            
            // Add a small delay to allow the OS to flush the serial buffer.
            std::thread::sleep(std::time::Duration::from_millis(50));

            let display_text = if text.len() > 80 { 
                format!("{}...", &text[..77])
            } else { 
                text.replace('\n', "\\n").replace('\r', "\\r")
            };
            let msg_type = if is_dm { "DM (reliable)" } else { "broadcast" };
            debug!("Sent TextPacket ({}): from=0x{:08x} to=0x{:08x} channel={} id={} want_ack={} priority={} ({} bytes payload) text='{}'", 
                  msg_type, self.our_node_id.unwrap_or(0), dest, channel, packet_id, is_dm, 
                  if is_dm { 70 } else { 0 }, payload.len(), display_text);
            if log::log_enabled!(log::Level::Trace) {
                let mut hex = String::with_capacity(payload.len()*2);
                for b in &payload { use std::fmt::Write; let _=write!(&mut hex, "{:02x}", b); }
                trace!("ToRadio payload hex:{}", hex);
            }
        }
        #[cfg(not(feature = "serial"))]
        {
            debug!("(mock) Would send TextPacket to 0x{:08x}: '{}'", dest, text);
        }
        
        // For DMs, send a heartbeat immediately after to try to trigger radio transmission
        if is_dm {
            if let Err(e) = self.send_heartbeat() {
                warn!("Failed to send heartbeat after DM: {}", e);
            } else {
                trace!("Sent heartbeat after DM to trigger radio activity");
            }
        }
        
        Ok(())
    }
}

// (Public crate visibility no longer required; kept private above.)

#[cfg(feature = "meshtastic-proto")]
impl MeshtasticDevice {
    /// Send a ToRadio.WantConfigId request to trigger the node database/config push.
    pub fn send_want_config(&mut self, request_id: u32) -> Result<()> {
        use proto::to_radio::PayloadVariant;
        let msg = proto::ToRadio { payload_variant: Some(PayloadVariant::WantConfigId(request_id)) };
        #[cfg(feature = "meshtastic-proto")]
        { self.last_want_config_sent = Some(std::time::Instant::now()); }
        self.send_toradio(msg)
    }

    /// Send a heartbeat frame (optional, can help keep link active)
    pub fn send_heartbeat(&mut self) -> Result<()> {
        use proto::{Heartbeat, ToRadio};
        use proto::to_radio::PayloadVariant;
        // nonce can be any incrementing value; for now just use a simple timestamp-based low bits
        let nonce = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() & 0xffff) as u32;
        let hb = Heartbeat { nonce };
        let msg = ToRadio { payload_variant: Some(PayloadVariant::Heartbeat(hb)) };
        self.send_toradio(msg)
    }

    fn send_toradio(&mut self, msg: proto::ToRadio) -> Result<()> {
        use prost::Message;
        #[cfg(feature = "serial")]
        if let Some(ref mut port) = self.port {
            let mut payload = Vec::with_capacity(256);
            msg.encode(&mut payload)?;
            if payload.len() > u16::MAX as usize { return Err(anyhow!("payload too large")); }
            let mut hdr = [0u8;4]; hdr[0]=0x94; hdr[1]=0xC3; hdr[2]=((payload.len()>>8)&0xFF) as u8; hdr[3]=(payload.len()&0xFF) as u8;
            port.write_all(&hdr)?; port.write_all(&payload)?; port.flush()?;
            debug!("Sent ToRadio LEN frame ({} bytes payload)", payload.len());
        }
        Ok(())
    }
    /// Ensure a want_config request is active; resend occasionally until sync completes.
    #[cfg(feature = "meshtastic-proto")]
    pub fn ensure_want_config(&mut self) -> Result<()> {
        if self.config_request_id.is_none() {
            let mut id: u32 = rand::random(); if id == 0 { id = 1; }
            self.config_request_id = Some(id);
            debug!("Generated config_request_id=0x{:08x}", id);
            self.send_want_config(id)?;
            return Ok(());
        }
        if self.initial_sync_complete() { return Ok(()); }
        if let Some(last) = self.last_want_config_sent { if last.elapsed() > std::time::Duration::from_secs(7) {
            let id = self.config_request_id.unwrap();
            debug!("Resending want_config_id=0x{:08x}", id);
            self.send_want_config(id)?;
        }}
        Ok(())
    }
}

/// Create a shared serial port connection for both reader and writer
#[cfg(feature = "serial")]
async fn create_shared_serial_port(port_name: &str, baud_rate: u32) -> Result<Arc<Mutex<Box<dyn SerialPort>>>> {
    info!("Opening shared serial port {} at {} baud", port_name, baud_rate);
    
    let mut builder = serialport::new(port_name, baud_rate)
        .timeout(Duration::from_millis(500));
    #[cfg(unix)] { 
        builder = builder.data_bits(serialport::DataBits::Eight)
            .stop_bits(serialport::StopBits::One)
            .parity(serialport::Parity::None); 
    }
    let mut port = builder
        .open()
        .map_err(|e| anyhow!("Failed to open serial port {}: {}", port_name, e))?;
    
    // Toggle DTR/RTS to reset/ensure device wakes
    let _ = port.write_data_terminal_ready(true);
    let _ = port.write_request_to_send(true);
    sleep(Duration::from_millis(150)).await;
    
    // Clear any existing buffered startup text
    let mut purge_buf = [0u8; 512];
    if let Ok(available) = port.bytes_to_read() { 
        if available > 0 { 
            let _ = port.read(&mut purge_buf); 
        } 
    }
    
    debug!("Shared serial port initialized successfully");
    Ok(Arc::new(Mutex::new(port)))
}

#[cfg(feature = "meshtastic-proto")]
impl MeshtasticReader {
    /// Create a new reader task with shared port
    pub async fn new(
        shared_port: Arc<Mutex<Box<dyn SerialPort>>>,
        text_event_tx: mpsc::UnboundedSender<TextEvent>,
        control_rx: mpsc::UnboundedReceiver<ControlMessage>,
        writer_control_tx: mpsc::UnboundedSender<ControlMessage>,
    ) -> Result<Self> {
        info!("Initializing Meshtastic reader with shared port");
        
        Ok(MeshtasticReader {
            #[cfg(feature = "serial")]
            port: shared_port,
            slip: slip::SlipDecoder::new(),
            rx_buf: Vec::new(),
            text_event_tx,
            control_rx,
            writer_control_tx,
            node_cache: NodeCache::new(),
            cache_file_path: "data/node_cache.json".to_string(),
            nodes: std::collections::HashMap::new(),
            our_node_id: None,
            binary_frames_seen: false,
        })
    }
    
    /// Create a mock reader for non-serial builds
    #[cfg(not(feature = "serial"))]
    pub async fn new_mock(
        text_event_tx: mpsc::UnboundedSender<TextEvent>,
        control_rx: mpsc::UnboundedReceiver<ControlMessage>,
        writer_control_tx: mpsc::UnboundedSender<ControlMessage>,
    ) -> Result<Self> {
        info!("Initializing mock Meshtastic reader");
        
        Ok(MeshtasticReader {
            slip: slip::SlipDecoder::new(),
            rx_buf: Vec::new(),
            text_event_tx,
            control_rx,
            writer_control_tx,
            node_cache: NodeCache::new(),
            cache_file_path: "data/node_cache.json".to_string(),
            nodes: std::collections::HashMap::new(),
            our_node_id: None,
            binary_frames_seen: false,
        })
    }

    /// Run the continuous reading task
    pub async fn run(mut self) -> Result<()> {
        info!("Starting Meshtastic reader task");
        
        // Load node cache
        if let Err(e) = self.load_node_cache() {
            warn!("Failed to load node cache: {}", e);
        }
        
        let mut interval = tokio::time::interval(Duration::from_millis(10));
        
        loop {
            tokio::select! {
                // Check for control messages
                control_msg = self.control_rx.recv() => {
                    match control_msg {
                        Some(ControlMessage::Shutdown) => {
                            info!("Reader task received shutdown signal");
                            break;
                        }
                        Some(ControlMessage::DeviceStatus) => {
                            debug!("Reader: binary_frames_seen={}, our_node_id={:?}, node_count={}", 
                                   self.binary_frames_seen, self.our_node_id, self.nodes.len());
                        }
                        Some(_) => {
                            // Other control messages not handled by reader
                        }
                        None => {
                            warn!("Control channel closed, shutting down reader");
                            break;
                        }
                    }
                }
                
                // Read from device
                _ = interval.tick() => {
                    if let Err(e) = self.read_and_process().await {
                        match e.downcast_ref::<std::io::Error>() {
                            Some(io_err) if io_err.kind() == std::io::ErrorKind::Interrupted => {
                                debug!("Reader interrupted (EINTR), likely shutdown in progress");
                                break;
                            }
                            _ => {
                                error!("Reader error: {}", e);
                                // Continue running unless it's a fatal error
                            }
                        }
                    }
                }
            }
        }
        
        info!("Meshtastic reader task shutting down");
        Ok(())
    }

    async fn read_and_process(&mut self) -> Result<()> {
        #[cfg(feature = "serial")]
        {
            let mut buffer = [0; 1024];
            let read_result = {
                let mut port = self.port.lock().unwrap();
                port.read(&mut buffer)
            };
            
            match read_result {
                Ok(bytes_read) if bytes_read > 0 => {
                    let raw_slice = &buffer[..bytes_read];
                    trace!("RAW {} bytes: {}", bytes_read, hex_snippet(raw_slice, 64));
                    
                    // Try length-prefixed framing first: 0x94 0xC3 len_hi len_lo
                    self.rx_buf.extend_from_slice(raw_slice);
                    self.process_framed_messages().await?;
                    
                    // Try SLIP framing
                    let frames = self.slip.push(raw_slice);
                    if !frames.is_empty() { 
                        self.binary_frames_seen = true; 
                    }
                    for frame in frames {
                        trace!("SLIP frame {} bytes", frame.len());
                        self.process_protobuf_frame(&frame).await?;
                    }
                    
                    // Fallback: treat as text for legacy compatibility
                    let message = String::from_utf8_lossy(raw_slice);
                    if let Some(parsed) = self.parse_legacy_text(&message) {
                        debug!("Legacy text message: {}", parsed);
                        // Convert to TextEvent if possible
                        if let Some(event) = self.text_to_event(&parsed) {
                            let _ = self.text_event_tx.send(event);
                        }
                    }
                }
                Ok(_) => {
                    // No data available, normal
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    // Timeout is normal
                }
                Err(e) => {
                    return Err(anyhow!("Serial read error: {}", e));
                }
            }
        }
        
        #[cfg(not(feature = "serial"))]
        {
            // Mock implementation for testing
            sleep(Duration::from_millis(100)).await;
        }
        
        Ok(())
    }

    async fn process_framed_messages(&mut self) -> Result<()> {
        loop {
            if self.rx_buf.len() < 4 { break; }
            
            // Realign to header if needed
            if !(self.rx_buf[0] == 0x94 && self.rx_buf[1] == 0xC3) {
                if let Some(pos) = self.rx_buf.iter().position(|&b| b == 0x94) { 
                    if pos > 0 { 
                        self.rx_buf.drain(0..pos); 
                    } 
                }
                else { 
                    self.rx_buf.clear(); 
                    break; 
                }
                if self.rx_buf.len() < 4 { break; }
                if !(self.rx_buf[0]==0x94 && self.rx_buf[1]==0xC3) { continue; }
            }
            
            let declared = ((self.rx_buf[2] as usize) << 8) | (self.rx_buf[3] as usize);
            if declared == 0 || declared > 8192 { 
                self.rx_buf.drain(0..1); 
                continue; 
            }
            if self.rx_buf.len() < 4 + declared { break; }
            
            let frame: Vec<u8> = self.rx_buf[4..4+declared].to_vec();
            self.rx_buf.drain(0..4+declared);
            
            self.binary_frames_seen = true;
            self.process_protobuf_frame(&frame).await?;
        }
        Ok(())
    }

    async fn process_protobuf_frame(&mut self, data: &[u8]) -> Result<()> {
        use proto::{FromRadio, PortNum};
        use proto::from_radio::PayloadVariant as FRPayload;
        use proto::mesh_packet::PayloadVariant as MPPayload;
        use prost::Message;
        
        let bytes = BytesMut::from(data);
        if let Ok(msg) = FromRadio::decode(&mut bytes.freeze()) {
            match msg.payload_variant.as_ref() {
                Some(FRPayload::ConfigCompleteId(_id)) => {
                    debug!("Received config_complete_id");
                }
                Some(FRPayload::Packet(pkt)) => {
                    if let Some(MPPayload::Decoded(data_msg)) = &pkt.payload_variant {
                        let port = PortNum::try_from(data_msg.portnum).unwrap_or(PortNum::UnknownApp);

                        // Correlate explicit ACKs (priority=ACK and reply_id set)
                        if pkt.priority == 120 && data_msg.reply_id != 0 {
                            debug!(
                                "ACK received: id={} from=0x{:08x} to=0x{:08x} port={:?}",
                                data_msg.reply_id, pkt.from, pkt.to, port
                            );
                            let _ = self.writer_control_tx.send(ControlMessage::AckReceived(data_msg.reply_id));
                        }

                        // Decode routing control messages and report errors tied to a specific id
                        if matches!(port, PortNum::RoutingApp) {
                            let mut b = BytesMut::from(&data_msg.payload[..]).freeze();
                            if let Ok(routing) = proto::Routing::decode(&mut b) {
                                use proto::routing::{Variant as RVar, Error as RErr};
                                if let Some(RVar::ErrorReason(e)) = routing.variant {
                                    if let Some(err) = RErr::try_from(e).ok() {
                                        let corr_id = if data_msg.request_id != 0 { data_msg.request_id } else { data_msg.reply_id };
                                        match err {
                                            RErr::None => {
                                                // Treat OK as delivery confirmation for the correlated id
                                                debug!(
                                                    "Routing status OK for id={} from=0x{:08x} to=0x{:08x}",
                                                    corr_id, pkt.from, pkt.to
                                                );
                                                if corr_id != 0 {
                                                    let _ = self.writer_control_tx.send(ControlMessage::AckReceived(corr_id));
                                                }
                                            }
                                            _ => {
                                                warn!(
                                                    "Routing error for id={} from=0x{:08x} to=0x{:08x}: {:?}",
                                                    corr_id, pkt.from, pkt.to, err
                                                );
                                                let _ = self.writer_control_tx.send(ControlMessage::RoutingError { id: corr_id, reason: e });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        match port {
                            PortNum::TextMessageApp => {
                                if let Ok(text) = std::str::from_utf8(&data_msg.payload) {
                                    let dest = if pkt.to != 0 { Some(pkt.to) } else { None };
                                    let is_direct = match (dest, self.our_node_id) { 
                                        (Some(d), Some(our)) if d == our => true, 
                                        _ => false 
                                    };
                                    let channel = Some(pkt.channel as u32);
                                    
                                    let event = TextEvent { 
                                        source: pkt.from, 
                                        dest, 
                                        is_direct, 
                                        channel, 
                                        content: text.to_string() 
                                    };
                                    
                                    let _ = self.text_event_tx.send(event);
                                }
                            }
                            PortNum::TextMessageCompressedApp => {
                                // Handle compressed text messages
                                let maybe_text = if data_msg.payload.iter().all(|b| b.is_ascii() && !b.is_ascii_control()) {
                                    Some(String::from_utf8_lossy(&data_msg.payload).to_string())
                                } else { 
                                    None 
                                };
                                
                                if let Some(text) = maybe_text {
                                    let dest = if pkt.to != 0 { Some(pkt.to) } else { None };
                                    let is_direct = match (dest, self.our_node_id) { 
                                        (Some(d), Some(our)) if d == our => true, 
                                        _ => false 
                                    };
                                    let channel = Some(pkt.channel as u32);
                                    
                                    let event = TextEvent { 
                                        source: pkt.from, 
                                        dest, 
                                        is_direct, 
                                        channel, 
                                        content: text
                                    };
                                    
                                    let _ = self.text_event_tx.send(event);
                                }
                            }
                            _ => {
                                if let Some(summary) = summarize_known_port_payload(port, &data_msg.payload) {
                                    debug!("Non-text packet from {}: port={:?} {}", pkt.from, port, summary);
                                } else {
                                    debug!("Non-text packet from {}: port={:?} len={} hex={}...", pkt.from, port, data_msg.payload.len(), hex_snippet(&data_msg.payload, 16));
                                }
                            }
                        }
                    }
                }
                Some(FRPayload::MyInfo(info)) => {
                    // Only send node ID to writer if we don't already know it
                    if self.our_node_id.is_none() {
                        self.our_node_id = Some(info.my_node_num);
                        debug!("Got our node ID: {}", info.my_node_num);
                        
                        // Notify the writer about our node ID (first time only)
                        if let Err(e) = self.writer_control_tx.send(ControlMessage::SetNodeId(info.my_node_num)) {
                            warn!("Failed to send node ID to writer: {}", e);
                        } else {
                            debug!("Sent node ID {} to writer", info.my_node_num);
                        }
                    } else {
                        // We already know our node ID, no need to spam the writer
                        trace!("Received duplicate MyInfo, ignoring (node ID already known)");
                    }
                }
                Some(FRPayload::NodeInfo(n)) => {
                    if let Some(user) = &n.user {
                        let long_name = user.long_name.clone();
                        let short_name = user.short_name.clone();
                        
                        self.nodes.insert(n.num, n.clone());
                        self.node_cache.update_node(n.num, long_name, short_name);
                        
                        // Save cache (best effort)
                        if let Err(e) = self.save_node_cache() {
                            debug!("Failed to save node cache: {}", e);
                        }
                        
                        debug!("Updated node info for {}: {} ({})", n.num, user.long_name, user.short_name);
                    }
                }
                Some(FRPayload::Config(config)) => {
                    let config_type = match &config.payload_variant {
                        Some(proto::config::PayloadVariant::Device(_)) => "device",
                        Some(proto::config::PayloadVariant::Position(_)) => "position",
                        Some(proto::config::PayloadVariant::Power(_)) => "power",
                        Some(proto::config::PayloadVariant::Network(_)) => "network",
                        Some(proto::config::PayloadVariant::Display(_)) => "display",
                        Some(proto::config::PayloadVariant::Lora(_)) => "lora",
                        Some(proto::config::PayloadVariant::Bluetooth(_)) => "bluetooth",
                        Some(proto::config::PayloadVariant::Security(_)) => "security",
                        Some(proto::config::PayloadVariant::Sessionkey(_)) => "sessionkey",
                        Some(proto::config::PayloadVariant::DeviceUi(_)) => "device_ui",
                        None => "unknown",
                    };
                    debug!("Received config: {}", config_type);
                }
                Some(FRPayload::LogRecord(log)) => {
                    debug!("Received log_record: {}", log.message);
                }
                Some(FRPayload::Rebooted(_)) => {
                    debug!("Received rebooted");
                }
                Some(FRPayload::ModuleConfig(module_config)) => {
                    let module_type = match &module_config.payload_variant {
                        Some(proto::module_config::PayloadVariant::Mqtt(_)) => "mqtt",
                        Some(proto::module_config::PayloadVariant::Serial(_)) => "serial",
                        Some(proto::module_config::PayloadVariant::ExternalNotification(_)) => "external_notification",
                        Some(proto::module_config::PayloadVariant::StoreForward(_)) => "store_forward",
                        Some(proto::module_config::PayloadVariant::RangeTest(_)) => "range_test",
                        Some(proto::module_config::PayloadVariant::Telemetry(_)) => "telemetry",
                        Some(proto::module_config::PayloadVariant::CannedMessage(_)) => "canned_message",
                        Some(proto::module_config::PayloadVariant::Audio(_)) => "audio",
                        Some(proto::module_config::PayloadVariant::RemoteHardware(_)) => "remote_hardware",
                        Some(proto::module_config::PayloadVariant::NeighborInfo(_)) => "neighbor_info",
                        Some(proto::module_config::PayloadVariant::AmbientLighting(_)) => "ambient_lighting",
                        Some(proto::module_config::PayloadVariant::DetectionSensor(_)) => "detection_sensor",
                        Some(proto::module_config::PayloadVariant::Paxcounter(_)) => "paxcounter",
                        None => "unknown",
                    };
                    debug!("Received moduleConfig: {}", module_type);
                }
                Some(FRPayload::Channel(channel)) => {
                    let channel_name = channel.settings.as_ref()
                        .map(|s| if s.name.is_empty() { "Default".to_string() } else { s.name.clone() })
                        .unwrap_or_else(|| "Disabled".to_string());
                    debug!("Received channel: index={} name='{}'", channel.index, channel_name);
                }
                Some(FRPayload::FileInfo(file_info)) => {
                    debug!("Received fileInfo: '{}' ({} bytes)", file_info.file_name, file_info.size_bytes);
                }
                Some(FRPayload::QueueStatus(qs)) => {
                    // res is an error/status code; free/maxlen reflect outgoing queue capacity
                    // mesh_packet_id links to a specific send attempt (0 when not applicable)
                    if qs.res != 0 {
                        debug!(
                            "Received queueStatus: res={} FREE={}/{} id={} (non-zero res)",
                            qs.res, qs.free, qs.maxlen, qs.mesh_packet_id
                        );
                    } else {
                        debug!(
                            "Received queueStatus: res={} free={}/{} mesh_packet_id={}",
                            qs.res, qs.free, qs.maxlen, qs.mesh_packet_id
                        );
                    }
                }
                Some(FRPayload::XmodemPacket(_)) => {
                    debug!("Received xmodemPacket");
                }
                Some(FRPayload::Metadata(_)) => {
                    debug!("Received metadata");
                }
                Some(FRPayload::MqttClientProxyMessage(_)) => {
                    debug!("Received mqttClientProxyMessage");
                }
                Some(FRPayload::ClientNotification(_)) => {
                    debug!("Received clientNotification");
                }
                Some(FRPayload::DeviceuiConfig(_)) => {
                    debug!("Received deviceuiConfig");
                }
                None => {
                    debug!("FromRadio message with no payload");
                }
            }
        }
        Ok(())
    }

    fn parse_legacy_text(&self, message: &str) -> Option<String> {
        let message = message.trim();
        
        // Look for text message format: FROM:1234567890 MSG:Hello World
        if let Some(from_start) = message.find("FROM:") {
            if let Some(msg_start) = message.find("MSG:") {
                let from_end = message[from_start + 5..].find(' ').unwrap_or(message.len() - from_start - 5);
                let from_node = &message[from_start + 5..from_start + 5 + from_end];
                let msg_content = &message[msg_start + 4..];
                
                return Some(format!("{}:{}", from_node, msg_content));
            }
        }
        
        None
    }

    fn text_to_event(&self, text: &str) -> Option<TextEvent> {
        // Parse legacy text format into TextEvent
        if let Some(colon_pos) = text.find(':') {
            let (from_str, content) = text.split_at(colon_pos);
            let content = &content[1..]; // Remove the colon
            
            if let Ok(source) = from_str.parse::<u32>() {
                return Some(TextEvent {
                    source,
                    dest: None, // Legacy messages don't have explicit dest
                    is_direct: false, // Assume public for legacy
                    channel: Some(0), // Assume primary channel
                    content: content.to_string(),
                });
            }
        }
        None
    }

    fn load_node_cache(&mut self) -> Result<()> {
        if std::path::Path::new(&self.cache_file_path).exists() {
            match NodeCache::load_from_file(&self.cache_file_path) {
                Ok(cache) => {
                    info!("Loaded {} cached nodes from {}", cache.nodes.len(), self.cache_file_path);
                    // Merge cached nodes into runtime nodes
                    for (node_id, cached_node) in &cache.nodes {
                        if !self.nodes.contains_key(node_id) {
                            let mut node_info = proto::NodeInfo {
                                num: *node_id,
                                ..Default::default()
                            };
                            node_info.user = Some(proto::User {
                                long_name: cached_node.long_name.clone(),
                                short_name: cached_node.short_name.clone(),
                                ..Default::default()
                            });
                            self.nodes.insert(*node_id, node_info);
                        }
                    }
                    self.node_cache = cache;
                    Ok(())
                },
                Err(e) => {
                    warn!("Failed to load node cache: {}", e);
                    Ok(()) // Continue without cache
                }
            }
        } else {
            info!("No node cache file found at {}, starting fresh", self.cache_file_path);
            Ok(())
        }
    }

    fn save_node_cache(&self) -> Result<()> {
        // Ensure directory exists
        if let Some(parent) = std::path::Path::new(&self.cache_file_path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        self.node_cache.save_to_file(&self.cache_file_path)
    }
}

#[cfg(feature = "meshtastic-proto")]
impl MeshtasticWriter {
    /// Create a new writer task with shared port
    pub async fn new(
        shared_port: Arc<Mutex<Box<dyn SerialPort>>>,
        outgoing_rx: mpsc::UnboundedReceiver<OutgoingMessage>,
        control_rx: mpsc::UnboundedReceiver<ControlMessage>,
        tuning: WriterTuning,
    ) -> Result<Self> {
        info!("Initializing Meshtastic writer with shared port");
        
        Ok(MeshtasticWriter {
            #[cfg(feature = "serial")]
            port: shared_port,
            outgoing_rx,
            control_rx,
            our_node_id: None,
            config_request_id: None,
            last_want_config_sent: None,
            pending: std::collections::HashMap::new(),
            last_high_priority_sent: None,
            last_text_send: None,
            tuning,
        })
    }
    
    /// Create a mock writer for non-serial builds
    #[cfg(not(feature = "serial"))]
    pub async fn new_mock(
        outgoing_rx: mpsc::UnboundedReceiver<OutgoingMessage>,
        control_rx: mpsc::UnboundedReceiver<ControlMessage>,
        tuning: WriterTuning,
    ) -> Result<Self> {
        info!("Initializing mock Meshtastic writer");
        
        Ok(MeshtasticWriter {
            outgoing_rx,
            control_rx,
            our_node_id: None,
            config_request_id: None,
            last_want_config_sent: None,
            pending: std::collections::HashMap::new(),
            last_high_priority_sent: None,
            last_text_send: None,
            tuning,
        })
    }

    /// Run the writer task
    pub async fn run(mut self) -> Result<()> {
        info!("Starting Meshtastic writer task");
        
        // Send a single WantConfigId at startup to fetch node db and config
        if self.config_request_id.is_none() {
            let mut id: u32 = rand::random();
            if id == 0 { id = 1; }
            self.config_request_id = Some(id);
            info!("Requesting initial config from radio (want_config_id=0x{:08x})", id);
            if let Err(e) = self.send_want_config(id) {
                warn!("Initial config request failed: {}", e);
            }
        }

        let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(30));
        
        loop {
            tokio::select! {
                // Handle outgoing messages
                msg = self.outgoing_rx.recv() => {
                    match msg {
                        Some(outgoing) => {
                            if let Err(e) = self.send_message(&outgoing).await {
                                error!("Failed to send message: {}", e);
                            }
                        }
                        None => {
                            warn!("Outgoing message channel closed, shutting down writer");
                            break;
                        }
                    }
                }
                
                // Handle control messages
                control_msg = self.control_rx.recv() => {
                    match control_msg {
                        Some(ControlMessage::Shutdown) => {
                            info!("Writer task received shutdown signal");
                            break;
                        }
                        Some(ControlMessage::AckReceived(id)) => {
                            if let Some(p) = self.pending.remove(&id) {
                                info!("Delivered id={} to=0x{:08x} ({}), attempts={}", id, p.to, p.content_preview, p.attempts);
                            } else {
                                debug!("Delivered id={} (no pending entry)", id);
                            }
                        }
                        Some(ControlMessage::RoutingError { id, reason }) => {
                            // Map reason to enum when possible to decide if transient
                            #[allow(unused_imports)]
                            use crate::protobuf::meshtastic_generated as proto;
                            let transient = match proto::routing::Error::try_from(reason) {
                                Ok(proto::routing::Error::RateLimitExceeded)
                                | Ok(proto::routing::Error::DutyCycleLimit)
                                | Ok(proto::routing::Error::Timeout) => true,
                                _ => false,
                            };
                            if transient {
                                if let Some(p) = self.pending.get_mut(&id) {
                                    // Keep current backoff stage; just ensure next_due is at least the stage delay from now
                                    let backoffs = &self.tuning.dm_resend_backoff_seconds;
                                    let stage = backoffs.get(p.backoff_idx as usize)
                                        .copied()
                                        .unwrap_or_else(|| *backoffs.last().unwrap_or(&16));
                                    let delay = std::time::Duration::from_secs(stage);
                                    let min_due = std::time::Instant::now() + delay;
                                    if p.next_due < min_due { p.next_due = min_due; }
                                    warn!(
                                        "Transient routing error (reason={}) for id={} to=0x{:08x} ({}); will retry in {}s (stage {})",
                                        reason, id, p.to, p.content_preview, stage, p.backoff_idx
                                    );
                                } else {
                                    warn!("Transient routing error for id={} (no pending entry): reason={}", id, reason);
                                }
                            } else {
                                if let Some(p) = self.pending.remove(&id) {
                                    warn!("Failed id={} to=0x{:08x} ({}): reason={}", id, p.to, p.content_preview, reason);
                                } else {
                                    warn!("Failed id={} (routing error, no pending entry): reason={}", id, reason);
                                }
                            }
                        }
                        Some(ControlMessage::ConfigRequest(id)) => {
                            self.config_request_id = Some(id);
                            if let Err(e) = self.send_want_config(id) {
                                error!("Failed to send config request: {}", e);
                            }
                        }
                        Some(ControlMessage::Heartbeat) => {
                            if let Err(e) = self.send_heartbeat() {
                                error!("Failed to send heartbeat: {}", e);
                            }
                        }
                        Some(ControlMessage::SetNodeId(node_id)) => {
                            self.our_node_id = Some(node_id);
                            debug!("Writer: received node ID {}", node_id);
                        }
                        Some(_) => {
                            // Other control messages
                        }
                        None => {
                            warn!("Control channel closed, shutting down writer");
                            break;
                        }
                    }
                }
                
                // Periodic heartbeat
                _ = heartbeat_interval.tick() => {
                    // periodic heartbeat
                    if let Err(e) = self.send_heartbeat() { debug!("Heartbeat send error: {:?}", e); }

                    // scan pending for resends
                    const MAX_ATTEMPTS: u8 = 3;
                    let now = std::time::Instant::now();
                    // collect ids to resend or expire
                    let mut to_resend: Vec<u32> = Vec::new();
                    let mut to_expire: Vec<(u32, PendingSend)> = Vec::new();
                    for (id, p) in self.pending.iter_mut() {
                        if p.next_due <= now {
                            if p.attempts < MAX_ATTEMPTS {
                                p.attempts += 1;
                                to_resend.push(*id);
                            } else {
                                to_expire.push((*id, p.clone()));
                            }
                        }
                    }
                    for id in to_resend.into_iter() {
                        if let Some(p) = self.pending.get(&id).cloned() {
                            let to = p.to; let channel = p.channel; let content = p.full_content.clone();
                            // Re-send with same id and same full content
                            if let Err(e) = self.resend_text_packet(id, to, channel, &content).await {
                                warn!("Resend failed id={} to=0x{:08x}: {}", id, p.to, e);
                            } else {
                                // After a resend, increase backoff step and schedule next_due
                                if let Some(pp) = self.pending.get_mut(&id) {
                                    let backoffs = &self.tuning.dm_resend_backoff_seconds;
                                    let max_idx = (backoffs.len().saturating_sub(1)) as u8;
                                    let new_idx = std::cmp::min(pp.backoff_idx.saturating_add(1), max_idx);
                                    pp.backoff_idx = new_idx;
                                    let delay = std::time::Duration::from_secs(backoffs[new_idx as usize]);
                                    pp.next_due = std::time::Instant::now() + delay;
                                }
                                info!("Resent id={} to=0x{:08x} (attempt {})", id, p.to, self.pending.get(&id).map(|p| p.attempts).unwrap_or(0));
                            }
                        }
                    }
                    for (id, p) in to_expire.into_iter() {
                        self.pending.remove(&id);
                        warn!("Failed id={} to=0x{:08x} ({}): max attempts reached", id, p.to, p.content_preview);
                    }
                }
                
            }
        }
        
        info!("Meshtastic writer task shutting down");
        Ok(())
    }

    async fn send_message(&mut self, msg: &OutgoingMessage) -> Result<()> {
    // Enforce a minimum gap between text packet transmissions across the queue
    let min_gap = std::cmp::max(self.tuning.min_send_gap_ms, 2000);
    self.enforce_min_send_gap(Duration::from_millis(min_gap)).await;
        use proto::{ToRadio, MeshPacket, Data, PortNum};
        use proto::to_radio::PayloadVariant as TRPayload;
        use proto::mesh_packet::PayloadVariant as MPPayload;
        use prost::Message;
        
        let dest = msg.to_node.unwrap_or(0xffffffff);
        
        let data_msg = Data {
            portnum: PortNum::TextMessageApp as i32,
            payload: msg.content.as_bytes().to_vec().into(),
            want_response: false,
            dest: 0,
            source: 0,
            request_id: 0,
            reply_id: 0,
            emoji: 0,
            bitfield: None,
            ..Default::default()
        };
        
        let from_node = self.our_node_id.ok_or_else(|| 
            anyhow!("Cannot send message: our_node_id not yet known")
        )?;
        
        let is_dm = msg.to_node.is_some() && dest != 0xffffffff;
        let packet_id = if is_dm {
            use std::time::{SystemTime, UNIX_EPOCH};
            let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
            (since_epoch.as_secs() as u32) ^ (since_epoch.subsec_nanos())
        } else {
            0
        };
        
        let priority = match msg.priority { MessagePriority::High => 70, MessagePriority::Normal => 0 };

        // Pacing to reduce airtime fairness rate limiting
        // - If a reliable DM was just sent, delay a normal-priority broadcast slightly
        // - If another reliable DM is queued immediately after one, insert a small gap
        if let Some(last_hi) = self.last_high_priority_sent {
            let elapsed = last_hi.elapsed();
            if priority == 0 {
                // Normal broadcast shortly after a reliable DM: configurable delay
                let target_gap = std::time::Duration::from_millis(self.tuning.post_dm_broadcast_gap_ms);
                if elapsed < target_gap {
                    let wait = target_gap - elapsed;
                    debug!("Pacing: delaying broadcast by {}ms after recent reliable DM", wait.as_millis());
                    sleep(wait).await;
                }
            } else if priority == 70 {
                // Back-to-back reliable DMs: configurable gap
                let target_gap = std::time::Duration::from_millis(self.tuning.dm_to_dm_gap_ms);
                if elapsed < target_gap {
                    let wait = target_gap - elapsed;
                    debug!("Pacing: delaying reliable DM by {}ms to avoid rate limit", wait.as_millis());
                    sleep(wait).await;
                }
            }
        }
        
        let pkt = MeshPacket {
            from: from_node,
            to: dest,
            channel: msg.channel,
            payload_variant: Some(MPPayload::Decoded(data_msg)),
            id: packet_id,
            rx_time: 0,
            rx_snr: 0.0,
            hop_limit: 3,
            want_ack: is_dm,
            priority,
            ..Default::default()
        };
        
        let toradio = ToRadio { payload_variant: Some(TRPayload::Packet(pkt)) };
        
        #[cfg(feature = "serial")]
        {
            let mut payload = Vec::with_capacity(128);
            toradio.encode(&mut payload)?;
            let mut hdr = [0u8; 4];
            hdr[0] = 0x94;
            hdr[1] = 0xC3;
            hdr[2] = ((payload.len() >> 8) & 0xFF) as u8;
            hdr[3] = (payload.len() & 0xFF) as u8;
            
            {
                let mut port = self.port.lock().unwrap();
                port.write_all(&hdr)?;
                port.write_all(&payload)?;
                port.flush()?;
            }
            
            // Small delay to allow OS to flush the serial buffer
            sleep(Duration::from_millis(50)).await;
            // Record the time of this text packet send for gating
            self.last_text_send = Some(std::time::Instant::now());
            
            let display_text = if msg.content.len() > 80 {
                format!("{}...", &msg.content[..77])
            } else {
                msg.content.replace('\n', "\\n").replace('\r', "\\r")
            };
            
            let msg_type = if is_dm { "DM (reliable)" } else { "broadcast" };
            debug!(
                "Sent TextPacket ({}): to=0x{:08x} channel={} id={} want_ack={} priority={} ({} bytes) text='{}'",
                msg_type,
                dest,
                msg.channel,
                packet_id,
                is_dm,
                priority,
                payload.len(),
                display_text
            );

            // For DMs, record pending and proactively send a heartbeat to nudge immediate radio TX
            if is_dm {
                // capture a small preview for logging
                let preview = if msg.content.len() > 40 { format!("{}...", &msg.content[..37].replace('\n', "\\n").replace('\r', "\\r")) } else { msg.content.replace('\n', "\\n").replace('\r', "\\r") };
                let now = std::time::Instant::now();
                self.pending.insert(packet_id, PendingSend {
                    to: dest,
                    channel: msg.channel,
                    full_content: msg.content.clone(),
                    content_preview: preview,
                    attempts: 1,
                    // First retry scheduled after the first configured backoff stage
                    next_due: now + std::time::Duration::from_secs(*self.tuning.dm_resend_backoff_seconds.first().unwrap_or(&4)),
                    backoff_idx: 0,
                });
                if let Err(e) = self.send_heartbeat() {
                    warn!("Failed to send heartbeat after DM: {}", e);
                } else {
                    trace!("Sent heartbeat after DM to trigger radio activity");
                }
                // Update pacing marker for high-priority
                self.last_high_priority_sent = Some(std::time::Instant::now());
            } else if priority == 0 {
                // For broadcasts, do not update high-priority marker
            }
        }
        
        #[cfg(not(feature = "serial"))]
        {
            debug!("(mock) Would send TextPacket to 0x{:08x}: '{}'", dest, msg.content);
        }
        
        Ok(())
    }

    /// Re-send a previously attempted reliable text packet with the same id
    #[cfg(feature = "meshtastic-proto")]
    async fn resend_text_packet(&mut self, packet_id: u32, dest: u32, channel: u32, content: &str) -> Result<()> {
        use proto::{ToRadio, MeshPacket, Data, PortNum};
        use proto::to_radio::PayloadVariant as TRPayload;
        use proto::mesh_packet::PayloadVariant as MPPayload;
        use prost::Message;

        let from_node = self.our_node_id.ok_or_else(|| anyhow!("Cannot resend: our_node_id not yet known"))?;

        let data_msg = Data {
            portnum: PortNum::TextMessageApp as i32,
            payload: content.as_bytes().to_vec().into(),
            want_response: false,
            dest: 0,
            source: 0,
            request_id: 0,
            reply_id: 0,
            emoji: 0,
            bitfield: None,
            ..Default::default()
        };
        let pkt = MeshPacket {
            from: from_node,
            to: dest,
            channel,
            payload_variant: Some(MPPayload::Decoded(data_msg)),
            id: packet_id,
            rx_time: 0,
            rx_snr: 0.0,
            hop_limit: 3,
            want_ack: true,
            priority: 70,
            ..Default::default()
        };
        let toradio = ToRadio { payload_variant: Some(TRPayload::Packet(pkt)) };

        #[cfg(feature = "serial")]
        {
            // Enforce minimum gap between text packet transmissions
            let min_gap = std::cmp::max(self.tuning.min_send_gap_ms, 2000);
            self.enforce_min_send_gap(Duration::from_millis(min_gap)).await;
            let mut payload = Vec::with_capacity(128);
            toradio.encode(&mut payload)?;
            let mut hdr = [0u8; 4];
            hdr[0] = 0x94; hdr[1] = 0xC3;
            hdr[2] = ((payload.len() >> 8) & 0xFF) as u8;
            hdr[3] = (payload.len() & 0xFF) as u8;
            let mut port = self.port.lock().unwrap();
            port.write_all(&hdr)?; port.write_all(&payload)?; port.flush()?;
            debug!("Re-sent TextPacket DM: to=0x{:08x} channel={} id={} priority=70 ({} bytes)", dest, channel, packet_id, payload.len());
            self.last_text_send = Some(std::time::Instant::now());
        }
        Ok(())
    }

    /// Ensure at least `min_gap` has elapsed since the last text packet send
    async fn enforce_min_send_gap(&mut self, min_gap: Duration) {
        if let Some(last) = self.last_text_send {
            let elapsed = last.elapsed();
            if elapsed < min_gap {
                let wait = min_gap - elapsed;
                debug!(
                    "Gating: waiting {}ms to respect minimum {}ms between text sends",
                    wait.as_millis(),
                    min_gap.as_millis()
                );
                sleep(wait).await;
            }
        }
    }

    fn send_want_config(&mut self, request_id: u32) -> Result<()> {
        use proto::to_radio::PayloadVariant;
        let msg = proto::ToRadio { payload_variant: Some(PayloadVariant::WantConfigId(request_id)) };
        self.last_want_config_sent = Some(std::time::Instant::now());
        self.send_toradio(msg)
    }

    fn send_heartbeat(&mut self) -> Result<()> {
        use proto::{Heartbeat, ToRadio};
        use proto::to_radio::PayloadVariant;
        
        let nonce = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .unwrap().as_millis() & 0xffff) as u32;
        let hb = Heartbeat { nonce };
        let msg = ToRadio { payload_variant: Some(PayloadVariant::Heartbeat(hb)) };
        self.send_toradio(msg)
    }

    fn send_toradio(&mut self, msg: proto::ToRadio) -> Result<()> {
        use prost::Message;
        
        #[cfg(feature = "serial")]
        {
            let mut payload = Vec::with_capacity(256);
            msg.encode(&mut payload)?;
            if payload.len() > u16::MAX as usize { 
                return Err(anyhow!("payload too large")); 
            }
            let mut hdr = [0u8; 4];
            hdr[0] = 0x94;
            hdr[1] = 0xC3;
            hdr[2] = ((payload.len() >> 8) & 0xFF) as u8;
            hdr[3] = (payload.len() & 0xFF) as u8;
            
            let mut port = self.port.lock().unwrap();
            port.write_all(&hdr)?;
            port.write_all(&payload)?;
            port.flush()?;
            
            debug!("Sent ToRadio LEN frame ({} bytes payload)", payload.len());
        }
        Ok(())
    }

    // Removed ensure_want_config: WantConfigId is now sent only once at startup.

    #[allow(dead_code)]
    pub fn set_our_node_id(&mut self, node_id: u32) {
        self.our_node_id = Some(node_id);
        debug!("Writer: set our_node_id to {}", node_id);
    }
}

/// Convenience function to create and initialize the reader/writer system
#[cfg(feature = "meshtastic-proto")]
pub async fn create_reader_writer_system(
    port_name: &str,
    baud_rate: u32,
    tuning: WriterTuning,
) -> Result<(
    MeshtasticReader,
    MeshtasticWriter,
    mpsc::UnboundedReceiver<TextEvent>,
    mpsc::UnboundedSender<OutgoingMessage>,
    mpsc::UnboundedSender<ControlMessage>,
    mpsc::UnboundedSender<ControlMessage>,
)> {
    // Create shared serial port
    #[cfg(feature = "serial")]
    let shared_port = create_shared_serial_port(port_name, baud_rate).await?;
    
    // Create channels
    let (text_event_tx, text_event_rx) = mpsc::unbounded_channel::<TextEvent>();
    let (outgoing_tx, outgoing_rx) = mpsc::unbounded_channel::<OutgoingMessage>();
    let (reader_control_tx, reader_control_rx) = mpsc::unbounded_channel::<ControlMessage>();
    let (writer_control_tx, writer_control_rx) = mpsc::unbounded_channel::<ControlMessage>();

    // Create reader and writer with shared port
    #[cfg(feature = "serial")]
    let reader = MeshtasticReader::new(shared_port.clone(), text_event_tx, reader_control_rx, writer_control_tx.clone()).await?;
    #[cfg(feature = "serial")]
    let writer = MeshtasticWriter::new(shared_port, outgoing_rx, writer_control_rx, tuning.clone()).await?;

    #[cfg(not(feature = "serial"))]
    let (reader, writer) = {
        warn!("Serial not available, using mock reader/writer");
        (
            MeshtasticReader::new_mock(text_event_tx, reader_control_rx, writer_control_tx.clone()).await?,
            MeshtasticWriter::new_mock(outgoing_rx, writer_control_rx, tuning).await?,
        )
    };

    Ok((reader, writer, text_event_rx, outgoing_tx, reader_control_tx, writer_control_tx))
}
