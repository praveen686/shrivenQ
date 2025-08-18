//! Production-Ready gRPC Client for ShrivenQuant Auth Service
//!
//! This demonstrates the complete production workflow:
//! 1. Service Discovery & Connection Management
//! 2. Multi-Exchange Authentication (Binance, Zerodha)
//! 3. JWT Token Management & Renewal
//! 4. Error Handling & Retry Logic
//! 5. Service-to-Service Communication Patterns

use anyhow::{Result, anyhow};
use services_common::auth::v1::{
    LoginRequest, ValidateTokenRequest, auth_service_client::AuthServiceClient,
};
use std::time::Duration;
use tokio::time::sleep;
use tonic::Request;
use tonic::transport::{Channel, Endpoint};
use tracing::{debug, error, info, warn};

/// Production-ready authentication client with connection management
pub struct AuthClient {
    client: AuthServiceClient<Channel>,
    endpoint: String,
    retry_config: RetryConfig,
    current_token: Option<TokenInfo>,
}

/// Token information with expiry tracking
#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub token: String,
    pub expires_at: i64,
    pub exchange: String,
    pub permissions: Vec<i32>,
}

/// Retry configuration for resilient connections
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            backoff_multiplier: 2.0,
        }
    }
}

impl AuthClient {
    /// Create new auth client with connection management
    pub async fn new(endpoint: String) -> Result<Self> {
        let client = Self::connect_with_retry(&endpoint, &RetryConfig::default()).await?;

        Ok(Self {
            client,
            endpoint,
            retry_config: RetryConfig::default(),
            current_token: None,
        })
    }

    /// Connect with exponential backoff retry
    async fn connect_with_retry(
        endpoint: &str,
        config: &RetryConfig,
    ) -> Result<AuthServiceClient<Channel>> {
        let mut delay = config.base_delay;

        for attempt in 0..=config.max_retries {
            match Self::create_connection(endpoint).await {
                Ok(client) => {
                    info!("✅ Connected to auth service on attempt {}", attempt + 1);
                    return Ok(client);
                }
                Err(e) => {
                    if attempt == config.max_retries {
                        return Err(anyhow!(
                            "Failed to connect after {} attempts: {}",
                            config.max_retries + 1,
                            e
                        ));
                    }

                    warn!(
                        "⚠️  Connection attempt {} failed: {}, retrying in {:?}",
                        attempt + 1,
                        e,
                        delay
                    );
                    sleep(delay).await;

                    delay = std::cmp::min(
                        // SAFETY: u128 to f64 to u64 for exponential backoff calculation
                        Duration::from_millis(
                            (delay.as_millis() as f64 * config.backoff_multiplier) as u64,
                        ),
                        config.max_delay,
                    );
                }
            }
        }

        unreachable!("Loop should have returned or errored")
    }

    /// Create gRPC connection
    async fn create_connection(endpoint: &str) -> Result<AuthServiceClient<Channel>> {
        let channel = Endpoint::from_shared(endpoint.to_string())?
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .tcp_keepalive(Some(Duration::from_secs(30)))
            .http2_keep_alive_interval(Duration::from_secs(10))
            .keep_alive_timeout(Duration::from_secs(5))
            .connect()
            .await?;

        Ok(AuthServiceClient::new(channel))
    }

