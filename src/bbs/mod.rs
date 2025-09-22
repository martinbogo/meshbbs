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

pub use server::BbsServer;
pub use session::Session;
pub use commands::CommandProcessor;
pub use public::{PublicState, PublicCommandParser};