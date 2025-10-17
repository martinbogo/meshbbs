//! # API Module
//!
//! REST API endpoints for the admin dashboard.

pub mod auth;
pub mod npcs;
pub mod users;
pub mod topics;
pub mod stats;
pub mod schema;

pub use auth::*;
pub use npcs::*;
pub use users::*;
pub use topics::*;
pub use stats::*;
pub use schema::*;
