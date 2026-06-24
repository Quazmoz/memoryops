use std::path::Path;

use axum::{extract::State, Json};
use common::{error::AppResult, AppError, AppState};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::security::{
    decrypt_secret_legacy_or_current, encrypt_secret, generate_api_key, hash_secret,
};

#[derive(Debug, Deserialize)]
pub struct AdminLoginRequest {
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AdminLoginResponse {
    pub ok: bool,
}

pub async fn ensure_root_password(state: &AppState) -> AppResult<()> {
    if let Some(existing) = fetch_root_password_plaintext(state).await? {
        write_root_password_file(&existing);
        return Ok(());
    }

    let password = std::env::var("MEMORYOPS_ROOT_PASSWORD")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(generate_root_password);
    let password_hash = hash_secret(&password)?;
    let password_enc = encrypt_secret(state.app_secret_key.as_ref().as_str(), &password)?;

    let inserted = sqlx::query(
        r#"
        INSERT INTO app_admin_credentials (id, root_password_hash, root_password_enc)
        VALUES (TRUE, $1, $2)
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(password_hash)
    .bind(password_enc)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    if inserted.rows_affected() == 0 {
        if let Some(existing) = fetch_root_password_plaintext(state).await? {
            write_root_password_file(&existing);
        }
        return Ok(());
    }

    write_root_password_file(&password);
    tracing::warn!(
        path = %root_password_file_path(),
        "MemoryOps root password generated; read this file from the API container or set MEMORYOPS_ROOT_PASSWORD before first startup"
    );
    Ok(())
}

#[axum::debug_handler]
pub async fn login(
    State(state): State<AppState>,
    Json(request): Json<AdminLoginRequest>,
) -> AppResult<Json<AdminLoginResponse>> {
    let password = request.password.trim();
    if password.is_empty() {
        return Err(AppError::Unauthorized);
    }

    if verify_root_password(&state, password).await? {
        Ok(Json(AdminLoginResponse { ok: true }))
    } else {
        Err(AppError::Unauthorized)
    }
}

pub async fn verify_root_password(state: &AppState, password: &str) -> AppResult<bool> {
    let Some(hash) = sqlx::query_scalar::<_, Option<String>>(
        "SELECT root_password_hash FROM app_admin_credentials WHERE id = TRUE",
    )
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .flatten() else {
        return Ok(false);
    };

    let password = password.to_owned();
    tokio::task::spawn_blocking(move || common::auth::verify_secret(&password, &hash))
        .await
        .map_err(|error| AppError::Internal(anyhow::anyhow!(error)))
}

async fn fetch_root_password_plaintext(state: &AppState) -> AppResult<Option<String>> {
    let ciphertext = sqlx::query_scalar::<_, Option<String>>(
        "SELECT root_password_enc FROM app_admin_credentials WHERE id = TRUE",
    )
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .flatten();

    let Some(ciphertext) = ciphertext else {
        return Ok(None);
    };

    let decrypted =
        decrypt_secret_legacy_or_current(state.app_secret_key.as_ref().as_str(), &ciphertext)?;
    Ok(Some(decrypted.plaintext))
}

fn generate_root_password() -> String {
    match generate_api_key(Uuid::now_v7()) {
        Ok((key, _)) => format!("root_{}", key.trim_start_matches("mops_")),
        Err(_) => format!("root_{}", Uuid::now_v7().simple()),
    }
}

fn root_password_file_path() -> String {
    std::env::var("MEMORYOPS_ROOT_PASSWORD_FILE")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/tmp/memoryops-root-password".to_owned())
}

fn write_root_password_file(password: &str) {
    let path = root_password_file_path();
    let path_ref = Path::new(&path);
    if let Some(parent) = path_ref.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            tracing::warn!(?error, path = %path, "failed to create root password file directory");
            return;
        }
    }

    if let Err(error) = std::fs::write(path_ref, format!("{password}\n")) {
        tracing::warn!(?error, path = %path, "failed to write root password file");
    }
}
