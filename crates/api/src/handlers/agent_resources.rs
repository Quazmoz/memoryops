use axum::{extract::Path, extract::Query, extract::State, Extension, Json};
use chrono::{DateTime, Utc};
use common::{
    audit::spawn_audit_log, auth::AuthContext, error::AppResult, models::AuditAction, AppError,
    AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres};
use uuid::Uuid;

const MAX_RESOURCE_NAME_LEN: usize = 64;
const MAX_RESOURCE_TITLE_LEN: usize = 120;
const MAX_RESOURCE_DESCRIPTION_LEN: usize = 500;
const MAX_RESOURCE_BODY_LEN: usize = 100_000;
const MAX_RESOURCE_CONTENT_LEN: usize = 120_000;
const MAX_CHANGE_NOTE_LEN: usize = 500;

const AGENT_RESOURCE_COLUMNS: &str = "id, workspace_id, kind, assistant, name, filename, title, \
     description, body, content, metadata, version, created_at, updated_at";

const AGENT_RESOURCE_VERSION_COLUMNS: &str = "id, resource_id, workspace_id, kind, assistant, \
     name, filename, title, description, body, content, metadata, version, change_note, \
     created_by, created_at";

#[derive(Debug, Deserialize)]
pub struct AgentResourceListQuery {
    pub kind: Option<String>,
    pub assistant: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentResourceRequest {
    pub kind: String,
    pub assistant: Option<String>,
    pub name: String,
    pub title: String,
    pub description: String,
    pub body: String,
    pub content: Option<String>,
    pub metadata: Option<Value>,
    pub change_note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAgentResourceRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub body: Option<String>,
    pub content: Option<String>,
    pub metadata: Option<Value>,
    pub change_note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RollbackAgentResourceRequest {
    pub change_note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentResourceKind {
    Skill,
    Agent,
    Prompt,
    Instruction,
}

impl AgentResourceKind {
    fn parse(value: &str) -> AppResult<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "skill" => Ok(Self::Skill),
            "agent" => Ok(Self::Agent),
            "prompt" => Ok(Self::Prompt),
            "instruction" => Ok(Self::Instruction),
            _ => Err(AppError::Validation(
                "Resource kind must be one of skill, agent, prompt, or instruction".to_owned(),
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Skill => "skill",
            Self::Agent => "agent",
            Self::Prompt => "prompt",
            Self::Instruction => "instruction",
        }
    }

    fn title_label(self) -> &'static str {
        match self {
            Self::Skill => "Skill",
            Self::Agent => "Agent",
            Self::Prompt => "Prompt",
            Self::Instruction => "Instruction",
        }
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AgentResource {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub kind: String,
    pub assistant: String,
    pub name: String,
    pub filename: String,
    pub title: String,
    pub description: String,
    pub body: String,
    pub content: String,
    pub metadata: Value,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AgentResourceSummary {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub kind: String,
    pub assistant: String,
    pub name: String,
    pub filename: String,
    pub title: String,
    pub description: String,
    pub metadata: Value,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AgentResourceVersion {
    pub id: Uuid,
    pub resource_id: Uuid,
    pub workspace_id: Uuid,
    pub kind: String,
    pub assistant: String,
    pub name: String,
    pub filename: String,
    pub title: String,
    pub description: String,
    pub body: String,
    pub content: String,
    pub metadata: Value,
    pub version: i32,
    pub change_note: Option<String>,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct ResourceWriteState {
    title: String,
    description: String,
    body: String,
    content: String,
    metadata: Value,
}

#[derive(Clone, Copy)]
pub struct SkillResourceInput<'a> {
    pub assistant: &'a str,
    pub name: &'a str,
    pub filename: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub instructions: &'a str,
    pub content: &'a str,
}

pub async fn seed_skill_resource(
    db: &PgPool,
    workspace_id: Uuid,
    input: SkillResourceInput<'_>,
) -> Result<(), AppError> {
    let metadata = json!({ "seeded": true });
    sqlx::query(
        r#"
        INSERT INTO agent_resources (
            workspace_id, kind, assistant, name, filename, title, description,
            body, content, metadata
        )
        VALUES ($1, 'skill', $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (workspace_id, kind, assistant, name) DO NOTHING
        "#,
    )
    .bind(workspace_id)
    .bind(input.assistant)
    .bind(input.name)
    .bind(input.filename)
    .bind(input.title)
    .bind(input.description)
    .bind(input.instructions)
    .bind(input.content)
    .bind(metadata)
    .execute(db)
    .await
    .map_err(AppError::Database)?;

    let resource = sqlx::query_as::<_, AgentResource>(
        &format!("SELECT {AGENT_RESOURCE_COLUMNS} FROM agent_resources WHERE workspace_id = $1 AND kind = 'skill' AND assistant = $2 AND name = $3"),
    )
    .bind(workspace_id)
    .bind(input.assistant)
    .bind(input.name)
    .fetch_one(db)
    .await
    .map_err(AppError::Database)?;

    insert_agent_resource_version_from_pool(db, &resource, Some("seeded initial version"), None)
        .await
}

pub async fn upsert_skill_resource_versioned(
    db: &PgPool,
    workspace_id: Uuid,
    input: SkillResourceInput<'_>,
    change_note: Option<&str>,
    created_by: Option<&str>,
) -> Result<(), AppError> {
    let metadata = json!({ "source": "legacy-agent-skills-api" });
    let mut tx = db.begin().await.map_err(AppError::Database)?;

    let resource = sqlx::query_as::<_, AgentResource>(&format!(
        r#"
            UPDATE agent_resources
            SET filename = $4,
                title = $5,
                description = $6,
                body = $7,
                content = $8,
                metadata = $9,
                version = version + 1
            WHERE workspace_id = $1 AND kind = 'skill' AND assistant = $2 AND name = $3
            RETURNING {AGENT_RESOURCE_COLUMNS}
            "#
    ))
    .bind(workspace_id)
    .bind(input.assistant)
    .bind(input.name)
    .bind(input.filename)
    .bind(input.title)
    .bind(input.description)
    .bind(input.instructions)
    .bind(input.content)
    .bind(&metadata)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Database)?;

    let resource = if let Some(resource) = resource {
        resource
    } else {
        sqlx::query_as::<_, AgentResource>(&format!(
            r#"
                INSERT INTO agent_resources (
                    workspace_id, kind, assistant, name, filename, title, description,
                    body, content, metadata
                )
                VALUES ($1, 'skill', $2, $3, $4, $5, $6, $7, $8, $9)
                RETURNING {AGENT_RESOURCE_COLUMNS}
                "#
        ))
        .bind(workspace_id)
        .bind(input.assistant)
        .bind(input.name)
        .bind(input.filename)
        .bind(input.title)
        .bind(input.description)
        .bind(input.instructions)
        .bind(input.content)
        .bind(&metadata)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_agent_resource_write_error)?
    };

    upsert_legacy_agent_skill(&mut tx, workspace_id, skill_input_from_resource(&resource)).await?;
    insert_agent_resource_version(&mut tx, &resource, change_note, created_by).await?;
    tx.commit().await.map_err(AppError::Database)?;

    Ok(())
}

#[axum::debug_handler]
pub async fn list_agent_resources(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(query): Query<AgentResourceListQuery>,
) -> AppResult<Json<Vec<AgentResourceSummary>>> {
    ensure_default_skill_resources(&state, auth.workspace_id, query.kind.as_deref()).await?;

    let kind = query
        .kind
        .as_deref()
        .map(AgentResourceKind::parse)
        .transpose()?;
    let assistant = query
        .assistant
        .as_deref()
        .map(validate_assistant)
        .transpose()?;

    let mut sql = "SELECT id, workspace_id, kind, assistant, name, filename, title, description, \
         metadata, version, created_at, updated_at \
         FROM agent_resources WHERE workspace_id = $1"
        .to_string();
    if kind.is_some() {
        sql.push_str(" AND kind = $2");
    }
    if assistant.is_some() {
        sql.push_str(if kind.is_some() {
            " AND assistant = $3"
        } else {
            " AND assistant = $2"
        });
    }
    sql.push_str(" ORDER BY kind ASC, assistant ASC, LOWER(title) ASC, name ASC");

    let mut query_builder = sqlx::query_as::<_, AgentResourceSummary>(&sql).bind(auth.workspace_id);
    if let Some(kind) = kind {
        query_builder = query_builder.bind(kind.as_str());
    }
    if let Some(assistant) = assistant {
        query_builder = query_builder.bind(assistant);
    }

    let resources = query_builder
        .fetch_all(&state.db)
        .await
        .map_err(AppError::Database)?;

    Ok(Json(resources))
}

#[axum::debug_handler]
pub async fn get_agent_resource(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((kind, assistant, name)): Path<(String, String, String)>,
) -> AppResult<Json<AgentResource>> {
    ensure_default_skill_resources(&state, auth.workspace_id, Some(&kind)).await?;
    let kind = AgentResourceKind::parse(&kind)?;
    let assistant = validate_assistant_for_kind(kind, &assistant)?;
    let name = validate_resource_name(&name)?;

    Ok(Json(
        fetch_agent_resource(&state.db, auth.workspace_id, kind, assistant, name).await?,
    ))
}

#[axum::debug_handler]
pub async fn create_agent_resource(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Json(request): Json<CreateAgentResourceRequest>,
) -> AppResult<Json<AgentResource>> {
    let kind = AgentResourceKind::parse(&request.kind)?;
    let assistant = validate_assistant_for_kind(
        kind,
        request
            .assistant
            .as_deref()
            .unwrap_or(default_assistant(kind)),
    )?;
    let name = validate_resource_name(&request.name)?;
    let title = validate_title(&request.title)?;
    let description = validate_description(&request.description)?;
    let body = validate_body(&request.body)?;
    let content = normalize_content(kind, title, description, body, request.content.as_deref())?;
    let metadata = validate_metadata(request.metadata.unwrap_or_else(|| json!({})))?;
    let change_note = validate_change_note(request.change_note.as_deref())?;
    let filename = resource_filename(name);

    let mut tx = state.db.begin().await.map_err(AppError::Database)?;

    let resource = sqlx::query_as::<_, AgentResource>(&format!(
        r#"
            INSERT INTO agent_resources (
                workspace_id, kind, assistant, name, filename, title, description,
                body, content, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING {AGENT_RESOURCE_COLUMNS}
            "#
    ))
    .bind(auth.workspace_id)
    .bind(kind.as_str())
    .bind(assistant)
    .bind(name)
    .bind(&filename)
    .bind(title)
    .bind(description)
    .bind(body)
    .bind(&content)
    .bind(&metadata)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_agent_resource_write_error)?;

    if kind == AgentResourceKind::Skill {
        upsert_legacy_agent_skill(
            &mut tx,
            auth.workspace_id,
            skill_input_from_resource(&resource),
        )
        .await?;
    }
    insert_agent_resource_version(&mut tx, &resource, change_note, Some(auth.actor().as_str()))
        .await?;

    tx.commit().await.map_err(AppError::Database)?;

    spawn_audit_log(
        state.db.clone(),
        auth.workspace_id,
        auth.actor(),
        AuditAction::AgentResourceCreated,
        resource.id,
        "agent_resource",
        Some(json!({
            "kind": resource.kind,
            "assistant": resource.assistant,
            "name": resource.name,
            "version": resource.version,
        })),
    );

    Ok(Json(resource))
}

#[axum::debug_handler]
pub async fn update_agent_resource(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((kind, assistant, name)): Path<(String, String, String)>,
    Json(request): Json<UpdateAgentResourceRequest>,
) -> AppResult<Json<AgentResource>> {
    let kind = AgentResourceKind::parse(&kind)?;
    let assistant = validate_assistant_for_kind(kind, &assistant)?;
    let name = validate_resource_name(&name)?;
    let change_note = validate_change_note(request.change_note.as_deref())?;

    let mut tx = state.db.begin().await.map_err(AppError::Database)?;
    let current = sqlx::query_as::<_, ResourceWriteState>(
        r#"
        SELECT title, description, body, content, metadata
        FROM agent_resources
        WHERE workspace_id = $1 AND kind = $2 AND assistant = $3 AND name = $4
        "#,
    )
    .bind(auth.workspace_id)
    .bind(kind.as_str())
    .bind(assistant)
    .bind(name)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("agent_resource:{}/{}/{}", kind.as_str(), assistant, name),
    })?;

    let title = match request.title.as_deref() {
        Some(value) => validate_title(value)?,
        None => current.title.as_str(),
    };
    let description = match request.description.as_deref() {
        Some(value) => validate_description(value)?,
        None => current.description.as_str(),
    };
    let body = match request.body.as_deref() {
        Some(value) => validate_body(value)?,
        None => current.body.as_str(),
    };
    let metadata = match request.metadata {
        Some(value) => validate_metadata(value)?,
        None => current.metadata,
    };
    let content = if request.content.is_none()
        && request.title.is_none()
        && request.description.is_none()
        && request.body.is_none()
    {
        current.content
    } else {
        normalize_content(kind, title, description, body, request.content.as_deref())?
    };

    let resource = sqlx::query_as::<_, AgentResource>(&format!(
        r#"
            UPDATE agent_resources
            SET title = $5,
                description = $6,
                body = $7,
                content = $8,
                metadata = $9,
                version = version + 1
            WHERE workspace_id = $1 AND kind = $2 AND assistant = $3 AND name = $4
            RETURNING {AGENT_RESOURCE_COLUMNS}
            "#
    ))
    .bind(auth.workspace_id)
    .bind(kind.as_str())
    .bind(assistant)
    .bind(name)
    .bind(title)
    .bind(description)
    .bind(body)
    .bind(&content)
    .bind(&metadata)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::Database)?;

    if kind == AgentResourceKind::Skill {
        upsert_legacy_agent_skill(
            &mut tx,
            auth.workspace_id,
            skill_input_from_resource(&resource),
        )
        .await?;
    }
    insert_agent_resource_version(&mut tx, &resource, change_note, Some(auth.actor().as_str()))
        .await?;

    tx.commit().await.map_err(AppError::Database)?;

    spawn_audit_log(
        state.db.clone(),
        auth.workspace_id,
        auth.actor(),
        AuditAction::AgentResourceUpdated,
        resource.id,
        "agent_resource",
        Some(json!({
            "kind": resource.kind,
            "assistant": resource.assistant,
            "name": resource.name,
            "version": resource.version,
            "change_note": change_note,
        })),
    );

    Ok(Json(resource))
}

#[axum::debug_handler]
pub async fn delete_agent_resource(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((kind, assistant, name)): Path<(String, String, String)>,
) -> AppResult<Json<Value>> {
    let kind = AgentResourceKind::parse(&kind)?;
    let assistant = validate_assistant_for_kind(kind, &assistant)?;
    let name = validate_resource_name(&name)?;

    let mut tx = state.db.begin().await.map_err(AppError::Database)?;
    let row: Option<(Uuid,)> = sqlx::query_as(
        r#"
        DELETE FROM agent_resources
        WHERE workspace_id = $1 AND kind = $2 AND assistant = $3 AND name = $4
        RETURNING id
        "#,
    )
    .bind(auth.workspace_id)
    .bind(kind.as_str())
    .bind(assistant)
    .bind(name)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Database)?;

    let Some((resource_id,)) = row else {
        return Err(AppError::NotFound {
            resource: format!("agent_resource:{}/{}/{}", kind.as_str(), assistant, name),
        });
    };

    // Keep the legacy agent_skills table in sync. Without this, deleting a skill
    // here would leave an orphan row that makes the legacy create endpoint report
    // a spurious conflict when the same name is recreated.
    if kind == AgentResourceKind::Skill {
        sqlx::query(
            "DELETE FROM agent_skills WHERE workspace_id = $1 AND assistant = $2 AND name = $3",
        )
        .bind(auth.workspace_id)
        .bind(assistant)
        .bind(name)
        .execute(&mut *tx)
        .await
        .map_err(AppError::Database)?;
    }

    tx.commit().await.map_err(AppError::Database)?;

    spawn_audit_log(
        state.db.clone(),
        auth.workspace_id,
        auth.actor(),
        AuditAction::AgentResourceDeleted,
        resource_id,
        "agent_resource",
        Some(json!({
            "kind": kind.as_str(),
            "assistant": assistant,
            "name": name,
        })),
    );

    Ok(Json(json!({ "deleted": true })))
}

#[axum::debug_handler]
pub async fn list_agent_resource_versions(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((kind, assistant, name)): Path<(String, String, String)>,
) -> AppResult<Json<Vec<AgentResourceVersion>>> {
    let kind = AgentResourceKind::parse(&kind)?;
    let assistant = validate_assistant_for_kind(kind, &assistant)?;
    let name = validate_resource_name(&name)?;

    let versions = sqlx::query_as::<_, AgentResourceVersion>(&format!(
        r#"
            SELECT {AGENT_RESOURCE_VERSION_COLUMNS}
            FROM agent_resource_versions
            WHERE workspace_id = $1 AND kind = $2 AND assistant = $3 AND name = $4
            ORDER BY version DESC
            "#
    ))
    .bind(auth.workspace_id)
    .bind(kind.as_str())
    .bind(assistant)
    .bind(name)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    if versions.is_empty() {
        let _ = fetch_agent_resource(&state.db, auth.workspace_id, kind, assistant, name).await?;
    }

    Ok(Json(versions))
}

#[axum::debug_handler]
pub async fn get_agent_resource_version(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((kind, assistant, name, version)): Path<(String, String, String, i32)>,
) -> AppResult<Json<AgentResourceVersion>> {
    let kind = AgentResourceKind::parse(&kind)?;
    let assistant = validate_assistant_for_kind(kind, &assistant)?;
    let name = validate_resource_name(&name)?;

    let version = sqlx::query_as::<_, AgentResourceVersion>(&format!(
        r#"
            SELECT {AGENT_RESOURCE_VERSION_COLUMNS}
            FROM agent_resource_versions
            WHERE workspace_id = $1 AND kind = $2 AND assistant = $3 AND name = $4 AND version = $5
            "#
    ))
    .bind(auth.workspace_id)
    .bind(kind.as_str())
    .bind(assistant)
    .bind(name)
    .bind(version)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!(
            "agent_resource_version:{}/{}/{}@{}",
            kind.as_str(),
            assistant,
            name,
            version
        ),
    })?;

    Ok(Json(version))
}

