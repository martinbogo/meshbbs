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