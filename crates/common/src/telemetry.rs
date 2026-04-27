use tracing_subscriber::EnvFilter;

use crate::{
    config::{LogFormat, TelemetryConfig},
    error::ConfigError,
};

#[derive(Debug)]
pub struct TelemetryGuard;

pub fn init_telemetry(config: &TelemetryConfig) -> Result<TelemetryGuard, ConfigError> {
    match config.log_format {
        LogFormat::Json => tracing_subscriber::fmt()
            .json()
            .with_env_filter(env_filter())
            .try_init()
            .map_err(|error| ConfigError::Telemetry(error.to_string()))?,
        LogFormat::Pretty => tracing_subscriber::fmt()
            .pretty()
            .with_env_filter(env_filter())
            .try_init()
            .map_err(|error| ConfigError::Telemetry(error.to_string()))?,
    }

    Ok(TelemetryGuard)
}

fn env_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
}
