//! Unit tests for gRPC authentication service

use super::test_utils::*;
use auth_service::grpc::AuthServiceGrpc;
use auth_service::{AuthService, Permission};
use services_common::proto::auth::v1::{
    LoginRequest, ValidateTokenRequest, RefreshTokenRequest, RevokeTokenRequest,
    GetPermissionsRequest, Permission as ProtoPermission,
    auth_service_server::AuthService as GrpcAuthService,
};
use std::time::Duration;
use std::sync::Arc;
use tonic::{Request, Status};

#[tokio::test]
async fn test_grpc_login_success() {
    let mock_service = Arc::new(MockAuthService::new());
    let context = create_test_auth_context("test_user");
    
    mock_service.add_user("test_user".to_string(), context.clone()).await;
    
    let grpc_service = AuthServiceGrpc::new(mock_service);
    
    let request = Request::new(LoginRequest {
        username: "test_user".to_string(),
        password: "password".to_string(),
        exchange: "binance".to_string(),
    });
    
    let response = grpc_service.login(request).await;
    assert!(response.is_ok());
    
    let login_response = response.unwrap().into_inner();
    assert!(!login_response.token.is_empty());
    assert!(!login_response.refresh_token.is_empty());
    assert!(login_response.expires_at > 0);
    assert!(!login_response.permissions.is_empty());
    
    // Should have ReadMarketData and PlaceOrders permissions (from test context)
    assert!(login_response.permissions.contains(&(ProtoPermission::ReadMarketData as i32)));
    assert!(login_response.permissions.contains(&(ProtoPermission::PlaceOrders as i32)));
}

#[tokio::test]
async fn test_grpc_login_failure() {
    let mock_service = Arc::new(MockAuthService::new());
    mock_service.set_should_fail(true).await;
    
    let grpc_service = AuthServiceGrpc::new(mock_service);
    
    let request = Request::new(LoginRequest {
        username: "nonexistent_user".to_string(),
        password: "password".to_string(),
        exchange: "binance".to_string(),
    });
    
    let response = grpc_service.login(request).await;
    assert!(response.is_err());
    
    let status = response.err().unwrap();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
    assert!(status.message().contains("Mock authentication failure"));
}

#[tokio::test]
async fn test_grpc_validate_token_success() {
    let mock_service = Arc::new(MockAuthService::new());
    let context = create_test_auth_context("token_user");
    
    // Generate a token first
    let token = mock_service.generate_token(&context).await.unwrap();
    
    let grpc_service = AuthServiceGrpc::new(mock_service);
    
    let request = Request::new(ValidateTokenRequest { token });
    
    let response = grpc_service.validate_token(request).await;
    assert!(response.is_ok());
    
    let validate_response = response.unwrap().into_inner();
    assert!(validate_response.valid);
    assert_eq!(validate_response.user_id, "token_user");
    assert!(!validate_response.permissions.is_empty());
    
    // Check specific permissions
    assert!(validate_response.permissions.contains(&(ProtoPermission::ReadMarketData as i32)));
    assert!(validate_response.permissions.contains(&(ProtoPermission::PlaceOrders as i32)));
}

#[tokio::test]
async fn test_grpc_validate_token_failure() {
    let mock_service = Arc::new(MockAuthService::new());
    let grpc_service = AuthServiceGrpc::new(mock_service);
    
    let request = Request::new(ValidateTokenRequest {
        token: "invalid_token".to_string(),
    });
    
    let response = grpc_service.validate_token(request).await;
    assert!(response.is_ok()); // gRPC call succeeds, but validation fails
    
    let validate_response = response.unwrap().into_inner();
    assert!(!validate_response.valid);
    assert!(validate_response.user_id.is_empty());
    assert!(validate_response.permissions.is_empty());
}

#[tokio::test]
async fn test_grpc_refresh_token_success() {
    let mock_service = Arc::new(MockAuthService::new());
    let context = create_test_auth_context("refresh_user");
    
    // Generate initial token (simulate refresh token)
    let refresh_token = mock_service.generate_token(&context).await.unwrap();
    
    let grpc_service = AuthServiceGrpc::new(mock_service);
    
    let original_refresh_token = refresh_token.clone();
    let request = Request::new(RefreshTokenRequest { refresh_token });
    
    let response = grpc_service.refresh_token(request).await;
    assert!(response.is_ok());
    
    let refresh_response = response.unwrap().into_inner();
    assert!(!refresh_response.token.is_empty());
    assert!(!refresh_response.refresh_token.is_empty());
    assert!(refresh_response.expires_at > 0);
    
    // New refresh token should be different from the old one
    assert_ne!(refresh_response.refresh_token, original_refresh_token);
}

