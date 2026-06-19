//! Production audit logging.
//!
//! Two write paths are exposed, with deliberately different reliability:
//!
//! * **Required path** — [`write_audit`] / [`write_audit_in_conn`]. Synchronous,
//!   error-returning. Use for security/compliance-sensitive actions (key
//!   lifecycle, secret reveal, config/integration changes, erasure, hard
//!   delete). Call it inside the business transaction, or immediately after and
//!   propagate the error so the operation fails if the audit row cannot be
//!   written. These events are **never silently dropped**.
//!
//! * **Best-effort path** — [`spawn_audit_event`] / [`spawn_audit_log`]. Async,
//!   non-blocking. Use for high-volume operational events (embedding,
//!   observation ingest, tool invocation, scheduler maintenance). It does *not*
//!   drop on a full in-memory queue: writes acquire a bounded permit by
//!   awaiting it, and on write failure the (already redacted) event is enqueued
//!   to the durable `audit_outbox` table for the background drainer to retry.
//!
//! All payloads (`before`/`after`/`metadata`/`diff`) are recursively redacted
//! and size-bounded before persistence — see [`redact_json`] — so secrets never
//! reach the audit log or the outbox.
//!
//! Rows are chained with an HMAC-SHA256 hash keyed by `AUDIT_SIGNING_KEY` (or
//! `APP_SECRET_KEY`), giving tamper-evidence (not tamper-proofing) verifiable
//! through [`verify_audit_chain`].

use std::sync::{Arc, OnceLock};

use chrono::{DateTime, SecondsFormat, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::Sha256;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{auth::AuthContext, models::AuditAction};

type HmacSha256 = Hmac<Sha256>;

/// Sentinel `prev_hash` for the first hashed row in a workspace chain.
pub const AUDIT_CHAIN_GENESIS: &str = "GENESIS";

/// Hash input schema version, mixed into every hash so a future format change
/// can be distinguished during verification.
const AUDIT_HASH_VERSION: u8 = 1;

/// Substrings (case-insensitive) that mark a JSON key as sensitive. Any matching
/// field's value is replaced with [`REDACTED`] before persistence.
const SENSITIVE_KEY_SUBSTRINGS: &[&str] = &[
    "secret",
    "token",
    "password",
    "passwd",
    "credential",
    "authorization",
    "auth_secret",
    "auth_token",
    "bearer",
    "cookie",
    "api_key",
    "apikey",
    "webhook_secret",
    "plaintext_secret",
    "connection_string",
    "connectionstring",
    "database_url",
    "private_key",
    "access_key",
    "client_secret",
    "session_token",
    // Broad, per audit policy: any field name containing "key" is redacted.
    "key",
];

/// Replacement marker for redacted scalar values.
pub const REDACTED: &str = "[REDACTED]";

/// Maximum length of a single string value before it is summarised.
const MAX_STRING_LEN: usize = 1024;
/// Maximum number of array elements retained before truncation.
const MAX_ARRAY_LEN: usize = 256;
/// Maximum recursion depth before a subtree is collapsed.
const MAX_DEPTH: usize = 12;
/// Maximum serialized byte size of a full redacted payload before it is replaced
/// with a summary object.
const MAX_PAYLOAD_BYTES: usize = 32 * 1024;
/// Bounded concurrency for best-effort audit writes. Permits are *awaited*, not
/// `try_acquire`d, so a busy system applies backpressure instead of dropping.
const AUDIT_BEST_EFFORT_PERMITS: usize = 128;
/// Outbox retry backoff cap.
const OUTBOX_MAX_ATTEMPTS_BACKOFF_SECS: i64 = 300;

static AUDIT_PERMITS: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
static AUDIT_SIGNING_KEY: OnceLock<Option<Zeroizing<Vec<u8>>>> = OnceLock::new();

// ─────────────────────────────────────────────────────────────────────────────
// Request context
// ─────────────────────────────────────────────────────────────────────────────

/// Per-request context captured by middleware and threaded into audit events so
/// rows are traceable back to the originating HTTP request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequestContext {
    pub request_id: Option<String>,
    pub correlation_id: Option<String>,
    /// Real client IP, resolved with trusted-proxy rules. Never the raw
    /// `X-Forwarded-For` value unless the peer is a trusted proxy.
    pub source_ip: Option<String>,
    pub user_agent: Option<String>,
    pub method: Option<String>,
    pub route: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// AuditEvent
// ─────────────────────────────────────────────────────────────────────────────

/// A fully-described audit event prior to persistence. Build with [`AuditEvent::new`]
/// and the chaining setters, then hand to [`write_audit`] (required) or
/// [`spawn_audit_event`] (best-effort).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub occurred_at: DateTime<Utc>,
    pub actor: String,
    pub action: AuditAction,
    pub target_id: Uuid,
    pub target_type: String,

    pub actor_type: Option<String>,
    pub actor_id: Option<String>,
    pub actor_display: Option<String>,
    pub api_key_id: Option<Uuid>,
    pub api_key_prefix: Option<String>,

    pub request_id: Option<String>,
    pub correlation_id: Option<String>,
    pub source_ip: Option<String>,
    pub user_agent: Option<String>,
    pub route: Option<String>,
    pub method: Option<String>,
    pub status_code: Option<i32>,
    pub reason: Option<String>,

    pub target_name: Option<String>,
    pub target_version: Option<i32>,

    pub severity: Option<String>,
    pub category: Option<String>,
    pub success: bool,
    pub error_code: Option<String>,

    pub before: Option<Value>,
    pub after: Option<Value>,
    pub metadata: Option<Value>,
    pub diff: Option<Value>,
}

