use common::{
    error::{AppResult, ProviderError},
    models::{MemoryType, MemoryUnit, RawEvent},
    AppError, AppState,
};
use serde_json::json;
use uuid::Uuid;

use crate::{
    embedder, extractor, scope,
    store::{self, NewMemoryUnit},
};

use super::fast::{count_tokens, importance_score};

pub async fn run_slow_path(state: &AppState, event: &RawEvent) -> AppResult<MemoryUnit> {
    let raw_content = extractor::extract_text(event)?;
    let content = summarize_or_fallback(state, &raw_content).await?;
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

async fn summarize_or_fallback(state: &AppState, raw_content: &str) -> AppResult<String> {
    match state.llm_provider.summarize(raw_content, 200).await {
        Ok(summary) if summary.trim().is_empty() => Ok(raw_content.to_owned()),
        Ok(summary) => Ok(summary),
        Err(ProviderError::NotConfigured) => Ok(raw_content.to_owned()),
        Err(error @ ProviderError::RateLimited { .. }) => Err(AppError::Provider(error)),
        Err(error @ ProviderError::Request(_)) | Err(error @ ProviderError::InvalidResponse(_)) => {
            tracing::warn!(error = ?error, "LLM summarization failed; falling back to raw content");
            Ok(raw_content.to_owned())
        }
    }
}
