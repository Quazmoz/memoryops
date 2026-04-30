use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Extension, Json,
};
use common::{auth::AuthContext, AppError, AppState};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::require_workspace;

const DEFAULT_TAG_LIMIT: u32 = 50;
const MAX_TAG_LIMIT: u32 = 200;

#[derive(Debug, Deserialize)]
pub struct ListTagsQuery {
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TagsResponse {
    pub tags: Vec<TagSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct TagSummary {
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TagCursor {
    count: i64,
    name: String,
}

#[axum::debug_handler]
pub async fn list_tags(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
    Query(query): Query<ListTagsQuery>,
) -> Result<impl IntoResponse, AppError> {
    require_workspace(&auth, id)?;
    let limit = resolved_limit(query.limit);
    let cursor = parse_cursor(query.cursor.as_deref())?;
    let cursor_count = cursor.as_ref().map(|cursor| cursor.count);
    let cursor_name = cursor.as_ref().map(|cursor| cursor.name.as_str());
    let fetch_limit = i64::from(limit.saturating_add(1));

    let mut rows = sqlx::query_as::<_, TagSummary>(
        r#"
        WITH tag_counts AS (
            SELECT tag.name AS name,
                   COUNT(*)::BIGINT AS count
            FROM memory_units
            CROSS JOIN LATERAL UNNEST(tags) AS tag(name)
            WHERE workspace_id = $1
              AND deleted_at IS NULL
              AND tag.name <> ''
            GROUP BY tag.name
        )
        SELECT name, count
        FROM tag_counts
        WHERE ($2::BIGINT IS NULL OR $3::TEXT IS NULL OR count < $2 OR (count = $2 AND name > $3))
        ORDER BY count DESC, name ASC
        LIMIT $4
        "#,
    )
    .bind(auth.workspace_id)
    .bind(cursor_count)
    .bind(cursor_name)
    .bind(fetch_limit)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    let has_next = rows.len() > limit as usize;
    if has_next {
        rows.truncate(limit as usize);
    }
    let next_cursor = if has_next {
        rows.last().map(encode_cursor)
    } else {
        None
    };

    Ok(Json(TagsResponse {
        tags: rows,
        next_cursor,
    }))
}

fn resolved_limit(limit: Option<u32>) -> u32 {
    limit.unwrap_or(DEFAULT_TAG_LIMIT).clamp(1, MAX_TAG_LIMIT)
}

fn parse_cursor(raw: Option<&str>) -> Result<Option<TagCursor>, AppError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let Some((count, name)) = raw.split_once(':') else {
        return Err(AppError::Validation("invalid tag cursor".to_owned()));
    };
    let count = count
        .parse::<i64>()
        .map_err(|_| AppError::Validation("invalid tag cursor".to_owned()))?;
    if count < 0 || name.trim().is_empty() {
        return Err(AppError::Validation("invalid tag cursor".to_owned()));
    }

    Ok(Some(TagCursor {
        count,
        name: name.to_owned(),
    }))
}

fn encode_cursor(tag: &TagSummary) -> String {
    format!("{}:{}", tag.count, tag.name)
}

#[cfg(test)]
mod tests {
    use common::models::MemoryType;
    use serde_json::json;
    use sqlx::PgPool;

    use super::*;

    #[test]
    fn tag_limit_defaults_and_clamps() {
        assert_eq!(resolved_limit(None), DEFAULT_TAG_LIMIT);
        assert_eq!(resolved_limit(Some(0)), 1);
        assert_eq!(resolved_limit(Some(500)), MAX_TAG_LIMIT);
    }

    #[test]
    fn tag_cursor_round_trips_count_and_name() {
        let tag = TagSummary {
            name: "pr-review".to_owned(),
            count: 42,
        };

        let parsed = match parse_cursor(Some(&encode_cursor(&tag))) {
            Ok(Some(cursor)) => cursor,
            Ok(None) => panic!("cursor should parse"),
            Err(error) => panic!("cursor should be valid: {error}"),
        };

        assert_eq!(parsed.count, 42);
        assert_eq!(parsed.name, "pr-review");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn tag_aggregation_query_returns_counts(pool: PgPool) {
        let workspace_id = Uuid::now_v7();
        insert_workspace(&pool, workspace_id).await;
        insert_memory(&pool, workspace_id, &["deploy", "pr-review"]).await;
        insert_memory(&pool, workspace_id, &["deploy"]).await;

        let rows = sqlx::query_as::<_, TagSummary>(
            r#"
            SELECT tag.name AS name,
                   COUNT(*)::BIGINT AS count
            FROM memory_units
            CROSS JOIN LATERAL UNNEST(tags) AS tag(name)
            WHERE workspace_id = $1
              AND deleted_at IS NULL
            GROUP BY tag.name
            ORDER BY count DESC, name ASC
            "#,
        )
        .bind(workspace_id)
        .fetch_all(&pool)
        .await;
        let rows = match rows {
            Ok(rows) => rows,
            Err(error) => panic!("tag query should succeed: {error}"),
        };

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "deploy");
        assert_eq!(rows[0].count, 2);
        assert_eq!(rows[1].name, "pr-review");
        assert_eq!(rows[1].count, 1);
    }

    async fn insert_workspace(pool: &PgPool, workspace_id: Uuid) {
        let result = sqlx::query("INSERT INTO workspaces (id, name, config) VALUES ($1, $2, $3)")
            .bind(workspace_id)
            .bind(format!("workspace-{workspace_id}"))
            .bind(json!({}))
            .execute(pool)
            .await;

        if let Err(error) = result {
            panic!("workspace insert should succeed: {error}");
        }
    }

    async fn insert_memory(pool: &PgPool, workspace_id: Uuid, tags: &[&str]) {
        let result = sqlx::query(
            r#"
            INSERT INTO memory_units (
                id,
                workspace_id,
                scope,
                memory_type,
                content,
                entities,
                importance_score,
                tags
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(workspace_id)
        .bind(json!({
            "workspace_id": workspace_id,
            "agent_id": null,
            "user_id": null,
            "repo": null
        }))
        .bind(MemoryType::Episodic)
        .bind("tagged memory")
        .bind(json!([]))
        .bind(0.7_f32)
        .bind(tags.iter().map(|tag| (*tag).to_owned()).collect::<Vec<_>>())
        .execute(pool)
        .await;

        if let Err(error) = result {
            panic!("memory insert should succeed: {error}");
        }
    }
}