impl AuditEvent {
    pub fn new(
        workspace_id: Uuid,
        action: AuditAction,
        target_id: Uuid,
        target_type: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            workspace_id,
            occurred_at: now_micros(),
            actor: "system".to_owned(),
            action,
            target_id,
            target_type: target_type.into(),
            actor_type: None,
            actor_id: None,
            actor_display: None,
            api_key_id: None,
            api_key_prefix: None,
            request_id: None,
            correlation_id: None,
            source_ip: None,
            user_agent: None,
            route: None,
            method: None,
            status_code: None,
            reason: None,
            target_name: None,
            target_version: None,
            severity: None,
            category: None,
            success: true,
            error_code: None,
            before: None,
            after: None,
            metadata: None,
            diff: None,
        }
    }

    /// Set the actor from an authenticated API key context.
    pub fn actor_api_key(mut self, auth: &AuthContext) -> Self {
        self.actor = auth.actor();
        self.actor_type = Some("api_key".to_owned());
        self.actor_id = Some(auth.key_id.to_string());
        self.actor_display = Some(auth.actor());
        self.api_key_id = Some(auth.key_id);
        self.api_key_prefix = Some(auth.key_prefix.clone());
        self
    }

    /// Set a free-form actor string (used by the back-compat shim and system tasks).
    pub fn actor_string(mut self, actor: impl Into<String>) -> Self {
        let actor = actor.into();
        // Best-effort actor_type inference from the conventional `kind:id` form.
        if self.actor_type.is_none() {
            if let Some((kind, id)) = actor.split_once(':') {
                self.actor_type = Some(kind.to_owned());
                self.actor_id.get_or_insert_with(|| id.to_owned());
            }
        }
        self.actor = actor;
        self
    }

    pub fn actor_type(mut self, actor_type: impl Into<String>) -> Self {
        self.actor_type = Some(actor_type.into());
        self
    }

    /// Apply captured request context.
    pub fn request_context(mut self, ctx: &RequestContext) -> Self {
        self.request_id = ctx.request_id.clone();
        self.correlation_id = ctx.correlation_id.clone();
        self.source_ip = ctx.source_ip.clone();
        self.user_agent = ctx.user_agent.clone();
        self.method = ctx.method.clone();
        self.route = ctx.route.clone();
        self
    }

    /// Apply optional request context (convenience for handlers that receive
    /// `Option<RequestContext>`).
    pub fn maybe_request_context(self, ctx: Option<&RequestContext>) -> Self {
        match ctx {
            Some(ctx) => self.request_context(ctx),
            None => self,
        }
    }

    pub fn target_name(mut self, name: impl Into<String>) -> Self {
        self.target_name = Some(name.into());
        self
    }

    pub fn target_version(mut self, version: i32) -> Self {
        self.target_version = Some(version);
        self
    }

    pub fn before(mut self, value: Value) -> Self {
        self.before = Some(value);
        self
    }

    pub fn after(mut self, value: Value) -> Self {
        self.after = Some(value);
        self
    }

    pub fn metadata(mut self, value: Value) -> Self {
        self.metadata = Some(value);
        self
    }

    pub fn diff(mut self, value: Option<Value>) -> Self {
        self.diff = value;
        self
    }

    pub fn reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    pub fn severity(mut self, severity: impl Into<String>) -> Self {
        self.severity = Some(severity.into());
        self
    }

    pub fn status_code(mut self, code: i32) -> Self {
        self.status_code = Some(code);
        self
    }

    /// Mark this event as a failure with an error code.
    pub fn failure(mut self, error_code: impl Into<String>) -> Self {
        self.success = false;
        self.error_code = Some(error_code.into());
        self
    }

    fn resolved_severity(&self) -> String {
        self.severity
            .clone()
            .unwrap_or_else(|| self.action.default_severity().as_str().to_owned())
    }

    fn resolved_category(&self) -> String {
        self.category
            .clone()
            .unwrap_or_else(|| self.action.category().as_str().to_owned())
    }

    /// Redacted, size-bounded payloads as they will be persisted.
    fn redacted_payloads(&self) -> RedactedPayloads {
        RedactedPayloads {
            before: self.before.as_ref().map(redact_payload),
            after: self.after.as_ref().map(redact_payload),
            metadata: self.metadata.as_ref().map(redact_payload),
            diff: self.diff.as_ref().map(redact_payload),
        }
    }
}

struct RedactedPayloads {
    before: Option<Value>,
    after: Option<Value>,
    metadata: Option<Value>,
    diff: Option<Value>,
}

