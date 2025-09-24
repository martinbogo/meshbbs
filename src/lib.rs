//! # MeshBBS - Bulletin Board System for Meshtastic Networks
//!
//! MeshBBS is a modern Bulletin Board System (BBS) designed specifically for Meshtastic mesh networks.
//! It provides traditional BBS functionality over long-range, low-power radio networks using LoRa technology.
//!
//! ## Features
//!
//! - **Meshtastic Integration**: Direct communication with Meshtastic devices via serial or Bluetooth
//! - **Message Boards**: Traditional BBS-style message topics and forums
//! - **User Management**: Role-based access control (User, Moderator, Sysop)
//! - **Security**: Argon2id password hashing with configurable parameters
//! - **Async Design**: Built with Tokio for high performance
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use meshbbs::config::Config;
//! use meshbbs::bbs::BbsServer;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Load configuration
//!     let config = Config::load("config.toml").await?;
//!     
//!     // Create and start BBS server
//!     let mut server = BbsServer::new(config).await?;
//!     server.run().await?;
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Module Organization
//!
//! - [`bbs`] - Core BBS functionality including server, sessions, and commands
//! - [`meshtastic`] - Meshtastic device communication and protocol handling
//! - [`storage`] - Message and user data persistence layer
//! - [`config`] - Configuration management and validation
//! - [`validation`] - Input validation and sanitization utilities
//! - [`protobuf`] - Protocol buffer definitions for Meshtastic integration
//!
//! ## Architecture
//!
//! MeshBBS uses a modular architecture with clear separation of concerns:
//!
//! ```text
//! ┌─────────────────┐
//! │   BBS Server    │ ← Core application logic
//! └─────────────────┘
//!          │
//! ┌─────────────────┐
//! │   Meshtastic    │ ← Device communication
//! │   Interface     │
//! └─────────────────┘
//!          │
//! ┌─────────────────┐
//! │   Storage       │ ← Data persistence
//! │   Layer         │
//! └─────────────────┘
//! ```
//!
//! ## Examples
//!
//! See the `examples/` directory for complete usage examples and the main binary
//! implementation in `src/main.rs` for a full application example.

// Re-export modules so that feature-gated protobuf module path exists.

pub mod bbs;
pub mod meshtastic;
pub mod config;
pub mod storage;
pub mod validation;
pub mod protobuf; // always declare; internal stubs handle feature gating
pub mod metrics; // new metrics module (Phase 3 scaffold)
