use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use axum::{extract::Path as AxumPath, Json};
use common::error::AppResult;
use common::AppError;
use serde::{Deserialize, Serialize};

const MAX_SKILL_NAME_LEN: usize = 64;
const MAX_SKILL_TITLE_LEN: usize = 120;
const MAX_SKILL_DESCRIPTION_LEN: usize = 500;
const MAX_SKILL_INSTRUCTIONS_LEN: usize = 50_000;

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentSkill {
    pub name: String,
    pub filename: String,
    pub assistant: String,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize)]
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

#[axum::debug_handler]
pub async fn list_agent_skills() -> AppResult<Json<Vec<AgentSkill>>> {
    Ok(Json(list_agent_skills_under_root(&skill_root_dir())))
}

#[axum::debug_handler]
pub async fn get_agent_skill(
    AxumPath((assistant, name)): AxumPath<(String, String)>,
) -> AppResult<Json<AgentSkillContent>> {
    let assistant = validate_assistant(&assistant)?;
    let name = validate_skill_name(&name)?;
    Ok(Json(read_agent_skill_from_root(
        &skill_root_dir(),
        assistant,
        name,
    )?))
}

#[axum::debug_handler]
pub async fn create_agent_skill(
    Json(request): Json<CreateAgentSkillRequest>,
) -> AppResult<Json<AgentSkillContent>> {
    let assistant = validate_assistant(&request.assistant)?;
    let name = validate_skill_name(&request.name)?;
    let title = validate_title(&request.title)?;
    let description = validate_description(&request.description)?;
    let instructions = validate_instructions(&request.instructions)?;

    Ok(Json(write_new_agent_skill(
        &skill_root_dir(),
        assistant,
        name,
        title,
        description,
        instructions,
    )?))
}

#[axum::debug_handler]
pub async fn update_agent_skill(
    AxumPath((assistant, name)): AxumPath<(String, String)>,
    Json(request): Json<UpdateAgentSkillRequest>,
) -> AppResult<Json<AgentSkillContent>> {
    let assistant = validate_assistant(&assistant)?;
    let name = validate_skill_name(&name)?;
    let title = validate_title(&request.title)?;
    let description = validate_description(&request.description)?;
    let instructions = validate_instructions(&request.instructions)?;

    Ok(Json(update_existing_agent_skill(
        &skill_root_dir(),
        assistant,
        name,
        title,
        description,
        instructions,
    )?))
}

fn skill_root_dir() -> PathBuf {
    PathBuf::from(".")
}

fn list_agent_skills_under_root(root: &Path) -> Vec<AgentSkill> {
    let mut skills = Vec::new();

    if let Err(error) = scan_directory(root, "gemini", &mut skills) {
        tracing::warn!(?error, "failed to scan gemini skills directory");
    }
    if let Err(error) = scan_directory(root, "claude", &mut skills) {
        tracing::warn!(?error, "failed to scan claude skills directory");
    }

    skills.sort_by(|left, right| {
        left.assistant
            .cmp(&right.assistant)
            .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
            .then_with(|| left.name.cmp(&right.name))
    });

    skills
}

fn read_agent_skill_from_root(
    root: &Path,
    assistant: &str,
    name: &str,
) -> AppResult<AgentSkillContent> {
    let filename = format!("{name}.md");
    let path = agent_skill_path(root, assistant, name);
    let content = read_skill_file(&path, assistant, name)?;
    let parsed = parse_markdown_metadata(&content, name);

    Ok(AgentSkillContent {
        name: name.to_owned(),
        filename,
        assistant: assistant.to_owned(),
        title: parsed.title,
        description: parsed.description,
        instructions: parsed.instructions,
        content,
    })
}

fn write_new_agent_skill(
    root: &Path,
    assistant: &str,
    name: &str,
    title: &str,
    description: &str,
    instructions: &str,
) -> AppResult<AgentSkillContent> {
    let filename = format!("{name}.md");
    let path = agent_skill_path(root, assistant, name);
    if path.exists() {
        return Err(AppError::Conflict(format!(
            "Agent skill '{assistant}/{name}' already exists"
        )));
    }

    write_skill_file(&path, assistant, name, title, description, instructions, true)?;

    Ok(AgentSkillContent {
        name: name.to_owned(),
        filename,
        assistant: assistant.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        instructions: instructions.to_owned(),
        content: compose_skill_markdown(title, description, instructions),
    })
}

fn update_existing_agent_skill(
    root: &Path,
    assistant: &str,
    name: &str,
    title: &str,
    description: &str,
    instructions: &str,
) -> AppResult<AgentSkillContent> {
    let filename = format!("{name}.md");
    let path = agent_skill_path(root, assistant, name);
    if !path.exists() || !path.is_file() {
        return Err(AppError::NotFound {
            resource: format!("agent_skill:{assistant}/{name}"),
        });
    }

    write_skill_file(&path, assistant, name, title, description, instructions, false)?;

    Ok(AgentSkillContent {
        name: name.to_owned(),
        filename,
        assistant: assistant.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        instructions: instructions.to_owned(),
        content: compose_skill_markdown(title, description, instructions),
    })
}