fn now_micros() -> DateTime<Utc> {
    let now = Utc::now();
    DateTime::from_timestamp_micros(now.timestamp_micros()).unwrap_or(now)
}

// ─────────────────────────────────────────────────────────────────────────────
// Redaction
// ─────────────────────────────────────────────────────────────────────────────

/// Returns true if a JSON key name should have its value redacted.
pub fn is_sensitive_key(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    SENSITIVE_KEY_SUBSTRINGS
        .iter()
        .any(|needle| lowered.contains(needle))
}

/// Recursively redact a JSON value: sensitive keys are masked, long strings and
/// large arrays are summarised, and excessive depth is collapsed. Idempotent —
/// re-redacting already-redacted output is a no-op.
pub fn redact_json(value: &Value) -> Value {
    redact_value(value, 0)
}

/// Redact and then enforce a maximum serialized payload size.
fn redact_payload(value: &Value) -> Value {
    let redacted = redact_json(value);
    match serde_json::to_vec(&redacted) {
        Ok(bytes) if bytes.len() > MAX_PAYLOAD_BYTES => json!({
            "truncated": true,
            "length": bytes.len(),
            "reason": "payload exceeds audit size limit",
        }),
        _ => redacted,
    }
}

fn redact_value(value: &Value, depth: usize) -> Value {
    if depth >= MAX_DEPTH {
        return json!({ "truncated": true, "reason": "max depth" });
    }
    match value {
        Value::Object(map) => {
            let mut out = Map::with_capacity(map.len());
            for (key, val) in map {
                if is_sensitive_key(key) {
                    out.insert(key.clone(), Value::String(REDACTED.to_owned()));
                } else {
                    out.insert(key.clone(), redact_value(val, depth + 1));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => {
            let truncated = items.len() > MAX_ARRAY_LEN;
            let mut out: Vec<Value> = items
                .iter()
                .take(MAX_ARRAY_LEN)
                .map(|item| redact_value(item, depth + 1))
                .collect();
            if truncated {
                out.push(json!({
                    "truncated": true,
                    "omitted": items.len() - MAX_ARRAY_LEN,
                }));
            }
            Value::Array(out)
        }
        Value::String(text) if text.chars().count() > MAX_STRING_LEN => {
            let preview: String = text.chars().take(64).collect();
            json!({
                "truncated": true,
                "length": text.chars().count(),
                "preview": preview,
            })
        }
        other => other.clone(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Hashing / tamper-evidence
// ─────────────────────────────────────────────────────────────────────────────

/// Resolve the audit signing key from the environment, once. Prefers a
/// dedicated `AUDIT_SIGNING_KEY`; falls back to `APP_SECRET_KEY`. When neither is
/// set, hashing is disabled (rows are still written; `seq`/`hash` are NULL).
fn audit_signing_key() -> Option<&'static Zeroizing<Vec<u8>>> {
    AUDIT_SIGNING_KEY
        .get_or_init(|| {
            let resolved = std::env::var("AUDIT_SIGNING_KEY")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| {
                    std::env::var("APP_SECRET_KEY")
                        .ok()
                        .filter(|value| !value.trim().is_empty())
                });
            match resolved {
                Some(key) => Some(Zeroizing::new(key.into_bytes())),
                None => {
                    tracing::warn!(
                        "AUDIT_SIGNING_KEY / APP_SECRET_KEY unset; audit hash chain disabled"
                    );
                    None
                }
            }
        })
        .as_ref()
}

/// Whether the tamper-evident hash chain is active.
pub fn audit_hashing_enabled() -> bool {
    audit_signing_key().is_some()
}

/// Build the canonical object that is hashed for a row. Used identically at write
/// and verify time so recomputation is deterministic.
#[allow(clippy::too_many_arguments)]
fn hash_canonical_object(
    seq: i64,
    workspace_id: Uuid,
    id: Uuid,
    prev_hash: &str,
    occurred_at: &str,
    actor: &str,
    action: &str,
    target_type: &str,
    target_id: Uuid,
    success: bool,
    severity: &str,
    category: &str,
    before: &Option<Value>,
    after: &Option<Value>,
    metadata: &Option<Value>,
    diff: &Option<Value>,
) -> Value {
    json!({
        "v": AUDIT_HASH_VERSION,
        "seq": seq,
        "workspace_id": workspace_id,
        "id": id,
        "prev_hash": prev_hash,
        "occurred_at": occurred_at,
        "actor": actor,
        "action": action,
        "target_type": target_type,
        "target_id": target_id,
        "success": success,
        "severity": severity,
        "category": category,
        "before": before,
        "after": after,
        "metadata": metadata,
        "diff": diff,
    })
}

/// Deterministic, key-sorted JSON serialization (independent of serde_json's
/// `preserve_order` feature) so hashes are stable across processes.
pub fn canonical_json_string(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => out.push_str(&escape_json_string(s)),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&escape_json_string(key));
                out.push(':');
                if let Some(val) = map.get(*key) {
                    write_canonical(val, out);
                }
            }
            out.push('}');
        }
    }
}

