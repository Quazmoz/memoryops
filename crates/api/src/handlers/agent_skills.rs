use std::{fs, path::Path};
use axum::{extract::Path as AxumPath, Json};
use serde::{Deserialize, Serialize};
use common::error::AppResult;
use common::AppError;

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentSkill {
    pub name: String,
    pub filename: String,
    pub assistant: String, // "gemini" or "claude"
    pub title: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentSkillContent {
    pub name: String,
    pub filename: String,
    pub assistant: String,
    pub content: String,
}

#[axum::debug_handler]
pub async fn list_agent_skills() -> AppResult<Json<Vec<AgentSkill>>> {
    let mut skills = Vec::new();

    // Scan .gemini/skills
    if let Err(e) = scan_directory("gemini", &mut skills) {
        tracing::warn!("Failed to scan gemini skills directory: {:?}", e);
    }

    // Scan .claude/skills
    if let Err(e) = scan_directory("claude", &mut skills) {
        tracing::warn!("Failed to scan claude skills directory: {:?}", e);
    }

    Ok(Json(skills))
}

#[axum::debug_handler]
pub async fn get_agent_skill(
    AxumPath((assistant, name)): AxumPath<(String, String)>,
) -> AppResult<Json<AgentSkillContent>> {
    if assistant != "gemini" && assistant != "claude" {
        return Err(AppError::Validation("Assistant must be either 'gemini' or 'claude'".to_owned()));
    }

    // Sanitize name to prevent path traversal (alphanumeric and underscores only)
    if name.chars().any(|c| !c.is_ascii_alphanumeric() && c != '_') {
        return Err(AppError::Validation("Invalid skill name".to_owned()));
    }

    let filename = format!("{}.md", name);
    let path = Path::new(".")
        .join(format!(".{}", assistant))
        .join("skills")
        .join(&filename);

    if !path.exists() || !path.is_file() {
        return Err(AppError::NotFound {
            resource: format!("agent_skill:{}/{}", assistant, name),
        });
    }

    let content = fs::read_to_string(&path)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to read skill file: {}", e)))?;

    Ok(Json(AgentSkillContent {
        name,
        filename,
        assistant,
        content,
    }))
}

fn scan_directory(assistant: &str, list: &mut Vec<AgentSkill>) -> std::io::Result<()> {
    let dir_path = Path::new(".").join(format!(".{}", assistant)).join("skills");
    if !dir_path.exists() || !dir_path.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(dir_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().map_or(false, |ext| ext == "md") {
            let filename = path.file_name().unwrap().to_string_lossy().into_owned();
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            
            if let Ok(content) = fs::read_to_string(&path) {
                let (title, description) = parse_markdown_metadata(&content, &name);
                list.push(AgentSkill {
                    name,
                    filename,
                    assistant: assistant.to_owned(),
                    title,
                    description,
                });
            }
        }
    }

    Ok(())
}

fn parse_markdown_metadata(content: &str, fallback_name: &str) -> (String, String) {
    let mut title = fallback_name.to_owned();
    let mut description = String::new();

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("# Skill:") {
            title = line["# Skill:".len()..].trim().to_owned();
        } else if line.starts_with("# ") && title == fallback_name {
            title = line["# ".len()..].trim().to_owned();
        } else if line.starts_with("**Description:**") {
            description = line["**Description:**".len()..].trim().to_owned();
        }
    }

    if description.is_empty() {
        description = format!("Instructions on how to configure and run the {} agent skill.", title);
    }

    (title, description)
}
