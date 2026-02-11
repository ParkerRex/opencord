use discord_service::{
    KvSessionStore, MemorySessionStore, ServiceError, SessionStore, SessionTokenState,
};
use discord_storage::{GatewaySessionRecord, MemoryStore};

#[tokio::test]
async fn memory_session_store_persists_gateway_state_with_token() {
    let store = MemorySessionStore::new();

    store
        .set_discord_token("session-1", "token-1".to_owned())
        .await
        .expect("set token should succeed");

    store
        .save_gateway_session(
            "session-1",
            GatewaySessionRecord {
                session_id: "gateway-session".to_owned(),
                seq: 12,
                resume_url: "wss://gateway.discord.gg/?v=10&encoding=json".to_owned(),
            },
        )
        .await
        .expect("save gateway session should succeed");

    let loaded = store
        .load_gateway_session("session-1")
        .await
        .expect("load gateway session should succeed")
        .expect("gateway session should exist");

    assert_eq!(loaded.session_id, "gateway-session");
    assert_eq!(loaded.seq, 12);
}

#[tokio::test]
async fn kv_session_store_round_trips_gateway_state() {
    let kv_backend = MemoryStore::new();
    let store = KvSessionStore::new(kv_backend, "service.session.");

    store
        .set_discord_token("session-1", "token-1".to_owned())
        .await
        .expect("set token should succeed");

    let expected = GatewaySessionRecord {
        session_id: "gateway-session".to_owned(),
        seq: 99,
        resume_url: "wss://resume.discord.gg/?v=10&encoding=json".to_owned(),
    };

    store
        .save_gateway_session("session-1", expected.clone())
        .await
        .expect("save gateway session should succeed");

    let loaded = store
        .load_gateway_session("session-1")
        .await
        .expect("load gateway session should succeed");

    assert_eq!(loaded, Some(expected));

    store
        .clear_gateway_session("session-1")
        .await
        .expect("clear gateway session should succeed");

    let cleared = store
        .load_gateway_session("session-1")
        .await
        .expect("load after clear should succeed");
    assert!(cleared.is_none());
}

#[tokio::test]
async fn saving_gateway_state_for_unknown_session_returns_unauthorized() {
    let store = MemorySessionStore::new();

    let error = store
        .save_gateway_session(
            "unknown-session",
            GatewaySessionRecord {
                session_id: "gateway-session".to_owned(),
                seq: 1,
                resume_url: "wss://gateway.discord.gg/?v=10&encoding=json".to_owned(),
            },
        )
        .await
        .expect_err("saving without session should fail");

    assert!(matches!(error, ServiceError::UnauthorizedSession));
}

#[tokio::test]
async fn token_state_round_trip_preserves_refresh_metadata() {
    let store = MemorySessionStore::new();
    let token_state = SessionTokenState {
        access_token: "access-token".to_owned(),
        token_type: Some("Bearer".to_owned()),
        refresh_token: Some("refresh-token".to_owned()),
        scope: Some("identify guilds".to_owned()),
        expires_at_unix_ms: Some(1_800_000_000_000),
    };

    store
        .set_token_state("session-1", token_state.clone())
        .await
        .expect("set token state should succeed");

    let loaded = store
        .get_token_state("session-1")
        .await
        .expect("get token state should succeed");

    assert_eq!(loaded, Some(token_state));
}