fn escape_json_string(s: &str) -> String {
    // Delegate to serde_json for correct escaping of a standalone string.
    Value::String(s.to_owned()).to_string()
}

fn hmac_hex(key: &[u8], message: &str) -> String {
    let mut mac = match HmacSha256::new_from_slice(key) {
        Ok(mac) => mac,
        // HMAC accepts keys of any length; this branch is unreachable in practice.
        Err(_) => return String::new(),
    };
    mac.update(message.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

// ─────────────────────────────────────────────────────────────────────────────
// Reliable (required) write path
// ─────────────────────────────────────────────────────────────────────────────

/// Write an audit row reliably, opening its own transaction. Returns the row id.
///
/// Use for security/compliance-sensitive actions and propagate the error so the
/// business operation fails if the audit row cannot be persisted.
pub async fn write_audit(pool: &PgPool, event: &AuditEvent) -> Result<Uuid, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let id = write_audit_in_conn(&mut tx, event).await?;
    tx.commit().await?;
    Ok(id)
}

/// Write an audit row using a caller-managed connection/transaction. The caller
/// MUST be inside a transaction (the hash chain takes a transaction-scoped
/// advisory lock to serialize per-workspace sequencing).
pub async fn write_audit_in_conn(
    conn: &mut PgConnection,
    event: &AuditEvent,
) -> Result<Uuid, sqlx::Error> {
    let payloads = event.redacted_payloads();
    let severity = event.resolved_severity();
    let category = event.resolved_category();
    let occurred_at_str = event
        .occurred_at
        .to_rfc3339_opts(SecondsFormat::Micros, true);

    let (seq, prev_hash, hash) = if let Some(key) = audit_signing_key() {
        // Serialize sequencing per workspace for the duration of this tx.
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(advisory_key(event.workspace_id))
            .execute(&mut *conn)
            .await?;

        let last: Option<(i64, Option<String>)> = sqlx::query_as(
            "SELECT seq, hash FROM audit_log \
             WHERE workspace_id = $1 AND seq IS NOT NULL \
             ORDER BY seq DESC LIMIT 1",
        )
        .bind(event.workspace_id)
        .fetch_optional(&mut *conn)
        .await?;

        let (prev_seq, prev_hash) = match last {
            Some((prev_seq, prev_hash)) => (prev_seq, prev_hash),
            None => (0, None),
        };
        let seq = prev_seq + 1;
        let prev_for_chain = prev_hash.unwrap_or_else(|| AUDIT_CHAIN_GENESIS.to_owned());
        let canonical = canonical_json_string(&hash_canonical_object(
            seq,
            event.workspace_id,
            event.id,
            &prev_for_chain,
            &occurred_at_str,
            &event.actor,
            event.action.as_str(),
            &event.target_type,
            event.target_id,
            event.success,
            &severity,
            &category,
            &payloads.before,
            &payloads.after,
            &payloads.metadata,
            &payloads.diff,
        ));
        let hash = hmac_hex(key.as_slice(), &canonical);
        (Some(seq), Some(prev_for_chain), Some(hash))
    } else {
        (None, None, None)
    };

    sqlx::query(
        r#"
        INSERT INTO audit_log (
            id, workspace_id, actor, action, target_id, target_type, diff, occurred_at,
            request_id, correlation_id, actor_type, actor_id, actor_display, api_key_id,
            api_key_prefix, source_ip, user_agent, route, method, status_code, reason,
            target_name, target_version, severity, category, success, error_code,
            before, after, metadata, seq, prev_hash, hash
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8,
            $9, $10, $11, $12, $13, $14,
            $15, $16, $17, $18, $19, $20, $21,
            $22, $23, $24, $25, $26, $27,
            $28, $29, $30, $31, $32, $33
        )
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(event.id)
    .bind(event.workspace_id)
    .bind(&event.actor)
    .bind(event.action)
    .bind(event.target_id)
    .bind(&event.target_type)
    .bind(&payloads.diff)
    .bind(event.occurred_at)
    .bind(&event.request_id)
    .bind(&event.correlation_id)
    .bind(&event.actor_type)
    .bind(&event.actor_id)
    .bind(&event.actor_display)
    .bind(event.api_key_id)
    .bind(&event.api_key_prefix)
    .bind(&event.source_ip)
    .bind(&event.user_agent)
    .bind(&event.route)
    .bind(&event.method)
    .bind(event.status_code)
    .bind(&event.reason)
    .bind(&event.target_name)
    .bind(event.target_version)
    .bind(&severity)
    .bind(&category)
    .bind(event.success)
    .bind(&event.error_code)
    .bind(&payloads.before)
    .bind(&payloads.after)
    .bind(&payloads.metadata)
    .bind(seq)
    .bind(&prev_hash)
    .bind(&hash)
    .execute(&mut *conn)
    .await?;

    Ok(event.id)
}

/// Stable 64-bit advisory-lock key derived from a workspace id.
fn advisory_key(workspace_id: Uuid) -> i64 {
    let bytes = workspace_id.as_bytes();
    let mut buf = [0_u8; 8];
    buf.copy_from_slice(&bytes[0..8]);
    i64::from_be_bytes(buf)
}

// ─────────────────────────────────────────────────────────────────────────────
// Best-effort write path (+ durable outbox fallback)
// ─────────────────────────────────────────────────────────────────────────────

fn audit_permits() -> Arc<tokio::sync::Semaphore> {
    AUDIT_PERMITS
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(AUDIT_BEST_EFFORT_PERMITS)))
        .clone()
}

