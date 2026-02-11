use discord_http::DiscordHttpClient;
use discord_service::{
    DiscordOAuthCodeExchanger, KvSessionStore, MemorySessionStore, OAuthConfig, RealtimeEnvelope,
    SessionTokenState,
    UnsupportedOAuthCodeExchanger, router,
};
use discord_storage::MemoryStore;
use std::sync::Arc;
use url::Url;

#[test]
fn service_public_api_lock() {
    let http =
        DiscordHttpClient::new(Url::parse("https://discord.com/api/v10").expect("valid URL"));
    let sessions = Arc::new(MemorySessionStore::new());

    let state = discord_service::ServiceState::new(
        http.clone(),
        sessions,
        Some(OAuthConfig {
            authorize_url: Url::parse("https://discord.com/oauth2/authorize").expect("valid URL"),
            client_id: "client-id".to_owned(),
            redirect_uri: Url::parse("opencord://auth/callback").expect("valid URL"),
            scope: "identify guilds".to_owned(),
        }),
        Arc::new(UnsupportedOAuthCodeExchanger),
    )
    .with_gateway_intents(641);

    let _ = state.event_sender();
    let _ = router(state);

    let _ = KvSessionStore::new(MemoryStore::new(), "session.");

    let _ = DiscordOAuthCodeExchanger::new(
        Url::parse("https://discord.com/api/oauth2/token").expect("valid URL"),
        "client-id",
        "client-secret",
        Url::parse("opencord://auth/callback").expect("valid URL"),
    );

    let _ = RealtimeEnvelope::Notification {
        session_id: None,
        title: "title".to_owned(),
        body: "body".to_owned(),
    };

    let _ = SessionTokenState {
        access_token: "token".to_owned(),
        token_type: Some("Bearer".to_owned()),
        refresh_token: Some("refresh".to_owned()),
        scope: Some("identify guilds".to_owned()),
        expires_at_unix_ms: Some(1),
    };
}
