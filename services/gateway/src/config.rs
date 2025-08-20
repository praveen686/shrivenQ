//! Configuration for the API Gateway

use anyhow::Result;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// API Gateway configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// HTTP server configuration
    pub server: ServerConfig,
    /// gRPC service endpoints
    pub services: ServiceEndpoints,
    /// Authentication configuration
    pub auth: AuthConfig,
    /// Rate limiting configuration
    pub rate_limiting: RateLimitConfig,
    /// CORS configuration
    pub cors: CorsConfig,
    /// Monitoring configuration
    pub monitoring: MonitoringConfig,
}

/// HTTP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server host
    pub host: String,
    /// Server port
    pub port: u16,
    /// Request timeout in seconds
    pub timeout_seconds: u64,
    /// Maximum request body size in bytes
    pub max_body_size: usize,
    /// Enable compression
    pub compression: bool,
    /// TLS configuration (optional)
    pub tls: Option<TlsConfig>,
}

/// TLS configuration for HTTPS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Path to certificate file
    pub cert_path: String,
    /// Path to private key file
    pub key_path: String,
}

/// gRPC service endpoints configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEndpoints {
    /// Authentication service endpoint
    pub auth_service: String,
    /// Execution service endpoint
    pub execution_service: String,
    /// Market data service endpoint
    pub market_data_service: String,
    /// Risk management service endpoint
    pub risk_service: String,
    /// Portfolio manager service endpoint (if available)
    pub portfolio_service: Option<String>,
    /// Reporting service endpoint (if available)
    pub reporting_service: Option<String>,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// JWT secret key for token validation
    pub jwt_secret: String,
    /// Token expiration time in seconds
    pub token_expiry_seconds: u64,
    /// Refresh token expiration in seconds
    pub refresh_token_expiry_seconds: u64,
    /// Allowed token algorithms
    pub allowed_algorithms: Vec<String>,
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Enable rate limiting
    pub enabled: bool,
    /// Default rate limit per minute
    pub requests_per_minute: u32,
    /// Burst capacity
    pub burst_size: u32,
    /// Rate limits per endpoint
    pub endpoint_limits: FxHashMap<String, EndpointRateLimit>,
}

/// Per-endpoint rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointRateLimit {
    /// Requests per minute for this endpoint
    pub requests_per_minute: u32,
    /// Burst capacity for this endpoint
    pub burst_size: u32,
}

/// CORS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig {
    /// Enable CORS
    pub enabled: bool,
    /// Allowed origins
    pub allowed_origins: Vec<String>,
    /// Allowed methods
    pub allowed_methods: Vec<String>,
    /// Allowed headers
    pub allowed_headers: Vec<String>,
    /// Allow credentials
    pub allow_credentials: bool,
    /// Max age for preflight requests
    pub max_age_seconds: u64,
}

/// Monitoring and metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Enable Prometheus metrics
    pub metrics_enabled: bool,
    /// Metrics endpoint path
    pub metrics_path: String,
    /// Enable request tracing
    pub tracing_enabled: bool,
    /// Health check endpoint path
    pub health_path: String,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 8080,
                timeout_seconds: 30,
                max_body_size: 1024 * 1024, // 1MB
                compression: true,
                tls: None,
            },
            services: ServiceEndpoints {
                auth_service: "http://127.0.0.1:50051".to_string(),
                execution_service: "http://127.0.0.1:50052".to_string(),
                market_data_service: "http://127.0.0.1:50053".to_string(),
                risk_service: "http://127.0.0.1:50054".to_string(),
                portfolio_service: Some("http://127.0.0.1:50055".to_string()),
                reporting_service: Some("http://127.0.0.1:50056".to_string()),
            },
            auth: AuthConfig {
                jwt_secret: "your-secret-key-here".to_string(),
                token_expiry_seconds: 3600,              // 1 hour
                refresh_token_expiry_seconds: 86400 * 7, // 7 days
                allowed_algorithms: vec!["HS256".to_string()],
            },
            rate_limiting: RateLimitConfig {
                enabled: true,
                requests_per_minute: 60,
                burst_size: 10,
                endpoint_limits: FxHashMap::default(),
            },
            cors: CorsConfig {
                enabled: true,
                allowed_origins: vec!["*".to_string()],
                allowed_methods: vec![
                    "GET".to_string(),
                    "POST".to_string(),
                    "PUT".to_string(),
                    "DELETE".to_string(),
                    "OPTIONS".to_string(),
                ],
                allowed_headers: vec![
                    "Authorization".to_string(),
                    "Content-Type".to_string(),
                    "X-Requested-With".to_string(),
                ],
                allow_credentials: true,
                max_age_seconds: 86400, // 24 hours
            },
            monitoring: MonitoringConfig {
                metrics_enabled: true,
                metrics_path: "/metrics".to_string(),
                tracing_enabled: true,
                health_path: "/health".to_string(),
            },
        }
    }
}

impl GatewayConfig {
    /// Load configuration from file
    pub fn from_file(path: &str) -> Result<Self> {
        let settings = config::Config::builder()
            .add_source(config::File::with_name(path))
            .add_source(config::Environment::with_prefix("GATEWAY"))
            .build()?;

        Ok(settings.try_deserialize()?)
    }

    /// Get server address
    #[must_use] pub fn server_address(&self) -> String {
        format!("{}:{}", self.server.host, self.server.port)
    }
}
