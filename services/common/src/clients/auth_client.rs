//! Auth service gRPC client wrapper

use anyhow::Result;
use crate::proto::auth::v1::{
    LoginRequest, LoginResponse, ValidateTokenRequest, ValidateTokenResponse,
    auth_service_client::AuthServiceClient as GrpcClient,
};
use tonic::transport::Channel;
use tracing::{debug, error};

/// Auth service client wrapper
pub struct AuthClient {
    client: GrpcClient<Channel>,
    endpoint: String,
}

impl AuthClient {
    /// Create new auth client
    pub async fn new(endpoint: &str) -> Result<Self> {
        debug!("Connecting to auth service at {}", endpoint);
        let channel = Channel::from_shared(endpoint.to_string())?
            .connect()
            .await?;

        Ok(Self {
            client: GrpcClient::new(channel),
            endpoint: endpoint.to_string(),
        })
    }

    /// Authenticate user
    pub async fn login(&mut self, username: &str, password: &str) -> Result<LoginResponse> {
        let request = tonic::Request::new(LoginRequest {
            username: username.to_string(),
            password: password.to_string(),
            exchange: String::new(), // Optional
        });

        debug!("Sending login request for user: {}", username);
        match self.client.login(request).await {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => {
                error!("Login failed: {}", e);
                Err(e.into())
            }
        }
    }

    /// Validate token
    pub async fn validate_token(&mut self, token: &str) -> Result<ValidateTokenResponse> {
        let request = tonic::Request::new(ValidateTokenRequest {
            token: token.to_string(),
        });

        debug!("Validating token");
        match self.client.validate_token(request).await {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => {
                error!("Token validation failed: {}", e);
                Err(e.into())
            }
        }
    }

    /// Get endpoint
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}
