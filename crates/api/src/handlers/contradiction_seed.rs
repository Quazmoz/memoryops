use axum::{extract::Path, extract::State, Extension, Json};
use common::{auth::AuthContext, error::AppResult, AppError, AppState};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use super::require_workspace;

const DEMO_AGENT_ID: &str = "demo-contradiction-seeder";
const DEMO_USER_ID: &str = "demo-user";
const DEMO_REPO: &str = "memoryops/demo";

#[derive(Debug, Clone, Copy)]
struct DemoContradictionPair {
    left: &'static str,
    right: &'static str,
    similarity: f32,
    conflict_score: f32,
}

#[derive(Debug, Serialize)]
pub struct SeedDemoContradictionsResponse {
    pub created_memories: usize,
    pub created_flags: usize,
    pub reopened_flags: usize,
    pub flags: Vec<SeededContradictionFlag>,
}

#[derive(Debug, Serialize)]
pub struct SeededContradictionFlag {
    pub id: Uuid,
    pub memory_id_a: Uuid,
    pub memory_id_b: Uuid,
    pub created: bool,
    pub reopened: bool,
}

#[derive(Debug)]
struct EnsuredMemory {
    id: Uuid,
    created: bool,
}

#[derive(Debug)]
struct EnsuredFlag {
    id: Uuid,
    created: bool,
    reopened: bool,
}

pub async fn seed_demo_contradictions(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<SeedDemoContradictionsResponse>> {
    require_workspace(&auth, id)?;
    ensure_demo_seed_allowed()?;

    let pairs = demo_pairs();
    let mut created_memories = 0usize;
    let mut created_flags = 0usize;
    let mut reopened_flags = 0usize;
    let mut flags = Vec::with_capacity(pairs.len());

    for pair in pairs {
        let left = ensure_demo_memory(&state, id, pair.left).await?;
        let right = ensure_demo_memory(&state, id, pair.right).await?;
        created_memories += usize::from(left.created) + usize::from(right.created);

        let flag = ensure_demo_flag(
            &state,
            id,
            left.id,
            right.id,
            pair.similarity,
            pair.conflict_score,
        )
        .await?;
        created_flags += usize::from(flag.created);
        reopened_flags += usize::from(flag.reopened);
        flags.push(SeededContradictionFlag {
            id: flag.id,
            memory_id_a: left.id,
            memory_id_b: right.id,
            created: flag.created,
            reopened: flag.reopened,
        });
    }

    Ok(Json(SeedDemoContradictionsResponse {
        created_memories,
        created_flags,
        reopened_flags,
        flags,
    }))
}

fn ensure_demo_seed_allowed() -> AppResult<()> {
    let is_production = std::env::var("APP_ENV")
        .map(|value| value.trim().eq_ignore_ascii_case("production"))
        .unwrap_or(false);

    if is_production {
        return Err(AppError::Forbidden);
    }

    Ok(())
}

fn demo_pairs() -> Vec<DemoContradictionPair> {
    vec![
        DemoContradictionPair {
            left: "Demo contradiction: Qdrant vector search is enabled for the MemoryOps workspace.",
            right: "Demo contradiction: Qdrant vector search is disabled for the MemoryOps workspace.",
            similarity: 0.94,
            conflict_score: 0.91,
        },
        DemoContradictionPair {
            left: "Demo contradiction: Episodic memory retention is configured for 30 days.",
            right: "Demo contradiction: Episodic memory retention is configured for 7 days.",
            similarity: 0.92,
            conflict_score: 0.86,
        },
        DemoContradictionPair {
            left: "Demo contradiction: Slack ingestion is active and processing workspace events.",
            right: "Demo contradiction: Slack ingestion is inactive and not processing workspace events.",
            similarity: 0.90,
            conflict_score: 0.84,
        },
    ]
}

async fn ensure_demo_memory(
    state: &AppState,
    workspace_id: Uuid,
    content: &str,
) -> AppResult<EnsuredMemory> {
    if let Some(id) = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT id
        FROM memory_units
        WHERE workspace_id = $1
          AND content = $2
          AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(content)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    {
        return Ok(EnsuredMemory { id, created: false });
    }

    let id = Uuid::now_v7();
    let scope = json!({
        "workspace_id": workspace_id,
        "agent_id": DEMO_AGENT_ID,
        "user_id": DEMO_USER_ID,
        "repo": DEMO_REPO,
    });
    let tags = vec!["demo".to_owned(), "contradiction".to_owned()];

    sqlx::query(
        r#"
        INSERT INTO memory_units (
            id, workspace_id, scope, memory_type, scope_visibility,
            content, entities, importance_score,
            source_events, embedding_id, token_count, tags
        )
        VALUES ($1, $2, $3, 'semantic'::memory_type, 'workspace', $4, $5, $6, $7, NULL, NULL, $8)
        "#,
    )
    .bind(id)
    .bind(workspace_id)
    .bind(scope)
    .bind(content)
    .bind(json!([]))
    .bind(0.9_f32)
    .bind(Vec::<Uuid>::new())
    .bind(tags)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(EnsuredMemory { id, created: true })
}

async fn ensure_demo_flag(
    state: &AppState,
    workspace_id: Uuid,
    memory_id_a: Uuid,
    memory_id_b: Uuid,
    similarity: f32,
    conflict_score: f32,
) -> AppResult<EnsuredFlag> {
    #[derive(Debug, sqlx::FromRow)]
    struct ExistingFlag {
        id: Uuid,
        resolution: String,
    }

    let existing = sqlx::query_as::<_, ExistingFlag>(
        r#"
        SELECT id, resolution::TEXT AS resolution
        FROM contradiction_flags
        WHERE workspace_id = $1
          AND (
            (memory_id_a = $2 AND memory_id_b = $3)
            OR
            (memory_id_a = $3 AND memory_id_b = $2)
          )
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(memory_id_a)
    .bind(memory_id_b)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?;

    if let Some(existing) = existing {
        let reopened = existing.resolution != "open";
        if reopened {
            sqlx::query(
                r#"
                UPDATE contradiction_flags
                SET resolution = 'open'::contradiction_resolution,
                    resolved_by = NULL,
                    resolved_at = NULL,
                    notes = 'Reopened by demo contradiction seed data',
                    kept_memory_id = NULL,
                    discarded_memory_id = NULL
                WHERE workspace_id = $1 AND id = $2
                "#,
            )
            .bind(workspace_id)
            .bind(existing.id)
            .execute(&state.db)
            .await
            .map_err(AppError::Database)?;
        }

        return Ok(EnsuredFlag {
            id: existing.id,
            created: false,
            reopened,
        });
    }

    let id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO contradiction_flags (
            id, workspace_id, memory_id_a, memory_id_b,
            similarity, conflict_score, resolution, notes
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'open'::contradiction_resolution, $7)
        "#,
    )
    .bind(id)
    .bind(workspace_id)
    .bind(memory_id_a)
    .bind(memory_id_b)
    .bind(similarity)
    .bind(conflict_score)
    .bind("Seeded deterministic demo contradiction")
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(EnsuredFlag {
        id,
        created: true,
        reopened: false,
    })
}
