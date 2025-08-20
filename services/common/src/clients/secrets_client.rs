//! Secrets Manager client for secure credential retrieval

use crate::proto::secrets::v1::{
    secrets_service_client::SecretsServiceClient,
    GetCredentialRequest, GetCredentialResponse,
    StoreCredentialRequest, StoreCredentialResponse,
    ListKeysRequest, ListKeysResponse,
};
use anyhow::{Result, Context};
use std::collections::HashMap;
use tonic::transport::Channel;
use tracing::{info, warn, error};

/// Client for interacting with the Secrets Manager service
#[derive(Clone)]
pub struct SecretsClient {
    client: SecretsServiceClient<Channel>,
    service_name: String,
}

impl std::fmt::Debug for SecretsClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretsClient")
            .field("service_name", &self.service_name)
            .field("client", &"SecretsServiceClient")
            .finish()
    }
}

impl SecretsClient {
    /// Create a new secrets client
    pub async fn new(endpoint: &str, service_name: &str) -> Result<Self> {
        let channel = Channel::from_shared(endpoint.to_string())?
            .connect()
            .await
            .context("Failed to connect to secrets service")?;
        
        let client = SecretsServiceClient::new(channel);
        
        Ok(Self {
            client,
            service_name: service_name.to_string(),
        })
    }
    
    /// Get a credential from the secrets manager
    pub async fn get_credential(&mut self, key: &str) -> Result<String> {
        info!("Fetching credential: {} for service: {}", key, self.service_name);
        
        let request = tonic::Request::new(GetCredentialRequest {
            key: key.to_string(),
            service_name: self.service_name.clone(),
        });
        
        let response = self.client
            .get_credential(request)
            .await
            .context("Failed to get credential from secrets service")?;
        
        let inner = response.into_inner();
        
        if !inner.found {
            return Err(anyhow::anyhow!("Credential not found: {}", key));
        }
        
        Ok(inner.value)
    }
    
    /// Store a credential in the secrets manager
    pub async fn store_credential(&mut self, key: &str, value: &str) -> Result<()> {
        warn!("Storing credential: {} for service: {}", key, self.service_name);
        
        let request = tonic::Request::new(StoreCredentialRequest {
            key: key.to_string(),
            value: value.to_string(),
            service_name: self.service_name.clone(),
            metadata: HashMap::new(),
        });
        
        let response = self.client
            .store_credential(request)
            .await
            .context("Failed to store credential")?;
        
        let inner = response.into_inner();
        
        if !inner.success {
            return Err(anyhow::anyhow!("Failed to store credential: {}", inner.message));
        }
        
        Ok(())
    }
    
    /// Get multiple credentials at once
    pub async fn get_credentials(&mut self, keys: &[&str]) -> Result<HashMap<String, String>> {
        let mut credentials = HashMap::new();
        
        for key in keys {
            match self.get_credential(key).await {
                Ok(value) => {
                    credentials.insert(key.to_string(), value);
                }
                Err(e) => {
                    error!("Failed to get credential {}: {}", key, e);
                    // Continue with other credentials
                }
            }
        }
        
        Ok(credentials)
    }
    
    /// List available credential keys
    pub async fn list_keys(&mut self) -> Result<Vec<String>> {
        let request = tonic::Request::new(ListKeysRequest {
            service_filter: self.service_name.clone(),
        });
        
        let response = self.client
            .list_keys(request)
            .await
            .context("Failed to list keys")?;
        
        Ok(response.into_inner().keys)
    }
}

/// Builder for creating a SecretsClient with default configuration
pub struct SecretsClientBuilder {
    endpoint: String,
    service_name: String,
}

impl SecretsClientBuilder {
    /// Create a new builder with default endpoint
    pub fn new(service_name: &str) -> Self {
        Self {
            endpoint: "http://127.0.0.1:50053".to_string(),
            service_name: service_name.to_string(),
        }
    }
    
    /// Set a custom endpoint
    pub fn endpoint(mut self, endpoint: &str) -> Self {
        self.endpoint = endpoint.to_string();
        self
    }
    
    /// Build the client
    pub async fn build(self) -> Result<SecretsClient> {
        SecretsClient::new(&self.endpoint, &self.service_name).await
    }
}

/// Convenience function to get credentials with fallback to environment variables
pub async fn get_credential_with_fallback(
    client: &mut SecretsClient,
    key: &str,
    env_var: &str,
) -> Result<String> {
    // Try secrets manager first
    match client.get_credential(key).await {
        Ok(value) => Ok(value),
        Err(_) => {
            // Fall back to environment variable
            std::env::var(env_var)
                .context(format!("Credential {} not found in secrets manager or environment", key))
        }
    }
}