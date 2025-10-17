//! # Web UI Module
//!
//! Admin web dashboard for MeshBBS management.
//!
//! This module provides a web-based interface for:
//! - Content management (NPCs, Achievements, Rooms, Objects, Quests, Companions)
//! - Player administration and moderation
//! - System monitoring and metrics
//! - Configuration editing
//! - JSON seed file management
//!
//! ## Security
//!
//! - Disabled by default (must be explicitly enabled in config)
//! - Uses same admin password as BBS (sysop level required)
//! - TLS/HTTPS by default (self-signed cert auto-generated)
//! - Rate limiting on login and API requests
//! - Session token rotation to prevent replay attacks
//! - Mandatory audit logging of all admin actions
//!
//! ## Architecture
//!
//! ```text
//! Web UI (Vanilla JS) ←→ Axum REST API ←→ Sled Database ←→ BBS Engine
//!                               ↓
//!                          WebSocket (Metrics)
//! ```

#[cfg(feature = "webui")]
pub mod server;

#[cfg(feature = "webui")]
pub mod auth;

#[cfg(feature = "webui")]
pub mod tls;

#[cfg(feature = "webui")]
pub mod audit;

#[cfg(feature = "webui")]
pub mod schema;

#[cfg(feature = "webui")]
pub mod api;

#[cfg(feature = "webui")]
pub use server::start_webui_server;
