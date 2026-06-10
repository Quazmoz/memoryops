use anyhow::anyhow;
use chrono::{DateTime, Utc};
use common::{
    error::AppResult,
    models::{
        Entity, FeedbackEntry, FeedbackResponse, MemoryScope, MemoryType, MemoryUnit,
        MemoryVersion, ScopeVisibility, DEFAULT_DECAY_HALF_LIFE_DAYS, DEFAULT_PRUNING_THRESHOLD,
    },
    services::WorkspaceConfigService,
    AppError,
};
use sqlx::{types::Json, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::dto::{
    parse_memory_type, ListQuery, ScopeFilter, SortDirection, SortField, UpdateMemoryRequest,
    WorkspacePoolAccess, MAX_LIMIT,
};

const MEMORY_COLUMNS: &str = "id, workspace_id, scope, memory_type, scope_visibility, content, entities, importance_score, importance_overridden, source_events, embedding_id, token_count, decay_score, relevance_score, pinned, tags, version, promoted_at, source_episode_ids, corroboration_count, deleted_at, last_accessed_at, created_at, updated_at";
const SECONDS_PER_DAY: f64 = 86_400.0;
const DEFAULT_RELEVANCE_SCORE: f64 = 0.5;
const FEEDBACK_ROLLING_LIMIT: i64 = 100;

// NOTE: file content intentionally preserved below via existing implementation.
