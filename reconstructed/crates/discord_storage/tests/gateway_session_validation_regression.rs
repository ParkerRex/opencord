use discord_storage::{
    GATEWAY_SESSION_STORAGE_KEY, GatewaySessionRecord, KeyValueStore, MemoryStore,
    load_gateway_session, save_gateway_session,
};

#[tokio::test]
async fn save_rejects_empty_session_id() {
    let store = MemoryStore::new();
    let record = GatewaySessionRecord {
        session_id: "   ".to_owned(),
        seq: 1,
        resume_url: "wss://gateway.discord.gg/?v=10&encoding=json".to_owned(),
    };

    let err = save_gateway_session(&store, &record)
        .await
        .expect_err("empty session_id should fail validation");

    assert!(err.to_string().contains("session_id"));
}

#[tokio::test]
async fn load_rejects_persisted_record_with_empty_resume_url() {
    let store = MemoryStore::new();

    let poisoned = GatewaySessionRecord {
        session_id: "session-123".to_owned(),
        seq: 9,
        resume_url: "".to_owned(),
    };

    let bytes = serde_json::to_vec(&poisoned).expect("serialize poisoned record");
    store
        .set(GATEWAY_SESSION_STORAGE_KEY, bytes)
        .await
        .expect("inject poisoned payload");

    let err = load_gateway_session(&store)
        .await
        .expect_err("poisoned payload should fail validation");

    assert!(err.to_string().contains("resume_url"));
}
