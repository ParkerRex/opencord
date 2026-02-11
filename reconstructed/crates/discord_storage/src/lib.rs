use async_trait::async_trait;
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewaySessionRecord {
    pub session_id: String,
    pub seq: u64,
    pub resume_url: String,
}

pub const GATEWAY_SESSION_STORAGE_KEY: &str = "gateway.session.v1";

pub async fn load_gateway_session<S>(
    store: &S,
) -> Result<Option<GatewaySessionRecord>, StorageError>
where
    S: KeyValueStore + ?Sized,
{
    load_gateway_session_at_key(store, GATEWAY_SESSION_STORAGE_KEY).await
}

pub async fn load_gateway_session_at_key<S>(
    store: &S,
    key: &str,
) -> Result<Option<GatewaySessionRecord>, StorageError>
where
    S: KeyValueStore + ?Sized,
{
    let Some(value) = store.get(key).await? else {
        return Ok(None);
    };

    let session = serde_json::from_slice::<GatewaySessionRecord>(&value).map_err(|error| {
        StorageError::Backend(format!("failed to deserialize gateway session: {error}"))
    })?;

    validate_gateway_session(&session)?;
    Ok(Some(session))
}

pub async fn save_gateway_session<S>(
    store: &S,
    session: &GatewaySessionRecord,
) -> Result<(), StorageError>
where
    S: KeyValueStore + ?Sized,
{
    save_gateway_session_at_key(store, GATEWAY_SESSION_STORAGE_KEY, session).await
}

pub async fn save_gateway_session_at_key<S>(
    store: &S,
    key: &str,
    session: &GatewaySessionRecord,
) -> Result<(), StorageError>
where
    S: KeyValueStore + ?Sized,
{
    validate_gateway_session(session)?;

    let payload = serde_json::to_vec(session).map_err(|error| {
        StorageError::Backend(format!("failed to serialize gateway session: {error}"))
    })?;
    store.set(key, payload).await
}

pub async fn clear_gateway_session<S>(store: &S) -> Result<(), StorageError>
where
    S: KeyValueStore + ?Sized,
{
    clear_gateway_session_at_key(store, GATEWAY_SESSION_STORAGE_KEY).await
}

pub async fn clear_gateway_session_at_key<S>(store: &S, key: &str) -> Result<(), StorageError>
where
    S: KeyValueStore + ?Sized,
{
    store.delete(key).await
}

fn validate_gateway_session(session: &GatewaySessionRecord) -> Result<(), StorageError> {
    if session.session_id.trim().is_empty() {
        return Err(StorageError::Backend(
            "gateway session_id cannot be empty".to_owned(),
        ));
    }

    if session.resume_url.trim().is_empty() {
        return Err(StorageError::Backend(
            "gateway resume_url cannot be empty".to_owned(),
        ));
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::{
        GATEWAY_SESSION_STORAGE_KEY, GatewaySessionRecord, KeyValueStore, MemoryStore,
        clear_gateway_session, load_gateway_session, save_gateway_session,
    };

    #[tokio::test]
    async fn gateway_session_roundtrips_through_store() {
        let store = MemoryStore::new();
        let record = GatewaySessionRecord {
            session_id: "session-1".to_owned(),
            seq: 42,
            resume_url: "wss://gateway.discord.gg/?v=10&encoding=json".to_owned(),
        };

        save_gateway_session(&store, &record)
            .await
            .expect("save should succeed");

        let loaded = load_gateway_session(&store)
            .await
            .expect("load should succeed");

        assert_eq!(loaded, Some(record));
    }

    #[tokio::test]
    async fn gateway_session_can_be_cleared() {
        let store = MemoryStore::new();
        let record = GatewaySessionRecord {
            session_id: "session-1".to_owned(),
            seq: 42,
            resume_url: "wss://gateway.discord.gg/?v=10&encoding=json".to_owned(),
        };

        save_gateway_session(&store, &record)
            .await
            .expect("save should succeed");
        clear_gateway_session(&store)
            .await
            .expect("clear should succeed");

        let loaded = load_gateway_session(&store)
            .await
            .expect("load should succeed");
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn gateway_session_load_rejects_invalid_payload() {
        let store = MemoryStore::new();
        store
            .set(GATEWAY_SESSION_STORAGE_KEY, b"not-json".to_vec())
            .await
            .expect("set should succeed");

        let error = load_gateway_session(&store)
            .await
            .expect_err("invalid payload should fail");

        let message = error.to_string();
        assert!(message.contains("failed to deserialize gateway session"));
    }
}
