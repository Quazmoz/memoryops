use common::{auth::AuthContext, error::AppResult, AppState};

use crate::handlers::retrieve::{execute_retrieve, RetrieveRequest, RetrieveResponse};

pub struct RetrievalService<'a> {
    state: &'a AppState,
}

impl<'a> RetrievalService<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    pub async fn retrieve(
        &self,
        auth: Option<&AuthContext>,
        request: RetrieveRequest,
    ) -> AppResult<RetrieveResponse> {
        execute_retrieve(self.state, auth, request).await
    }
}