/// Best-effort, non-blocking audit write. Never drops on a full queue: it awaits
/// a bounded permit and, if the direct write fails, enqueues the (redacted) event
/// to the durable outbox for retry.
pub fn spawn_audit_event(db: PgPool, event: AuditEvent) {
    let permits = audit_permits();
    tokio::spawn(async move {
        let _permit = permits.acquire_owned().await;
        if let Err(error) = write_audit(&db, &event).await {
            tracing::warn!(
                error = ?error,
                workspace_id = %event.workspace_id,
                action = %event.action.as_str(),
                "best-effort audit write failed; enqueuing to outbox"
            );
            if let Err(outbox_error) = enqueue_outbox(&db, &event).await {
                tracing::error!(
                    error = ?outbox_error,
                    workspace_id = %event.workspace_id,
                    action = %event.action.as_str(),
                    "failed to enqueue audit event to outbox; event lost"
                );
            }
        }
    });
}

/// Backward-compatible best-effort shim matching the original signature. New code
/// should prefer [`spawn_audit_event`] with full context.
pub fn spawn_audit_log(
    db: PgPool,
    workspace_id: Uuid,
    actor: String,
    action: AuditAction,
    target_id: Uuid,
    target_type: impl Into<String>,
    diff: Option<serde_json::Value>,
) {
    let event = AuditEvent::new(workspace_id, action, target_id, target_type)
        .actor_string(actor)
        .diff(diff);
    spawn_audit_event(db, event);
}

/// Persist a redacted snapshot of an event to the durable outbox.
pub async fn enqueue_outbox(pool: &PgPool, event: &AuditEvent) -> Result<(), sqlx::Error> {
    // Store an already-redacted snapshot so the outbox never holds secrets.
    let payloads = event.redacted_payloads();
    let mut snapshot = event.clone();
    snapshot.before = payloads.before;
    snapshot.after = payloads.after;
    snapshot.metadata = payloads.metadata;
    snapshot.diff = payloads.diff;

    let payload = serde_json::to_value(&snapshot).map_err(|error| {
        sqlx::Error::Encode(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            error,
        )))
    })?;

    sqlx::query(
        "INSERT INTO audit_outbox (id, workspace_id, payload) VALUES ($1, $2, $3) \
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(Uuid::now_v7())
    .bind(event.workspace_id)
    .bind(payload)
    .execute(pool)
    .await
    .map(|_| ())
}

/// Drain due outbox rows into `audit_log`. Returns the number of events written.
/// Failures bump the attempt counter with linear backoff and are retried later.
pub async fn drain_audit_outbox(pool: &PgPool, batch_size: i64) -> Result<usize, sqlx::Error> {
    let rows: Vec<(Uuid, Value, i32)> = sqlx::query_as(
        "SELECT id, payload, attempts FROM audit_outbox \
         WHERE next_attempt_at <= now() ORDER BY created_at LIMIT $1",
    )
    .bind(batch_size)
    .fetch_all(pool)
    .await?;

    let mut written = 0_usize;
    for (outbox_id, payload, attempts) in rows {
        let event: AuditEvent = match serde_json::from_value(payload) {
            Ok(event) => event,
            Err(error) => {
                // Undecodable payloads are poison; drop them so they don't block
                // the queue forever, but record loudly.
                tracing::error!(error = ?error, %outbox_id, "dropping undecodable audit outbox row");
                let _ = sqlx::query("DELETE FROM audit_outbox WHERE id = $1")
                    .bind(outbox_id)
                    .execute(pool)
                    .await;
                continue;
            }
        };

        match write_audit(pool, &event).await {
            Ok(_) => {
                sqlx::query("DELETE FROM audit_outbox WHERE id = $1")
                    .bind(outbox_id)
                    .execute(pool)
                    .await?;
                written += 1;
            }
            Err(error) => {
                let backoff =
                    ((i64::from(attempts) + 1) * 10).min(OUTBOX_MAX_ATTEMPTS_BACKOFF_SECS);
                sqlx::query(
                    "UPDATE audit_outbox \
                     SET attempts = attempts + 1, last_error = $2, \
                         next_attempt_at = now() + make_interval(secs => $3) \
                     WHERE id = $1",
                )
                .bind(outbox_id)
                .bind(error.to_string())
                .bind(backoff as f64)
                .execute(pool)
                .await?;
                tracing::warn!(error = ?error, %outbox_id, "audit outbox retry failed");
            }
        }
    }
    Ok(written)
}

