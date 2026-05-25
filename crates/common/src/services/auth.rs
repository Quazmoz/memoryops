use crate::{
    auth::{self, AuthContext},
    error::AppResult,
    AppState,
};
use deadpool_redis::Pool;
use sqlx::PgPool;

#[derive(Clone)]
pub struct AuthService {
    db: PgPool,
    redis: Pool,
}

impl AuthService {
    pub fn new(db: PgPool, redis: Pool) -> Self {
        Self { db, redis }
    }

    pub fn from_state(state: &AppState) -> Self {
        Self::new(state.db.clone(), state.redis.clone())
    }

    pub async fn authenticate_api_key(&self, api_key: &str) -> AppResult<AuthContext> {
        let context = match self.redis.get().await {
            Ok(mut redis) => auth::validate_api_key_cached(&self.db, &mut redis, api_key).await?,
            Err(error) => {
                tracing::warn!(
                    error = ?error,
                    "auth cache unavailable; falling back to database API key validation"
                );
                auth::validate_api_key(&self.db, api_key).await?
            }
        };
        auth::spawn_last_used_update(self.db.clone(), context.key_id);
        Ok(context)
    }
}