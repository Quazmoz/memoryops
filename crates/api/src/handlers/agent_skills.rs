use std::path::Path;

use axum::{extract::Path as AxumPath, extract::State, Extension, Json};
use common::{auth::AuthContext, error::AppResult, AppError, AppState};
use serde::{Deserialize, Serialize};

const MAX_SKILL_NAME_LEN: usize = 64;
const MAX_SKILL_TITLE_LEN: usize = 120;
const MAX_SKILL_DESCRIPTION_LEN: usize = 500;
const MAX_SKILL_INSTRUCTIONS_LEN: usize = 50_000;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct AgentSkill {
    pub name: String,
    pub filename: String,
    pub assistant: String,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct AgentSkillContent {
    pub name: String,
    pub filename: String,
    pub assistant: String,
    pub title: String,
    pub description: String,
    pub instructions: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentSkillRequest {
    pub assistant: String,
    pub name: String,
    pub title: String,
    pub description: String,
    pub instructions: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAgentSkillRequest {
    pub title: String,
    pub description: String,
    pub instructions: String,
}

#[derive(Debug)]
struct ParsedAgentSkillMarkdown {
    title: String,
    description: String,
    instructions: String,
}

pub async fn seed_default_skills(
    db: &sqlx::PgPool,
    workspace_id: uuid::Uuid,
) -> Result<(), AppError> {
    let root = Path::new(".");
    let mut skills = Vec::new();

    // scan gemini
    let gemini_dir = root.join(".gemini").join("skills");
    if gemini_dir.exists() && gemini_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(gemini_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|ext| ext == "md") {
                    if let Some(name) = path.file_stem().and_then(|v| v.to_str()) {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            skills.push((name.to_owned(), "gemini".to_owned(), content));
                        }
                    }
                }
            }
        }
    }

    // scan claude
    let claude_dir = root.join(".claude").join("skills");
    if claude_dir.exists() && claude_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(claude_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|ext| ext == "md") {
                    if let Some(name) = path.file_stem().and_then(|v| v.to_str()) {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            skills.push((name.to_owned(), "claude".to_owned(), content));
                        }
                    }
                }
            }
        }
    }

    for (name, assistant, content) in skills {
        let parsed = parse_markdown_metadata(&content, &name);
        let filename = format!("{name}.md");

        sqlx::query(
            r#"
            INSERT INTO agent_skills (workspace_id, name, filename, assistant, title, description, instructions, content)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (workspace_id, assistant, name) DO NOTHING
            "#
        )
        .bind(workspace_id)
        .bind(name)
        .bind(filename)
        .bind(assistant)
        .bind(parsed.title)
        .bind(parsed.description)
        .bind(parsed.instructions)
        .bind(content)
        .execute(db)
        .await
        .map_err(AppError::Database)?;
    }

    Ok(())
}

#[axum::debug_handler]
pub async fn list_agent_skills(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> AppResult<Json<Vec<AgentSkill>>> {
    let workspace_id = auth.workspace_id;
    let mut skills = sqlx::query_as::<_, AgentSkill>(
        r#"
        SELECT name, filename, assistant, title, description
        FROM agent_skills
        WHERE workspace_id = $1
        ORDER BY assistant ASC, LOWER(title) ASC, name ASC
        "#,
    )
    .bind(workspace_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    if skills.is_empty() {
        if let Err(err) = seed_default_skills(&state.db, workspace_id).await {
            tracing::warn!(?err, "failed to auto-seed default agent skills");
        }
        skills = sqlx::query_as::<_, AgentSkill>(
            r#"
            SELECT name, filename, assistant, title, description
            FROM agent_skills
            WHERE workspace_id = $1
            ORDER BY assistant ASC, LOWER(title) ASC, name ASC
            "#,
        )
        .bind(workspace_id)
        .fetch_all(&state.db)
        .await
        .map_err(AppError::Database)?;
    }

    Ok(Json(skills))
}

#[axum::debug_handler]
pub async fn get_agent_skill(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    AxumPath((assistant, name)): AxumPath<(String, String)>,
) -> AppResult<Json<AgentSkillContent>> {
    let workspace_id = auth.workspace_id;
    let assistant = validate_assistant(&assistant)?;
    let name = validate_skill_name(&name)?;

    let skill = sqlx::query_as::<_, AgentSkillContent>(
        r#"
        SELECT name, filename, assistant, title, description, instructions, content
        FROM agent_skills
        WHERE workspace_id = $1 AND assistant = $2 AND name = $3
        "#,
    )
    .bind(workspace_id)
    .bind(assistant)
    .bind(name)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?;

    if let Some(skill) = skill {
        Ok(Json(skill))
    } else {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM agent_skills WHERE workspace_id = $1",
        )
        .bind(workspace_id)
        .fetch_one(&state.db)
        .await
        .map_err(AppError::Database)?;

        if count == 0 {
            if let Err(err) = seed_default_skills(&state.db, workspace_id).await {
                tracing::warn!(?err, "failed to auto-seed default agent skills during get");
            }
            let skill = sqlx::query_as::<_, AgentSkillContent>(
                r#"
                SELECT name, filename, assistant, title, description, instructions, content
                FROM agent_skills
                WHERE workspace_id = $1 AND assistant = $2 AND name = $3
                "#,
            )
            .bind(workspace_id)
            .bind(assistant)
            .bind(name)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::Database)?;

            if let Some(skill) = skill {
                return Ok(Json(skill));
            }
        }

        Err(AppError::NotFound {
            resource: format!("agent_skill:{assistant}/{name}"),
        })
    }
}

