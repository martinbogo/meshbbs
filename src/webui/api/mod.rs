//! # API Module
//!
//! REST API endpoints for the admin dashboard.

pub mod auth;
pub mod npcs;
pub mod users;

pub use auth::*;
pub use npcs::*;
pub use users::*;