#[axum::debug_handler]
pub async fn rollback_agent_resource(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((kind, assistant, name, version)): Path<(String, String, String, i32)>,
    Json(request): Json<RollbackAgentResourceRequest>,
) -> AppResult<Json<AgentResource>> {
    let kind = AgentResourceKind::parse(&kind)?;
    let assistant = validate_assistant_for_kind(kind, &assistant)?;
    let name = validate_resource_name(&name)?;
    let change_note = validate_change_note(request.change_note.as_deref())?;

    let mut tx = state.db.begin().await.map_err(AppError::Database)?;

    let snapshot = sqlx::query_as::<_, AgentResourceVersion>(&format!(
        r#"
            SELECT {AGENT_RESOURCE_VERSION_COLUMNS}
            FROM agent_resource_versions
            WHERE workspace_id = $1 AND kind = $2 AND assistant = $3 AND name = $4 AND version = $5
            "#
    ))
    .bind(auth.workspace_id)
    .bind(kind.as_str())
    .bind(assistant)
    .bind(name)
    .bind(version)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!(
            "agent_resource_version:{}/{}/{}@{}",
            kind.as_str(),
            assistant,
            name,
            version
        ),
    })?;

    let resource = sqlx::query_as::<_, AgentResource>(&format!(
        r#"
            UPDATE agent_resources
            SET filename = $5,
                title = $6,
                description = $7,
                body = $8,
                content = $9,
                metadata = $10,
                version = version + 1
            WHERE workspace_id = $1 AND kind = $2 AND assistant = $3 AND name = $4
            RETURNING {AGENT_RESOURCE_COLUMNS}
            "#
    ))
    .bind(auth.workspace_id)
    .bind(kind.as_str())
    .bind(assistant)
    .bind(name)
    .bind(&snapshot.filename)
    .bind(&snapshot.title)
    .bind(&snapshot.description)
    .bind(&snapshot.body)
    .bind(&snapshot.content)
    .bind(&snapshot.metadata)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("agent_resource:{}/{}/{}", kind.as_str(), assistant, name),
    })?;

    if kind == AgentResourceKind::Skill {
        upsert_legacy_agent_skill(
            &mut tx,
            auth.workspace_id,
            skill_input_from_resource(&resource),
        )
        .await?;
    }
    let note = change_note
        .map(str::to_owned)
        .unwrap_or_else(|| format!("rollback to v{version}"));
    insert_agent_resource_version(&mut tx, &resource, Some(&note), Some(auth.actor().as_str()))
        .await?;

    tx.commit().await.map_err(AppError::Database)?;

    spawn_audit_log(
        state.db.clone(),
        auth.workspace_id,
        auth.actor(),
        AuditAction::AgentResourceRolledBack,
        resource.id,
        "agent_resource",
        Some(json!({
            "kind": resource.kind,
            "assistant": resource.assistant,
            "name": resource.name,
            "version": resource.version,
            "rolled_back_to": version,
        })),
    );

    Ok(Json(resource))
}

