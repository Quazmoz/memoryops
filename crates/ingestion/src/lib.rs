pub mod github;
mod queue;
mod router;
mod store;

pub use queue::{publish_raw_event, STREAM_KEY};
pub use router::ingestion_router;
pub use store::{insert_raw_event, NewRawEvent};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_exports_include_stream_key() {
        assert_eq!(STREAM_KEY, "memoryops:raw_events");
    }
}
