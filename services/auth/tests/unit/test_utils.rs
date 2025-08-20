//! Common test utilities and mocks

use anyhow::Result;
use auth_service::{AuthContext, AuthService, Permission};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Mock authentication service for testing
pub struct MockAuthService {
    pub users: Arc<RwLock<FxHashMap<String, AuthContext>>>,
    pub tokens: Arc<RwLock<FxHashMap<String, AuthContext>>>,
    pub revoked_tokens: Arc<RwLock<Vec<String>>>,
    pub should_fail: Arc<RwLock<bool>>,
}

impl MockAuthService {
    pub fn new() -> Self {
        Self {
            users: Arc::new(RwLock::new(FxHashMap::default())),
            tokens: Arc::new(RwLock::new(FxHashMap::default())),
            revoked_tokens: Arc::new(RwLock::new(Vec::new())),
            should_fail: Arc::new(RwLock::new(false)),
        }
    }

    pub async fn add_user(&self, username: String, context: AuthContext) {
        self.users.write().await.insert(username, context);
    }

    pub async fn set_should_fail(&self, fail: bool) {
        *self.should_fail.write().await = fail;
    }
}

#[tonic::async_trait]
impl AuthService for MockAuthService {
    async fn authenticate(&self, username: &str, _password: &str) -> Result<AuthContext> {
        if *self.should_fail.read().await {
            return Err(anyhow::anyhow!("Mock authentication failure"));
        }

        // Check if user exists
        if let Some(context) = self.users.read().await.get(username).cloned() {
            return Ok(context);
        }
        
        // If not found and username starts with known prefixes, create default user
        if username.starts_with("demo_") || username.starts_with("fallback_") || 
           username.starts_with("test_") || username == "demo_user" || 
           username == "fallback_user" {
            let context = create_test_auth_context(username);
            self.users.write().await.insert(username.to_string(), context.clone());
            return Ok(context);
        }
        
        Err(anyhow::anyhow!("User not found"))
    }

    async fn validate_token(&self, token: &str) -> Result<AuthContext> {
        if self.revoked_tokens.read().await.contains(&token.to_string()) {
            return Err(anyhow::anyhow!("Token revoked"));
        }

        if let Some(context) = self.tokens.read().await.get(token).cloned() {
            Ok(context)
        } else {
            Err(anyhow::anyhow!("Invalid token"))
        }
    }

    async fn generate_token(&self, context: &AuthContext) -> Result<String> {
        if *self.should_fail.read().await {
            return Err(anyhow::anyhow!("Mock token generation failure"));
        }

        let token = format!("mock_token_{}", uuid::Uuid::new_v4());
        self.tokens.write().await.insert(token.clone(), context.clone());
        Ok(token)
    }

    async fn check_permission(&self, context: &AuthContext, permission: Permission) -> bool {
        context.permissions.contains(&permission) || 
        context.permissions.contains(&Permission::Admin)
    }

    async fn revoke_token(&self, token: &str) -> Result<()> {
        self.revoked_tokens.write().await.push(token.to_string());
        self.tokens.write().await.remove(token);
        Ok(())
    }
}

/// Create a test auth context with default permissions
pub fn create_test_auth_context(user_id: &str) -> AuthContext {
    let mut api_keys = FxHashMap::default();
    api_keys.insert("test_exchange".to_string(), "test_api_key".to_string());

    let mut metadata = FxHashMap::default();
    metadata.insert("login_time".to_string(), chrono::Utc::now().to_rfc3339());
    metadata.insert("test_mode".to_string(), "true".to_string());

    AuthContext {
        user_id: user_id.to_string(),
        permissions: vec![Permission::ReadMarketData, Permission::PlaceOrders],
        api_keys,
        metadata,
    }
}

/// Create a test auth context with admin permissions
pub fn create_admin_auth_context(user_id: &str) -> AuthContext {
    let mut api_keys = FxHashMap::default();
    api_keys.insert("admin_exchange".to_string(), "admin_api_key".to_string());

    let mut metadata = FxHashMap::default();
    metadata.insert("login_time".to_string(), chrono::Utc::now().to_rfc3339());
    metadata.insert("role".to_string(), "admin".to_string());

    AuthContext {
        user_id: user_id.to_string(),
        permissions: vec![Permission::Admin],
        api_keys,
        metadata,
    }
}

