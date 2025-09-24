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
use std::collections::VecDeque;

#[cfg(feature = "meshtastic-proto")]
use bytes::BytesMut;
#[cfg(feature = "meshtastic-proto")]
use prost::Message;
#[cfg(feature = "meshtastic-proto")]
use crate::protobuf::meshtastic_generated as proto;

// Provide hex_snippet early so it is in scope for receive_message (always available)
fn hex_snippet(data: &[u8], max: usize) -> String {
    use std::cmp::min;
    data.iter().take(min(max, data.len())).map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join("")
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
                        debug!("RAW {} bytes: {}", bytes_read, hex_snippet(raw_slice, 64));
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
                                debug!("SLIP frame {} bytes", frame.len());
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
    let bytes = BytesMut::from(data);
        if let Ok(msg) = FromRadio::decode(&mut bytes.freeze()) {
            match msg.payload_variant.as_ref()? {
                FRPayload::ConfigCompleteId(id) => { return Some(format!("CONFIG_COMPLETE:{id}")); }
                FRPayload::Packet(pkt) => {
                    if let Some(MPPayload::Decoded(data_msg)) = &pkt.payload_variant {
                        let port = PortNum::try_from(data_msg.portnum).unwrap_or(PortNum::UnknownApp);
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
                            _ => return Some(format!("PKT:{}:port={:?}:len={}", pkt.from, port, data_msg.payload.len())),
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
    pub fn next_text_event(&mut self) -> Option<TextEvent> { self.text_events.pop_front() }
    /// Build and send a text message MeshPacket via ToRadio (feature gated).
    /// to: Some(node_id) for direct, None for broadcast
    /// channel: channel index (0 primary)
    #[cfg(feature = "meshtastic-proto")]
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
            let display_text = if text.len() > 80 { 
                format!("{}...", &text[..77])
            } else { 
                text.replace('\n', "\\n").replace('\r', "\\r")
            };
            let msg_type = if is_dm { "DM (reliable)" } else { "broadcast" };
            info!("Sent TextPacket ({}): from=0x{:08x} to=0x{:08x} channel={} id={} want_ack={} priority={} ({} bytes payload) text='{}'", 
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