async fn fetch_agent_resource(
    db: &PgPool,
    workspace_id: Uuid,
    kind: AgentResourceKind,
    assistant: &str,
    name: &str,
) -> AppResult<AgentResource> {
    sqlx::query_as::<_, AgentResource>(&format!(
        "SELECT {AGENT_RESOURCE_COLUMNS} FROM agent_resources \
             WHERE workspace_id = $1 AND kind = $2 AND assistant = $3 AND name = $4"
    ))
    .bind(workspace_id)
    .bind(kind.as_str())
    .bind(assistant)
    .bind(name)
    .fetch_optional(db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("agent_resource:{}/{}/{}", kind.as_str(), assistant, name),
    })
}

async fn ensure_default_skill_resources(
    state: &AppState,
    workspace_id: Uuid,
    requested_kind: Option<&str>,
) -> AppResult<()> {
    if requested_kind
        .map(|kind| kind.trim().eq_ignore_ascii_case("skill"))
        .unwrap_or(true)
    {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM agent_resources WHERE workspace_id = $1 AND kind = 'skill'",
        )
        .bind(workspace_id)
        .fetch_one(&state.db)
        .await
        .map_err(AppError::Database)?;

        if count == 0 {
            if let Err(err) =
                super::agent_skills::seed_default_skills(&state.db, workspace_id).await
            {
                tracing::warn!(?err, "failed to auto-seed default agent resources");
            }
        }
    }

    Ok(())
}

