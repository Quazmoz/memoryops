use anyhow::anyhow;
use chrono::{DateTime, Utc};
use common::{
    error::AppResult,
    models::{EventType, RawEvent, Source},
    AppError,
};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NewRawEvent {
    pub workspace_id: Uuid,
    pub source: Source,
    pub event_type: EventType,
    pub actor: String,
    pub payload: serde_json::Value,
    pub idempotency_key: String,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub enum InsertRawEventOutcome {
    Inserted(RawEvent),
    Existing(RawEvent),
}

pub async fn insert_raw_event(db: &PgPool, event: &NewRawEvent) -> AppResult<RawEvent> {
    let result = sqlx::query_as::<_, RawEvent>(
        r#"
        INSERT INTO raw_events (
            id,
            workspace_id,
            source,
            event_type,
            actor,
            payload,
            idempotency_key,
            occurred_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id,
            workspace_id,
            source,
            event_type,
            actor,
            payload,
            idempotency_key,
            occurred_at,
            ingested_at
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(event.workspace_id)
    .bind(event.source)
    .bind(event.event_type)
    .bind(&event.actor)
    .bind(&event.payload)
    .bind(&event.idempotency_key)
    .bind(event.occurred_at)
    .fetch_one(db)
    .await;

    match result {
        Ok(raw_event) => Ok(raw_event),
        Err(error) if is_unique_violation(&error) => {
            select_raw_event_by_workspace_and_idempotency_key(
                db,
                event.workspace_id,
                &event.idempotency_key,
            )
            .await?
            .ok_or_else(|| {
                AppError::Internal(anyhow!(
                    "idempotency unique violation without existing raw event"
                ))
            })
        }
        Err(error) => Err(AppError::Database(error)),
    }
}

pub async fn insert_raw_event_in_tx(
    transaction: &mut Transaction<'_, Postgres>,
    event: &NewRawEvent,
) -> AppResult<InsertRawEventOutcome> {
    let inserted = sqlx::query_as::<_, RawEvent>(
        r#"
        INSERT INTO raw_events (
            id,
            workspace_id,
            source,
            event_type,
            actor,
            payload,
            idempotency_key,
            occurred_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (workspace_id, idempotency_key) DO NOTHING
        RETURNING id,
            workspace_id,
            source,
            event_type,
            actor,
            payload,
            idempotency_key,
            occurred_at,
            ingested_at
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(event.workspace_id)
    .bind(event.source)
    .bind(event.event_type)
    .bind(&event.actor)
    .bind(&event.payload)
    .bind(&event.idempotency_key)
    .bind(event.occurred_at)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(AppError::Database)?;

    if let Some(raw_event) = inserted {
        return Ok(InsertRawEventOutcome::Inserted(raw_event));
    }

    let existing = select_raw_event_by_workspace_and_idempotency_key(
        &mut **transaction,
        event.workspace_id,
        &event.idempotency_key,
    )
    .await?
    .ok_or_else(|| {
        AppError::Internal(anyhow!("idempotency conflict without existing raw event"))
    })?;
    Ok(InsertRawEventOutcome::Existing(existing))
}

pub(crate) async fn workspace_exists(db: &PgPool, workspace_id: Uuid) -> AppResult<bool> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(workspace_id)
    .fetch_one(db)
    .await
    .map_err(AppError::Database)
}

pub(crate) async fn raw_event_needs_publish(db: &PgPool, raw_event_id: Uuid) -> AppResult<bool> {
    sqlx::query_scalar::<_, bool>(
        "SELECT NOT EXISTS(SELECT 1 FROM processing_state WHERE raw_event_id = $1)",
    )
    .bind(raw_event_id)
    .fetch_one(db)
    .await
    .map_err(AppError::Database)
}

async fn select_raw_event_by_workspace_and_idempotency_key(
    db: impl sqlx::Executor<'_, Database = Postgres>,
    workspace_id: Uuid,
    idempotency_key: &str,
) -> AppResult<Option<RawEvent>> {
    sqlx::query_as::<_, RawEvent>(
        r#"
        SELECT id,
            workspace_id,
            source,
            event_type,
            actor,
            payload,
            idempotency_key,
            occurred_at,
            ingested_at
        FROM raw_events
                WHERE workspace_id = $1
                    AND idempotency_key = $2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(idempotency_key)
    .fetch_optional(db)
    .await
    .map_err(AppError::Database)
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    match error {
        sqlx::Error::Database(database_error) => database_error.code().as_deref() == Some("23505"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    async fn insert_workspace(pool: &PgPool, workspace_id: Uuid) {
        let name = format!("workspace-{workspace_id}");
        let result = sqlx::query("INSERT INTO workspaces (id, name, config) VALUES ($1, $2, $3)")
            .bind(workspace_id)
            .bind(name)
            .bind(json!({}))
            .execute(pool)
            .await;

        if let Err(error) = result {
            panic!("test workspace insert should succeed: {error}");
        }
    }

    fn new_event(workspace_id: Uuid, idempotency_key: &str) -> NewRawEvent {
        NewRawEvent {
            workspace_id,
            source: Source::GitHub,
            event_type: EventType::PullRequest,
            actor: "octocat".to_owned(),
            payload: json!({ "sender": { "login": "octocat" } }),
            idempotency_key: idempotency_key.to_owned(),
            occurred_at: Utc::now(),
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn insert_and_retrieve_raw_event(pool: PgPool) {
        let workspace_id = Uuid::now_v7();
        insert_workspace(&pool, workspace_id).await;
        let event = new_event(workspace_id, "github:test-delivery");

        let inserted = match insert_raw_event(&pool, &event).await {
            Ok(inserted) => inserted,
            Err(error) => panic!("raw event insert should succeed: {error}"),
        };
        let retrieved = match select_raw_event_by_workspace_and_idempotency_key(
            &pool,
            workspace_id,
            &event.idempotency_key,
        )
        .await
        {
            Ok(Some(retrieved)) => retrieved,
            Ok(None) => panic!("inserted raw event should be retrievable"),
            Err(error) => panic!("raw event lookup should succeed: {error}"),
        };

        assert_eq!(retrieved.id, inserted.id);
        assert_eq!(retrieved.workspace_id, workspace_id);
        assert_eq!(retrieved.event_type, EventType::PullRequest);
        assert_eq!(retrieved.actor, "octocat");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn duplicate_idempotency_key_is_graceful(pool: PgPool) {
        let workspace_id = Uuid::now_v7();
        insert_workspace(&pool, workspace_id).await;
        let event = new_event(workspace_id, "github:duplicate-delivery");

        let first = match insert_raw_event(&pool, &event).await {
            Ok(inserted) => inserted,
            Err(error) => panic!("first raw event insert should succeed: {error}"),
        };
        let second = match insert_raw_event(&pool, &event).await {
            Ok(inserted) => inserted,
            Err(error) => panic!("duplicate raw event insert should be graceful: {error}"),
        };

        assert_eq!(second.id, first.id);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn duplicate_idempotency_keys_are_scoped_by_workspace(pool: PgPool) {
        let first_workspace = Uuid::now_v7();
        let second_workspace = Uuid::now_v7();
        insert_workspace(&pool, first_workspace).await;
        insert_workspace(&pool, second_workspace).await;

        let first = match insert_raw_event(&pool, &new_event(first_workspace, "shared-key")).await {
            Ok(inserted) => inserted,
            Err(error) => panic!("first raw event insert should succeed: {error}"),
        };
        let second = match insert_raw_event(&pool, &new_event(second_workspace, "shared-key")).await
        {
            Ok(inserted) => inserted,
            Err(error) => panic!("second raw event insert should succeed: {error}"),
        };

        assert_ne!(first.id, second.id);
        assert_eq!(first.workspace_id, first_workspace);
        assert_eq!(second.workspace_id, second_workspace);
    }
}
