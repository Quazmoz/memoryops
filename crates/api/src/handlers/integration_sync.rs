use anyhow::anyhow;
use axum::{extract::Path, extract::State, http::StatusCode, Extension, Json};
use chrono::{DateTime, Utc};
use common::{
    auth::AuthContext,
    error::AppResult,
    models::{EventType, Source},
    AppError, AppState,
};
use ingestion::{
    queue::{publish_raw_event_with_mode, PublishMode},
    store::{insert_raw_event, raw_event_needs_publish, NewRawEvent},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::security::decrypt_secret_legacy_or_current;

use super::require_workspace;

const DEFAULT_SYNC_LIMIT: usize = 25;
const MAX_SYNC_LIMIT: usize = 100;
const GITHUB_API_BASE: &str = "https://api.github.com";

#[derive(Debug, Deserialize)]
pub struct ConnectorSyncRequest {
    pub repo: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct ConnectorSyncResponse {
    pub source: Source,
    pub queued_events: usize,
    pub skipped_events: usize,
    pub status: &'static str,
    pub message: String,
}

#[axum::debug_handler]
pub async fn start_connector_sync(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((workspace_id, source)): Path<(Uuid, Source)>,
    Json(request): Json<ConnectorSyncRequest>,
) -> AppResult<(StatusCode, Json<ConnectorSyncResponse>)> {
    require_workspace(&auth, workspace_id)?;
    let response = match source {
        Source::GitHub => sync_github_issues(&state, workspace_id, request).await?,
        Source::Slack | Source::Jira | Source::Linear | Source::Observation => {
            return Err(AppError::Validation(format!(
                "API sync adapter for {source:?} is not implemented yet"
            )));
        }
    };

    Ok((StatusCode::ACCEPTED, Json(response)))
}

async fn sync_github_issues(
    state: &AppState,
    workspace_id: Uuid,
    request: ConnectorSyncRequest,
) -> AppResult<ConnectorSyncResponse> {
    let repo = normalize_github_repo(request.repo.as_deref())?;
    let limit = request
        .limit
        .unwrap_or(DEFAULT_SYNC_LIMIT)
        .clamp(1, MAX_SYNC_LIMIT);
    let credential = integration_api_credential(state, workspace_id, Source::GitHub).await?;
    let issues = fetch_github_issues(&credential, &repo, request.since, limit).await?;
    let mut redis = state
        .redis
        .get()
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?;
    let mut queued_events = 0usize;
    let mut skipped_events = 0usize;

    for item in issues {
        let Some(number) = item.get("number").and_then(Value::as_i64) else {
            skipped_events += 1;
            continue;
        };
        let is_pull_request = item.get("pull_request").is_some();
        let event_type = if is_pull_request {
            EventType::PullRequest
        } else {
            EventType::Issue
        };
        let actor = item
            .pointer("/user/login")
            .and_then(Value::as_str)
            .unwrap_or("github-api-sync")
            .to_owned();
        let occurred_at = github_item_time(&item).unwrap_or_else(Utc::now);
        let updated_marker = item
            .get("updated_at")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let payload = github_item_payload(&repo, &item, is_pull_request);
        let idempotency_key = format!(
            "github:api-sync:{workspace_id}:{repo}:{}:{number}:{updated_marker}",
            if is_pull_request {
                "pull_request"
            } else {
                "issue"
            }
        );

        let event = insert_raw_event(
            &state.db,
            &NewRawEvent {
                workspace_id,
                source: Source::GitHub,
                event_type,
                actor,
                payload,
                idempotency_key,
                occurred_at,
            },
        )
        .await?;

        if raw_event_needs_publish(&state.db, event.id).await? {
            publish_raw_event_with_mode(&mut *redis, &event, PublishMode::Strict).await?;
            queued_events += 1;
        } else {
            skipped_events += 1;
        }
    }

    update_sync_health(state, workspace_id, Source::GitHub, queued_events).await?;

    Ok(ConnectorSyncResponse {
        source: Source::GitHub,
        queued_events,
        skipped_events,
        status: "queued",
        message: format!(
            "Queued {queued_events} GitHub API events from {repo}; skipped {skipped_events}."
        ),
    })
}

async fn fetch_github_issues(
    credential: &str,
    repo: &str,
    since: Option<DateTime<Utc>>,
    limit: usize,
) -> AppResult<Vec<Value>> {
    let mut url = format!(
        "{GITHUB_API_BASE}/repos/{repo}/issues?state=all&sort=updated&direction=desc&per_page={limit}"
    );
    if let Some(since) = since {
        url.push_str("&since=");
        url.push_str(&since.to_rfc3339());
    }

    let response = reqwest::Client::new()
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "MemoryOps/0.20")
        .bearer_auth(credential)
        .send()
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Validation(format!(
            "GitHub API sync failed with {status}: {}",
            truncate(&body, 300)
        )));
    }

    response
        .json::<Vec<Value>>()
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))
}

