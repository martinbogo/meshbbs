use anyhow::Result;
use log::{info, warn, debug};
use tokio::time::sleep; // for short polling delay
use tokio::sync::mpsc;
use std::collections::HashMap;

use crate::config::Config;
use crate::meshtastic::{MeshtasticDevice, TextEvent};
use crate::storage::Storage;
use super::session::Session;
use super::public::{PublicState, PublicCommandParser, PublicCommand};

/// Main BBS server that coordinates all operations
pub struct BbsServer {
    config: Config,
    storage: Storage,
    device: Option<MeshtasticDevice>,
    sessions: HashMap<String, Session>,
    message_tx: Option<mpsc::UnboundedSender<String>>,
    public_state: PublicState,
    public_parser: PublicCommandParser,
}

impl BbsServer {
    /// Create a new BBS server instance
    pub async fn new(config: Config) -> Result<Self> {
        let storage = Storage::new(&config.storage.data_dir).await?;
        
        Ok(BbsServer {
            config,
            storage,
            device: None,
            sessions: HashMap::new(),
            message_tx: None,
            public_state: PublicState::new(
                std::time::Duration::from_secs(20),
                std::time::Duration::from_secs(300)
            ),
            public_parser: PublicCommandParser::new(),
        })
    }

    /// Connect to a Meshtastic device
    pub async fn connect_device(&mut self, port: &str) -> Result<()> {
        info!("Connecting to Meshtastic device on {}", port);
        
        let device = MeshtasticDevice::new(port, self.config.meshtastic.baud_rate).await?;
        self.device = Some(device);
        
        Ok(())
    }

    /// Start the BBS server main loop
    pub async fn run(&mut self) -> Result<()> {
        info!("BBS '{}' started by {}", self.config.bbs.name, self.config.bbs.sysop);
        
        let (tx, mut rx) = mpsc::unbounded_channel();
        self.message_tx = Some(tx);
        
        // Main message processing loop
        loop {
            // First drain any text events outside the select to avoid borrowing self across await points in same branch.
            // Drain text events first collecting them to avoid holding device borrow across awaits
            let mut drained_events = Vec::new();
            if let Some(dev) = &mut self.device {
                while let Some(ev) = dev.next_text_event() { drained_events.push(ev); }
            }
            for ev in drained_events { if let Err(e) = self.route_text_event(ev).await { warn!("route_text_event error: {e:?}"); } }

            tokio::select! {
                // Handle incoming messages from Meshtastic
                msg = self.receive_message() => {
                    if let Ok(Some(summary)) = msg {
                        debug!("Legacy summary: {}", summary);
                    }
                }
                // Handle internal messages
                msg = rx.recv() => {
                    if let Some(internal_msg) = msg {
                        debug!("Processing internal message: {}", internal_msg);
                    }
                }
                // Handle graceful shutdown
                _ = tokio::signal::ctrl_c() => {
                    info!("Received shutdown signal");
                    break;
                }
                // Idle delay
                _ = sleep(std::time::Duration::from_millis(25)) => {}
            }
        }
        
        self.shutdown().await?;
        Ok(())
    }

    /// Receive a message from the Meshtastic device
    async fn receive_message(&mut self) -> Result<Option<String>> {
        if let Some(ref mut device) = self.device {
            device.receive_message().await
        } else {
            // No device connected, wait a bit
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            Ok(None)
        }
    }

    /// Process an incoming message from the mesh network
    async fn process_incoming_message(&mut self, message: String) -> Result<()> {
        debug!("Processing incoming message: {}", message);
        
        // Parse message format: "FROM_NODE:MESSAGE_CONTENT"
        if let Some((from_node, content)) = message.split_once(':') {
            let session_id = from_node.to_string();
            
            // Get or create session for this node
            if !self.sessions.contains_key(&session_id) {
                let session = Session::new(session_id.clone(), from_node.to_string());
                self.sessions.insert(session_id.clone(), session);
                info!("New session created for node: {}", from_node);
            }
            
            if let Some(session) = self.sessions.get_mut(&session_id) {
                let response = session.process_command(content, &mut self.storage).await?;
                
                if !response.is_empty() {
                    self.send_message(&session_id, &response).await?;
                }
            }
        }
        
        Ok(())
    }