// ─────────────────────────────────────────────────────────────────────────────
// Retention
// ─────────────────────────────────────────────────────────────────────────────

/// Delete audit rows older than `retention_days`. Returns rows removed. Callers
/// must never invoke this unless retention is explicitly configured — there is
/// no default pruning.
pub async fn prune_audit_log(pool: &PgPool, retention_days: i32) -> Result<u64, sqlx::Error> {
    if retention_days <= 0 {
        return Ok(0);
    }
    let result =
        sqlx::query("DELETE FROM audit_log WHERE occurred_at < now() - make_interval(days => $1)")
            .bind(retention_days)
            .execute(pool)
            .await?;
    Ok(result.rows_affected())
}

// ─────────────────────────────────────────────────────────────────────────────
// Verification
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct AuditChainVerification {
    /// Whether the hash chain is enabled (signing key present).
    pub enabled: bool,
    /// Whether every checked row verified and linked correctly.
    pub verified: bool,
    /// Number of hashed rows checked.
    pub checked: i64,
    /// First sequence number that failed verification, if any.
    pub first_broken_seq: Option<i64>,
    pub message: String,
}

#[derive(sqlx::FromRow)]
struct ChainRow {
    seq: i64,
    id: Uuid,
    workspace_id: Uuid,
    occurred_at: DateTime<Utc>,
    actor: String,
    action: String,
    target_type: String,
    target_id: Uuid,
    success: bool,
    severity: String,
    category: Option<String>,
    before: Option<Value>,
    after: Option<Value>,
    metadata: Option<Value>,
    diff: Option<Value>,
    prev_hash: Option<String>,
    hash: Option<String>,
}

