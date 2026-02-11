use async_trait::async_trait;
use discord_storage::{KeyValueStore, StorageError};
use sqlx::{PgPool, Row};
use thiserror::Error;

const DEFAULT_TABLE_NAME: &str = "opencord_kv_store";

#[derive(Debug, Error)]
pub enum PgStoreError {
    #[error("namespace cannot be empty")]
    EmptyNamespace,
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

#[derive(Clone, Debug)]
pub struct PostgresKeyValueStore {
    pool: PgPool,
    namespace: String,
    table_name: &'static str,
}

impl PostgresKeyValueStore {
    pub fn try_new(pool: PgPool, namespace: impl Into<String>) -> Result<Self, PgStoreError> {
        let namespace = namespace.into();
        if namespace.trim().is_empty() {
            return Err(PgStoreError::EmptyNamespace);
        }

        Ok(Self {
            pool,
            namespace,
            table_name: DEFAULT_TABLE_NAME,
        })
    }

    pub async fn connect(
        database_url: &str,
        namespace: impl Into<String>,
    ) -> Result<Self, PgStoreError> {
        let pool = PgPool::connect(database_url).await?;
        Self::try_new(pool, namespace)
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    pub async fn migrate(&self) -> Result<(), PgStoreError> {
        let create_table = format!(
            "CREATE TABLE IF NOT EXISTS {} (\
                namespace TEXT NOT NULL,\
                key TEXT NOT NULL,\
                value BYTEA NOT NULL,\
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),\
                PRIMARY KEY(namespace, key)\
             )",
            self.table_name
        );
        sqlx::query(&create_table).execute(&self.pool).await?;

        let create_index = format!(
            "CREATE INDEX IF NOT EXISTS {}_updated_at_idx ON {} (updated_at)",
            self.table_name, self.table_name
        );
        sqlx::query(&create_index).execute(&self.pool).await?;
        Ok(())
    }
}

fn sqlx_to_storage_error(error: sqlx::Error) -> StorageError {
    StorageError::Backend(format!("postgres key-value store error: {error}"))
}

#[async_trait]
impl KeyValueStore for PostgresKeyValueStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let query = format!(
            "SELECT value FROM {} WHERE namespace = $1 AND key = $2",
            self.table_name
        );
        let row = sqlx::query(&query)
            .bind(&self.namespace)
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(sqlx_to_storage_error)?;

        Ok(row.map(|r| r.get::<Vec<u8>, _>("value")))
    }

    async fn set(&self, key: &str, value: Vec<u8>) -> Result<(), StorageError> {
        let query = format!(
            "INSERT INTO {} (namespace, key, value, updated_at) \
             VALUES ($1, $2, $3, NOW()) \
             ON CONFLICT(namespace, key) \
             DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
            self.table_name
        );
        sqlx::query(&query)
            .bind(&self.namespace)
            .bind(key)
            .bind(value)
            .execute(&self.pool)
            .await
            .map_err(sqlx_to_storage_error)?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let query = format!(
            "DELETE FROM {} WHERE namespace = $1 AND key = $2",
            self.table_name
        );
        sqlx::query(&query)
            .bind(&self.namespace)
            .bind(key)
            .execute(&self.pool)
            .await
            .map_err(sqlx_to_storage_error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::PostgresKeyValueStore;
    use discord_storage::{
        GatewaySessionRecord, clear_gateway_session_at_key, load_gateway_session_at_key,
        save_gateway_session_at_key,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test]
    async fn rejects_empty_namespace() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost")
            .expect("lazy pool should be valid");

        let err =
            PostgresKeyValueStore::try_new(pool, "   ").expect_err("empty namespace should fail");
        assert!(matches!(err, super::PgStoreError::EmptyNamespace));
    }

    #[tokio::test]
    async fn gateway_session_round_trip_when_database_is_available() {
        let Some(database_url) = std::env::var("OPENCORD_TEST_DATABASE_URL").ok() else {
            return;
        };

        let store = PostgresKeyValueStore::connect(&database_url, "test.gateway")
            .await
            .expect("connect should succeed");
        store.migrate().await.expect("migrate should succeed");

        let key = format!(
            "gateway.session.test.{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        );

        let record = GatewaySessionRecord {
            session_id: "session-pg".to_owned(),
            seq: 77,
            resume_url: "wss://gateway.discord.gg/?v=10&encoding=json".to_owned(),
        };

        save_gateway_session_at_key(&store, &key, &record)
            .await
            .expect("save should succeed");

        let loaded = load_gateway_session_at_key(&store, &key)
            .await
            .expect("load should succeed")
            .expect("record should exist");
        assert_eq!(loaded, record);

        clear_gateway_session_at_key(&store, &key)
            .await
            .expect("clear should succeed");
    }
}