#[tokio::test]
async fn test_grpc_refresh_token_failure() {
    let mock_service = Arc::new(MockAuthService::new());
    let grpc_service = AuthServiceGrpc::new(mock_service);
    
    let request = Request::new(RefreshTokenRequest {
        refresh_token: "invalid_refresh_token".to_string(),
    });
    
    let response = grpc_service.refresh_token(request).await;
    assert!(response.is_err());
    
    let status = response.err().unwrap();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
    assert!(status.message().contains("Invalid refresh token"));
}

#[tokio::test]
async fn test_grpc_revoke_token_success() {
    let mock_service = Arc::new(MockAuthService::new());
    let context = create_test_auth_context("revoke_user");
    
    // Generate a token to revoke
    let token = mock_service.generate_token(&context).await.unwrap();
    
    let grpc_service = AuthServiceGrpc::new(mock_service.clone());
    
    let request = Request::new(RevokeTokenRequest { token: token.clone() });
    
    let response = grpc_service.revoke_token(request).await;
    assert!(response.is_ok());
    
    let revoke_response = response.unwrap().into_inner();
    assert!(revoke_response.success);
    
    // Verify token is actually revoked
    let validate_result = mock_service.validate_token(&token).await;
    assert!(validate_result.is_err());
}

#[tokio::test]
async fn test_grpc_get_permissions() {
    let mock_service = Arc::new(MockAuthService::new());
    let grpc_service = AuthServiceGrpc::new(mock_service);
    
    let request = Request::new(GetPermissionsRequest {
        user_id: "test_user".to_string(),
    });
    
    let response = grpc_service.get_permissions(request).await;
    assert!(response.is_ok());
    
    let permissions_response = response.unwrap().into_inner();
    assert!(!permissions_response.permissions.is_empty());
    
    // Should have default permissions
    assert!(permissions_response.permissions.contains(&(ProtoPermission::ReadMarketData as i32)));
    assert!(permissions_response.permissions.contains(&(ProtoPermission::ViewPositions as i32)));
}

#[tokio::test]
async fn test_grpc_permission_conversion() {
    // Test internal to proto permission conversion
    let internal_permissions = vec![
        Permission::ReadMarketData,
        Permission::PlaceOrders,
        Permission::CancelOrders,
        Permission::ViewPositions,
        Permission::ModifyRiskLimits,
        Permission::Admin,
    ];
    
    let proto_permissions: Vec<i32> = internal_permissions
        .iter()
        .map(|p| {
            match p {
                Permission::ReadMarketData => ProtoPermission::ReadMarketData as i32,
                Permission::PlaceOrders => ProtoPermission::PlaceOrders as i32,
                Permission::CancelOrders => ProtoPermission::CancelOrders as i32,
                Permission::ViewPositions => ProtoPermission::ViewPositions as i32,
                Permission::ModifyRiskLimits => ProtoPermission::ModifyRiskLimits as i32,
                Permission::Admin => ProtoPermission::Admin as i32,
            }
        })
        .collect();
    
    // Verify all permissions are converted correctly
    assert_eq!(proto_permissions.len(), 6);
    assert!(proto_permissions.contains(&(ProtoPermission::ReadMarketData as i32)));
    assert!(proto_permissions.contains(&(ProtoPermission::PlaceOrders as i32)));
    assert!(proto_permissions.contains(&(ProtoPermission::CancelOrders as i32)));
    assert!(proto_permissions.contains(&(ProtoPermission::ViewPositions as i32)));
    assert!(proto_permissions.contains(&(ProtoPermission::ModifyRiskLimits as i32)));
    assert!(proto_permissions.contains(&(ProtoPermission::Admin as i32)));
}

