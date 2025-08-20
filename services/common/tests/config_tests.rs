//! Comprehensive unit tests for configuration management
//!
//! Tests cover:
//! - Service endpoint configuration
//! - Service discovery configuration
//! - Configuration parsing and validation
//! - Default values and customization
//! - Serialization and deserialization

use services_common::{ServiceDiscoveryConfig, ServiceEndpoints};
use rstest::*;
use serde_json;

// Service Endpoints Tests
#[rstest]
#[test]
fn test_service_endpoints_defaults() {
    let endpoints = ServiceEndpoints::default();
    
    assert_eq!(endpoints.auth_service, "http://localhost:50051");
    assert_eq!(endpoints.market_data_service, "http://localhost:50052");
    assert_eq!(endpoints.risk_service, "http://localhost:50053");
    assert_eq!(endpoints.execution_service, "http://localhost:50054");
    assert_eq!(endpoints.data_aggregator_service, "http://localhost:50055");
}

#[rstest]
#[test]
fn test_service_endpoints_customization() {
    let endpoints = ServiceEndpoints {
        auth_service: "https://auth.example.com:443".to_string(),
        market_data_service: "grpc://market-data.internal:9090".to_string(),
        risk_service: "http://risk-manager:8080".to_string(),
        execution_service: "https://execution.prod:443".to_string(),
        data_aggregator_service: "grpc://aggregator.internal:9091".to_string(),
    };

    assert_eq!(endpoints.auth_service, "https://auth.example.com:443");
    assert_eq!(endpoints.market_data_service, "grpc://market-data.internal:9090");
    assert_eq!(endpoints.risk_service, "http://risk-manager:8080");
    assert_eq!(endpoints.execution_service, "https://execution.prod:443");
    assert_eq!(endpoints.data_aggregator_service, "grpc://aggregator.internal:9091");
}

#[rstest]
#[test]
fn test_service_endpoints_clone() {
    let original = ServiceEndpoints {
        auth_service: "http://auth:8001".to_string(),
        market_data_service: "http://market:8002".to_string(),
        risk_service: "http://risk:8003".to_string(),
        execution_service: "http://exec:8004".to_string(),
        data_aggregator_service: "http://data:8005".to_string(),
    };

    let cloned = original.clone();

    assert_eq!(original.auth_service, cloned.auth_service);
    assert_eq!(original.market_data_service, cloned.market_data_service);
    assert_eq!(original.risk_service, cloned.risk_service);
    assert_eq!(original.execution_service, cloned.execution_service);
    assert_eq!(original.data_aggregator_service, cloned.data_aggregator_service);
}

#[rstest]
#[test]
fn test_service_endpoints_debug() {
    let endpoints = ServiceEndpoints::default();
    let debug_str = format!("{:?}", endpoints);
    
    assert!(debug_str.contains("ServiceEndpoints"));
    assert!(debug_str.contains("auth_service"));
    assert!(debug_str.contains("localhost"));
}

#[rstest]
#[test]
fn test_service_endpoints_serialization() -> Result<(), serde_json::Error> {
    let endpoints = ServiceEndpoints {
        auth_service: "http://test-auth:9001".to_string(),
        market_data_service: "http://test-market:9002".to_string(),
        risk_service: "http://test-risk:9003".to_string(),
        execution_service: "http://test-exec:9004".to_string(),
        data_aggregator_service: "http://test-data:9005".to_string(),
    };

    // Serialize to JSON
    let json = serde_json::to_string(&endpoints)?;
    assert!(json.contains("test-auth:9001"));
    assert!(json.contains("test-market:9002"));

    // Deserialize from JSON
    let deserialized: ServiceEndpoints = serde_json::from_str(&json)?;
    assert_eq!(endpoints.auth_service, deserialized.auth_service);
    assert_eq!(endpoints.market_data_service, deserialized.market_data_service);
    assert_eq!(endpoints.risk_service, deserialized.risk_service);
    assert_eq!(endpoints.execution_service, deserialized.execution_service);
    assert_eq!(endpoints.data_aggregator_service, deserialized.data_aggregator_service);

    Ok(())
}

#[rstest]
#[test]
fn test_service_endpoints_partial_deserialization() -> Result<(), serde_json::Error> {
    // Test that partial JSON works with defaults
    let partial_json = r#"{
        "auth_service": "http://custom-auth:8080",
        "risk_service": "http://custom-risk:8081"
    }"#;

    let endpoints: ServiceEndpoints = serde_json::from_str(partial_json)?;
    
    // Custom values should be preserved
    assert_eq!(endpoints.auth_service, "http://custom-auth:8080");
    assert_eq!(endpoints.risk_service, "http://custom-risk:8081");
    
    // Other values should use defaults
    assert_eq!(endpoints.market_data_service, "http://localhost:50052");
    assert_eq!(endpoints.execution_service, "http://localhost:50054");
    assert_eq!(endpoints.data_aggregator_service, "http://localhost:50055");

    Ok(())
}

