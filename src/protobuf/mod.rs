//! Generated protobuf modules
//!
//! This module conditionally includes code generated from Meshtastic protobuf definitions
//! when the `meshtastic-proto` feature is enabled.

#[cfg(feature = "meshtastic-proto")]
pub mod meshtastic_generated {
    //! Generated Meshtastic protobuf types.
    //! build.rs compiles all .proto files; prost emits one file per package.
    //! We attempt to include known Meshtastic top-level modules.
    
    // All Meshtastic protos share the same `meshtastic` package, so prost
    // emits a single `meshtastic.rs` file containing all definitions.
    include!(concat!(env!("OUT_DIR"), "/meshtastic.rs"));
}

#[cfg(not(feature = "meshtastic-proto"))]
pub mod meshtastic_generated {
    //! Stub implementations when protobufs are not compiled.
    #[derive(Debug, Clone)]
    pub struct Placeholder {
        pub note: String,
    }
}