async fn insert_agent_resource_version(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    resource: &AgentResource,
    change_note: Option<&str>,
    created_by: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO agent_resource_versions (
            resource_id, workspace_id, kind, assistant, name, filename, title,
            description, body, content, metadata, version, change_note, created_by
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        ON CONFLICT (resource_id, version) DO NOTHING
        "#,
    )
    .bind(resource.id)
    .bind(resource.workspace_id)
    .bind(&resource.kind)
    .bind(&resource.assistant)
    .bind(&resource.name)
    .bind(&resource.filename)
    .bind(&resource.title)
    .bind(&resource.description)
    .bind(&resource.body)
    .bind(&resource.content)
    .bind(&resource.metadata)
    .bind(resource.version)
    .bind(change_note)
    .bind(created_by)
    .execute(&mut **tx)
    .await
    .map_err(AppError::Database)?;

    Ok(())
}

async fn insert_agent_resource_version_from_pool(
    db: &PgPool,
    resource: &AgentResource,
    change_note: Option<&str>,
    created_by: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO agent_resource_versions (
            resource_id, workspace_id, kind, assistant, name, filename, title,
            description, body, content, metadata, version, change_note, created_by
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        ON CONFLICT (resource_id, version) DO NOTHING
        "#,
    )
    .bind(resource.id)
    .bind(resource.workspace_id)
    .bind(&resource.kind)
    .bind(&resource.assistant)
    .bind(&resource.name)
    .bind(&resource.filename)
    .bind(&resource.title)
    .bind(&resource.description)
    .bind(&resource.body)
    .bind(&resource.content)
    .bind(&resource.metadata)
    .bind(resource.version)
    .bind(change_note)
    .bind(created_by)
    .execute(db)
    .await
    .map_err(AppError::Database)?;

    Ok(())
}

