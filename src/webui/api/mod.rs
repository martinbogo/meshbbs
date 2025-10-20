//! # API Module
//!
//! REST API endpoints for the admin dashboard.

pub mod activity;
pub mod apps;
pub mod audit;
pub mod auth;
pub mod content;
pub mod fortune;
pub mod messages;
pub mod npcs;
pub mod schema;
pub mod stats;
pub mod tinymush;
pub mod topics;
pub mod users;

pub use activity::*;
pub use apps::*;
pub use audit::*;
pub use auth::*;
pub use content::*;
pub use fortune::*;
pub use messages::*;
pub use npcs::*;
pub use schema::*;
pub use stats::*;
pub use tinymush::*;
pub use topics::*;
pub use users::*;