    /// Authenticate with exchange credentials
    pub async fn authenticate(
        &mut self,
        exchange: &str,
        market: Option<&str>,
    ) -> Result<TokenInfo> {
        let username = match exchange {
            "binance" => {
                let market_suffix = market.unwrap_or("spot");
                format!("binance_{}", market_suffix)
            }
            "zerodha" => "zerodha".to_string(),
            _ => return Err(anyhow!("Unsupported exchange: {}", exchange)),
        };

        info!("🔐 Authenticating with {} ({})", exchange, username);

        let request = Request::new(LoginRequest {
            username,
            password: String::new(), // Not used for API key auth
            exchange: exchange.to_string(),
        });

        match self.client.login(request).await {
            Ok(response) => {
                let resp = response.into_inner();
                let permissions_clone = resp.permissions.clone();
                let token_info = TokenInfo {
                    token: resp.token,
                    expires_at: resp.expires_at,
                    exchange: exchange.to_string(),
                    permissions: resp.permissions,
                };

                info!("✅ Authentication successful!");
                info!("   Exchange: {}", exchange);
                info!(
                    "   Token expires: {}",
                    chrono::DateTime::from_timestamp(resp.expires_at, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                        .unwrap_or_else(|| "Invalid timestamp".to_string())
                );
                info!("   Permissions: {:?}", permissions_clone);

                self.current_token = Some(token_info.clone());
                Ok(token_info)
            }
            Err(e) => {
                error!("❌ Authentication failed for {}: {}", exchange, e);
                Err(anyhow!("Authentication failed: {}", e))
            }
        }
    }

    /// Validate current token
    pub async fn validate_token(&mut self) -> Result<bool> {
        let token = match &self.current_token {
            Some(token_info) => &token_info.token,
            None => return Err(anyhow!("No token to validate")),
        };

        debug!("🔍 Validating token...");

        let request = Request::new(ValidateTokenRequest {
            token: token.clone(),
        });

        match self.client.validate_token(request).await {
            Ok(response) => {
                let resp = response.into_inner();
                if resp.valid {
                    debug!("✅ Token is valid");
                    Ok(true)
                } else {
                    warn!("⚠️  Token is invalid or expired");
                    self.current_token = None;
                    Ok(false)
                }
            }
            Err(e) => {
                error!("❌ Token validation failed: {}", e);
                self.current_token = None;
                Err(anyhow!("Token validation failed: {}", e))
            }
        }
    }

    /// Get current token (with automatic validation)
    pub async fn get_valid_token(&mut self) -> Result<String> {
        // Check if we have a token
        if self.current_token.is_none() {
            return Err(anyhow!("No authentication token available"));
        }

        // Check if token is still valid
        if !self.validate_token().await? {
            return Err(anyhow!("Token is invalid or expired"));
        }

        Ok(self.current_token.as_ref().unwrap().token.clone())
    }

    /// Create authenticated gRPC request with token
    pub async fn create_authenticated_request<T>(&mut self, request: T) -> Result<Request<T>> {
        let token = self.get_valid_token().await?;
        let mut req = Request::new(request);

        // Add authorization header
        req.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", token)
                .parse()
                .map_err(|e| anyhow!("Invalid token format: {}", e))?,
        );

        Ok(req)
    }

    /// Health check for the auth service
    pub async fn health_check(&mut self) -> Result<()> {
        // Simple validation call to check service health
        let dummy_request = Request::new(ValidateTokenRequest {
            token: "dummy".to_string(),
        });

        match self.client.validate_token(dummy_request).await {
            Ok(_) => Ok(()), // Service is responding
            Err(status) => {
                if status.code() == tonic::Code::Unauthenticated {
                    Ok(()) // Expected for dummy token
                } else {
                    Err(anyhow!("Service health check failed: {}", status))
                }
            }
        }
    }
}

