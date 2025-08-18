//! `ShrivenQuant` Protocol Buffer definitions
//!
//! This module contains all gRPC service definitions and message types
//! for inter-service communication in the `ShrivenQuant` platform.

// Include the generated proto code
pub mod auth {
    pub mod v1 {
        tonic::include_proto!("shrivenquant.auth.v1");
    }
}

pub mod marketdata {
    pub mod v1 {
        tonic::include_proto!("shrivenquant.marketdata.v1");
    }
}

pub mod risk {
    pub mod v1 {
        tonic::include_proto!("shrivenquant.risk.v1");
    }
}

pub mod execution {
    pub mod v1 {
        tonic::include_proto!("shrivenquant.execution.v1");
    }
}

pub mod trading {
    pub mod v1 {
        tonic::include_proto!("shrivenquant.trading.v1");
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