pub mod github;
pub mod jira;
pub mod linear;
pub mod observation;
mod queue;
mod router;
pub mod slack;
mod store;
mod webhook;

pub use observation::ingest::{ingest_observation, ObservationInput, ObservationOutput};
pub use queue::{publish_raw_event, publish_raw_event_with_mode, PublishMode, STREAM_KEY};
pub use router::{ingestion_router, observation_router};
pub use store::{insert_raw_event, raw_event_needs_publish, NewRawEvent};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_exports_include_stream_key() {
        assert_eq!(STREAM_KEY, "memoryops:raw_events");
    }
}
