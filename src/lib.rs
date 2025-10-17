//! # Meshbbs - Bulletin Board System for Meshtastic Networks
//!
//! Meshbbs is a modern Bulletin Board System (BBS) designed specifically for Meshtastic mesh networks.
//! It provides traditional BBS functionality over long-range, low-power radio networks using LoRa technology.
//!
//! ## Features
//!
//! - **Compact Command UI**: Single-letter navigation optimized for ≤230-byte frames, including contextual prompts and inline help.
//! - **Meshtastic Integration**: Direct communication with Meshtastic devices over USB/UART serial links, with optional protobuf packet decoding.
//! - **Message Boards**: Topic and subtopic hierarchy with paged threads, filters, and UTF-8 safe rendering.
//! - **TinyMUSH Game Engine**: Complete MUD/MUSH engine with 20+ rooms, NPCs, companions, quests, crafting, and achievements.
//! - **Data-Driven Content**: All game content loaded from JSON files in `data/seeds/` with admin commands for runtime editing.
//! - **Optional Mini-Games**: Public-channel Slot, Magic 8-Ball, and Fortune commands, plus the TinyHack DM roguelike when enabled.
//! - **User Management**: Role-based access control (User, Moderator, Sysop) with granular topic permissions.
//! - **Security**: Argon2id password hashing, input sanitization, and UTF-8 safe truncation helpers.
//! - **Daemon Mode**: Production-ready background service support (Linux/macOS) with graceful shutdown and TTY-aware logging.
//! - **Welcome System**: Automated greeting system for new mesh nodes with personalized name suggestions.
//! - **Async Design**: Built with Tokio for high performance on constrained hardware.
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
//! - [`tmush`] - TinyMUSH game engine with rooms, NPCs, quests, and crafting
//! - [`validation`] - Input validation and sanitization utilities
//! - [`logutil`] - Logging utilities with TTY detection
//! - [`metrics`] - Performance metrics and monitoring
//! - [`protobuf`] - Protocol buffer definitions for Meshtastic integration
//!
//! ## Architecture
//!
//! Meshbbs uses a modular architecture with clear separation of concerns:
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
pub mod config;
pub mod logutil;
pub mod meshtastic;
pub mod metrics; // new metrics module (Phase 3 scaffold)
pub mod protobuf; // always declare; internal stubs handle feature gating
pub mod storage;
pub mod tmush;
pub mod validation;

#[cfg(feature = "webui")]
pub mod webui;
