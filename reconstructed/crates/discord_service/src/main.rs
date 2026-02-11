use anyhow::{Context, Result, bail};
use discord_http::DiscordHttpClient;
use discord_persistence_pg::PostgresKeyValueStore;
use discord_service::{
    DiscordOAuthCodeExchanger, MemorySessionStore, OAuthCodeExchanger, OAuthConfig, ServiceState,
    UnsupportedOAuthCodeExchanger, router,
};
use std::{env, sync::Arc};
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::info;
use url::Url;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            env::var("RUST_LOG")
                .unwrap_or_else(|_| "discord_service=info,tower_http=info".to_owned()),
        )
        .init();

    let bind_addr = env::var("OPENCORD_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());
    let discord_api_base_url = env::var("DISCORD_API_BASE_URL")
        .unwrap_or_else(|_| "https://discord.com/api/v10".to_owned());
    let gateway_intents = load_gateway_intents()?;
    let oauth_config = load_oauth_config()?;
    let oauth_exchanger: Arc<dyn OAuthCodeExchanger> = load_oauth_exchanger(oauth_config.as_ref())?;

    let http = DiscordHttpClient::new(
        Url::parse(&discord_api_base_url)
            .with_context(|| format!("invalid DISCORD_API_BASE_URL: {discord_api_base_url}"))?,
    );

    let state = if let Ok(database_url) = env::var("OPENCORD_DATABASE_URL") {
        let namespace =
            env::var("OPENCORD_KV_NAMESPACE").unwrap_or_else(|_| "opencord.service.v1".to_owned());
        let session_key_prefix =
            env::var("OPENCORD_SESSION_KEY_PREFIX").unwrap_or_else(|_| "auth.session.".to_owned());

        let store = PostgresKeyValueStore::connect(&database_url, namespace)
            .await
            .context("failed to connect PostgresKeyValueStore")?;
        store
            .migrate()
            .await
            .context("failed to run postgres key-value migration")?;

        ServiceState::from_key_value_store(
            http,
            store,
            session_key_prefix,
            oauth_config,
            oauth_exchanger,
        )
        .with_gateway_intents(gateway_intents)
    } else {
        ServiceState::new(
            http,
            Arc::new(MemorySessionStore::new()),
            oauth_config,
            oauth_exchanger,
        )
        .with_gateway_intents(gateway_intents)
    };

    let app = router(state).layer(TraceLayer::new_for_http());

    let listener = TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("failed to bind OPENCORD_BIND_ADDR={bind_addr}"))?;

    info!(bind_addr = %bind_addr, "discord service listening");
    axum::serve(listener, app)
        .await
        .context("discord service failed")?;

    Ok(())
}

fn load_oauth_config() -> Result<Option<OAuthConfig>> {
    let client_id = env::var("DISCORD_OAUTH_CLIENT_ID").ok();
    let authorize_url = env::var("DISCORD_OAUTH_AUTHORIZE_URL").ok();
    let redirect_uri = env::var("DISCORD_OAUTH_REDIRECT_URI").ok();
    let scope = env::var("DISCORD_OAUTH_SCOPE").ok();

    if client_id.is_none() && authorize_url.is_none() && redirect_uri.is_none() && scope.is_none() {
        return Ok(None);
    }

    let client_id = client_id.ok_or_else(|| {
        anyhow::anyhow!("DISCORD_OAUTH_CLIENT_ID must be set when OAuth is configured")
    })?;
    let authorize_url = authorize_url.ok_or_else(|| {
        anyhow::anyhow!("DISCORD_OAUTH_AUTHORIZE_URL must be set when OAuth is configured")
    })?;
    let redirect_uri = redirect_uri.ok_or_else(|| {
        anyhow::anyhow!("DISCORD_OAUTH_REDIRECT_URI must be set when OAuth is configured")
    })?;

    let authorize_url = Url::parse(&authorize_url)
        .with_context(|| format!("invalid DISCORD_OAUTH_AUTHORIZE_URL: {authorize_url}"))?;
    let redirect_uri = Url::parse(&redirect_uri)
        .with_context(|| format!("invalid DISCORD_OAUTH_REDIRECT_URI: {redirect_uri}"))?;

    let scope = scope.unwrap_or_else(|| "identify guilds".to_owned());
    if scope.trim().is_empty() {
        bail!("DISCORD_OAUTH_SCOPE cannot be empty when OAuth is configured");
    }

    Ok(Some(OAuthConfig {
        authorize_url,
        client_id,
        redirect_uri,
        scope,
    }))
}

fn load_oauth_exchanger(oauth_config: Option<&OAuthConfig>) -> Result<Arc<dyn OAuthCodeExchanger>> {
    let Some(config) = oauth_config else {
        return Ok(Arc::new(UnsupportedOAuthCodeExchanger));
    };

    let client_secret = match env::var("DISCORD_OAUTH_CLIENT_SECRET") {
        Ok(value) => value,
        Err(_) => return Ok(Arc::new(UnsupportedOAuthCodeExchanger)),
    };

    let token_url = env::var("DISCORD_OAUTH_TOKEN_URL")
        .unwrap_or_else(|_| "https://discord.com/api/oauth2/token".to_owned());
    let token_url = Url::parse(&token_url)
        .with_context(|| format!("invalid DISCORD_OAUTH_TOKEN_URL: {token_url}"))?;

    Ok(Arc::new(DiscordOAuthCodeExchanger::new(
        token_url,
        config.client_id.clone(),
        client_secret,
        config.redirect_uri.clone(),
    )))
}

fn load_gateway_intents() -> Result<u64> {
    let raw = env::var("OPENCORD_GATEWAY_INTENTS").ok();
    let Some(raw) = raw else {
        return Ok(641);
    };

    raw.parse::<u64>()
        .with_context(|| format!("invalid OPENCORD_GATEWAY_INTENTS: {raw}"))
}
