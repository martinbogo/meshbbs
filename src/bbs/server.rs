use anyhow::Result;
use log::{info, warn, error, debug};
use tokio::sync::mpsc;
use std::collections::HashMap;
use uuid::Uuid;

use crate::config::Config;
use crate::meshtastic::MeshtasticDevice;
use crate::storage::Storage;
use super::session::Session;

/// Main BBS server that coordinates all operations
pub struct BbsServer {
    config: Config,
    storage: Storage,
    device: Option<MeshtasticDevice>,
    sessions: HashMap<String, Session>,
    message_tx: Option<mpsc::UnboundedSender<String>>,
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
            tokio::select! {
                // Handle incoming messages from Meshtastic
                msg = self.receive_message() => {
                    if let Ok(Some(message)) = msg {
                        self.process_incoming_message(message).await?;
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