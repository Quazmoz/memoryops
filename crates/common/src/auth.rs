use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthContext {
    pub workspace_id: Uuid,
    pub key_id: Uuid,
    pub key_prefix: String,
}

impl AuthContext {
    pub fn actor(&self) -> String {
        format!("api_key:{}", self.key_id)
    }
}
