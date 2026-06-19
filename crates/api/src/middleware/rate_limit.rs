use std::{
    net::{IpAddr, SocketAddr},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::anyhow;
use axum::extract::connect_info::ConnectInfo;
use axum::{body::Body, extract::State, http::Request, middleware::Next, response::Response};
use common::{auth::AuthContext, config::RateLimitConfig, error::AppResult, AppError, AppState};
use tokio::time::{timeout, Duration};

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

    /// Look up the RPM limit from the live config rather than hardcoded constants.
    fn limit(self, cfg: &RateLimitConfig) -> i64 {
        match self {
            RateLimitGroup::Ingest => i64::from(cfg.ingest_rpm),
            RateLimitGroup::Memory => i64::from(cfg.retrieve_rpm),
            RateLimitGroup::Api => i64::from(cfg.api_rpm),
            RateLimitGroup::Dashboard => i64::from(cfg.dashboard_rpm),
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
    let Some(subject) = rate_limit_subject(&request, &state) else {
        return Ok(next.run(request).await);
    };

    let limit = group.limit(&state.config.rate_limit);
    enforce_limit(&state, subject, group, limit).await?;
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
    limit: i64,
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

    if count > limit {
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

    // Dashboard read-only routes — separate bucket so continuous UI polling
    // cannot exhaust the workspace's general API quota.
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

fn rate_limit_subject(request: &Request<Body>, state: &AppState) -> Option<RateLimitSubject> {
    if let Some(workspace_id) = workspace_id_from_auth_context(request) {
        return Some(RateLimitSubject::Workspace(workspace_id));
    }

    match resolve_client_ip(request, &state.trusted_proxy_cidrs) {
        Some(ip) => Some(RateLimitSubject::Ip(ip)),
        None => {
            tracing::warn!("rate limit skipped because client IP was unavailable");
            None
        }
    }
}

fn workspace_id_from_auth_context(request: &Request<Body>) -> Option<uuid::Uuid> {
    request
        .extensions()
        .get::<AuthContext>()
        .map(|ctx| ctx.workspace_id)
}

/// Resolve the real client IP, honoring `X-Forwarded-For` only when the
/// direct peer is in the trusted-proxy CIDR list.
pub fn resolve_client_ip(
    request: &Request<Body>,
    trusted_proxy_cidrs: &[(IpAddr, u8)],
) -> Option<IpAddr> {
    let peer_ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip())
        .or_else(|| request.extensions().get::<SocketAddr>().map(SocketAddr::ip))?;

    if is_trusted_proxy(peer_ip, trusted_proxy_cidrs) {
        // Honor the leftmost (client) address in X-Forwarded-For
        if let Some(xff) = request.headers().get("x-forwarded-for") {
            if let Ok(raw) = xff.to_str() {
                if let Some(first) = raw.split(',').next() {
                    if let Ok(ip) = first.trim().parse::<IpAddr>() {
                        return Some(ip);
                    }
                }
            }
        }
        // Fall back to X-Real-IP
        if let Some(xri) = request.headers().get("x-real-ip") {
            if let Ok(raw) = xri.to_str() {
                if let Ok(ip) = raw.trim().parse::<IpAddr>() {
                    return Some(ip);
                }
            }
        }
    }

    // Untrusted peer: use the direct connection IP, ignoring any XFF header.
    Some(peer_ip)
}

/// Returns true if `peer` falls within any of the trusted CIDR ranges.
fn is_trusted_proxy(peer: IpAddr, cidrs: &[(IpAddr, u8)]) -> bool {
    cidrs
        .iter()
        .any(|(net_addr, prefix_len)| ip_in_cidr(peer, *net_addr, *prefix_len))
}

/// Constant-time CIDR membership check without external crates in this module.
/// Parsing is done in `main.rs` using `ipnet`.
fn ip_in_cidr(ip: IpAddr, network: IpAddr, prefix_len: u8) -> bool {
    match (ip, network) {
        (IpAddr::V4(ip), IpAddr::V4(net)) => {
            let shift = 32u32.saturating_sub(u32::from(prefix_len));
            let mask = if prefix_len == 0 {
                0u32
            } else {
                u32::MAX << shift
            };
            u32::from(ip) & mask == u32::from(net) & mask
        }
        (IpAddr::V6(ip), IpAddr::V6(net)) => {
            let shift = 128u32.saturating_sub(u32::from(prefix_len));
            let mask = if prefix_len == 0 {
                0u128
            } else {
                u128::MAX << shift
            };
            u128::from(ip) & mask == u128::from(net) & mask
        }
        _ => false,
    }
}

fn unix_timestamp_secs() -> AppResult<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| AppError::Internal(anyhow!(error)))?;
    i64::try_from(duration.as_secs()).map_err(|error| AppError::Internal(anyhow!(error)))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use axum::http::Request as HttpRequest;
    use common::config::RateLimitConfig;

    fn test_cfg(ingest: u32, retrieve: u32, api: u32, dashboard: u32) -> RateLimitConfig {
        RateLimitConfig {
            ingest_rpm: ingest,
            retrieve_rpm: retrieve,
            api_rpm: api,
            dashboard_rpm: dashboard,
        }
    }

    fn request_with_peer(peer: SocketAddr, xff: Option<&str>) -> Request<Body> {
        let mut builder = HttpRequest::builder().uri("/v1/workspaces");
        if let Some(xff) = xff {
            builder = builder.header("x-forwarded-for", xff);
        }
        let mut request = builder.body(Body::empty()).expect("request should build");
        request.extensions_mut().insert(ConnectInfo(peer));
        request
    }

    #[test]
    fn endpoint_groups_follow_m6_defaults() {
        assert!(matches!(
            endpoint_group("/v1/ingest/github/018f1234-0000-7000-8000-000000000000"),
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

    // ── Config-driven limits ──────────────────────────────────────────────────

    #[test]
    fn rate_limit_group_uses_config_values() {
        let cfg = test_cfg(500, 75, 200, 800);
        assert_eq!(RateLimitGroup::Ingest.limit(&cfg), 500);
        assert_eq!(RateLimitGroup::Memory.limit(&cfg), 75);
        assert_eq!(RateLimitGroup::Api.limit(&cfg), 200);
        assert_eq!(RateLimitGroup::Dashboard.limit(&cfg), 800);
    }

    #[test]
    fn rate_limit_group_defaults_match_config_toml() {
        // Values mirror the defaults in config.toml so a regression is caught here.
        let cfg = test_cfg(300, 60, 120, 600);
        assert_eq!(RateLimitGroup::Ingest.limit(&cfg), 300);
        assert_eq!(RateLimitGroup::Memory.limit(&cfg), 60);
        assert_eq!(RateLimitGroup::Api.limit(&cfg), 120);
        assert_eq!(RateLimitGroup::Dashboard.limit(&cfg), 600);
    }

    // ── Trusted proxy / client IP ─────────────────────────────────────────────

    #[test]
    fn ip_in_cidr_matches_exact_host() {
        let network: IpAddr = "10.0.0.0".parse().unwrap();
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        assert!(ip_in_cidr(ip, network, 8));
    }

    #[test]
    fn ip_in_cidr_rejects_outside_range() {
        let network: IpAddr = "10.0.0.0".parse().unwrap();
        let ip: IpAddr = "192.168.1.1".parse().unwrap();
        assert!(!ip_in_cidr(ip, network, 8));
    }

    #[test]
    fn is_trusted_proxy_true_for_loopback_cidr() {
        let cidrs: Vec<(IpAddr, u8)> = vec![("127.0.0.1".parse().unwrap(), 32)];
        assert!(is_trusted_proxy("127.0.0.1".parse().unwrap(), &cidrs));
    }

    #[test]
    fn is_trusted_proxy_false_for_empty_list() {
        assert!(!is_trusted_proxy("10.0.0.1".parse().unwrap(), &[]));
    }

    #[test]
    fn resolve_client_ip_uses_xff_only_from_trusted_proxy() {
        let request = request_with_peer(
            SocketAddr::from(([127, 0, 0, 1], 12345)),
            Some("203.0.113.10, 127.0.0.1"),
        );
        let cidrs = vec![("127.0.0.1".parse().unwrap(), 32)];

        assert_eq!(
            resolve_client_ip(&request, &cidrs),
            Some("203.0.113.10".parse().unwrap())
        );
    }

    #[test]
    fn resolve_client_ip_ignores_spoofed_xff_from_untrusted_peer() {
        let request = request_with_peer(
            SocketAddr::from(([198, 51, 100, 20], 12345)),
            Some("203.0.113.10"),
        );
        let cidrs = vec![("127.0.0.1".parse().unwrap(), 32)];

        assert_eq!(
            resolve_client_ip(&request, &cidrs),
            Some("198.51.100.20".parse().unwrap())
        );
    }
}
