use axum::{extract::State, Json};
use common::{
    error::AppResult,
    models::{MemoryType, ScopeVisibility, WorkspaceConfig},
    AppError, AppState,
};
use serde::Serialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::keys;

const DEFAULT_WORKSPACE_NAME: &str = "Capsule Corp Memory Lab";
const PUBLIC_API_KEY_CONFIG_KEY: &str = "public_api_key";
const PUBLIC_API_KEY_PREFIX_CONFIG_KEY: &str = "public_api_key_prefix";
const DBZ_SEED_TAG: &str = "dragon-ball-z";

#[derive(Debug, Serialize)]
pub struct DefaultWorkspaceResponse {
    pub id: Uuid,
    pub name: String,
    pub api_key: String,
}

struct SeedMemory {
    content: &'static str,
    memory_type: MemoryType,
    importance_score: f32,
    tags: &'static [&'static str],
}

const SEED_MEMORIES: &[SeedMemory] = &[
    SeedMemory {
        content: "Capsule Corp operating rule: treat the seven Dragon Balls as high-value artifacts. A complete set can summon Shenron for a wish, so every radar hit needs provenance, custody, and duplicate checks before action.",
        memory_type: MemoryType::Semantic,
        importance_score: 0.88,
        tags: &[DBZ_SEED_TAG, "dragon-balls", "shenron", "artifact-tracking"],
    },
    SeedMemory {
        content: "Goku is the benchmark responder for impossible incidents: he defends Earth with allies, keeps training after every loss, and became a Super Saiyan during the Namek crisis against Frieza.",
        memory_type: MemoryType::Semantic,
        importance_score: 0.86,
        tags: &[DBZ_SEED_TAG, "goku", "super-saiyan", "incident-response"],
    },
    SeedMemory {
        content: "Vegeta should be modeled as a high-pride rival stakeholder: Saiyan prince, blunt reviewer, relentless optimizer, and often useful once goals align with protecting Earth.",
        memory_type: MemoryType::Semantic,
        importance_score: 0.82,
        tags: &[DBZ_SEED_TAG, "vegeta", "stakeholders", "saiyan"],
    },
    SeedMemory {
        content: "Namek mission recap: Bulma, Gohan, and Krillin pursued the Namekian Dragon Balls while Frieza and Vegeta competed for immortality. Expect rapidly shifting custody, alliances, and risk.",
        memory_type: MemoryType::Episodic,
        importance_score: 0.78,
        tags: &[DBZ_SEED_TAG, "namek", "frieza", "mission-log"],
    },
    SeedMemory {
        content: "Piccolo pattern: former antagonist, precise tactician, and trusted mentor for Gohan. Do not discard a source only because its earliest record looked adversarial.",
        memory_type: MemoryType::Semantic,
        importance_score: 0.74,
        tags: &[DBZ_SEED_TAG, "piccolo", "gohan", "trust-calibration"],
    },
    SeedMemory {
        content: "Android/Cell saga planning note: future Trunks is a time-sensitive source. His warnings about Dr. Gero's androids are valuable, but timelines can diverge and require validation.",
        memory_type: MemoryType::Episodic,
        importance_score: 0.8,
        tags: &[DBZ_SEED_TAG, "trunks", "androids", "timeline-risk"],
    },
    SeedMemory {
        content: "Senzu bean inventory is emergency-only. It can restore a fighter from critical condition, so allocate it like scarce incident capacity rather than routine comfort.",
        memory_type: MemoryType::Semantic,
        importance_score: 0.72,
        tags: &[DBZ_SEED_TAG, "senzu", "capacity", "recovery"],
    },
    SeedMemory {
        content: "Fusion techniques are temporary force multipliers. Use them for hard deadlines or existential threats, then preserve the handoff because the combined state will not last.",
        memory_type: MemoryType::Semantic,
        importance_score: 0.7,
        tags: &[DBZ_SEED_TAG, "fusion", "handoff", "operations"],
    },
];

#[axum::debug_handler]
pub async fn get_default_workspace(
    State(state): State<AppState>,
) -> AppResult<Json<DefaultWorkspaceResponse>> {
    let response = ensure_default_workspace(&state).await?;
    Ok(Json(response))
}