// Service Discovery Configuration Tests
#[rstest]
#[test]
fn test_service_discovery_config_defaults() {
    let config = ServiceDiscoveryConfig::default();
    
    assert!(!config.enabled);
    assert!(config.consul_endpoint.is_none());
    assert!(config.etcd_endpoint.is_none());
    assert_eq!(config.refresh_interval_secs, 30);
}

#[rstest]
#[test]
fn test_service_discovery_config_customization() {
    let config = ServiceDiscoveryConfig {
        enabled: true,
        consul_endpoint: Some("http://consul.internal:8500".to_string()),
        etcd_endpoint: Some("http://etcd.cluster:2379".to_string()),
        refresh_interval_secs: 60,
    };

    assert!(config.enabled);
    assert_eq!(config.consul_endpoint.unwrap(), "http://consul.internal:8500");
    assert_eq!(config.etcd_endpoint.unwrap(), "http://etcd.cluster:2379");
    assert_eq!(config.refresh_interval_secs, 60);
}

#[rstest]
#[test]
fn test_service_discovery_config_consul_only() {
    let config = ServiceDiscoveryConfig {
        enabled: true,
        consul_endpoint: Some("https://consul.prod:8501".to_string()),
        etcd_endpoint: None,
        refresh_interval_secs: 15,
    };

    assert!(config.enabled);
    assert!(config.consul_endpoint.is_some());
    assert!(config.etcd_endpoint.is_none());
    assert_eq!(config.refresh_interval_secs, 15);
}

#[rstest]
#[test]
fn test_service_discovery_config_etcd_only() {
    let config = ServiceDiscoveryConfig {
        enabled: true,
        consul_endpoint: None,
        etcd_endpoint: Some("https://etcd-cluster:2380".to_string()),
        refresh_interval_secs: 45,
    };

    assert!(config.enabled);
    assert!(config.consul_endpoint.is_none());
    assert!(config.etcd_endpoint.is_some());
    assert_eq!(config.refresh_interval_secs, 45);
}

#[rstest]
#[test]
fn test_service_discovery_config_clone() {
    let original = ServiceDiscoveryConfig {
        enabled: true,
        consul_endpoint: Some("http://consul:8500".to_string()),
        etcd_endpoint: Some("http://etcd:2379".to_string()),
        refresh_interval_secs: 120,
    };

    let cloned = original.clone();

    assert_eq!(original.enabled, cloned.enabled);
    assert_eq!(original.consul_endpoint, cloned.consul_endpoint);
    assert_eq!(original.etcd_endpoint, cloned.etcd_endpoint);
    assert_eq!(original.refresh_interval_secs, cloned.refresh_interval_secs);
}

#[rstest]
#[test]
fn test_service_discovery_config_debug() {
    let config = ServiceDiscoveryConfig {
        enabled: true,
        consul_endpoint: Some("http://consul:8500".to_string()),
        etcd_endpoint: None,
        refresh_interval_secs: 30,
    };

    let debug_str = format!("{:?}", config);
    
    assert!(debug_str.contains("ServiceDiscoveryConfig"));
    assert!(debug_str.contains("enabled: true"));
    assert!(debug_str.contains("consul:8500"));
    assert!(debug_str.contains("etcd_endpoint: None"));
}