async fn upsert_legacy_agent_skill(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    workspace_id: Uuid,
    input: SkillResourceInput<'_>,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO agent_skills (
            workspace_id, name, filename, assistant, title, description, instructions, content
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (workspace_id, assistant, name) DO UPDATE
        SET filename = EXCLUDED.filename,
            title = EXCLUDED.title,
            description = EXCLUDED.description,
            instructions = EXCLUDED.instructions,
            content = EXCLUDED.content,
            updated_at = NOW()
        "#,
    )
    .bind(workspace_id)
    .bind(input.name)
    .bind(input.filename)
    .bind(input.assistant)
    .bind(input.title)
    .bind(input.description)
    .bind(input.instructions)
    .bind(input.content)
    .execute(&mut **tx)
    .await
    .map_err(AppError::Database)?;

    Ok(())
}

fn skill_input_from_resource(resource: &AgentResource) -> SkillResourceInput<'_> {
    SkillResourceInput {
        assistant: &resource.assistant,
        name: &resource.name,
        filename: &resource.filename,
        title: &resource.title,
        description: &resource.description,
        instructions: &resource.body,
        content: &resource.content,
    }
}

fn validate_assistant(value: &str) -> AppResult<&str> {
    let trimmed = value.trim();
    match trimmed {
        "generic" | "openai" | "claude" | "gemini" => Ok(trimmed),
        _ => Err(AppError::Validation(
            "Assistant must be one of generic, openai, claude, or gemini".to_owned(),
        )),
    }
}

