pub mod github;
pub mod jira;
pub mod linear;
pub mod observation;
mod queue;
mod router;
pub mod slack;
mod store;

pub use observation::ingest::{ingest_observation, ObservationInput, ObservationOutput};
pub use queue::{publish_raw_event, STREAM_KEY};
pub use router::{ingestion_router, observation_router};
pub use store::{insert_raw_event, NewRawEvent};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_exports_include_stream_key() {
        assert_eq!(STREAM_KEY, "memoryops:raw_events");
    }
}