#[axum::debug_handler]
pub async fn create_agent_skill(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Json(request): Json<CreateAgentSkillRequest>,
) -> AppResult<Json<AgentSkillContent>> {
    let workspace_id = auth.workspace_id;
    let assistant = validate_assistant(&request.assistant)?;
    let name = validate_skill_name(&request.name)?;
    let title = validate_title(&request.title)?;
    let description = validate_description(&request.description)?;
    let instructions = validate_instructions(&request.instructions)?;

    let filename = format!("{name}.md");
    let content = compose_skill_markdown(title, description, instructions);

    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM agent_skills WHERE workspace_id = $1 AND assistant = $2 AND name = $3)"
    )
    .bind(workspace_id)
    .bind(assistant)
    .bind(name)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Database)?;

    if exists {
        return Err(AppError::Conflict(format!(
            "Agent skill '{assistant}/{name}' already exists"
        )));
    }

    let skill = sqlx::query_as::<_, AgentSkillContent>(
        r#"
        INSERT INTO agent_skills (workspace_id, name, filename, assistant, title, description, instructions, content)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING name, filename, assistant, title, description, instructions, content
        "#
    )
    .bind(workspace_id)
    .bind(name)
    .bind(filename)
    .bind(assistant)
    .bind(title)
    .bind(description)
    .bind(instructions)
    .bind(content)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(skill))
}

#[axum::debug_handler]
pub async fn update_agent_skill(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    AxumPath((assistant, name)): AxumPath<(String, String)>,
    Json(request): Json<UpdateAgentSkillRequest>,
) -> AppResult<Json<AgentSkillContent>> {
    let workspace_id = auth.workspace_id;
    let assistant = validate_assistant(&assistant)?;
    let name = validate_skill_name(&name)?;
    let title = validate_title(&request.title)?;
    let description = validate_description(&request.description)?;
    let instructions = validate_instructions(&request.instructions)?;

    let content = compose_skill_markdown(title, description, instructions);

    let skill = sqlx::query_as::<_, AgentSkillContent>(
        r#"
        UPDATE agent_skills
        SET title = $4,
            description = $5,
            instructions = $6,
            content = $7,
            updated_at = NOW()
        WHERE workspace_id = $1 AND assistant = $2 AND name = $3
        RETURNING name, filename, assistant, title, description, instructions, content
        "#,
    )
    .bind(workspace_id)
    .bind(assistant)
    .bind(name)
    .bind(title)
    .bind(description)
    .bind(instructions)
    .bind(content)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("agent_skill:{assistant}/{name}"),
    })?;

    Ok(Json(skill))
}

fn validate_assistant(assistant: &str) -> AppResult<&str> {
    let trimmed = assistant.trim();
    if trimmed == "gemini" || trimmed == "claude" {
        Ok(trimmed)
    } else {
        Err(AppError::Validation(
            "Assistant must be either 'gemini' or 'claude'".to_owned(),
        ))
    }
}

fn validate_skill_name(name: &str) -> AppResult<&str> {
    let trimmed = name.trim();
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return Err(AppError::Validation("Skill name is required".to_owned()));
    };

    if trimmed.len() > MAX_SKILL_NAME_LEN {
        return Err(AppError::Validation(format!(
            "Skill name must be at most {MAX_SKILL_NAME_LEN} characters"
        )));
    }
    if !first.is_ascii_lowercase() {
        return Err(AppError::Validation(
            "Skill name must start with a lowercase letter".to_owned(),
        ));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(AppError::Validation(
            "Skill name may only contain lowercase letters, digits, underscores, and hyphens"
                .to_owned(),
        ));
    }

    Ok(trimmed)
}

