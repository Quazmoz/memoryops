use sqlx::PgPool;
use uuid::Uuid;

use crate::models::AuditAction;

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
    tokio::spawn(async move {
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