fn validate_assistant_for_kind(kind: AgentResourceKind, value: &str) -> AppResult<&str> {
    let assistant = validate_assistant(value)?;
    if kind == AgentResourceKind::Skill && assistant != "claude" && assistant != "gemini" {
        return Err(AppError::Validation(
            "Skill resources must target either claude or gemini".to_owned(),
        ));
    }
    Ok(assistant)
}

fn default_assistant(kind: AgentResourceKind) -> &'static str {
    match kind {
        AgentResourceKind::Skill => "claude",
        AgentResourceKind::Agent | AgentResourceKind::Prompt | AgentResourceKind::Instruction => {
            "generic"
        }
    }
}

fn validate_resource_name(name: &str) -> AppResult<&str> {
    let trimmed = name.trim();
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return Err(AppError::Validation("Resource name is required".to_owned()));
    };

    if trimmed.len() > MAX_RESOURCE_NAME_LEN {
        return Err(AppError::Validation(format!(
            "Resource name must be at most {MAX_RESOURCE_NAME_LEN} characters"
        )));
    }
    if !first.is_ascii_lowercase() {
        return Err(AppError::Validation(
            "Resource name must start with a lowercase letter".to_owned(),
        ));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(AppError::Validation(
            "Resource name may only contain lowercase letters, digits, underscores, and hyphens"
                .to_owned(),
        ));
    }

    Ok(trimmed)
}

