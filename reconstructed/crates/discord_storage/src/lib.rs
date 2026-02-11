use async_trait::async_trait;
use std::collections::HashMap;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait KeyValueStore: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError>;
    async fn set(&self, key: &str, value: Vec<u8>) -> Result<(), StorageError>;
    async fn delete(&self, key: &str) -> Result<(), StorageError>;
}

#[derive(Debug, Default)]
pub struct MemoryStore {
    inner: RwLock<HashMap<String, Vec<u8>>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl KeyValueStore for MemoryStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self.inner.read().await.get(key).cloned())
    }

    async fn set(&self, key: &str, value: Vec<u8>) -> Result<(), StorageError> {
        self.inner.write().await.insert(key.to_owned(), value);
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.inner.write().await.remove(key);
        Ok(())
    }
}
