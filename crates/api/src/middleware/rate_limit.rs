use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::anyhow;
use axum::{body::Body, extract::State, http::Request, middleware::Next, response::Response};
use common::{auth::AuthContext, error::AppResult, AppError, AppState};
use uuid::Uuid;

const INGEST_RPM: i64 = 300;
const MEMORY_RPM: i64 = 120;
const API_RPM: i64 = 120;

#[derive(Debug, Clone, Copy)]
enum RateLimitGroup {
    Ingest,
    Memory,
    Api,
}

impl RateLimitGroup {
    fn as_str(self) -> &'static str {
        match self {
            RateLimitGroup::Ingest => "ingest",
            RateLimitGroup::Memory => "memory",
            RateLimitGroup::Api => "api",
        }
    }

    fn limit(self) -> i64 {
        match self {
            RateLimitGroup::Ingest => INGEST_RPM,
            RateLimitGroup::Memory => MEMORY_RPM,
            RateLimitGroup::Api => API_RPM,
        }
    }
}

pub async fn rate_limit(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> AppResult<Response> {
    let path = request.uri().path();
    let Some(group) = endpoint_group(path) else {
        return Ok(next.run(request).await);
    };
    let Some(workspace_id) = workspace_id_from_request(&request) else {
        return Ok(next.run(request).await);
    };

    enforce_limit(&state, workspace_id, group).await?;
    Ok(next.run(request).await)
}

async fn enforce_limit(
    state: &AppState,
    workspace_id: Uuid,
    group: RateLimitGroup,
) -> AppResult<()> {
    let now = unix_timestamp_secs()?;
    let window_start = now - (now % 60);
    let expires_at = window_start + 60;
    let key = format!("rate:{workspace_id}:{}:{window_start}", group.as_str());
    let mut redis = state.redis.clone();
    let (count, _expires_set) = redis::pipe()
        .cmd("INCR")
        .arg(&key)
        .cmd("EXPIREAT")
        .arg(&key)
        .arg(expires_at)
        .query_async::<(i64, bool)>(&mut redis)
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?;

    if count > group.limit() {
        let retry_after_secs = u64::try_from((expires_at - now).max(1)).unwrap_or(60);
        return Err(AppError::RateLimited { retry_after_secs });
    }

    Ok(())
}

fn endpoint_group(path: &str) -> Option<RateLimitGroup> {
    if path.starts_with("/v1/ingest/") {
        Some(RateLimitGroup::Ingest)
    } else if path.starts_with("/v1/memory") || path.starts_with("/v1/retrieve") {
        Some(RateLimitGroup::Memory)
    } else if path.starts_with("/v1/workspaces") {
        Some(RateLimitGroup::Api)
    } else {
        None
    }
}

fn workspace_id_from_request(request: &Request<Body>) -> Option<Uuid> {
    if let Some(context) = request.extensions().get::<AuthContext>() {
        return Some(context.workspace_id);
    }
    if let Some(header) = request.headers().get("x-workspace-id") {
        if let Ok(raw) = header.to_str() {
            if let Ok(workspace_id) = Uuid::parse_str(raw) {
                return Some(workspace_id);
            }
        }
    }

    request.uri().query().and_then(workspace_id_from_query)
}

fn workspace_id_from_query(query: &str) -> Option<Uuid> {
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        if name == "workspace_id" {
            Uuid::parse_str(value).ok()
        } else {
            None
        }
    })
}

fn unix_timestamp_secs() -> AppResult<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| AppError::Internal(anyhow!(error)))?;
    i64::try_from(duration.as_secs()).map_err(|error| AppError::Internal(anyhow!(error)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_groups_follow_m6_defaults() {
        assert!(matches!(
            endpoint_group("/v1/ingest/github"),
            Some(RateLimitGroup::Ingest)
        ));
        assert!(matches!(
            endpoint_group("/v1/memory/search"),
            Some(RateLimitGroup::Memory)
        ));
        assert!(matches!(
            endpoint_group("/v1/retrieve"),
            Some(RateLimitGroup::Memory)
        ));
        assert!(matches!(
            endpoint_group("/v1/workspaces"),
            Some(RateLimitGroup::Api)
        ));
    }
}
