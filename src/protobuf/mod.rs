//! Generated protobuf modules
//!
//! This module conditionally includes code generated from Meshtastic protobuf definitions
//! when the `meshtastic-proto` feature is enabled.

#[cfg(feature = "meshtastic-proto")]
pub mod meshtastic_generated {
    //! Generated Meshtastic protobuf types.
    //! build.rs compiles all .proto files; prost emits one file per package (meshtastic.rs).
    //! We wrap the include in a submodule with broad allow() attributes to suppress
    //! hundreds of dead_code/unused warnings for portions of the API we don't (yet) use.
    #[allow(dead_code, unused_imports, unused_variables, unused_mut, unused_macros)]
    #[allow(clippy::all)]
    mod inner {
        include!(concat!(env!("OUT_DIR"), "/meshtastic.rs"));
    }
    pub use inner::*;
}

#[cfg(not(feature = "meshtastic-proto"))]
pub mod meshtastic_generated {
    //! Stub implementations when protobufs are not compiled.
    #[derive(Debug, Clone)]
    pub struct Placeholder {
        pub note: String,
    }
}