async fn integration_api_credential(
    state: &AppState,
    workspace_id: Uuid,
    source: Source,
) -> AppResult<String> {
    let encrypted = sqlx::query_scalar::<_, Option<String>>(
        r#"
        SELECT api_token_enc
        FROM integrations
        WHERE workspace_id = $1
          AND source = $2
          AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(source)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .flatten()
    .ok_or_else(|| {
        AppError::Validation("API credential is required before running API sync".to_owned())
    })?;

    let decrypted =
        decrypt_secret_legacy_or_current(state.app_secret_key.as_ref().as_str(), &encrypted)?;
    if let Some(migrated) = decrypted.migrated_ciphertext.as_deref() {
        sqlx::query(
            r#"
            UPDATE integrations
            SET api_token_enc = $3
            WHERE workspace_id = $1 AND source = $2
            "#,
        )
        .bind(workspace_id)
        .bind(source)
        .bind(migrated)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?;
    }

    Ok(decrypted.plaintext)
}

async fn update_sync_health(
    state: &AppState,
    workspace_id: Uuid,
    source: Source,
    queued_events: usize,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE integrations
        SET last_sync_at = now(), api_sync_enabled = true
        WHERE workspace_id = $1 AND source = $2
        "#,
    )
    .bind(workspace_id)
    .bind(source)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    sqlx::query(
        r#"
        INSERT INTO integration_health (workspace_id, source, last_event_at, events_24h, status)
        VALUES ($1, $2, now(), $3, 'active'::integration_status)
        ON CONFLICT (workspace_id, source) DO UPDATE
        SET last_event_at = CASE
                WHEN $3 > 0 THEN now()
                ELSE integration_health.last_event_at
            END,
            events_24h = integration_health.events_24h + $3,
            status = 'active'::integration_status
        "#,
    )
    .bind(workspace_id)
    .bind(source)
    .bind(queued_events as i64)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(())
}

fn normalize_github_repo(repo: Option<&str>) -> AppResult<String> {
    let repo = repo
        .map(str::trim)
        .filter(|repo| !repo.is_empty())
        .ok_or_else(|| AppError::Validation("repo is required for GitHub API sync".to_owned()))?;
    let mut parts = repo.split('/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    if owner.is_empty() || name.is_empty() || parts.next().is_some() {
        return Err(AppError::Validation(
            "repo must use owner/name format".to_owned(),
        ));
    }
    if repo.chars().any(char::is_whitespace) {
        return Err(AppError::Validation(
            "repo must not contain whitespace".to_owned(),
        ));
    }
    Ok(repo.to_owned())
}

fn github_item_payload(repo: &str, item: &Value, is_pull_request: bool) -> Value {
    if is_pull_request {
        json!({
            "action": "api_sync",
            "pull_request": item,
            "repository": { "full_name": repo },
            "sync": { "source": "api", "resource": "issues", "adapter": "github" }
        })
    } else {
        json!({
            "action": "api_sync",
            "issue": item,
            "repository": { "full_name": repo },
            "sync": { "source": "api", "resource": "issues", "adapter": "github" }
        })
    }
}

fn github_item_time(item: &Value) -> Option<DateTime<Utc>> {
    item.get("updated_at")
        .or_else(|| item.get("created_at"))
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut truncated = value.to_owned();
    if truncated.len() > max_chars {
        truncated.truncate(max_chars);
    }
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_github_repo_accepts_owner_name() {
        match normalize_github_repo(Some("Quazmoz/memoryops")) {
            Ok(repo) => assert_eq!(repo, "Quazmoz/memoryops"),
            Err(error) => panic!("repo should normalize: {error}"),
        }
    }

    #[test]
    fn normalize_github_repo_rejects_invalid_values() {
        assert!(normalize_github_repo(None).is_err());
        assert!(normalize_github_repo(Some("owner")).is_err());
        assert!(normalize_github_repo(Some("owner/repo/extra")).is_err());
        assert!(normalize_github_repo(Some("owner /repo")).is_err());
    }
}
