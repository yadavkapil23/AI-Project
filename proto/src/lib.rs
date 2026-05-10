// Proto module: gRPC message definitions for AEGIS

// Auto-generated from protobuf
pub mod aegis {
    pub mod inference {
        include!(concat!(env!("OUT_DIR"), "/aegis.inference.rs"));
    }
    pub mod audit {
        include!(concat!(env!("OUT_DIR"), "/aegis.audit.rs"));
    }
}

pub use aegis::inference::*;
pub use aegis::audit::*;