fn validate_title(title: &str) -> AppResult<&str> {
    validate_single_line_text(title, "Resource title", MAX_RESOURCE_TITLE_LEN)
}

fn validate_description(description: &str) -> AppResult<&str> {
    validate_single_line_text(
        description,
        "Resource description",
        MAX_RESOURCE_DESCRIPTION_LEN,
    )
}

fn validate_single_line_text<'a>(
    value: &'a str,
    label: &str,
    max_len: usize,
) -> AppResult<&'a str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation(format!("{label} is required")));
    }
    if trimmed.len() > max_len {
        return Err(AppError::Validation(format!(
            "{label} must be at most {max_len} characters"
        )));
    }
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return Err(AppError::Validation(format!(
            "{label} must be a single line"
        )));
    }
    Ok(trimmed)
}

fn validate_body(body: &str) -> AppResult<&str> {
    let normalized = body.replace("\r\n", "\n");
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("Resource body is required".to_owned()));
    }
    if trimmed.len() > MAX_RESOURCE_BODY_LEN {
        return Err(AppError::Validation(format!(
            "Resource body must be at most {MAX_RESOURCE_BODY_LEN} characters"
        )));
    }
    Ok(body.trim())
}

fn validate_content(content: &str) -> AppResult<&str> {
    let normalized = content.replace("\r\n", "\n");
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation(
            "Resource content is required".to_owned(),
        ));
    }
    if trimmed.len() > MAX_RESOURCE_CONTENT_LEN {
        return Err(AppError::Validation(format!(
            "Resource content must be at most {MAX_RESOURCE_CONTENT_LEN} characters"
        )));
    }
    Ok(content.trim())
}

