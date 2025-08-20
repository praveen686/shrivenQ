//! gRPC service implementation for secrets management

use anyhow::Result;
use services_common::proto::secrets::v1::{
    secrets_service_server::{SecretsService, SecretsServiceServer},
    StoreCredentialRequest, StoreCredentialResponse,
    GetCredentialRequest, GetCredentialResponse,
    ListKeysRequest, ListKeysResponse,
    DeleteCredentialRequest, DeleteCredentialResponse,
    RotateKeysRequest, RotateKeysResponse,
    HealthCheckRequest, HealthCheckResponse,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use tracing::{info, warn, error};
use crate::SecretsManager;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct SecretsServiceImpl {
    manager: Arc<RwLock<SecretsManager>>,
    start_time: SystemTime,
    credentials_cache: Arc<RwLock<HashMap<String, String>>>,
}

impl SecretsServiceImpl {
    pub fn new(master_password: &str) -> Result<Self> {
        let manager = SecretsManager::new(master_password)?;
        Ok(Self {
            manager: Arc::new(RwLock::new(manager)),
            start_time: SystemTime::now(),
            credentials_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }
}

#[tonic::async_trait]
impl SecretsService for SecretsServiceImpl {
    async fn store_credential(
        &self,
        request: Request<StoreCredentialRequest>,
    ) -> Result<Response<StoreCredentialResponse>, Status> {
        let req = request.into_inner();
        
        info!(
            "Storing credential for key: {} from service: {}",
            req.key, req.service_name
        );
        
        let manager = self.manager.write().await;
        match manager.store_credential(&req.key, &req.value) {
            Ok(_) => {
                // Update cache
                let mut cache = self.credentials_cache.write().await;
                cache.insert(req.key.clone(), req.value);
                
                Ok(Response::new(StoreCredentialResponse {
                    success: true,
                    message: format!("Credential {} stored successfully", req.key),
                }))
            }
            Err(e) => {
                error!("Failed to store credential: {}", e);
                Ok(Response::new(StoreCredentialResponse {
                    success: false,
                    message: format!("Failed to store credential: {}", e),
                }))
            }
        }
    }
    
    async fn get_credential(
        &self,
        request: Request<GetCredentialRequest>,
    ) -> Result<Response<GetCredentialResponse>, Status> {
        let req = request.into_inner();
        
        info!(
            "Retrieving credential for key: {} from service: {}",
            req.key, req.service_name
        );
        
        // Check cache first
        {
            let cache = self.credentials_cache.read().await;
            if let Some(value) = cache.get(&req.key) {
                return Ok(Response::new(GetCredentialResponse {
                    value: value.clone(),
                    found: true,
                    last_updated: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64,
                }));
            }
        }
        
        // Not in cache, fetch from storage
        let manager = self.manager.read().await;
        match manager.get_credential(&req.key) {
            Ok(value) => {
                // Update cache
                let mut cache = self.credentials_cache.write().await;
                cache.insert(req.key.clone(), value.clone());
                
                info!("Successfully retrieved credential for key: {}", req.key);
                Ok(Response::new(GetCredentialResponse {
                    value,
                    found: true,
                    last_updated: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64,
                }))
            }
            Err(e) => {
                warn!("Credential not found for key: {} - {}", req.key, e);
                Ok(Response::new(GetCredentialResponse {
                    value: String::new(),
                    found: false,
                    last_updated: 0,
                }))
            }
        }
    }
    
    async fn list_keys(
        &self,
        request: Request<ListKeysRequest>,
    ) -> Result<Response<ListKeysResponse>, Status> {
        let req = request.into_inner();
        
        info!("Listing keys with filter: {:?}", req.service_filter);
        
        // For now, return hardcoded list of available keys
        // In production, this would query the actual storage
        let mut keys = vec![
            "ZERODHA_API_KEY".to_string(),
            "ZERODHA_API_SECRET".to_string(),
            "ZERODHA_USER_ID".to_string(),
            "ZERODHA_PASSWORD".to_string(),
            "ZERODHA_TOTP_SECRET".to_string(),
            "BINANCE_API_KEY".to_string(),
            "BINANCE_API_SECRET".to_string(),
        ];
        
        // Apply service filter if provided
        if !req.service_filter.is_empty() {
            keys.retain(|k| k.starts_with(&req.service_filter.to_uppercase()));
        }
        
        Ok(Response::new(ListKeysResponse { keys }))
    }
    
    async fn delete_credential(
        &self,
        request: Request<DeleteCredentialRequest>,
    ) -> Result<Response<DeleteCredentialResponse>, Status> {
        let req = request.into_inner();
        
        warn!(
            "Deleting credential for key: {} from service: {}",
            req.key, req.service_name
        );
        
        // Remove from cache
        let mut cache = self.credentials_cache.write().await;
        cache.remove(&req.key);
        
        // In production, would also remove from persistent storage
        Ok(Response::new(DeleteCredentialResponse {
            success: true,
            message: format!("Credential {} deleted", req.key),
        }))
    }
    
    async fn rotate_keys(
        &self,
        request: Request<RotateKeysRequest>,
    ) -> Result<Response<RotateKeysResponse>, Status> {
        let req = request.into_inner();
        
        warn!("Key rotation requested (force: {})", req.force);
        
        // Key rotation not implemented yet
        Ok(Response::new(RotateKeysResponse {
            success: false,
            keys_rotated: 0,
            message: "Key rotation not yet implemented".to_string(),
        }))
    }
    
    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        let uptime = SystemTime::now()
            .duration_since(self.start_time)
            .unwrap()
            .as_secs() as i64;
        
        Ok(Response::new(HealthCheckResponse {
            healthy: true,
            status: "Secrets service is running".to_string(),
            uptime_seconds: uptime,
        }))
    }
}

pub fn create_server(master_password: &str) -> Result<SecretsServiceServer<SecretsServiceImpl>> {
    let service = SecretsServiceImpl::new(master_password)?;
    Ok(SecretsServiceServer::new(service))
}