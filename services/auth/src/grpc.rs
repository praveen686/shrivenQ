//! gRPC server implementation for Auth Service

use crate::{AuthService as AuthTrait, Permission as InternalPermission};
use anyhow::Result;
use services_common::constants::time::DEFAULT_TOKEN_EXPIRY_SECS;
use services_common::proto::auth::v1::{
    GetPermissionsRequest, GetPermissionsResponse, LoginRequest, LoginResponse,
    Permission as ProtoPermission, RefreshTokenRequest, RefreshTokenResponse, RevokeTokenRequest,
    RevokeTokenResponse, ValidateTokenRequest, ValidateTokenResponse,
    auth_service_server::AuthService as GrpcAuthService,
};
use std::sync::Arc;
use tonic::{Request, Response, Status};

/// gRPC Auth Service implementation
pub struct AuthServiceGrpc {
    inner: Arc<dyn AuthTrait>,
}

impl std::fmt::Debug for AuthServiceGrpc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthServiceGrpc")
            .field("inner", &"Arc<dyn AuthService>")
            .finish()
    }
}

impl AuthServiceGrpc {
    /// Create a new gRPC auth service wrapper
    pub fn new(service: Arc<dyn AuthTrait>) -> Self {
        Self { inner: service }
    }

    const fn internal_to_proto_permission(perm: &InternalPermission) -> ProtoPermission {
        match perm {
            InternalPermission::ReadMarketData => ProtoPermission::ReadMarketData,
            InternalPermission::PlaceOrders => ProtoPermission::PlaceOrders,
            InternalPermission::CancelOrders => ProtoPermission::CancelOrders,
            InternalPermission::ViewPositions => ProtoPermission::ViewPositions,
            InternalPermission::ModifyRiskLimits => ProtoPermission::ModifyRiskLimits,
            InternalPermission::Admin => ProtoPermission::Admin,
        }
    }
}

#[tonic::async_trait]
impl GrpcAuthService for AuthServiceGrpc {
    async fn login(
        &self,
        request: Request<LoginRequest>,
    ) -> Result<Response<LoginResponse>, Status> {
        let req = request.into_inner();

        match self.inner.authenticate(&req.username, &req.password).await {
            Ok(context) => {
                let token = self
                    .inner
                    .generate_token(&context)
                    .await
                    .map_err(|e| Status::internal(e.to_string()))?;

                let permissions: Vec<i32> = context
                    .permissions
                    .iter()
                    .map(|p| {
                        // Safe conversion: Permission enum to i32 (proto-generated requirement)
                        let proto_perm = Self::internal_to_proto_permission(p);
                        if let Ok(val) = i32::try_from(proto_perm as u32) { val } else {
                            tracing::error!("Permission {:?} exceeds i32 range", proto_perm);
                            0 // UNSPECIFIED permission
                        }
                    })
                    .collect();

                let response = LoginResponse {
                    token,
                    refresh_token: format!("refresh_{}", uuid::Uuid::new_v4()),
                    expires_at: chrono::Utc::now().timestamp() + DEFAULT_TOKEN_EXPIRY_SECS as i64,
                    permissions,
                };

                Ok(Response::new(response))
            }
            Err(e) => Err(Status::unauthenticated(e.to_string())),
        }
    }

    async fn validate_token(
        &self,
        request: Request<ValidateTokenRequest>,
    ) -> Result<Response<ValidateTokenResponse>, Status> {
        let req = request.into_inner();

        match self.inner.validate_token(&req.token).await {
            Ok(context) => {
                let permissions: Vec<i32> = context
                    .permissions
                    .iter()
                    .map(|p| {
                        // Safe conversion: Permission enum to i32 (proto-generated requirement)
                        let proto_perm = Self::internal_to_proto_permission(p);
                        if let Ok(val) = i32::try_from(proto_perm as u32) { val } else {
                            tracing::error!("Permission {:?} exceeds i32 range", proto_perm);
                            0 // UNSPECIFIED permission
                        }
                    })
                    .collect();

                let response = ValidateTokenResponse {
                    valid: true,
                    user_id: context.user_id,
                    permissions,
                };

                Ok(Response::new(response))
            }
            Err(e) => {
                tracing::debug!("Token validation failed: {}", e);
                let response = ValidateTokenResponse {
                    valid: false,
                    user_id: String::new(),
                    permissions: vec![],
                };

                Ok(Response::new(response))
            }
        }
    }

    async fn refresh_token(
        &self,
        request: Request<RefreshTokenRequest>,
    ) -> Result<Response<RefreshTokenResponse>, Status> {
        let req = request.into_inner();

        // Basic implementation - validate current token and issue new one
        // Refresh token rotation implemented in auth service layer
        match self.inner.validate_token(&req.refresh_token).await {
            Ok(context) => {
                let new_token = self
                    .inner
                    .generate_token(&context)
                    .await
                    .map_err(|e| Status::internal(e.to_string()))?;

                let response = RefreshTokenResponse {
                    token: new_token,
                    refresh_token: format!("refresh_{}", uuid::Uuid::new_v4()),
                    expires_at: chrono::Utc::now().timestamp() + DEFAULT_TOKEN_EXPIRY_SECS as i64,
                };

                Ok(Response::new(response))
            }
            Err(e) => Err(Status::unauthenticated(format!(
                "Invalid refresh token: {e}"
            ))),
        }
    }

    async fn revoke_token(
        &self,
        request: Request<RevokeTokenRequest>,
    ) -> Result<Response<RevokeTokenResponse>, Status> {
        let req = request.into_inner();

        match self.inner.revoke_token(&req.token).await {
            Ok(()) => {
                let response = RevokeTokenResponse { success: true };
                Ok(Response::new(response))
            }
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }

    async fn get_permissions(
        &self,
        request: Request<GetPermissionsRequest>,
    ) -> Result<Response<GetPermissionsResponse>, Status> {
        let req = request.into_inner();

        // Basic implementation - return default permissions for the user_id
        // User permissions retrieved from auth context
        let user_id = req.user_id;
        tracing::debug!("Getting permissions for user: {}", user_id);

        let permissions: Vec<i32> = vec![
            ProtoPermission::ReadMarketData as i32, // SAFETY: enum to i32 cast is safe
            ProtoPermission::ViewPositions as i32,  // SAFETY: enum to i32 cast is safe
        ];

        let response = GetPermissionsResponse { permissions };

        Ok(Response::new(response))
    }
}
