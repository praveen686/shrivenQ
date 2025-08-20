//! Execution service gRPC client wrapper

use anyhow::Result;
use crate::proto::execution::v1::{
    CancelOrderRequest, CancelOrderResponse, GetOrderRequest, GetOrderResponse, SubmitOrderRequest,
    SubmitOrderResponse, execution_service_client::ExecutionServiceClient as GrpcClient,
};
use tonic::transport::Channel;
use tracing::{debug, error};

/// Execution service client wrapper
#[derive(Debug)]
pub struct ExecutionClient {
    client: GrpcClient<Channel>,
    endpoint: String,
}

impl ExecutionClient {
    /// Create new execution client
    pub async fn new(endpoint: &str) -> Result<Self> {
        debug!("Connecting to execution service at {}", endpoint);
        let channel = Channel::from_shared(endpoint.to_string())?
            .connect()
            .await?;

        Ok(Self {
            client: GrpcClient::new(channel),
            endpoint: endpoint.to_string(),
        })
    }

    /// Submit order
    pub async fn submit_order(
        &mut self,
        order_request: SubmitOrderRequest,
    ) -> Result<SubmitOrderResponse> {
        let request = tonic::Request::new(order_request);

        debug!("Submitting order");
        match self.client.submit_order(request).await {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => {
                error!("Submit order failed: {}", e);
                Err(e.into())
            }
        }
    }

    /// Cancel order
    pub async fn cancel_order(
        &mut self,
        order_id: i64,
        client_order_id: Option<String>,
    ) -> Result<CancelOrderResponse> {
        let request = tonic::Request::new(CancelOrderRequest {
            order_id,
            client_order_id: client_order_id.unwrap_or_default(),
        });

        debug!("Cancelling order");
        match self.client.cancel_order(request).await {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => {
                error!("Cancel order failed: {}", e);
                Err(e.into())
            }
        }
    }

    /// Get order status
    pub async fn get_order(
        &mut self,
        order_id: i64,
        client_order_id: Option<String>,
    ) -> Result<GetOrderResponse> {
        let request = tonic::Request::new(GetOrderRequest {
            order_id,
            client_order_id: client_order_id.unwrap_or_default(),
        });

        debug!("Getting order status");
        match self.client.get_order(request).await {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => {
                error!("Get order failed: {}", e);
                Err(e.into())
            }
        }
    }

    /// Get endpoint
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}
