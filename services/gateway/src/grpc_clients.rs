//! gRPC client connections and management

use anyhow::{Result, anyhow};
use tonic::transport::{Channel, Endpoint};
use tracing::{error, info, warn};

// Generated gRPC client code
pub mod auth {
    tonic::include_proto!("shrivenquant.auth.v1");
}

pub mod execution {
    tonic::include_proto!("shrivenquant.execution.v1");
}

pub mod market_data {
    tonic::include_proto!("shrivenquant.marketdata.v1");
}

pub mod risk {
    tonic::include_proto!("shrivenquant.risk.v1");
}

use auth::auth_service_client::AuthServiceClient;
use execution::execution_service_client::ExecutionServiceClient;
use market_data::market_data_service_client::MarketDataServiceClient;
use risk::risk_service_client::RiskServiceClient;

/// gRPC clients manager
#[derive(Clone)]
pub struct GrpcClients {
    /// Authentication service client
    pub auth: AuthServiceClient<Channel>,
    /// Execution service client
    pub execution: ExecutionServiceClient<Channel>,
    /// Market data service client
    pub market_data: MarketDataServiceClient<Channel>,
    /// Risk management service client
    pub risk: RiskServiceClient<Channel>,
}

impl GrpcClients {
    /// Create new gRPC clients with the given endpoints
    pub async fn new(
        auth_endpoint: &str,
        execution_endpoint: &str,
        market_data_endpoint: &str,
        risk_endpoint: &str,
    ) -> Result<Self> {
        info!("Connecting to gRPC services...");

        // Create channels for each service
        let auth_channel = Self::create_channel(auth_endpoint, "auth").await?;
        let execution_channel = Self::create_channel(execution_endpoint, "execution").await?;
        let market_data_channel = Self::create_channel(market_data_endpoint, "market_data").await?;
        let risk_channel = Self::create_channel(risk_endpoint, "risk").await?;

        // Create clients
        let auth = AuthServiceClient::new(auth_channel);
        let execution = ExecutionServiceClient::new(execution_channel);
        let market_data = MarketDataServiceClient::new(market_data_channel);
        let risk = RiskServiceClient::new(risk_channel);

        info!("Successfully connected to all gRPC services");

        Ok(Self {
            auth,
            execution,
            market_data,
            risk,
        })
    }

    /// Create a gRPC channel with retry and timeout configuration
    async fn create_channel(endpoint: &str, service_name: &str) -> Result<Channel> {
        info!("Connecting to {} service at {}", service_name, endpoint);

        let endpoint = Endpoint::from_shared(endpoint.to_string())
            .map_err(|e| anyhow!("Invalid {} endpoint: {}", service_name, e))?
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10))
            .tcp_keepalive(Some(std::time::Duration::from_secs(30)))
            .http2_keep_alive_interval(std::time::Duration::from_secs(30))
            .keep_alive_timeout(std::time::Duration::from_secs(5))
            .keep_alive_while_idle(true);

        // Attempt connection with retry
        for attempt in 1..=3 {
            match endpoint.connect().await {
                Ok(channel) => {
                    info!(
                        "Connected to {} service (attempt {})",
                        service_name, attempt
                    );
                    return Ok(channel);
                }
                Err(e) => {
                    warn!(
                        "Failed to connect to {} service (attempt {}): {}",
                        service_name, attempt, e
                    );
                    if attempt < 3 {
                        tokio::time::sleep(std::time::Duration::from_secs(1 << attempt)).await;
                    } else {
                        error!(
                            "Failed to connect to {} service after 3 attempts",
                            service_name
                        );
                        return Err(anyhow!(
                            "Failed to connect to {} service: {}",
                            service_name,
                            e
                        ));
                    }
                }
            }
        }

        unreachable!()
    }

    /// Health check for all services
    pub async fn health_check(&self) -> Result<HealthStatus> {
        let mut status = HealthStatus::default();

        // Check auth service
        status.auth = self.check_auth_health().await;

        // Check execution service
        status.execution = self.check_execution_health().await;

        // Check market data service
        status.market_data = self.check_market_data_health().await;

        // Check risk service
        status.risk = self.check_risk_health().await;

        status.overall = status.auth && status.execution && status.market_data && status.risk;

        Ok(status)
    }

    async fn check_auth_health(&self) -> bool {
        // Simple health check by calling a lightweight method
        let mut client = self.auth.clone();
        match client
            .get_permissions(auth::GetPermissionsRequest {
                user_id: "health_check".to_string(),
            })
            .await
        {
            Ok(_) => true,
            Err(status) if status.code() == tonic::Code::NotFound => true,
            Err(e) => {
                warn!("Auth service health check failed: {}", e);
                false
            }
        }
    }

    async fn check_execution_health(&self) -> bool {
        let mut client = self.execution.clone();
        match client.get_metrics(execution::GetMetricsRequest {}).await {
            Ok(_) => true,
            Err(e) => {
                warn!("Execution service health check failed: {}", e);
                false
            }
        }
    }

    async fn check_market_data_health(&self) -> bool {
        let mut client = self.market_data.clone();
        match client
            .get_snapshot(market_data::GetSnapshotRequest {
                symbols: vec!["HEALTH_CHECK".to_string()],
                exchange: "test".to_string(),
            })
            .await
        {
            Ok(_) => true,
            Err(status) if status.code() == tonic::Code::NotFound => true,
            Err(e) => {
                warn!("Market data service health check failed: {}", e);
                false
            }
        }
    }

    async fn check_risk_health(&self) -> bool {
        let mut client = self.risk.clone();
        match client.get_metrics(risk::GetMetricsRequest {}).await {
            Ok(_) => true,
            Err(e) => {
                warn!("Risk service health check failed: {}", e);
                false
            }
        }
    }
}

/// Health status for all services
#[derive(Debug, Default, serde::Serialize)]
pub struct HealthStatus {
    pub overall: bool,
    pub auth: bool,
    pub execution: bool,
    pub market_data: bool,
    pub risk: bool,
}
