use discord_storage::{
    GATEWAY_SESSION_STORAGE_KEY, GatewaySessionRecord, KeyValueStore, MemoryStore,
    clear_gateway_session, clear_gateway_session_at_key, load_gateway_session,
    load_gateway_session_at_key, save_gateway_session, save_gateway_session_at_key,
};

fn assert_key_value_store<T: KeyValueStore>() {}

async fn _storage_methods(store: &dyn KeyValueStore) {
    let _ = store.get("key").await;
    let _ = store.set("key", Vec::new()).await;
    let _ = store.delete("key").await;
}

async fn _gateway_session_methods(store: &impl KeyValueStore, record: &GatewaySessionRecord) {
    let _ = load_gateway_session(store).await;
    let _ = load_gateway_session_at_key(store, "k").await;
    let _ = save_gateway_session(store, record).await;
    let _ = save_gateway_session_at_key(store, "k", record).await;
    let _ = clear_gateway_session(store).await;
    let _ = clear_gateway_session_at_key(store, "k").await;
}

#[test]
fn storage_public_api_lock() {
    assert_key_value_store::<MemoryStore>();

    let _store = MemoryStore::new();
    let _key: &str = GATEWAY_SESSION_STORAGE_KEY;
    let _record = GatewaySessionRecord {
        session_id: "session-1".to_owned(),
        seq: 1,
        resume_url: "wss://gateway.discord.gg".to_owned(),
    };
}
