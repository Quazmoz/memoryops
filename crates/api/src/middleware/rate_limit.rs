use std::{
    net::{IpAddr, SocketAddr},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::anyhow;
use axum::extract::connect_info::ConnectInfo;
use axum::{body::Body, extract::State, http::Request, middleware::Next, response::Response};
use common::{auth::AuthContext, error::AppResult, AppError, AppState};
use tokio::time::{timeout, Duration};

const INGEST_RPM: i64 = 300;
pub const MEMORY_RPM: i64 = 120;
const API_RPM: i64 = 120;
const DASHBOARD_RPM: i64 = 600;
const RATE_LIMIT_REDIS_TIMEOUT_MS: u64 = 2000;
const RATE_LIMIT_SCRIPT: &str = r#"
local count = redis.call('INCR', KEYS[1])
if count == 1 then
    redis.call('EXPIREAT', KEYS[1], ARGV[1])
end
return count
"#;

#[derive(Debug, Clone, Copy)]
enum RateLimitGroup {
    Ingest,
    Memory,
    Api,
    Dashboard,
}

impl RateLimitGroup {
    fn as_str(self) -> &'static str {
        match self {
            RateLimitGroup::Ingest => "ingest",
            RateLimitGroup::Memory => "memory",
            RateLimitGroup::Api => "api",
            RateLimitGroup::Dashboard => "dashboard",
        }
    }

    fn limit(self) -> i64 {
        match self {
            RateLimitGroup::Ingest => INGEST_RPM,
            RateLimitGroup::Memory => MEMORY_RPM,
            RateLimitGroup::Api => API_RPM,
            RateLimitGroup::Dashboard => DASHBOARD_RPM,
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
    let Some(subject) = rate_limit_subject(&request) else {
        return Ok(next.run(request).await);
    };

    enforce_limit(&state, subject, group).await?;
    Ok(next.run(request).await)
}

#[derive(Debug, Clone)]
enum RateLimitSubject {
    Workspace(uuid::Uuid),
    Ip(IpAddr),
}

impl RateLimitSubject {
    fn key(&self) -> String {
        match self {
            Self::Workspace(workspace_id) => format!("workspace:{workspace_id}"),
            Self::Ip(ip) => format!("ip:{ip}"),
        }
    }

    fn log_value(&self) -> String {
        self.key()
    }
}

async fn enforce_limit(
    state: &AppState,
    subject: RateLimitSubject,
    group: RateLimitGroup,
) -> AppResult<()> {
    let now = unix_timestamp_secs()?;
    let window_start = now - (now % 60);
    let expires_at = window_start + 60;
    let subject_key = subject.key();
    let key = format!("rate:{subject_key}:{}:{window_start}", group.as_str());
    let mut redis = match state.redis.get().await {
        Ok(conn) => conn,
        Err(error) => {
            tracing::error!(
                error = ?error,
                subject = %subject.log_value(),
                group = group.as_str(),
                "rate limit redis pool get failed; fail-closed"
            );
            return Err(AppError::RateLimited {
                retry_after_secs: 5,
            });
        }
    };
    let redis_result = timeout(
        Duration::from_millis(RATE_LIMIT_REDIS_TIMEOUT_MS),
        redis::Script::new(RATE_LIMIT_SCRIPT)
            .key(&key)
            .arg(expires_at)
            .invoke_async::<i64>(&mut *redis),
    )
    .await;
    let count = match redis_result {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => {
            tracing::error!(
                error = ?error,
                subject = %subject.log_value(),
                group = group.as_str(),
                "rate limit check failed; denying request (fail-closed)"
            );
            return Err(AppError::RateLimited {
                retry_after_secs: 5,
            });
        }
        Err(_timeout) => {
            tracing::error!(
                subject = %subject.log_value(),
                group = group.as_str(),
                timeout_ms = RATE_LIMIT_REDIS_TIMEOUT_MS,
                "rate limit check timed out; denying request (fail-closed)"
            );
            return Err(AppError::RateLimited {
                retry_after_secs: 5,
            });
        }
    };

    if count > group.limit() {
        let retry_after_secs = u64::try_from((expires_at - now).max(1)).unwrap_or(60);
        return Err(AppError::RateLimited { retry_after_secs });
    }

    Ok(())
}

fn endpoint_group(path: &str) -> Option<RateLimitGroup> {
    if path.starts_with("/v1/ingest/") {
        if path.starts_with("/v1/ingest/observation") {
            return Some(RateLimitGroup::Api);
        }
        return Some(RateLimitGroup::Ingest);
    }

    if path.starts_with("/v1/memory") || path.starts_with("/v1/retrieve") {
        return Some(RateLimitGroup::Memory);
    }

    // Dashboard read-only routes - separated from the general Api bucket
    // so that continuous polling from the UI doesn't exhaust workspace quota
    if path.starts_with("/v1/workspaces/") {
        let after_id = path
            .trim_start_matches("/v1/workspaces/")
            .split_once('/')
            .map(|x| x.1)
            .unwrap_or("");

        if matches!(after_id, "stats" | "metrics" | "contradictions/count")
            || after_id.starts_with("stats/history")
        {
            return Some(RateLimitGroup::Dashboard);
        }
    }

    if path.starts_with("/v1/workspaces") {
        return Some(RateLimitGroup::Api);
    }

    None
}

fn rate_limit_subject(request: &Request<Body>) -> Option<RateLimitSubject> {
    if let Some(workspace_id) = workspace_id_from_auth_context(request) {
        return Some(RateLimitSubject::Workspace(workspace_id));
    }

    match client_ip_from_request(request) {
        Some(ip) => Some(RateLimitSubject::Ip(ip)),
        None => {
            tracing::warn!("rate limit skipped because client IP was unavailable");
            None
        }
    }
}

fn workspace_id_from_auth_context(request: &Request<Body>) -> Option<uuid::Uuid> {
    if let Some(context) = request.extensions().get::<AuthContext>() {
        return Some(context.workspace_id);
    }

    None
}

fn client_ip_from_request(request: &Request<Body>) -> Option<IpAddr> {
    request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip())
        .or_else(|| request.extensions().get::<SocketAddr>().map(SocketAddr::ip))
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
        assert!(matches!(
            endpoint_group("/v1/ingest/observation"),
            Some(RateLimitGroup::Api)
        ));
    }

    #[test]
    fn dashboard_routes_use_dashboard_bucket() {
        let id = "018f1234-0000-0000-0000-000000000000";
        assert!(matches!(
            endpoint_group(&format!("/v1/workspaces/{id}/stats")),
            Some(RateLimitGroup::Dashboard)
        ));
        assert!(matches!(
            endpoint_group(&format!("/v1/workspaces/{id}/stats/history")),
            Some(RateLimitGroup::Dashboard)
        ));
        assert!(matches!(
            endpoint_group(&format!("/v1/workspaces/{id}/metrics")),
            Some(RateLimitGroup::Dashboard)
        ));
        assert!(matches!(
            endpoint_group(&format!("/v1/workspaces/{id}/contradictions/count")),
            Some(RateLimitGroup::Dashboard)
        ));
        assert!(matches!(
            endpoint_group(&format!("/v1/workspaces/{id}/keys")),
            Some(RateLimitGroup::Api)
        ));
        assert!(matches!(
            endpoint_group("/v1/workspaces"),
            Some(RateLimitGroup::Api)
        ));
    }
}
