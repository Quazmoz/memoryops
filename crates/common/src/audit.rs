use std::sync::{Arc, OnceLock};

use sqlx::PgPool;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::models::AuditAction;

const AUDIT_LOG_MAX_IN_FLIGHT: usize = 64;
static AUDIT_LOG_PERMITS: OnceLock<Arc<Semaphore>> = OnceLock::new();

pub fn spawn_audit_log(
    db: PgPool,
    workspace_id: Uuid,
    actor: String,
    action: AuditAction,
    target_id: Uuid,
    target_type: impl Into<String>,
    diff: Option<serde_json::Value>,
) {
    let target_type = target_type.into();
    let permits = audit_log_permits();
    let Ok(permit) = permits.try_acquire_owned() else {
        tracing::warn!(
            workspace_id = %workspace_id,
            target_id = %target_id,
            action = ?action,
            "audit log write queue is full; dropping audit entry"
        );
        return;
    };

    tokio::spawn(async move {
        let _permit = permit;
        if let Err(error) = insert_audit_log(
            &db,
            workspace_id,
            actor,
            action,
            target_id,
            target_type,
            diff,
        )
        .await
        {
            tracing::warn!(error = ?error, "failed to write audit log entry");
        }
    });
}

fn audit_log_permits() -> Arc<Semaphore> {
    AUDIT_LOG_PERMITS
        .get_or_init(|| Arc::new(Semaphore::new(AUDIT_LOG_MAX_IN_FLIGHT)))
        .clone()
}

async fn insert_audit_log(
    db: &PgPool,
    workspace_id: Uuid,
    actor: String,
    action: AuditAction,
    target_id: Uuid,
    target_type: String,
    diff: Option<serde_json::Value>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO audit_log (id, workspace_id, actor, action, target_id, target_type, diff)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(workspace_id)
    .bind(actor)
    .bind(action)
    .bind(target_id)
    .bind(target_type)
    .bind(diff)
    .execute(db)
    .await
    .map(|_| ())
}
