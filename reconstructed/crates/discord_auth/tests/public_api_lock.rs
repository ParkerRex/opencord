use discord_auth::{AuthError, MemoryTokenProvider, TokenProvider};

fn assert_token_provider<T: TokenProvider>() {}

async fn _token_provider_methods(provider: &dyn TokenProvider) {
    let _ = provider.get_token().await;
    let _ = provider.set_token("token".to_owned()).await;
    let _ = provider.clear_token().await;
}

#[test]
fn auth_public_api_lock() {
    assert_token_provider::<MemoryTokenProvider>();

    let _provider = MemoryTokenProvider::new();
    let _provider = MemoryTokenProvider::from_token("token-123");

    let _ = AuthError::MissingToken;
    let _ = AuthError::Provider("provider error".to_owned());
}
