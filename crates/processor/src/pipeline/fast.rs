use anyhow::anyhow;
use common::{
    error::AppResult,
    models::{EventType, MemoryType, MemoryUnit, RawEvent, Source},
    AppError, AppState,
};
use serde_json::json;
use uuid::Uuid;

use crate::{
    embedder, extractor, scope,
    store::{self, NewMemoryUnit},
};

pub async fn run_fast_path(state: &AppState, event: &RawEvent) -> AppResult<MemoryUnit> {
    let content = extractor::extract_text(event)?;
    let memory_id = Uuid::now_v7();
    let embedding_id = embedder::embed_and_upsert(state, &memory_id, &content).await?;
    let token_count = count_tokens(&content)?;

    let unit = NewMemoryUnit {
        id: memory_id,
        workspace_id: event.workspace_id,
        scope: scope::build_scope(event),
        memory_type: MemoryType::Episodic,
        content,
        entities: json!([]),
        importance_score: importance_score(state, event),
        source_events: vec![event.id],
        embedding_id: Some(embedding_id),
        token_count: Some(token_count),
        tags: Vec::new(),
    };

    store::insert_memory_unit(&state.db, &unit).await
}

pub(crate) fn importance_score(state: &AppState, event: &RawEvent) -> f32 {
    (source_authority_weight(state, event.source) * 0.5)
        + (event_type_base_importance(event.event_type) * 0.5)
}

pub(crate) fn count_tokens(content: &str) -> AppResult<i32> {
    let tokenizer = tiktoken_rs::cl100k_base()
        .map_err(|error| AppError::Internal(anyhow!("failed to initialize tokenizer: {error}")))?;
    let token_count = tokenizer.encode_with_special_tokens(content).len();

    i32::try_from(token_count)
        .map_err(|error| AppError::Internal(anyhow!("token count exceeded i32 range: {error}")))
}

fn source_authority_weight(state: &AppState, source: Source) -> f32 {
    let authority = &state.config.retrieval.source_authority;
    match source {
        Source::GitHub => authority.github,
        Source::Slack => authority.slack,
        Source::Jira => authority.jira,
        Source::Linear => authority.linear,
    }
}

fn event_type_base_importance(event_type: EventType) -> f32 {
    match event_type {
        EventType::PullRequest => 0.8,
        EventType::PullRequestReview => 0.6,
        EventType::Push => 0.4,
        EventType::IssueComment => 0.5,
        EventType::Issue => 0.7,
        EventType::Message | EventType::Reaction => 0.3,
    }
}
