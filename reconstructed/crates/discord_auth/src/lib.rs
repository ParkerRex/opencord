use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("token is missing")]
    MissingToken,
    #[error("auth provider failed: {0}")]
    Provider(String),
}

#[async_trait]
pub trait TokenProvider: Send + Sync {
    async fn get_token(&self) -> Result<Option<String>, AuthError>;
    async fn set_token(&self, token: String) -> Result<(), AuthError>;
    async fn clear_token(&self) -> Result<(), AuthError>;
}

#[derive(Debug, Default)]
pub struct MemoryTokenProvider {
    token: RwLock<Option<String>>,
}

impl MemoryTokenProvider {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_token(token: impl Into<String>) -> Self {
        Self {
            token: RwLock::new(Some(token.into())),
        }
    }
}

#[async_trait]
impl TokenProvider for MemoryTokenProvider {
    async fn get_token(&self) -> Result<Option<String>, AuthError> {
        Ok(self.token.read().await.clone())
    }

    async fn set_token(&self, token: String) -> Result<(), AuthError> {
        *self.token.write().await = Some(token);
        Ok(())
    }

    async fn clear_token(&self) -> Result<(), AuthError> {
        *self.token.write().await = None;
        Ok(())
    }
}
