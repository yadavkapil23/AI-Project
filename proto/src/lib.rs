// Proto module: gRPC message and service definitions for AEGIS

// Auto-generated from protobuf
pub mod aegis {
    pub mod inference {
        include!(concat!(env!("OUT_DIR"), "/aegis.inference.rs"));
    }
    pub mod audit {
        include!(concat!(env!("OUT_DIR"), "/aegis.audit.rs"));
    }
    pub mod scheduling {
        // tonic-build emits both messages and service stubs here
        include!(concat!(env!("OUT_DIR"), "/aegis.scheduling.rs"));
    }
}

pub use aegis::inference::*;
pub use aegis::audit::*;

// Re-export scheduling under a clear namespace so callers can write
// `aegis_proto::scheduling::scheduling_service_client::SchedulingServiceClient`.
pub use aegis::scheduling;