pub async fn ensure_default_workspace(state: &AppState) -> AppResult<DefaultWorkspaceResponse> {
    let (workspace_id, name, mut config) = upsert_default_workspace(state).await?;
    let api_key = ensure_public_api_key(state, workspace_id, &mut config).await?;

    super::agent_resources::seed_all_default_agent_resources(state, workspace_id).await?;
    seed_default_memories(state, workspace_id).await?;

    Ok(DefaultWorkspaceResponse {
        id: workspace_id,
        name,
        api_key,
    })
}

async fn upsert_default_workspace(state: &AppState) -> AppResult<(Uuid, String, Value)> {
    if let Some(row) = sqlx::query_as::<_, (Uuid, String, Value)>(
        "SELECT id, name, config FROM workspaces WHERE name = $1 AND deleted_at IS NULL",
    )
    .bind(DEFAULT_WORKSPACE_NAME)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    {
        return Ok(row);
    }

    let workspace_id = Uuid::now_v7();
    let config = serde_json::to_value(WorkspaceConfig::default())
        .map_err(|error| AppError::Internal(anyhow::anyhow!(error)))?;

    sqlx::query_as::<_, (Uuid, String, Value)>(
        r#"
        INSERT INTO workspaces (id, name, config)
        VALUES ($1, $2, $3)
        ON CONFLICT (name) DO UPDATE
        SET deleted_at = NULL
        RETURNING id, name, config
        "#,
    )
    .bind(workspace_id)
    .bind(DEFAULT_WORKSPACE_NAME)
    .bind(config)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Database)
}

async fn ensure_public_api_key(
    state: &AppState,
    workspace_id: Uuid,
    config: &mut Value,
) -> AppResult<String> {
    if let Some(api_key) = config
        .get(PUBLIC_API_KEY_CONFIG_KEY)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let prefix = config
            .get(PUBLIC_API_KEY_PREFIX_CONFIG_KEY)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(prefix) = prefix {
            let active = sqlx::query_scalar::<_, bool>(
                r#"
                SELECT EXISTS(
                    SELECT 1 FROM api_keys
                    WHERE workspace_id = $1 AND prefix = $2 AND revoked = false
                )
                "#,
            )
            .bind(workspace_id)
            .bind(prefix)
            .fetch_one(&state.db)
            .await
            .map_err(AppError::Database)?;

            if active {
                return Ok(api_key.to_owned());
            }
        }
    }

    let (api_key, record) = keys::insert_key(&state.db, workspace_id, "public-default").await?;
    let object = config
        .as_object_mut()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("workspace config must be an object")))?;
    object.insert(PUBLIC_API_KEY_CONFIG_KEY.to_owned(), json!(api_key));
    object.insert(
        PUBLIC_API_KEY_PREFIX_CONFIG_KEY.to_owned(),
        json!(record.prefix),
    );

    sqlx::query("UPDATE workspaces SET config = $2 WHERE id = $1")
        .bind(workspace_id)
        .bind(&*config)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?;

    Ok(api_key)
}

async fn seed_default_memories(state: &AppState, workspace_id: Uuid) -> AppResult<()> {
    let existing = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM memory_units WHERE workspace_id = $1 AND $2 = ANY(tags)",
    )
    .bind(workspace_id)
    .bind(DBZ_SEED_TAG)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Database)?;

    if existing > 0 {
        return Ok(());
    }

    for memory in SEED_MEMORIES {
        let id = Uuid::now_v7();
        let scope = json!({
            "workspace_id": workspace_id,
            "source": "memoryops-default",
            "actor": "capsule-corp-seeder",
            "agent_id": null,
            "user_id": null,
            "repo": null,
        });
        let tags: Vec<String> = memory.tags.iter().map(|tag| (*tag).to_owned()).collect();

        sqlx::query(
            r#"
            INSERT INTO memory_units (
                id, workspace_id, scope, memory_type, scope_visibility, content,
                entities, importance_score, source_events, embedding_id, token_count, tags
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NULL, NULL, $10)
            "#,
        )
        .bind(id)
        .bind(workspace_id)
        .bind(scope)
        .bind(memory.memory_type)
        .bind(ScopeVisibility::Workspace)
        .bind(memory.content)
        .bind(json!([]))
        .bind(memory.importance_score)
        .bind(Vec::<Uuid>::new())
        .bind(tags)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?;
    }

    Ok(())
}