    async fn route_text_event(&mut self, ev: TextEvent) -> Result<()> {
        // Source node id string form
        let node_key = ev.source.to_string();
        if ev.is_direct {
            // Direct (private) path: ensure session exists, finalize pending login if any
            if !self.sessions.contains_key(&node_key) {
                let mut session = Session::new(node_key.clone(), node_key.clone());
                // If there was a pending login, apply username now
                if let Some(username) = self.public_state.take_pending(&node_key) {
                    session.login(username, 1).await?;
                } else {
                    // Optionally send a small greeting expecting LOGIN if not logged in
                    let _ = self.send_message(&node_key, "Welcome to MeshBBS (private). Type LOGIN <name> or HELP").await;
                }
                self.sessions.insert(node_key.clone(), session);
            }
            if let Some(session) = self.sessions.get_mut(&node_key) {
                let content = ev.content.trim();
                // Allow LOGIN inside DM (direct login path)
                if content.to_uppercase().starts_with("LOGIN ") && !session.is_logged_in() {
                    let username = content[6..].trim();
                    if !username.is_empty() { session.login(username.to_string(), 1).await?; }
                } else {
                    let response = session.process_command(content, &mut self.storage).await?;
                    if !response.is_empty() { self.send_message(&node_key, &response).await?; }
                }
            }
        } else {
            // Public channel event: parse lightweight commands
            self.public_state.prune_expired();
            let cmd = self.public_parser.parse(&ev.content);
            match cmd {
                PublicCommand::Help => {
                    if self.public_state.should_reply(&node_key) {
                        self.send_message(&node_key, "MeshBBS Public Help: LOGIN <name> to begin, then send me a direct message (DM) to continue.").await?;
                    }
                }
                PublicCommand::Login(username) => {
                    if self.public_state.should_reply(&node_key) {
                        self.public_state.set_pending(&node_key, username.clone());
                        let reply = format!("Login pending for '{}'. Open a direct message to this node and say HI or LOGIN <name> again to complete.", username);
                        self.send_message(&node_key, &reply).await?;
                    }
                }
                PublicCommand::Invalid(reason) => {
                    if self.public_state.should_reply(&node_key) {
                        let reply = format!("Invalid: {}", reason);
                        self.send_message(&node_key, &reply).await?;
                    }
                }
                PublicCommand::Unknown => {
                    // Ignore to reduce noise
                }
            }
        }
        Ok(())
    }

    /// Send a message to a specific node
    async fn send_message(&mut self, to_node: &str, message: &str) -> Result<()> {
        if let Some(ref mut device) = self.device {
            device.send_message(to_node, message).await?;
            debug!("Sent message to {}: {}", to_node, message);
        } else {
            warn!("No device connected, cannot send message");
        }
        Ok(())
    }

    /// Show BBS status and statistics
    pub async fn show_status(&self) -> Result<()> {
        println!("=== MeshBBS Status ===");
        println!("BBS Name: {}", self.config.bbs.name);
        println!("Sysop: {}", self.config.bbs.sysop);
        println!("Location: {}", self.config.bbs.location);
        println!("Active Sessions: {}", self.sessions.len());
        
        if self.device.is_some() {
            println!("Meshtastic Device: Connected");
        } else {
            println!("Meshtastic Device: Not connected");
        }
        
        // Storage statistics
        let stats = self.storage.get_statistics().await?;
        println!("Total Messages: {}", stats.total_messages);
        println!("Total Users: {}", stats.total_users);
        
        Ok(())
    }

    /// Gracefully shutdown the BBS server
    async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down BBS server...");
        
        // Close all sessions
        for (session_id, session) in &mut self.sessions {
            info!("Closing session: {}", session_id);
            session.logout().await?;
        }
        self.sessions.clear();
        
        // Disconnect device
        if let Some(device) = &mut self.device {
            device.disconnect().await?;
        }
        
        info!("BBS server shutdown complete");
        Ok(())
    }
}