fn write_skill_file(
    path: &Path,
    assistant: &str,
    name: &str,
    title: &str,
    description: &str,
    instructions: &str,
    create_new: bool,
) -> AppResult<()> {
    let Some(parent) = path.parent() else {
        return Err(AppError::Internal(anyhow::anyhow!(
            "Skill path missing parent directory for {assistant}/{name}"
        )));
    };

    fs::create_dir_all(parent).map_err(|error| {
        AppError::Internal(anyhow::anyhow!(
            "Failed to create skills directory for {assistant}/{name}: {error}"
        ))
    })?;

    let content = compose_skill_markdown(title, description, instructions);

    if create_new {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| {
                AppError::Internal(anyhow::anyhow!(
                    "Failed to create skill file for {assistant}/{name}: {error}"
                ))
            })?;

        file.write_all(content.as_bytes()).map_err(|error| {
            AppError::Internal(anyhow::anyhow!(
                "Failed to write skill file for {assistant}/{name}: {error}"
            ))
        })?;
    } else {
        fs::write(path, content).map_err(|error| {
            AppError::Internal(anyhow::anyhow!(
                "Failed to update skill file for {assistant}/{name}: {error}"
            ))
        })?;
    }

    Ok(())
}

fn read_skill_file(path: &Path, assistant: &str, name: &str) -> AppResult<String> {
    if !path.exists() || !path.is_file() {
        return Err(AppError::NotFound {
            resource: format!("agent_skill:{assistant}/{name}"),
        });
    }

    fs::read_to_string(path).map_err(|error| {
        AppError::Internal(anyhow::anyhow!(
            "Failed to read skill file for {assistant}/{name}: {error}"
        ))
    })
}

fn scan_directory(root: &Path, assistant: &str, list: &mut Vec<AgentSkill>) -> std::io::Result<()> {
    let dir_path = agent_skill_dir(root, assistant);
    if !dir_path.exists() || !dir_path.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(dir_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "md") {
            let Some(name) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            let filename = path
                .file_name()
                .and_then(|value| value.to_str())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{name}.md"));

            if let Ok(content) = fs::read_to_string(&path) {
                let parsed = parse_markdown_metadata(&content, name);
                list.push(AgentSkill {
                    name: name.to_owned(),
                    filename,
                    assistant: assistant.to_owned(),
                    title: parsed.title,
                    description: parsed.description,
                });
            }
        }
    }

    Ok(())
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
        description = format!(
            "Instructions on how to configure and run the {title} agent skill."
        );
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
        format!(
            "# Skill: {title}\n\n**Description:** {description}\n\n{trimmed_instructions}\n"
        )
    }
}

fn agent_skill_dir(root: &Path, assistant: &str) -> PathBuf {
    root.join(format!(".{assistant}")).join("skills")
}

fn agent_skill_path(root: &Path, assistant: &str, name: &str) -> PathBuf {
    agent_skill_dir(root, assistant).join(format!("{name}.md"))
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
        return Err(AppError::Validation(format!("{label} must be a single line")));
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

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn create_update_and_read_agent_skill_round_trip() {
        let root = create_test_root();

        let created = write_new_agent_skill(
            &root,
            "claude",
            "release_notes",
            "Release Notes",
            "Summarises changes",
            "## Trigger\n- On deploy",
        )
        .expect("skill should be created");

        assert_eq!(created.filename, "release_notes.md");
        assert_eq!(created.title, "Release Notes");

        let listed = list_agent_skills_under_root(&root);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "release_notes");

        let updated = update_existing_agent_skill(
            &root,
            "claude",
            "release_notes",
            "Release Notes",
            "Summarises release work",
            "## Trigger\n- Before deploy",
        )
        .expect("skill should update");

        assert_eq!(updated.description, "Summarises release work");

        let read_back = read_agent_skill_from_root(&root, "claude", "release_notes")
            .expect("skill should be readable");
        assert_eq!(read_back.instructions, "## Trigger\n- Before deploy");
        assert!(read_back.content.contains("Summarises release work"));

        remove_test_root(&root);
    }

    #[test]
    fn duplicate_skill_create_returns_conflict() {
        let root = create_test_root();

        write_new_agent_skill(
            &root,
            "gemini",
            "incident_brief",
            "Incident Brief",
            "Drafts an incident brief",
            "## Trigger\n- On incident",
        )
        .expect("first create should succeed");

        let error = write_new_agent_skill(
            &root,
            "gemini",
            "incident_brief",
            "Incident Brief",
            "Drafts an incident brief",
            "## Trigger\n- On incident",
        )
        .expect_err("duplicate create should fail");

        assert!(matches!(error, AppError::Conflict(_)));

        remove_test_root(&root);
    }

    fn create_test_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "memoryops-agent-skills-{}",
            Uuid::now_v7()
        ));
        fs::create_dir_all(&root).expect("test root should be created");
        root
    }

    fn remove_test_root(root: &Path) {
        let _ = fs::remove_dir_all(root);
    }
}
