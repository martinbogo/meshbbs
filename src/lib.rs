//! Library entry for meshbbs components used by binary and tests.
// Re-export modules so that feature-gated protobuf module path exists.

pub mod bbs;
pub mod meshtastic;
pub mod config;
pub mod storage;
pub mod validation;
pub mod protobuf; // always declare; internal stubs handle feature gating