/// Recompute and verify the hash chain for a workspace.
pub async fn verify_audit_chain(
    pool: &PgPool,
    workspace_id: Uuid,
) -> Result<AuditChainVerification, sqlx::Error> {
    let Some(key) = audit_signing_key() else {
        return Ok(AuditChainVerification {
            enabled: false,
            verified: false,
            checked: 0,
            first_broken_seq: None,
            message: "audit hash chain disabled (no signing key configured)".to_owned(),
        });
    };

    let rows: Vec<ChainRow> = sqlx::query_as(
        "SELECT seq, id, workspace_id, occurred_at, actor, action::text AS action, target_type, \
         target_id, success, severity, category, before, after, metadata, diff, prev_hash, hash \
         FROM audit_log WHERE workspace_id = $1 AND seq IS NOT NULL ORDER BY seq ASC",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await?;

    let mut expected_prev = AUDIT_CHAIN_GENESIS.to_owned();
    let mut checked = 0_i64;
    for row in &rows {
        let stored_prev = row.prev_hash.clone().unwrap_or_default();
        let stored_hash = row.hash.clone().unwrap_or_default();
        let category = row.category.clone().unwrap_or_default();
        let occurred_at_str = row.occurred_at.to_rfc3339_opts(SecondsFormat::Micros, true);

        if stored_prev != expected_prev {
            return Ok(AuditChainVerification {
                enabled: true,
                verified: false,
                checked,
                first_broken_seq: Some(row.seq),
                message: format!("broken chain link at seq {}", row.seq),
            });
        }

        let canonical = canonical_json_string(&hash_canonical_object(
            row.seq,
            row.workspace_id,
            row.id,
            &stored_prev,
            &occurred_at_str,
            &row.actor,
            &row.action,
            &row.target_type,
            row.target_id,
            row.success,
            &row.severity,
            &category,
            &row.before,
            &row.after,
            &row.metadata,
            &row.diff,
        ));
        let recomputed = hmac_hex(key.as_slice(), &canonical);
        if recomputed != stored_hash {
            return Ok(AuditChainVerification {
                enabled: true,
                verified: false,
                checked,
                first_broken_seq: Some(row.seq),
                message: format!("hash mismatch at seq {}", row.seq),
            });
        }

        expected_prev = stored_hash;
        checked += 1;
    }

    Ok(AuditChainVerification {
        enabled: true,
        verified: true,
        checked,
        first_broken_seq: None,
        message: format!("verified {checked} hashed audit rows"),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn redacts_sensitive_keys() {
        let input = json!({
            "auth_secret": "abc",
            "api_key": "mops_super_secret",
            "webhook_secret": "whsec_123",
            "database_url": "postgres://u:p@host/db",
            "name": "my-tool",
            "nested": {
                "bearer_token": "xyz",
                "endpoint": "https://example.com",
                "private_key": "-----BEGIN-----",
            },
            "list": [
                { "password": "hunter2", "label": "ok" },
            ],
        });
        let out = redact_json(&input);

        assert_eq!(out["auth_secret"], json!(REDACTED));
        assert_eq!(out["api_key"], json!(REDACTED));
        assert_eq!(out["webhook_secret"], json!(REDACTED));
        assert_eq!(out["database_url"], json!(REDACTED));
        assert_eq!(out["name"], json!("my-tool"));
        assert_eq!(out["nested"]["bearer_token"], json!(REDACTED));
        assert_eq!(out["nested"]["endpoint"], json!("https://example.com"));
        assert_eq!(out["nested"]["private_key"], json!(REDACTED));
        assert_eq!(out["list"][0]["password"], json!(REDACTED));
        assert_eq!(out["list"][0]["label"], json!("ok"));
    }

    #[test]
    fn redaction_is_idempotent() {
        let input = json!({ "token": "secret", "ok": "value" });
        let once = redact_json(&input);
        let twice = redact_json(&once);
        assert_eq!(once, twice);
        assert_eq!(twice["token"], json!(REDACTED));
    }

    #[test]
    fn no_secret_value_survives_redaction() {
        let secret = "mops_01234567_topsecretvalue";
        let input = json!({
            "api_key": secret,
            "config": { "auth_secret": secret, "plaintext_secret": secret },
        });
        let serialized = redact_json(&input).to_string();
        assert!(
            !serialized.contains(secret),
            "redacted output leaked a secret: {serialized}"
        );
    }

    #[test]
    fn long_strings_are_summarised() {
        let long = "x".repeat(MAX_STRING_LEN + 10);
        let out = redact_json(&json!({ "note": long }));
        assert_eq!(out["note"]["truncated"], json!(true));
        assert_eq!(out["note"]["length"], json!(MAX_STRING_LEN + 10));
    }

    #[test]
    fn large_arrays_are_truncated() {
        let items: Vec<Value> = (0..MAX_ARRAY_LEN + 50).map(|i| json!(i)).collect();
        let out = redact_json(&json!(items));
        let arr = out.as_array().expect("array");
        // MAX_ARRAY_LEN retained + 1 summary element.
        assert_eq!(arr.len(), MAX_ARRAY_LEN + 1);
        assert_eq!(arr[MAX_ARRAY_LEN]["truncated"], json!(true));
    }

    #[test]
    fn canonical_json_is_key_order_independent() {
        let a = json!({ "b": 1, "a": 2, "c": { "z": 1, "y": 2 } });
        let b = json!({ "c": { "y": 2, "z": 1 }, "a": 2, "b": 1 });
        assert_eq!(canonical_json_string(&a), canonical_json_string(&b));
    }

    #[test]
    fn hmac_is_deterministic_and_key_sensitive() {
        let msg = canonical_json_string(&json!({ "x": 1 }));
        assert_eq!(hmac_hex(b"key-one", &msg), hmac_hex(b"key-one", &msg));
        assert_ne!(hmac_hex(b"key-one", &msg), hmac_hex(b"key-two", &msg));
    }

    #[test]
    fn diff_field_change_alters_hash() {
        let before = Some(json!({ "endpoint": "https://a" }));
        let after_1 = Some(json!({ "endpoint": "https://b" }));
        let after_2 = Some(json!({ "endpoint": "https://c" }));
        let none = None;
        let wid = Uuid::now_v7();
        let id = Uuid::now_v7();
        let tid = Uuid::now_v7();
        let h1 = canonical_json_string(&hash_canonical_object(
            1,
            wid,
            id,
            AUDIT_CHAIN_GENESIS,
            "2026-01-01T00:00:00.000000Z",
            "actor",
            "tool_updated",
            "tool",
            tid,
            true,
            "notice",
            "tool",
            &before,
            &after_1,
            &none,
            &none,
        ));
        let h2 = canonical_json_string(&hash_canonical_object(
            1,
            wid,
            id,
            AUDIT_CHAIN_GENESIS,
            "2026-01-01T00:00:00.000000Z",
            "actor",
            "tool_updated",
            "tool",
            tid,
            true,
            "notice",
            "tool",
            &before,
            &after_2,
            &none,
            &none,
        ));
        assert_ne!(h1, h2);
    }

    #[test]
    fn event_builder_resolves_defaults() {
        let wid = Uuid::now_v7();
        let tid = Uuid::now_v7();
        let event = AuditEvent::new(wid, AuditAction::ToolSecretRevealed, tid, "tool")
            .actor_string("api_key:abc");
        assert_eq!(event.resolved_severity(), "critical");
        assert_eq!(event.resolved_category(), "tool");
        assert_eq!(event.actor_type.as_deref(), Some("api_key"));
    }
}

/// Database-backed audit tests. These require a Postgres instance (provided by
/// `sqlx::test` in CI) and therefore do not run in environments without one.
/// They compile-check every SQL binding in the reliable-write, outbox, prune,
/// and verification paths.
#[cfg(test)]
mod db_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use serde_json::json;
    use sqlx::PgPool;

    async fn seed_workspace(pool: &PgPool) -> Uuid {
        let id = Uuid::now_v7();
        sqlx::query("INSERT INTO workspaces (id, name, config) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(format!("ws-{id}"))
            .bind(json!({}))
            .execute(pool)
            .await
            .expect("seed workspace");
        id
    }

    async fn fetch_one(
        pool: &PgPool,
        audit_id: Uuid,
    ) -> (Option<i64>, Option<String>, Option<Value>) {
        sqlx::query_as::<_, (Option<i64>, Option<String>, Option<Value>)>(
            "SELECT seq, hash, after FROM audit_log WHERE id = $1",
        )
        .bind(audit_id)
        .fetch_one(pool)
        .await
        .expect("fetch audit row")
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn required_write_persists_and_redacts(pool: PgPool) {
        let ws = seed_workspace(&pool).await;
        let event = AuditEvent::new(ws, AuditAction::ConfigUpdated, ws, "workspace")
            .actor_string("api_key:abc")
            .before(json!({ "llm_api_key_env": "OPENAI_API_KEY" }))
            .after(json!({ "auth_secret": "supersecret", "model": "gpt" }));
        let id = write_audit(&pool, &event).await.expect("write audit");

        let (_seq, _hash, after) = fetch_one(&pool, id).await;
        let after = after.expect("after payload");
        // The secret value must never be persisted.
        assert_eq!(after["auth_secret"], json!(REDACTED));
        assert_eq!(after["model"], json!("gpt"));
        let stored = after.to_string();
        assert!(!stored.contains("supersecret"), "secret leaked: {stored}");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn hash_chain_links_and_verifies(pool: PgPool) {
        let ws = seed_workspace(&pool).await;
        for i in 0..3 {
            let event = AuditEvent::new(ws, AuditAction::KeyCreated, Uuid::now_v7(), "api_key")
                .actor_string("api_key:abc")
                .metadata(json!({ "i": i }));
            write_audit(&pool, &event).await.expect("write audit");
        }

        let verification = verify_audit_chain(&pool, ws).await.expect("verify");
        if verification.enabled {
            assert!(verification.verified, "{}", verification.message);
            assert_eq!(verification.checked, 3);

            // Tamper with a row; verification must now fail.
            sqlx::query(
                "UPDATE audit_log SET actor = 'tampered' WHERE workspace_id = $1 AND seq = 2",
            )
            .bind(ws)
            .execute(&pool)
            .await
            .expect("tamper");
            let after_tamper = verify_audit_chain(&pool, ws).await.expect("verify2");
            assert!(!after_tamper.verified);
            assert_eq!(after_tamper.first_broken_seq, Some(2));
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn outbox_enqueue_and_drain(pool: PgPool) {
        let ws = seed_workspace(&pool).await;
        let event = AuditEvent::new(ws, AuditAction::MemoryEmbedded, Uuid::now_v7(), "memory")
            .actor_string("system")
            .metadata(json!({ "token": "should-be-redacted" }));
        enqueue_outbox(&pool, &event).await.expect("enqueue");

        let pending: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM audit_outbox WHERE workspace_id = $1")
                .bind(ws)
                .fetch_one(&pool)
                .await
                .expect("count");
        assert_eq!(pending, 1);

        let drained = drain_audit_outbox(&pool, 10).await.expect("drain");
        assert_eq!(drained, 1);

        let remaining: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM audit_outbox WHERE workspace_id = $1")
                .bind(ws)
                .fetch_one(&pool)
                .await
                .expect("count2");
        assert_eq!(remaining, 0);

        // The drained row landed in audit_log with the redacted snapshot.
        let stored: Option<Value> =
            sqlx::query_scalar("SELECT metadata FROM audit_log WHERE id = $1")
                .bind(event.id)
                .fetch_one(&pool)
                .await
                .expect("fetch metadata");
        assert_eq!(stored.expect("metadata")["token"], json!(REDACTED));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn prune_removes_only_old_rows(pool: PgPool) {
        let ws = seed_workspace(&pool).await;
        let mut old = AuditEvent::new(ws, AuditAction::ToolInvoked, Uuid::now_v7(), "tool")
            .actor_string("system");
        old.occurred_at = Utc::now() - chrono::Duration::days(120);
        write_audit(&pool, &old).await.expect("write old");

        let recent = AuditEvent::new(ws, AuditAction::ToolInvoked, Uuid::now_v7(), "tool")
            .actor_string("system");
        write_audit(&pool, &recent).await.expect("write recent");

        let deleted = prune_audit_log(&pool, 90).await.expect("prune");
        assert_eq!(deleted, 1);

        let remaining: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE workspace_id = $1")
                .bind(ws)
                .fetch_one(&pool)
                .await
                .expect("count");
        assert_eq!(remaining, 1);

        // Retention of 0 is a no-op (non-destructive default).
        assert_eq!(prune_audit_log(&pool, 0).await.expect("noop"), 0);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn writes_are_workspace_isolated(pool: PgPool) {
        let ws_a = seed_workspace(&pool).await;
        let ws_b = seed_workspace(&pool).await;
        write_audit(
            &pool,
            &AuditEvent::new(ws_a, AuditAction::KeyCreated, Uuid::now_v7(), "api_key")
                .actor_string("api_key:a"),
        )
        .await
        .expect("write a");

        let count_b: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE workspace_id = $1")
                .bind(ws_b)
                .fetch_one(&pool)
                .await
                .expect("count b");
        assert_eq!(
            count_b, 0,
            "workspace B must not see workspace A audit rows"
        );
    }
}