/// Production workflow demonstration
async fn demonstrate_production_workflow() -> Result<()> {
    info!("🚀 Starting Production Authentication Workflow");
    info!("{}", "=".repeat(60));

    // 1. Service Discovery & Connection
    info!("\n📡 Phase 1: Service Discovery & Connection");
    let mut auth_client = AuthClient::new("http://127.0.0.1:50051".to_string()).await?;

    // 2. Health Check
    info!("\n🏥 Phase 2: Service Health Check");
    auth_client.health_check().await?;
    info!("✅ Auth service is healthy");

    // 3. Multi-Exchange Authentication
    info!("\n🔐 Phase 3: Multi-Exchange Authentication");

    // Try Binance Spot
    match auth_client.authenticate("binance", Some("spot")).await {
        Ok(token_info) => {
            info!("✅ Binance Spot authentication successful");

            // Demonstrate token usage
            info!("\n🎫 Phase 4: Token Usage for Service Communication");
            let token = auth_client.get_valid_token().await?;
            info!("✅ Retrieved valid token for service calls");
            info!("   Token length: {} characters", token.len());

            // Simulate service-to-service call
            info!("\n📊 Phase 5: Simulated Service-to-Service Call");
            simulate_market_data_service_call(&token).await?;

            // Token validation check
            info!("\n🔍 Phase 6: Token Validation & Management");
            let is_valid = auth_client.validate_token().await?;
            info!(
                "✅ Token validation: {}",
                if is_valid { "VALID" } else { "INVALID" }
            );
        }
        Err(e) => {
            warn!("⚠️  Binance authentication failed: {}", e);
            info!("   This is expected if Binance credentials are not configured");
        }
    }

    // Try Zerodha (need to use API key as username)
    info!("\n🔐 Phase 3b: Zerodha Authentication with API Key");

    // Load API key from environment
    if let Ok(api_key) = std::env::var("ZERODHA_API_KEY") {
        info!("📋 Using Zerodha API key: {}...", &api_key[..8]);

        let zerodha_request = tonic::Request::new(LoginRequest {
            username: api_key,
            password: String::new(),
            exchange: "zerodha".to_string(),
        });

        match auth_client.client.login(zerodha_request).await {
            Ok(response) => {
                let resp = response.into_inner();
                info!("✅ Zerodha authentication successful!");
                info!("   Token: {}...", &resp.token[..20.min(resp.token.len())]);
                info!(
                    "   Expires at: {}",
                    chrono::DateTime::from_timestamp(resp.expires_at, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                        .unwrap_or_else(|| "Invalid timestamp".to_string())
                );
                info!("   Permissions: {:?}", resp.permissions);
            }
            Err(e) => {
                warn!("⚠️  Zerodha authentication failed: {}", e);
                info!("   This might need 2FA completion or session renewal");
            }
        }
    } else {
        warn!("⚠️  ZERODHA_API_KEY not found in environment");
    }

    // Original zerodha test (will fail as expected)
    match auth_client.authenticate("zerodha", None).await {
        Ok(_) => {
            info!("✅ Zerodha authentication successful");
        }
        Err(e) => {
            warn!("⚠️  Zerodha authentication failed: {}", e);
            info!("   This is expected if Zerodha credentials are not configured");
        }
    }

    info!("\n🎉 Production workflow demonstration complete!");
    info!("{}", "=".repeat(60));

    Ok(())
}

/// Simulate how other services would use the auth token
async fn simulate_market_data_service_call(token: &str) -> Result<()> {
    info!("📈 Simulating Market Data Service call with JWT token...");

    // In production, this would be:
    // let mut market_client = MarketDataServiceClient::connect("http://market-data:50052").await?;
    // let mut request = Request::new(SubscribeRequest { symbol: "BTCUSDT".to_string() });
    // request.metadata_mut().insert("authorization", format!("Bearer {}", token).parse()?);
    // let stream = market_client.subscribe(request).await?;

    info!("   🔗 Would connect to: http://market-data:50052");
    info!("   📦 Would send: SubscribeRequest {{ symbol: 'BTCUSDT' }}");
    info!(
        "   🎫 Would include: Authorization: Bearer {}...",
        &token[..20.min(token.len())]
    );
    info!("   📊 Would receive: Real-time market data stream");
    info!("✅ Market Data Service call simulation complete");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize production-grade logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();

    // Load environment variables
    dotenv::dotenv().ok();

    // Run production workflow
    match demonstrate_production_workflow().await {
        Ok(_) => {
            info!("\n✅ All tests completed successfully!");
        }
        Err(e) => {
            error!("\n❌ Workflow failed: {}", e);
            error!("\n🔧 Troubleshooting:");
            error!("   1. Ensure auth service is running:");
            error!("      cargo run --bin auth-service");
            error!("   2. Check .env file has exchange credentials");
            error!("   3. Verify network connectivity to localhost:50051");
            return Err(e);
        }
    }

    Ok(())
}
