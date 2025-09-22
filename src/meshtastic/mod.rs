//! Meshtastic device communication module
//! 
//! This module handles communication with Meshtastic devices via serial port.

use anyhow::{Result, anyhow};
use log::{info, debug, warn, error};
use tokio::time::{sleep, Duration};

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
pub mod framer; // expose framer submodule
#[cfg(feature = "meshtastic-proto")]
pub mod slip;

#[cfg(feature = "serial")]
use serialport::SerialPort;

/// Represents a connection to a Meshtastic device
pub struct MeshtasticDevice {
    port_name: String,
    baud_rate: u32,
    #[cfg(feature = "serial")]
    port: Option<Box<dyn SerialPort>>,
    #[cfg(feature = "meshtastic-proto")]
    framer: Option<crate::meshtastic::framer::ProtoFramer>, // retained for potential BLE use
    #[cfg(feature = "meshtastic-proto")]
    slip: Option<crate::meshtastic::slip::SlipDecoder>,
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
    binary_frames_seen: bool,
    #[cfg(feature = "meshtastic-proto")]
    last_want_config_sent: Option<std::time::Instant>,
    #[cfg(feature = "meshtastic-proto")]
    rx_buf: Vec<u8>, // accumulation buffer for length-prefixed frames (0x94 0xC3 hdr)
}

impl MeshtasticDevice {
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
                    framer: Some(crate::meshtastic::framer::ProtoFramer::new()),
            #[cfg(feature = "meshtastic-proto")]
            slip: Some(crate::meshtastic::slip::SlipDecoder::new()),
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
            binary_frames_seen: false,
            #[cfg(feature = "meshtastic-proto")]
            last_want_config_sent: None,
            #[cfg(feature = "meshtastic-proto")]
            rx_buf: Vec::new(),
                })
        }
        
        #[cfg(not(feature = "serial"))]
        {
            warn!("Serial support not compiled in, using mock device");
            Ok(MeshtasticDevice {
                port_name: port_name.to_string(),
                baud_rate,
                #[cfg(feature = "meshtastic-proto")]
                framer: Some(crate::meshtastic::framer::ProtoFramer::new()),
                #[cfg(feature = "meshtastic-proto")]
                slip: Some(crate::meshtastic::slip::SlipDecoder::new()),
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
                binary_frames_seen: false,
                #[cfg(feature = "meshtastic-proto")]
                last_want_config_sent: None,
                #[cfg(feature = "meshtastic-proto")]
                rx_buf: Vec::new(),
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
                        #[cfg(feature = "meshtastic-proto")]
                        {
                            if let Some(ref mut slip) = self.slip {
                                let frames = slip.push(raw_slice);
                                if !frames.is_empty() { self.binary_frames_seen = true; }
                                for frame in frames {
                                    debug!("SLIP frame {} bytes", frame.len());
                                    if let Some(summary) = self.try_parse_protobuf_frame(&frame) {
                                        // update internal state
                                        self.update_state_from_summary(&summary);
                                        return Ok(Some(summary));
                                    }
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
        let formatted_message = format!("TO:{} MSG:{}\n", to_node, message);
        
        #[cfg(feature = "serial")]
        {
            if let Some(ref mut port) = self.port {
                port.write_all(formatted_message.as_bytes())
                    .map_err(|e| anyhow!("Failed to write to serial port: {}", e))?;
                port.flush()
                    .map_err(|e| anyhow!("Failed to flush serial port: {}", e))?;
                
                debug!("Sent to {}: {}", to_node, message);
            }
        }
        
        #[cfg(not(feature = "serial"))]
        {
            debug!("Mock send to {}: {}", to_node, message);
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
    fn try_parse_protobuf_frame(&self, data: &[u8]) -> Option<String> {
        use proto::{FromRadio, PortNum};
        use proto::from_radio::PayloadVariant as FRPayload;
        use proto::mesh_packet::PayloadVariant as MPPayload;
        let mut bytes = BytesMut::from(data);
        if let Ok(msg) = FromRadio::decode(&mut bytes.freeze()) {
            match msg.payload_variant.as_ref()? {
                FRPayload::ConfigCompleteId(id) => { return Some(format!("CONFIG_COMPLETE:{id}")); }
                FRPayload::Packet(pkt) => {
                    if let Some(MPPayload::Decoded(data_msg)) = &pkt.payload_variant {
                        let port = PortNum::from_i32(data_msg.portnum).unwrap_or(PortNum::UnknownApp);
                        match port {
                            PortNum::TextMessageApp => {
                                if let Ok(text) = std::str::from_utf8(&data_msg.payload) { return Some(format!("TEXT:{}:{}", pkt.from, text)); }
                            }
                            PortNum::TextMessageCompressedApp => {
                                let hex = data_msg.payload.iter().map(|b| format!("{:02x}", b)).collect::<String>();
                                return Some(format!("CTEXT:{}:{}", pkt.from, hex)); }
                            _ => return Some(format!("PKT:{}:port={:?}:len={}", pkt.from, port, data_msg.payload.len())),
                        }
                    }
                }
                FRPayload::MyInfo(info) => return Some(format!("MYINFO:{}", info.my_node_num)),
                FRPayload::NodeInfo(n) => return Some(format!("NODE:{}:{}", n.num, n.user.as_ref().map(|u| u.long_name.clone()).unwrap_or_default())),
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
                let mut ni = proto::NodeInfo { num: id, ..Default::default() };
                if parts.len() >=3 { let nm = parts[2].to_string(); if !nm.is_empty() { ni.user = Some(proto::User{ long_name: nm, ..Default::default()}); }}
                self.nodes.insert(id, ni);
            }}
        }
        else if summary == "CONFIG" { self.have_radio_config = true; }
        else if summary == "MODULE_CONFIG" { self.have_module_config = true; }
        else if summary.starts_with("CONFIG_COMPLETE:") {
            if let Some(id_str) = summary.split(':').nth(1) { if let Ok(id_val) = id_str.parse::<u32>() { if self.config_request_id == Some(id_val) { self.config_complete = true; }}}
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

    /// Get device information (placeholder)
    pub async fn get_device_info(&mut self) -> Result<DeviceInfo> {
        Ok(DeviceInfo { node_id: "SIMULATED".into(), hardware: "T-Beam".into(), firmware_version: "2.2.0".into(), channel: 0 })
    }
}

// (Public crate visibility no longer required; kept private above.)

#[cfg(feature = "meshtastic-proto")]
impl MeshtasticDevice {
    /// Send a ToRadio.WantConfigId request to trigger the node database/config push.
    pub fn send_want_config(&mut self, request_id: u32) -> Result<()> {
        use prost::Message;
        use proto::to_radio::PayloadVariant;
        let msg = proto::ToRadio { payload_variant: Some(PayloadVariant::WantConfigId(request_id)) };
        #[cfg(feature = "meshtastic-proto")]
        { self.last_want_config_sent = Some(std::time::Instant::now()); }
        self.send_toradio(msg)
    }

    /// Send a heartbeat frame (optional, can help keep link active)
    pub fn send_heartbeat(&mut self) -> Result<()> {
        use prost::Message;
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

/// Device information structure
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub node_id: String,
    pub hardware: String,
    pub firmware_version: String,
    pub channel: u8,
}