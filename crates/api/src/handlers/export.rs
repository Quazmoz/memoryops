use std::io;

use async_stream::stream;
use axum::{
    body::{Body, Bytes},
    extract::{Path, State},
    http::{header, HeaderValue, Response},
    Extension,
};
use common::{auth::AuthContext, error::AppResult, models::MemoryUnit, AppState};
use serde_json::Value;
use uuid::Uuid;

use super::require_workspace;

const EXPORT_CHUNK_SIZE: i64 = 500;

#[axum::debug_handler]
pub async fn export_workspace(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> AppResult<Response<Body>> {
    require_workspace(&auth, id)?;

    let db = state.db.clone();
    let stream = stream! {
        let mut cursor = None;
        loop {
            let rows = match sqlx::query_as::<_, MemoryUnit>(
                r#"
                SELECT id,
                    workspace_id,
                    scope,
                    memory_type,
                    scope_visibility,
                    content,
                    entities,
                    importance_score,
                    importance_overridden,
                    source_events,
                    embedding_id,
                    token_count,
                    decay_score,
                    relevance_score,
                    pinned,
                    tags,
                    version,
                    promoted_at,
                    source_episode_ids,
                    corroboration_count,
                    deleted_at,
                    last_accessed_at,
                    created_at,
                    updated_at
                FROM memory_units
                WHERE workspace_id = $1
                  AND deleted_at IS NULL
                  AND ($2::uuid IS NULL OR id > $2)
                ORDER BY id ASC
                LIMIT $3
                "#,
            )
                .bind(id)
                .bind(cursor)
                .bind(EXPORT_CHUNK_SIZE)
                .fetch_all(&db)
                .await
            {
                Ok(rows) => rows,
                Err(error) => {
                    yield Err::<Bytes, io::Error>(io_error(error));
                    break;
                }
            };

            if rows.is_empty() {
                break;
            }

            for memory in &rows {
                let mut line = match export_line(memory) {
                    Ok(line) => line,
                    Err(error) => {
                        yield Err::<Bytes, io::Error>(io_error(error));
                        break;
                    }
                };
                line.push('\n');
                yield Ok::<Bytes, io::Error>(Bytes::from(line));
            }

            cursor = rows.last().map(|memory| memory.id);
        }
    };

    let mut response = Response::new(Body::from_stream(stream));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-ndjson"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"memoryops-export.jsonl\""),
    );

    Ok(response)
}

fn export_line(memory: &MemoryUnit) -> Result<String, serde_json::Error> {
    let mut value = serde_json::to_value(memory)?;
    if let Value::Object(object) = &mut value {
        object.remove("embedding_id");
        object.remove("relevance_score");
    }
    serde_json::to_string(&value)
}

fn io_error(error: impl ToString) -> io::Error {
    io::Error::other(error.to_string())
}