fn validate_metadata(metadata: Value) -> AppResult<Value> {
    if metadata.is_object() {
        Ok(metadata)
    } else {
        Err(AppError::Validation(
            "Resource metadata must be a JSON object".to_owned(),
        ))
    }
}

fn validate_change_note(value: Option<&str>) -> AppResult<Option<&str>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() > MAX_CHANGE_NOTE_LEN {
        return Err(AppError::Validation(format!(
            "Change note must be at most {MAX_CHANGE_NOTE_LEN} characters"
        )));
    }
    Ok(Some(trimmed))
}

fn normalize_content(
    kind: AgentResourceKind,
    title: &str,
    description: &str,
    body: &str,
    content: Option<&str>,
) -> AppResult<String> {
    if let Some(content) = content {
        return validate_content(content).map(str::to_owned);
    }
    Ok(compose_resource_markdown(kind, title, description, body))
}

fn compose_resource_markdown(
    kind: AgentResourceKind,
    title: &str,
    description: &str,
    body: &str,
) -> String {
    let trimmed_body = body.trim();
    if trimmed_body.is_empty() {
        format!(
            "# {}: {title}\n\n**Description:** {description}\n",
            kind.title_label()
        )
    } else {
        format!(
            "# {}: {title}\n\n**Description:** {description}\n\n{trimmed_body}\n",
            kind.title_label()
        )
    }
}

fn resource_filename(name: &str) -> String {
    format!("{name}.md")
}

fn map_agent_resource_write_error(error: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(db_error) = &error {
        if db_error.code().as_deref() == Some("23505") {
            return AppError::Conflict(
                "An agent resource with this kind, assistant, and name already exists".to_owned(),
            );
        }
    }
    AppError::Database(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_resources_are_limited_to_claude_and_gemini() {
        assert!(validate_assistant_for_kind(AgentResourceKind::Skill, "claude").is_ok());
        assert!(validate_assistant_for_kind(AgentResourceKind::Skill, "gemini").is_ok());
        assert!(validate_assistant_for_kind(AgentResourceKind::Skill, "generic").is_err());
    }

    #[test]
    fn compose_resource_markdown_uses_kind_label() {
        let content = compose_resource_markdown(
            AgentResourceKind::Prompt,
            "Release Brief",
            "Drafts concise release notes",
            "Summarize changes in three bullets.",
        );

        assert!(content.starts_with("# Prompt: Release Brief"));
        assert!(content.contains("**Description:** Drafts concise release notes"));
        assert!(content.ends_with("Summarize changes in three bullets.\n"));
    }
}
