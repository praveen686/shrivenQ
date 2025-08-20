//! `ShrivenQuant` Protocol Buffer definitions
//!
//! This module contains all gRPC service definitions and message types
//! for inter-service communication in the `ShrivenQuant` platform.

// Include the generated proto code
/// Authentication service protobuf definitions
pub mod auth {
    /// Version 1 of the authentication service API
    #[allow(missing_docs)]
    #[allow(missing_debug_implementations)]
    pub mod v1 {
        tonic::include_proto!("shrivenquant.auth.v1");
    }
}

/// Market data service protobuf definitions
pub mod marketdata {
    /// Version 1 of the market data service API
    #[allow(missing_docs)]
    #[allow(missing_debug_implementations)]
    pub mod v1 {
        tonic::include_proto!("shrivenquant.marketdata.v1");
    }
}

/// Risk management service protobuf definitions
pub mod risk {
    /// Version 1 of the risk management service API
    #[allow(missing_docs)]
    #[allow(missing_debug_implementations)]
    pub mod v1 {
        tonic::include_proto!("shrivenquant.risk.v1");
    }
}

/// Order execution service protobuf definitions
pub mod execution {
    /// Version 1 of the execution service API
    #[allow(missing_docs)]
    #[allow(missing_debug_implementations)]
    pub mod v1 {
        tonic::include_proto!("shrivenquant.execution.v1");
    }
}

/// Trading service protobuf definitions
pub mod trading {
    /// Version 1 of the trading service API
    #[allow(missing_docs)]
    #[allow(missing_debug_implementations)]
    pub mod v1 {
        tonic::include_proto!("shrivenquant.trading.v1");
    }
}

/// Backtesting service protobuf definitions
pub mod backtesting {
    /// Version 1 of the backtesting service API
    #[allow(missing_docs)]
    #[allow(missing_debug_implementations)]
    pub mod v1 {
        tonic::include_proto!("shrivenquant.backtesting.v1");
    }
}

// Re-export commonly used types
pub use auth::v1::{
    auth_service_client::AuthServiceClient,
    auth_service_server::{AuthService, AuthServiceServer},
};

pub use marketdata::v1::{
    market_data_service_client::MarketDataServiceClient,
    market_data_service_server::{MarketDataService, MarketDataServiceServer},
};

pub use risk::v1::{
    risk_service_client::RiskServiceClient,
    risk_service_server::{RiskService, RiskServiceServer},
};

pub use execution::v1::{
    execution_service_client::ExecutionServiceClient,
    execution_service_server::{ExecutionService, ExecutionServiceServer},
};

pub use backtesting::v1::{
    backtesting_service_client::BacktestingServiceClient,
    backtesting_service_server::{BacktestingService, BacktestingServiceServer},
    RunBacktestRequest, RunBacktestResponse,
    GetBacktestStatusRequest, GetBacktestStatusResponse,
    GetBacktestResultsRequest, GetBacktestResultsResponse,
    StopBacktestRequest, StopBacktestResponse,
    ListBacktestsRequest, ListBacktestsResponse,
    PerformanceMetrics, EquityPoint, Trade, BacktestSummary,
};

/// Secrets management service protobuf definitions
pub mod secrets {
    /// Version 1 of the secrets service API
    #[allow(missing_docs)]
    #[allow(missing_debug_implementations)]
    pub mod v1 {
        tonic::include_proto!("shrivenquant.secrets.v1");
    }
}

pub use secrets::v1::{
    secrets_service_client::SecretsServiceClient,
    secrets_service_server::{SecretsService, SecretsServiceServer},
};