#[rstest]
#[test]
fn test_service_discovery_config_serialization() -> Result<(), serde_json::Error> {
    let config = ServiceDiscoveryConfig {
        enabled: true,
        consul_endpoint: Some("http://consul.test:8500".to_string()),
        etcd_endpoint: Some("http://etcd.test:2379".to_string()),
        refresh_interval_secs: 90,
    };

    // Serialize to JSON
    let json = serde_json::to_string(&config)?;
    assert!(json.contains(r#""enabled":true"#));
    assert!(json.contains("consul.test:8500"));
    assert!(json.contains("etcd.test:2379"));
    assert!(json.contains(r#""refresh_interval_secs":90"#));

    // Deserialize from JSON
    let deserialized: ServiceDiscoveryConfig = serde_json::from_str(&json)?;
    assert_eq!(config.enabled, deserialized.enabled);
    assert_eq!(config.consul_endpoint, deserialized.consul_endpoint);
    assert_eq!(config.etcd_endpoint, deserialized.etcd_endpoint);
    assert_eq!(config.refresh_interval_secs, deserialized.refresh_interval_secs);

    Ok(())
}

#[rstest]
#[test]
fn test_service_discovery_config_partial_json() -> Result<(), serde_json::Error> {
    let partial_json = r#"{
        "enabled": true,
        "consul_endpoint": "http://consul.local:8500"
    }"#;

    let config: ServiceDiscoveryConfig = serde_json::from_str(partial_json)?;
    
    // Specified values should be preserved
    assert!(config.enabled);
    assert_eq!(config.consul_endpoint.unwrap(), "http://consul.local:8500");
    
    // Unspecified values should use defaults
    assert!(config.etcd_endpoint.is_none());
    assert_eq!(config.refresh_interval_secs, 30);

    Ok(())
}

// Configuration Validation Tests
#[rstest]
#[test]
fn test_service_endpoints_url_formats() {
    let endpoints = ServiceEndpoints {
        auth_service: "http://localhost:50051".to_string(),
        market_data_service: "https://secure.example.com:443".to_string(),
        risk_service: "grpc://internal.service:9090".to_string(),
        execution_service: "http://[::1]:8080".to_string(), // IPv6
        data_aggregator_service: "unix:///tmp/socket".to_string(), // Unix socket
    };

    // All formats should be preserved as strings
    assert!(endpoints.auth_service.starts_with("http://"));
    assert!(endpoints.market_data_service.starts_with("https://"));
    assert!(endpoints.risk_service.starts_with("grpc://"));
    assert!(endpoints.execution_service.contains("[::1]"));
    assert!(endpoints.data_aggregator_service.starts_with("unix://"));
}

#[rstest]
#[test]
fn test_refresh_interval_edge_cases() {
    // Test very short refresh interval
    let short_config = ServiceDiscoveryConfig {
        enabled: true,
        consul_endpoint: Some("http://consul:8500".to_string()),
        etcd_endpoint: None,
        refresh_interval_secs: 1, // 1 second
    };

    assert_eq!(short_config.refresh_interval_secs, 1);

    // Test very long refresh interval
    let long_config = ServiceDiscoveryConfig {
        enabled: true,
        consul_endpoint: Some("http://consul:8500".to_string()),
        etcd_endpoint: None,
        refresh_interval_secs: 3600, // 1 hour
    };

    assert_eq!(long_config.refresh_interval_secs, 3600);

    // Test zero refresh interval
    let zero_config = ServiceDiscoveryConfig {
        enabled: true,
        consul_endpoint: Some("http://consul:8500".to_string()),
        etcd_endpoint: None,
        refresh_interval_secs: 0,
    };

    assert_eq!(zero_config.refresh_interval_secs, 0);
}

#[rstest]
#[test]
fn test_service_discovery_disabled_state() {
    let disabled_config = ServiceDiscoveryConfig {
        enabled: false,
        consul_endpoint: Some("http://consul:8500".to_string()),
        etcd_endpoint: Some("http://etcd:2379".to_string()),
        refresh_interval_secs: 30,
    };

    // When disabled, endpoints may still be configured but not used
    assert!(!disabled_config.enabled);
    assert!(disabled_config.consul_endpoint.is_some());
    assert!(disabled_config.etcd_endpoint.is_some());
}

// JSON Configuration File Tests
#[rstest]
#[test]
fn test_complete_configuration_json() -> Result<(), serde_json::Error> {
    let complete_json = r#"{
        "service_endpoints": {
            "auth_service": "http://auth.prod:8080",
            "market_data_service": "grpc://market.prod:9090",
            "risk_service": "http://risk.prod:8081",
            "execution_service": "grpc://exec.prod:9091",
            "data_aggregator_service": "http://data.prod:8082"
        },
        "service_discovery": {
            "enabled": true,
            "consul_endpoint": "http://consul.prod:8500",
            "etcd_endpoint": "http://etcd.prod:2379",
            "refresh_interval_secs": 60
        }
    }"#;

    // Parse the complete configuration
    let parsed: serde_json::Value = serde_json::from_str(complete_json)?;
    
    // Extract service endpoints
    let endpoints: ServiceEndpoints = serde_json::from_value(
        parsed["service_endpoints"].clone()
    )?;
    
    // Extract service discovery config
    let discovery: ServiceDiscoveryConfig = serde_json::from_value(
        parsed["service_discovery"].clone()
    )?;

    // Verify endpoints
    assert_eq!(endpoints.auth_service, "http://auth.prod:8080");
    assert_eq!(endpoints.market_data_service, "grpc://market.prod:9090");

    // Verify discovery config
    assert!(discovery.enabled);
    assert_eq!(discovery.consul_endpoint.unwrap(), "http://consul.prod:8500");
    assert_eq!(discovery.etcd_endpoint.unwrap(), "http://etcd.prod:2379");
    assert_eq!(discovery.refresh_interval_secs, 60);

    Ok(())
}

#[rstest]
#[test]
fn test_invalid_json_handling() {
    let invalid_json = r#"{
        "auth_service": "not-a-complete-config",
        "missing_fields": true
    }"#;

    // This should fail to deserialize into ServiceEndpoints
    let result: Result<ServiceEndpoints, _> = serde_json::from_str(invalid_json);
    assert!(result.is_err());
}

#[rstest]
#[test]
fn test_empty_configuration() -> Result<(), serde_json::Error> {
    let empty_json = "{}";

    // Should deserialize to defaults
    let endpoints: ServiceEndpoints = serde_json::from_str(empty_json)?;
    let discovery: ServiceDiscoveryConfig = serde_json::from_str(empty_json)?;

    // Should match default values
    let default_endpoints = ServiceEndpoints::default();
    let default_discovery = ServiceDiscoveryConfig::default();

    assert_eq!(endpoints.auth_service, default_endpoints.auth_service);
    assert_eq!(discovery.enabled, default_discovery.enabled);
    assert_eq!(discovery.refresh_interval_secs, default_discovery.refresh_interval_secs);

    Ok(())
}

// Configuration Builder Pattern Tests
#[rstest]
#[test]
fn test_service_endpoints_builder_pattern() {
    let mut endpoints = ServiceEndpoints::default();
    
    // Modify individual fields
    endpoints.auth_service = "http://custom-auth:9001".to_string();
    endpoints.market_data_service = "grpc://custom-market:9002".to_string();
    
    assert_eq!(endpoints.auth_service, "http://custom-auth:9001");
    assert_eq!(endpoints.market_data_service, "grpc://custom-market:9002");
    
    // Unmodified fields should retain defaults
    assert_eq!(endpoints.risk_service, "http://localhost:50053");
}

#[rstest]
#[test]
fn test_service_discovery_builder_pattern() {
    let mut config = ServiceDiscoveryConfig::default();
    
    // Enable and configure step by step
    config.enabled = true;
    config.consul_endpoint = Some("http://consul:8500".to_string());
    config.refresh_interval_secs = 45;
    
    assert!(config.enabled);
    assert!(config.consul_endpoint.is_some());
    assert!(config.etcd_endpoint.is_none()); // Unchanged
    assert_eq!(config.refresh_interval_secs, 45);
}

// Stress Tests for Configuration
#[rstest]
#[test]
fn test_configuration_with_unicode() -> Result<(), serde_json::Error> {
    let unicode_endpoints = ServiceEndpoints {
        auth_service: "http://認證服務:8080".to_string(), // Chinese characters
        market_data_service: "http://данные:8081".to_string(), // Cyrillic
        risk_service: "http://rīsk-服務:8082".to_string(), // Mixed
        execution_service: "http://localhost:8083".to_string(),
        data_aggregator_service: "http://localhost:8084".to_string(),
    };

    // Should serialize and deserialize correctly
    let json = serde_json::to_string(&unicode_endpoints)?;
    let deserialized: ServiceEndpoints = serde_json::from_str(&json)?;

    assert_eq!(unicode_endpoints.auth_service, deserialized.auth_service);
    assert_eq!(unicode_endpoints.market_data_service, deserialized.market_data_service);
    assert_eq!(unicode_endpoints.risk_service, deserialized.risk_service);

    Ok(())
}

#[rstest]
#[test]
fn test_configuration_with_long_strings() -> Result<(), serde_json::Error> {
    let long_endpoint = "http://".to_string() + &"a".repeat(1000) + ".example.com:8080";
    
    let endpoints = ServiceEndpoints {
        auth_service: long_endpoint.clone(),
        market_data_service: "http://localhost:50052".to_string(),
        risk_service: "http://localhost:50053".to_string(),
        execution_service: "http://localhost:50054".to_string(),
        data_aggregator_service: "http://localhost:50055".to_string(),
    };

    // Should handle long strings
    let json = serde_json::to_string(&endpoints)?;
    let deserialized: ServiceEndpoints = serde_json::from_str(&json)?;

    assert_eq!(endpoints.auth_service, deserialized.auth_service);
    assert_eq!(endpoints.auth_service.len(), long_endpoint.len());

    Ok(())
}