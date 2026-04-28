use std::{
    collections::VecDeque,
    marker::PhantomData,
    sync::{
        atomic::{AtomicU64, Ordering},
        LazyLock, Mutex,
    },
};

use serde::Serialize;
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

pub type Lazy<T> = LazyLock<T>;

#[derive(Debug, Clone)]
pub struct KeyValue {
    pub key: &'static str,
    pub value: &'static str,
}

#[derive(Debug)]
pub struct Counter<T> {
    value: AtomicU64,
    samples: AtomicU64,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Counter<T> {
    pub const fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
            samples: AtomicU64::new(0),
            _marker: PhantomData,
        }
    }
}

impl<T> Default for Counter<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl Counter<u64> {
    pub fn add(&self, value: u64, _attributes: &[KeyValue]) {
        self.value.fetch_add(value, Ordering::Relaxed);
        self.samples.fetch_add(1, Ordering::Relaxed);
    }

    pub fn total(&self) -> Option<u64> {
        if self.samples.load(Ordering::Relaxed) == 0 {
            None
        } else {
            Some(self.value.load(Ordering::Relaxed))
        }
    }
}

const HISTOGRAM_CAPACITY: usize = 2048;

#[derive(Debug)]
pub struct Histogram<T> {
    samples: Mutex<VecDeque<f64>>,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Histogram<T> {
    pub const fn new() -> Self {
        Self {
            samples: Mutex::new(VecDeque::new()),
            _marker: PhantomData,
        }
    }
}

impl<T> Default for Histogram<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl Histogram<f64> {
    pub fn record(&self, value: f64, _attributes: &[KeyValue]) {
        let Ok(mut samples) = self.samples.lock() else {
            return;
        };
        if samples.len() == HISTOGRAM_CAPACITY {
            samples.pop_front();
        }
        samples.push_back(value);
    }

    pub fn percentile(&self, quantile: f64) -> Option<f64> {
        let samples = self.samples.lock().ok()?;
        if samples.is_empty() {
            return None;
        }
        let mut buf: Vec<f64> = samples.iter().copied().collect();
        drop(samples);
        buf.sort_by(|a, b| a.total_cmp(b));
        let last_index = buf.len().saturating_sub(1);
        let position = (quantile.clamp(0.0, 1.0) * last_index as f64).round() as usize;
        buf.get(position).copied()
    }

    pub fn mean(&self) -> Option<f64> {
        let samples = self.samples.lock().ok()?;
        if samples.is_empty() {
            return None;
        }
        let sum: f64 = samples.iter().sum();
        let count = samples.len() as f64;
        Some(sum / count)
    }
}

pub static INGEST_EVENTS: Lazy<Counter<u64>> = Lazy::new(Counter::<u64>::new);
pub static SLOW_PATH_PROCESSED: Lazy<Counter<u64>> = Lazy::new(Counter::<u64>::new);
pub static SLOW_PATH_FAILED: Lazy<Counter<u64>> = Lazy::new(Counter::<u64>::new);
pub static RETRIEVAL_REQUESTS: Lazy<Counter<u64>> = Lazy::new(Counter::<u64>::new);
pub static EMBEDDING_LATENCY: Lazy<Histogram<f64>> = Lazy::new(Histogram::<f64>::new);
pub static LLM_LATENCY: Lazy<Histogram<f64>> = Lazy::new(Histogram::<f64>::new);
pub static TOKEN_PACK_BUDGET_USED: Lazy<Histogram<f64>> = Lazy::new(Histogram::<f64>::new);

#[derive(Debug, Clone, Serialize)]
pub struct MetricsValues {
    pub ingest_events_total: Option<u64>,
    pub slow_path_jobs_processed: Option<u64>,
    pub slow_path_jobs_failed: Option<u64>,
    pub retrieval_requests_total: Option<u64>,
    pub embedding_latency_p50_ms: Option<f64>,
    pub embedding_latency_p99_ms: Option<f64>,
    pub llm_latency_p50_ms: Option<f64>,
    pub llm_latency_p99_ms: Option<f64>,
    pub token_pack_budget_used_pct: Option<f64>,
}

pub fn metrics_snapshot() -> MetricsValues {
    MetricsValues {
        ingest_events_total: INGEST_EVENTS.total(),
        slow_path_jobs_processed: SLOW_PATH_PROCESSED.total(),
        slow_path_jobs_failed: SLOW_PATH_FAILED.total(),
        retrieval_requests_total: RETRIEVAL_REQUESTS.total(),
        embedding_latency_p50_ms: EMBEDDING_LATENCY.percentile(0.5),
        embedding_latency_p99_ms: EMBEDDING_LATENCY.percentile(0.99),
        llm_latency_p50_ms: LLM_LATENCY.percentile(0.5),
        llm_latency_p99_ms: LLM_LATENCY.percentile(0.99),
        token_pack_budget_used_pct: TOKEN_PACK_BUDGET_USED.mean(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_returns_none_until_recorded() {
        let counter: Counter<u64> = Counter::new();
        assert_eq!(counter.total(), None);
        counter.add(3, &[]);
        counter.add(4, &[]);
        assert_eq!(counter.total(), Some(7));
    }

    #[test]
    fn histogram_percentile_picks_sorted_position() {
        let histogram: Histogram<f64> = Histogram::new();
        for value in [
            10.0_f64, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0,
        ] {
            histogram.record(value, &[]);
        }
        assert_eq!(histogram.percentile(0.5), Some(60.0));
        assert_eq!(histogram.percentile(0.99), Some(100.0));
    }

    #[test]
    fn histogram_mean_averages_recorded_samples() {
        let histogram: Histogram<f64> = Histogram::new();
        histogram.record(40.0, &[]);
        histogram.record(60.0, &[]);
        assert_eq!(histogram.mean(), Some(50.0));
    }

    #[test]
    fn empty_histogram_returns_none() {
        let histogram: Histogram<f64> = Histogram::new();
        assert_eq!(histogram.percentile(0.5), None);
        assert_eq!(histogram.mean(), None);
    }
}
