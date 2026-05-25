use common::{crypto::decrypt_secret, models::Source, AppError, AppState};
use uuid::Uuid;

pub async fn workspace_webhook_secret(
    state: &AppState,
    workspace_id: Uuid,
    source: Source,
) -> Result<Option<String>, AppError> {
    let encrypted = sqlx::query_scalar::<_, Option<String>>(
        r#"
        SELECT webhook_secret_enc
        FROM integrations
        WHERE workspace_id = $1
          AND source = $2
          AND deleted_at IS NULL
        "#,
    )
    .bind(workspace_id)
    .bind(source)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .flatten();

    encrypted
        .map(|ciphertext| decrypt_secret(state.app_secret_key.as_ref().as_str(), &ciphertext))
        .transpose()
}