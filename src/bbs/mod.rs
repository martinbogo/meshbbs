//! BBS (Bulletin Board System) module
//! 
//! This module contains the core BBS functionality including:
//! - Server management
//! - User session handling
//! - Command processing
//! - Message area management

pub mod server;
pub mod session;
pub mod commands;
pub mod public;
pub mod roles;

pub use server::BbsServer;

// Re-export internal types only when feature enabled to reduce unused import warnings in binaries
#[allow(unused_imports)]
#[cfg(feature = "api-reexports")]
pub use session::Session;
#[cfg(feature = "api-reexports")]
pub use commands::CommandProcessor;
#[cfg(feature = "api-reexports")]
pub use public::{PublicState, PublicCommandParser};