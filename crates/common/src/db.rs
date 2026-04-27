use std::time::Duration;

use sqlx::{postgres::PgPoolOptions, PgPool};

use crate::config::DatabaseConfig;

pub async fn connect_pool(
    database_url: &str,
    config: &DatabaseConfig,
) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(Duration::from_secs(config.connect_timeout_secs))
        .connect(database_url)
        .await
}

pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../../migrations").run(pool).await
}