/// Create a test auth context with limited permissions
pub fn create_limited_auth_context(user_id: &str) -> AuthContext {
    let mut api_keys = FxHashMap::default();
    api_keys.insert("limited_exchange".to_string(), "limited_api_key".to_string());

    let mut metadata = FxHashMap::default();
    metadata.insert("login_time".to_string(), chrono::Utc::now().to_rfc3339());
    metadata.insert("permissions".to_string(), "read_only".to_string());

    AuthContext {
        user_id: user_id.to_string(),
        permissions: vec![Permission::ReadMarketData],
        api_keys,
        metadata,
    }
}

/// Mock HTTP client for testing network operations
pub struct MockHttpClient {
    pub responses: Arc<RwLock<FxHashMap<String, Result<String, String>>>>,
}

impl MockHttpClient {
    pub fn new() -> Self {
        Self {
            responses: Arc::new(RwLock::new(FxHashMap::default())),
        }
    }

    pub async fn set_response(&self, url: &str, response: Result<String, String>) {
        self.responses.write().await.insert(url.to_string(), response);
    }

    pub async fn get_response(&self, url: &str) -> Option<Result<String, String>> {
        self.responses.read().await.get(url).cloned()
    }
}

/// Generate test JWT token for testing
pub fn generate_test_jwt(context: &AuthContext, secret: &str) -> Result<String> {
    use jsonwebtoken::{encode, EncodingKey, Header};
    
    let key = EncodingKey::from_secret(secret.as_bytes());
    let header = Header::default();
    
    encode(&header, context, &key)
        .map_err(|e| anyhow::anyhow!("Failed to generate test JWT: {}", e))
}

/// Validate test JWT token
pub fn validate_test_jwt(token: &str, secret: &str) -> Result<AuthContext> {
    use jsonwebtoken::{decode, DecodingKey, Validation};
    
    let key = DecodingKey::from_secret(secret.as_bytes());
    let validation = Validation::default();
    
    decode::<AuthContext>(token, &key, &validation)
        .map(|data| data.claims)
        .map_err(|e| anyhow::anyhow!("Failed to validate test JWT: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_auth_service() {
        let mock = MockAuthService::new();
        let context = create_test_auth_context("test_user");
        
        mock.add_user("test_user".to_string(), context.clone()).await;
        
        // Test authentication
        let auth_result = mock.authenticate("test_user", "password").await;
        assert!(auth_result.is_ok());
        
        let authenticated_context = auth_result.unwrap();
        assert_eq!(authenticated_context.user_id, "test_user");
        
        // Test token generation
        let token = mock.generate_token(&authenticated_context).await.unwrap();
        assert!(token.starts_with("mock_token_"));
        
        // Test token validation
        let validate_result = mock.validate_token(&token).await;
        assert!(validate_result.is_ok());
        
        // Test permission check
        assert!(mock.check_permission(&authenticated_context, Permission::ReadMarketData).await);
        assert!(!mock.check_permission(&authenticated_context, Permission::Admin).await);
        
        // Test token revocation
        mock.revoke_token(&token).await.unwrap();
        let revoked_result = mock.validate_token(&token).await;
        assert!(revoked_result.is_err());
    }

    #[test]
    fn test_jwt_helpers() {
        let context = create_test_auth_context("jwt_test_user");
        let secret = "test_secret_key";
        
        // Generate token
        let token = generate_test_jwt(&context, secret).unwrap();
        assert!(!token.is_empty());
        
        // Validate token
        let validated_context = validate_test_jwt(&token, secret).unwrap();
        assert_eq!(validated_context.user_id, context.user_id);
        assert_eq!(validated_context.permissions, context.permissions);
        
        // Test invalid secret
        let invalid_result = validate_test_jwt(&token, "wrong_secret");
        assert!(invalid_result.is_err());
    }

    #[test]
    fn test_auth_context_creation() {
        let context = create_test_auth_context("test_user");
        assert_eq!(context.user_id, "test_user");
        assert!(context.permissions.contains(&Permission::ReadMarketData));
        assert!(context.permissions.contains(&Permission::PlaceOrders));
        
        let admin_context = create_admin_auth_context("admin_user");
        assert_eq!(admin_context.user_id, "admin_user");
        assert!(admin_context.permissions.contains(&Permission::Admin));
        
        let limited_context = create_limited_auth_context("limited_user");
        assert_eq!(limited_context.user_id, "limited_user");
        assert!(limited_context.permissions.contains(&Permission::ReadMarketData));
        assert!(!limited_context.permissions.contains(&Permission::PlaceOrders));
    }
}