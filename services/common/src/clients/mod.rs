//! gRPC client wrappers for inter-service communication

pub mod auth_client;
pub mod data_aggregator_client;
pub mod execution_client;
pub mod market_data_client;
pub mod risk_client;

pub use auth_client::AuthClient;
pub use data_aggregator_client::DataAggregatorClient;
pub use execution_client::ExecutionClient;
pub use market_data_client::MarketDataClient;
pub use risk_client::RiskClient;