fn validate_title(title: &str) -> AppResult<&str> {
    validate_single_line_text(title, "Skill title", MAX_SKILL_TITLE_LEN)
}

fn validate_description(description: &str) -> AppResult<&str> {
    validate_single_line_text(description, "Skill description", MAX_SKILL_DESCRIPTION_LEN)
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

fn validate_instructions(instructions: &str) -> AppResult<&str> {
    let normalized = instructions.replace("\r\n", "\n");
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation(
            "Skill instructions are required".to_owned(),
        ));
    }
    if trimmed.len() > MAX_SKILL_INSTRUCTIONS_LEN {
        return Err(AppError::Validation(format!(
            "Skill instructions must be at most {MAX_SKILL_INSTRUCTIONS_LEN} characters"
        )));
    }
    Ok(instructions.trim())
}

fn parse_markdown_metadata(content: &str, fallback_name: &str) -> ParsedAgentSkillMarkdown {
    let normalized = content.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.lines().collect();
    let mut title = fallback_name.to_owned();
    let mut description = String::new();
    let mut body_start = 0;

    let mut index = 0;
    while index < lines.len() && lines[index].trim().is_empty() {
        index += 1;
    }

    if let Some(line) = lines.get(index).map(|line| line.trim()) {
        if let Some(rest) = line.strip_prefix("# Skill:") {
            title = rest.trim().to_owned();
            body_start = index + 1;
        } else if let Some(rest) = line.strip_prefix("# ") {
            title = rest.trim().to_owned();
            body_start = index + 1;
        }
    }

    let mut description_index = body_start;
    while description_index < lines.len() && lines[description_index].trim().is_empty() {
        description_index += 1;
    }

    if let Some(line) = lines.get(description_index).map(|line| line.trim()) {
        if let Some(rest) = line.strip_prefix("**Description:**") {
            description = rest.trim().to_owned();
            body_start = description_index + 1;
        }
    }

    if description.is_empty() {
        description = format!("Instructions on how to configure and run the {title} agent skill.");
    }

    let instructions = lines
        .get(body_start..)
        .map(|tail| tail.join("\n"))
        .unwrap_or_default()
        .trim()
        .to_owned();

    ParsedAgentSkillMarkdown {
        title,
        description,
        instructions,
    }
}

