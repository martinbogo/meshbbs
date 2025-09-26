//! # BBS (Bulletin Board System) Core Module
//! 
//! This module implements the core BBS functionality for Meshbbs, providing a complete
//! bulletin board system experience over Meshtastic mesh networks.
//!
//! ## Components
//!
//! - [`server`] - Main BBS server implementation and lifecycle management
//! - [`session`] - User session handling and state management
//! - [`commands`] - Command processing and execution engine
//! - [`public`] - Public channel command parsing and discovery protocols
//! - [`roles`] - User role definitions and permission management
//!
//! ## Architecture
//!
//! The BBS module follows a layered architecture:
//!
//! ```text
//! ┌─────────────────┐
//! │  BbsServer      │ ← Main server coordinating all components
//! └─────────────────┘
//!          │
//! ┌─────────────────┐
//! │  Session        │ ← Individual user session management
//! │  Management     │
//! └─────────────────┘
//!          │
//! ┌─────────────────┐
//! │  Command        │ ← Command parsing and execution
//! │  Processing     │
//! └─────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,no_run
//! use meshbbs::bbs::BbsServer;
//! use meshbbs::config::Config;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = Config::load("config.toml").await?;
//!     let mut server = BbsServer::new(config).await?;
//!     server.run().await
//! }
//! ```
//!
//! ## Two-Phase Communication Model
//!
//! Meshbbs implements a unique two-phase communication model optimized for mesh networks:
//!
//! 1. **Public Discovery**: Lightweight commands on shared channel (`^HELP`, `^LOGIN`)
//! 2. **Private Sessions**: Full BBS interaction via direct messages
//!
//! This design minimizes mesh network traffic while providing rich BBS functionality.
//!
//! ## Session Lifecycle
//!
//! 1. User sends `^LOGIN username` on public channel
//! 2. BBS registers pending login for the user's node ID
//! 3. User opens direct message to BBS node
//! 4. BBS creates authenticated session
//! 5. User interacts with full BBS command set privately
//! 6. Session ends with `LOGOUT` or timeout

pub mod server;
pub mod session;
pub mod commands;
pub mod public;
pub mod roles;
pub mod dispatch;
pub mod slotmachine;

pub use server::BbsServer;

// Optional re-exports for downstream crates when feature enabled
#[cfg(feature = "api-reexports")]
#[allow(unused_imports)]
pub use session::Session;
#[cfg(feature = "api-reexports")]
#[allow(unused_imports)]
pub use commands::CommandProcessor;
#[cfg(feature = "api-reexports")]
#[allow(unused_imports)]
pub use public::{PublicState, PublicCommandParser};