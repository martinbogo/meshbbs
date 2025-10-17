//! # API Module
//!
//! REST API endpoints for the admin dashboard.

pub mod auth;
pub mod npcs;
pub mod users;
pub mod topics;

pub use auth::*;
pub use npcs::*;
pub use users::*;
pub use topics::*;