#[tokio::test]
async fn test_grpc_empty_username_login() {
    let mock_service = Arc::new(MockAuthService::new());
    let grpc_service = AuthServiceGrpc::new(mock_service);
    
    let request = Request::new(LoginRequest {
        username: "".to_string(),
        password: "password".to_string(),
        exchange: "binance".to_string(),
    });
    
    let response = grpc_service.login(request).await;
    assert!(response.is_err());
    
    let status = response.err().unwrap();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn test_grpc_empty_token_validation() {
    let mock_service = Arc::new(MockAuthService::new());
    let grpc_service = AuthServiceGrpc::new(mock_service);
    
    let request = Request::new(ValidateTokenRequest {
        token: "".to_string(),
    });
    
    let response = grpc_service.validate_token(request).await;
    assert!(response.is_ok()); // gRPC succeeds but validation fails
    
    let validate_response = response.unwrap().into_inner();
    assert!(!validate_response.valid);
    assert!(validate_response.user_id.is_empty());
    assert!(validate_response.permissions.is_empty());
}

#[tokio::test]
async fn test_grpc_admin_user_permissions() {
    let mock_service = Arc::new(MockAuthService::new());
    let admin_context = create_admin_auth_context("admin_user");
    
    mock_service.add_user("admin_user".to_string(), admin_context).await;
    
    let grpc_service = AuthServiceGrpc::new(mock_service);
    
    let request = Request::new(LoginRequest {
        username: "admin_user".to_string(),
        password: "admin_password".to_string(),
        exchange: "binance".to_string(),
    });
    
    let response = grpc_service.login(request).await.unwrap();
    let login_response = response.into_inner();
    
    // Admin should have Admin permission
    assert!(login_response.permissions.contains(&(ProtoPermission::Admin as i32)));
}

#[tokio::test]
async fn test_grpc_limited_user_permissions() {
    let mock_service = Arc::new(MockAuthService::new());
    let limited_context = create_limited_auth_context("limited_user");
    
    mock_service.add_user("limited_user".to_string(), limited_context).await;
    
    let grpc_service = AuthServiceGrpc::new(mock_service);
    
    let request = Request::new(LoginRequest {
        username: "limited_user".to_string(),
        password: "password".to_string(),
        exchange: "binance".to_string(),
    });
    
    let response = grpc_service.login(request).await.unwrap();
    let login_response = response.into_inner();
    
    // Should have only ReadMarketData permission
    assert!(login_response.permissions.contains(&(ProtoPermission::ReadMarketData as i32)));
    assert!(!login_response.permissions.contains(&(ProtoPermission::PlaceOrders as i32)));
    assert!(!login_response.permissions.contains(&(ProtoPermission::Admin as i32)));
}

#[tokio::test]
async fn test_grpc_concurrent_requests() {
    use std::sync::Arc;
    use tokio::task;

    let mock_service = Arc::new(MockAuthService::new());
    let context = create_test_auth_context("concurrent_user");
    mock_service.add_user("concurrent_user".to_string(), context).await;
    
    let grpc_service = Arc::new(AuthServiceGrpc::new(mock_service));
    let mut handles = Vec::new();
    
    // Spawn multiple concurrent login requests
    for i in 0..10 {
        let service = Arc::clone(&grpc_service);
        let handle = task::spawn(async move {
            let request = Request::new(LoginRequest {
                username: "concurrent_user".to_string(),
                password: format!("password_{}", i),
                exchange: "binance".to_string(),
    });
            
            let response = service.login(request).await;
            (i, response.is_ok())
        });
        handles.push(handle);
    }
    
    // Wait for all requests to complete
    let results = futures::future::try_join_all(handles).await.unwrap();
    
    // All requests should succeed
    for (i, success) in results {
        assert!(success, "Request {} failed", i);
    }
}

#[tokio::test]
async fn test_grpc_token_lifecycle_integration() {
    let mock_service = Arc::new(MockAuthService::new());
    let context = create_test_auth_context("lifecycle_user");
    mock_service.add_user("lifecycle_user".to_string(), context).await;
    
    let grpc_service = AuthServiceGrpc::new(mock_service);
    
    // 1. Login to get token
    let login_request = Request::new(LoginRequest {
        username: "lifecycle_user".to_string(),
        password: "password".to_string(),
        exchange: "binance".to_string(),
    });
    
    let login_response = grpc_service.login(login_request).await.unwrap().into_inner();
    let token = login_response.token;
    let refresh_token = login_response.refresh_token;
    
    // 2. Validate the token
    let validate_request = Request::new(ValidateTokenRequest { token: token.clone() });
    let validate_response = grpc_service.validate_token(validate_request).await.unwrap().into_inner();
    
    assert!(validate_response.valid);
    assert_eq!(validate_response.user_id, "lifecycle_user");
    
    // 3. Refresh the token
    let refresh_request = Request::new(RefreshTokenRequest { refresh_token });
    let refresh_response = grpc_service.refresh_token(refresh_request).await.unwrap().into_inner();
    
    let new_token = refresh_response.token;
    assert_ne!(new_token, token); // Should be different token
    
    // 4. Revoke the original token
    let revoke_request = Request::new(RevokeTokenRequest { token });
    let revoke_response = grpc_service.revoke_token(revoke_request).await.unwrap().into_inner();
    
    assert!(revoke_response.success);
    
    // 5. New token should still be valid
    let validate_new_request = Request::new(ValidateTokenRequest { token: new_token });
    let validate_new_response = grpc_service.validate_token(validate_new_request).await.unwrap().into_inner();
    
    assert!(validate_new_response.valid);
}

#[tokio::test]
async fn test_grpc_error_handling_edge_cases() {
    let mock_service = Arc::new(MockAuthService::new());
    let grpc_service = AuthServiceGrpc::new(mock_service.clone());
    
    // Test with extremely long username
    let long_username = "a".repeat(10000);
    let request = Request::new(LoginRequest {
        username: long_username,
        password: "password".to_string(),
        exchange: "binance".to_string(),
    });
    
    let response = grpc_service.login(request).await;
    assert!(response.is_err()); // Should fail for non-existent user
    
    // Test with unicode username
    let unicode_request = Request::new(LoginRequest {
        username: "用户测试".to_string(),
        password: "password".to_string(),
        exchange: "binance".to_string(),
    });
    
    let unicode_response = grpc_service.login(unicode_request).await;
    assert!(unicode_response.is_err()); // Should fail for non-existent user
    
    // Test with special characters
    let special_request = Request::new(LoginRequest {
        username: "user@test.com".to_string(),
        password: "p@$$w0rd!".to_string(),
        exchange: "binance".to_string(),
    });
    
    let special_response = grpc_service.login(special_request).await;
    assert!(special_response.is_err()); // Should fail for non-existent user
}

#[tokio::test]
async fn test_grpc_service_with_failing_backend() {
    let mock_service = Arc::new(MockAuthService::new());
    mock_service.set_should_fail(true).await;
    
    let grpc_service = AuthServiceGrpc::new(mock_service);
    
    // All operations should fail gracefully
    let login_request = Request::new(LoginRequest {
        username: "test_user".to_string(),
        password: "password".to_string(),
        exchange: "binance".to_string(),
    });
    
    let login_response = grpc_service.login(login_request).await;
    assert!(login_response.is_err());
    assert_eq!(login_response.err().unwrap().code(), tonic::Code::Unauthenticated);
    
    // Token validation should return invalid rather than error
    let validate_request = Request::new(ValidateTokenRequest {
        token: "any_token".to_string(),
    });
    
    let validate_response = grpc_service.validate_token(validate_request).await;
    assert!(validate_response.is_ok());
    
    let validate_result = validate_response.unwrap().into_inner();
    assert!(!validate_result.valid);
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn test_grpc_service_performance() {
        let mock_service = Arc::new(MockAuthService::new());
        let context = create_test_auth_context("perf_user");
        mock_service.add_user("perf_user".to_string(), context).await;
        
        let grpc_service = AuthServiceGrpc::new(mock_service);
        
        // Benchmark login operations
        let start = Instant::now();
        let mut tokens = Vec::new();
        
        for i in 0..100 {
            let request = Request::new(LoginRequest {
                username: "perf_user".to_string(),
                password: format!("password_{}", i),
                exchange: "binance".to_string(),
    });
            
            let response = grpc_service.login(request).await.unwrap();
            tokens.push(response.into_inner().token);
        }
        
        let login_duration = start.elapsed();
        println!("100 gRPC logins: {:?}", login_duration);
        
        // Benchmark token validation
        let start = Instant::now();
        
        for token in &tokens {
            let request = Request::new(ValidateTokenRequest {
                token: token.clone(),
            });
            
            let response = grpc_service.validate_token(request).await.unwrap();
            assert!(response.into_inner().valid);
        }
        
        let validation_duration = start.elapsed();
        println!("100 gRPC token validations: {:?}", validation_duration);
        
        // Performance assertions
        assert!(login_duration < Duration::from_secs(2));
        assert!(validation_duration < Duration::from_secs(1));
    }
}