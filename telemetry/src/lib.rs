// Telemetry module: OpenTelemetry + Prometheus integration

pub mod metrics;
pub mod tracing as tracing_module;

pub use metrics::*;
pub use tracing_module::*;

use anyhow::Result;

/// Initialize telemetry for AEGIS
pub async fn init_telemetry(service_name: &str) -> Result<()> {
    // Initialize tracing
    init_tracing(service_name)?;

    // Initialize metrics
    init_metrics()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_telemetry_init() {
        let result = init_telemetry("aegis-test").await;
        assert!(result.is_ok());
    }
}