fn compose_skill_markdown(title: &str, description: &str, instructions: &str) -> String {
    let trimmed_instructions = instructions.trim();
    if trimmed_instructions.is_empty() {
        format!("# Skill: {title}\n\n**Description:** {description}\n")
    } else {
        format!("# Skill: {title}\n\n**Description:** {description}\n\n{trimmed_instructions}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::providers::{FastEmbedProvider, OllamaProvider};
    use common::AppConfig;
    use qdrant_client::Qdrant;
    use sqlx::PgPool;
    use std::sync::Arc;
    use tokio::sync::Semaphore;
    use uuid::Uuid;

    #[test]
    fn parse_markdown_metadata_extracts_title_description_and_instructions() {
        let parsed = parse_markdown_metadata(
            "# Skill: Release Notes\n\n**Description:** Summarises changes\n\n## Trigger\n- On deploy\n",
            "release_notes",
        );

        assert_eq!(parsed.title, "Release Notes");
        assert_eq!(parsed.description, "Summarises changes");
        assert_eq!(parsed.instructions, "## Trigger\n- On deploy");
    }

    #[test]
    fn compose_skill_markdown_formats_expected_sections() {
        let content = compose_skill_markdown(
            "Release Notes",
            "Summarises changes",
            "## Trigger\n- On deploy",
        );

        assert!(content.starts_with("# Skill: Release Notes"));
        assert!(content.contains("**Description:** Summarises changes"));
        assert!(content.ends_with("- On deploy\n"));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn create_update_and_read_agent_skill_round_trip(pool: PgPool) {
        let workspace_id = Uuid::now_v7();
        sqlx::query("INSERT INTO workspaces (id, name) VALUES ($1, 'test-ws')")
            .bind(workspace_id)
            .execute(&pool)
            .await
            .unwrap();

        let state = AppState {
            db: pool.clone(),
            redis: deadpool_redis::Config::from_url("redis://localhost:16379")
                .create_pool(None)
                .unwrap(),
            qdrant: Qdrant::from_url("http://localhost:16333").build().unwrap(),
            processor_semaphore: Arc::new(Semaphore::new(1)),
            embedding_provider: Arc::new(FastEmbedProvider::new("test")),
            llm_provider: Arc::new(OllamaProvider::new("http://localhost:9", "test", 1, None)),
            config: Arc::new(
                AppConfig::from_toml_str(include_str!("../../../../config.toml")).unwrap(),
            ),
            app_secret_key: Arc::new(zeroize::Zeroizing::new("secret".to_owned())),
            trusted_proxy_cidrs: Arc::new(Vec::new()),
        };
        let auth = AuthContext {
            workspace_id,
            key_id: Uuid::now_v7(),
            key_prefix: "prefix".to_owned(),
        };

        // 1. Create skill
        let create_req = CreateAgentSkillRequest {
            assistant: "claude".to_owned(),
            name: "release_notes".to_owned(),
            title: "Release Notes".to_owned(),
            description: "Summarises changes".to_owned(),
            instructions: "## Trigger\n- On deploy".to_owned(),
        };

        let created = create_agent_skill(
            State(state.clone()),
            Extension(auth.clone()),
            Json(create_req),
        )
        .await
        .expect("create should succeed")
        .0;

        assert_eq!(created.filename, "release_notes.md");
        assert_eq!(created.title, "Release Notes");

        // 2. List skills
        let listed = list_agent_skills(State(state.clone()), Extension(auth.clone()))
            .await
            .expect("list should succeed")
            .0;

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "release_notes");

        // 3. Update skill
        let update_req = UpdateAgentSkillRequest {
            title: "Release Notes".to_owned(),
            description: "Summarises release work".to_owned(),
            instructions: "## Trigger\n- Before deploy".to_owned(),
        };

        let updated = update_agent_skill(
            State(state.clone()),
            Extension(auth.clone()),
            AxumPath(("claude".to_owned(), "release_notes".to_owned())),
            Json(update_req),
        )
        .await
        .expect("update should succeed")
        .0;

        assert_eq!(updated.description, "Summarises release work");

        // 4. Get skill
        let read_back = get_agent_skill(
            State(state.clone()),
            Extension(auth.clone()),
            AxumPath(("claude".to_owned(), "release_notes".to_owned())),
        )
        .await
        .expect("get should succeed")
        .0;

        assert_eq!(read_back.instructions, "## Trigger\n- Before deploy");
        assert!(read_back.content.contains("Summarises release work"));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn duplicate_skill_create_returns_conflict(pool: PgPool) {
        let workspace_id = Uuid::now_v7();
        sqlx::query("INSERT INTO workspaces (id, name) VALUES ($1, 'test-ws')")
            .bind(workspace_id)
            .execute(&pool)
            .await
            .unwrap();

        let state = AppState {
            db: pool.clone(),
            redis: deadpool_redis::Config::from_url("redis://localhost:16379")
                .create_pool(None)
                .unwrap(),
            qdrant: Qdrant::from_url("http://localhost:16333").build().unwrap(),
            processor_semaphore: Arc::new(Semaphore::new(1)),
            embedding_provider: Arc::new(FastEmbedProvider::new("test")),
            llm_provider: Arc::new(OllamaProvider::new("http://localhost:9", "test", 1, None)),
            config: Arc::new(
                AppConfig::from_toml_str(include_str!("../../../../config.toml")).unwrap(),
            ),
            app_secret_key: Arc::new(zeroize::Zeroizing::new("secret".to_owned())),
            trusted_proxy_cidrs: Arc::new(Vec::new()),
        };
        let auth = AuthContext {
            workspace_id,
            key_id: Uuid::now_v7(),
            key_prefix: "prefix".to_owned(),
        };

        let create_req = CreateAgentSkillRequest {
            assistant: "gemini".to_owned(),
            name: "incident_brief".to_owned(),
            title: "Incident Brief".to_owned(),
            description: "Drafts an incident brief".to_owned(),
            instructions: "## Trigger\n- On incident".to_owned(),
        };

        let _ = create_agent_skill(
            State(state.clone()),
            Extension(auth.clone()),
            Json(create_req),
        )
        .await
        .expect("first create should succeed");

        let create_req_dup = CreateAgentSkillRequest {
            assistant: "gemini".to_owned(),
            name: "incident_brief".to_owned(),
            title: "Incident Brief".to_owned(),
            description: "Drafts an incident brief".to_owned(),
            instructions: "## Trigger\n- On incident".to_owned(),
        };

        let error = create_agent_skill(
            State(state.clone()),
            Extension(auth.clone()),
            Json(create_req_dup),
        )
        .await
        .expect_err("duplicate create should fail");

        assert!(matches!(error, AppError::Conflict(_)));
    }
}
