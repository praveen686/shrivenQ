//! Unit tests for risk manager configuration

use risk_manager::{
    RiskLimits,
    config::{RiskConfig, AlertThresholds},
    limits::{StrategyLimits, ExchangeLimits, TimeLimits},
};
use rustc_hash::FxHashMap;
use serde_json;
use rstest::*;

#[fixture]
fn default_risk_limits() -> RiskLimits {
    RiskLimits::default()
}

#[fixture]
fn sample_alert_thresholds() -> AlertThresholds {
    AlertThresholds {
        exposure_warning_pct: 8000,    // 80%
        drawdown_warning_pct: 500,     // 5%
        loss_rate_warning: 10,         // 10 losses per minute
    }
}

#[fixture]
fn sample_strategy_limits() -> StrategyLimits {
    let mut custom_limits = FxHashMap::default();
    custom_limits.insert("max_leverage".to_string(), 30000); // 3x leverage in fixed point
    custom_limits.insert("correlation_limit".to_string(), 7500); // 75% correlation limit
    
    StrategyLimits {
        strategy_id: "momentum_strategy_v1".to_string(),
        max_allocation: 10_000_000, // $1M allocation
        max_positions: 50,
        allowed_symbols: vec!["BTCUSD".to_string(), "ETHUSD".to_string(), "ADAUSD".to_string()],
        custom_limits,
    }
}

#[fixture]
fn sample_exchange_limits() -> ExchangeLimits {
    ExchangeLimits {
        exchange: "binance".to_string(),
        max_order_rate: 100,     // 100 orders per second
        max_cancel_rate: 200,    // 200 cancels per second
        max_message_rate: 1000,  // 1000 messages per second
    }
}

#[fixture]
fn sample_time_limits() -> TimeLimits {
    TimeLimits {
        trading_hours: vec![(900, 1630), (1900, 2300)], // 9:00-16:30 and 19:00-23:00 UTC
        blackout_periods: vec![
            "2024-01-01".to_string(), // New Year's Day
            "2024-12-25".to_string(), // Christmas
        ],
        eod_flatten_time: Some(1600), // 16:00 UTC
    }
}

#[test]
fn test_risk_limits_default_values() {
    let limits = RiskLimits::default();
    
    // Test that all default values are reasonable
    assert!(limits.max_position_size > 0);
    assert!(limits.max_position_value > 0);
    assert!(limits.max_total_exposure > 0);
    assert!(limits.max_order_size > 0);
    assert!(limits.max_order_value > 0);
    assert!(limits.max_orders_per_minute > 0);
    assert!(limits.max_daily_loss < 0); // Should be negative (loss limit)
    assert!(limits.max_drawdown_pct > 0);
    assert!(limits.circuit_breaker_threshold > 0);
    assert!(limits.circuit_breaker_cooldown > 0);
}

#[test]
fn test_risk_limits_serialization() {
    let limits = RiskLimits::default();
    
    // Test JSON serialization
    let serialized = serde_json::to_string(&limits).unwrap();
    assert!(serialized.contains("max_position_size"));
    assert!(serialized.contains("max_order_value"));
    assert!(serialized.contains("circuit_breaker_threshold"));
    
    // Test deserialization
    let deserialized: RiskLimits = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.max_position_size, limits.max_position_size);
    assert_eq!(deserialized.max_daily_loss, limits.max_daily_loss);
    assert_eq!(deserialized.circuit_breaker_cooldown, limits.circuit_breaker_cooldown);
}

#[rstest]
fn test_alert_thresholds_validation(sample_alert_thresholds: AlertThresholds) {
    // Test that thresholds are within reasonable ranges
    assert!(sample_alert_thresholds.exposure_warning_pct > 0);
    assert!(sample_alert_thresholds.exposure_warning_pct <= 10000); // <= 100%
    
    assert!(sample_alert_thresholds.drawdown_warning_pct >= 0);
    assert!(sample_alert_thresholds.drawdown_warning_pct <= 10000); // <= 100%
    
    assert!(sample_alert_thresholds.loss_rate_warning > 0);
    assert!(sample_alert_thresholds.loss_rate_warning <= 1000); // Reasonable upper limit
}

#[rstest]
fn test_alert_thresholds_serialization(sample_alert_thresholds: AlertThresholds) {
    let serialized = serde_json::to_string(&sample_alert_thresholds).unwrap();
    let deserialized: AlertThresholds = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(deserialized.exposure_warning_pct, sample_alert_thresholds.exposure_warning_pct);
    assert_eq!(deserialized.drawdown_warning_pct, sample_alert_thresholds.drawdown_warning_pct);
    assert_eq!(deserialized.loss_rate_warning, sample_alert_thresholds.loss_rate_warning);
}

#[rstest]
fn test_strategy_limits_structure(sample_strategy_limits: StrategyLimits) {
    assert_eq!(sample_strategy_limits.strategy_id, "momentum_strategy_v1");
    assert_eq!(sample_strategy_limits.max_allocation, 10_000_000);
    assert_eq!(sample_strategy_limits.max_positions, 50);
    assert_eq!(sample_strategy_limits.allowed_symbols.len(), 3);
    assert!(sample_strategy_limits.allowed_symbols.contains(&"BTCUSD".to_string()));
    
    // Test custom limits
    assert!(sample_strategy_limits.custom_limits.contains_key("max_leverage"));
    assert_eq!(*sample_strategy_limits.custom_limits.get("max_leverage").unwrap(), 30000);
}

#[rstest]
fn test_strategy_limits_serialization(sample_strategy_limits: StrategyLimits) {
    let serialized = serde_json::to_string(&sample_strategy_limits).unwrap();
    let deserialized: StrategyLimits = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(deserialized.strategy_id, sample_strategy_limits.strategy_id);
    assert_eq!(deserialized.max_allocation, sample_strategy_limits.max_allocation);
    assert_eq!(deserialized.allowed_symbols, sample_strategy_limits.allowed_symbols);
    assert_eq!(deserialized.custom_limits, sample_strategy_limits.custom_limits);
}

#[rstest]
fn test_exchange_limits_structure(sample_exchange_limits: ExchangeLimits) {
    assert_eq!(sample_exchange_limits.exchange, "binance");
    assert_eq!(sample_exchange_limits.max_order_rate, 100);
    assert_eq!(sample_exchange_limits.max_cancel_rate, 200);
    assert_eq!(sample_exchange_limits.max_message_rate, 1000);
    
    // Test that rates are reasonable
    assert!(sample_exchange_limits.max_order_rate > 0);
    assert!(sample_exchange_limits.max_cancel_rate >= sample_exchange_limits.max_order_rate);
    assert!(sample_exchange_limits.max_message_rate >= sample_exchange_limits.max_order_rate);
}

#[rstest]
fn test_exchange_limits_serialization(sample_exchange_limits: ExchangeLimits) {
    let serialized = serde_json::to_string(&sample_exchange_limits).unwrap();
    let deserialized: ExchangeLimits = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(deserialized.exchange, sample_exchange_limits.exchange);
    assert_eq!(deserialized.max_order_rate, sample_exchange_limits.max_order_rate);
    assert_eq!(deserialized.max_cancel_rate, sample_exchange_limits.max_cancel_rate);
    assert_eq!(deserialized.max_message_rate, sample_exchange_limits.max_message_rate);
}

#[rstest]
fn test_time_limits_structure(sample_time_limits: TimeLimits) {
    assert_eq!(sample_time_limits.trading_hours.len(), 2);
    assert_eq!(sample_time_limits.trading_hours[0], (900, 1630));
    assert_eq!(sample_time_limits.trading_hours[1], (1900, 2300));
    
    assert_eq!(sample_time_limits.blackout_periods.len(), 2);
    assert!(sample_time_limits.blackout_periods.contains(&"2024-01-01".to_string()));
    
    assert_eq!(sample_time_limits.eod_flatten_time, Some(1600));
}

#[rstest]
fn test_time_limits_validation(sample_time_limits: TimeLimits) {
    // Validate trading hours format
    for (start, end) in &sample_time_limits.trading_hours {
        assert!(*start < *end, "Start time should be before end time");
        assert!(*start < 2400, "Start time should be valid 24h format");
        assert!(*end <= 2400, "End time should be valid 24h format");
    }
    
    // Validate EOD flatten time
    if let Some(eod_time) = sample_time_limits.eod_flatten_time {
        assert!(eod_time < 2400, "EOD flatten time should be valid 24h format");
    }
}

#[rstest]
fn test_time_limits_serialization(sample_time_limits: TimeLimits) {
    let serialized = serde_json::to_string(&sample_time_limits).unwrap();
    let deserialized: TimeLimits = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(deserialized.trading_hours, sample_time_limits.trading_hours);
    assert_eq!(deserialized.blackout_periods, sample_time_limits.blackout_periods);
    assert_eq!(deserialized.eod_flatten_time, sample_time_limits.eod_flatten_time);
}

#[rstest]
fn test_risk_config_complete_structure(
    default_risk_limits: RiskLimits,
    sample_alert_thresholds: AlertThresholds,
    sample_strategy_limits: StrategyLimits,
    sample_exchange_limits: ExchangeLimits,
) {
    let mut strategy_limits = FxHashMap::default();
    strategy_limits.insert("strategy1".to_string(), sample_strategy_limits);
    
    let mut exchange_limits = FxHashMap::default();
    exchange_limits.insert("binance".to_string(), sample_exchange_limits);
    
    let config = RiskConfig {
        limits: default_risk_limits,
        alert_thresholds: sample_alert_thresholds,
        strategy_limits,
        exchange_limits,
    };
    
    assert!(config.strategy_limits.contains_key("strategy1"));
    assert!(config.exchange_limits.contains_key("binance"));
}

#[rstest]
fn test_risk_config_serialization(
    default_risk_limits: RiskLimits,
    sample_alert_thresholds: AlertThresholds,
) {
    let config = RiskConfig {
        limits: default_risk_limits,
        alert_thresholds: sample_alert_thresholds,
        strategy_limits: FxHashMap::default(),
        exchange_limits: FxHashMap::default(),
    };
    
    let serialized = serde_json::to_string(&config).unwrap();
    let deserialized: RiskConfig = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(deserialized.limits.max_position_size, config.limits.max_position_size);
    assert_eq!(deserialized.alert_thresholds.exposure_warning_pct, 
               config.alert_thresholds.exposure_warning_pct);
}

#[test]
fn test_custom_risk_limits_creation() {
    let mut custom_limits = RiskLimits::default();
    
    // Modify limits for specific use case
    custom_limits.max_position_size = 5000;
    custom_limits.max_order_size = 1000;
    custom_limits.max_orders_per_minute = 20;
    custom_limits.max_daily_loss = -100_000;
    custom_limits.circuit_breaker_threshold = 3;
    custom_limits.circuit_breaker_cooldown = 600; // 10 minutes
    
    assert_eq!(custom_limits.max_position_size, 5000);
    assert_eq!(custom_limits.max_order_size, 1000);
    assert_eq!(custom_limits.max_orders_per_minute, 20);
    assert_eq!(custom_limits.max_daily_loss, -100_000);
    assert_eq!(custom_limits.circuit_breaker_threshold, 3);
    assert_eq!(custom_limits.circuit_breaker_cooldown, 600);
}

#[test]
fn test_multiple_strategy_limits() {
    let mut strategy_limits = FxHashMap::default();
    
    // Add multiple strategies with different limits
    for i in 1..=5 {
        let limits = StrategyLimits {
            strategy_id: format!("strategy_{}", i),
            max_allocation: 1_000_000 * i as u64,
            max_positions: 10 * i as u32,
            allowed_symbols: vec![format!("SYMBOL{}", i)],
            custom_limits: FxHashMap::default(),
        };
        strategy_limits.insert(limits.strategy_id.clone(), limits);
    }
    
    assert_eq!(strategy_limits.len(), 5);
    assert_eq!(strategy_limits.get("strategy_3").unwrap().max_allocation, 3_000_000);
    assert_eq!(strategy_limits.get("strategy_5").unwrap().max_positions, 50);
}

#[test]
fn test_multiple_exchange_limits() {
    let exchanges = ["binance", "coinbase", "kraken", "okx"];
    let mut exchange_limits = FxHashMap::default();
    
    for (i, exchange) in exchanges.iter().enumerate() {
        let limits = ExchangeLimits {
            exchange: exchange.to_string(),
            max_order_rate: 50 + (i as u32) * 25,
            max_cancel_rate: 100 + (i as u32) * 50,
            max_message_rate: 500 + (i as u32) * 250,
        };
        exchange_limits.insert(limits.exchange.clone(), limits);
    }
    
    assert_eq!(exchange_limits.len(), 4);
    assert_eq!(exchange_limits.get("binance").unwrap().max_order_rate, 50);
    assert_eq!(exchange_limits.get("okx").unwrap().max_order_rate, 125);
}

#[test]
fn test_trading_hours_parsing() {
    let time_limits = TimeLimits {
        trading_hours: vec![
            (930, 1600),  // 9:30 AM to 4:00 PM
            (1800, 2359), // 6:00 PM to 11:59 PM
        ],
        blackout_periods: vec![],
        eod_flatten_time: Some(1545), // 3:45 PM
    };
    
    // Test that hours are properly structured
    for (start, end) in &time_limits.trading_hours {
        let start_hours = start / 100;
        let start_minutes = start % 100;
        let end_hours = end / 100;
        let end_minutes = end % 100;
        
        assert!(start_hours <= 23, "Start hours should be valid");
        assert!(start_minutes <= 59, "Start minutes should be valid");
        assert!(end_hours <= 23, "End hours should be valid");
        assert!(end_minutes <= 59, "End minutes should be valid");
    }
}

#[test]
fn test_config_edge_cases() {
    // Test with zero/minimal limits
    let minimal_limits = RiskLimits {
        max_position_size: 1,
        max_position_value: 1,
        max_total_exposure: 1,
        max_order_size: 1,
        max_order_value: 1,
        max_orders_per_minute: 1,
        max_daily_loss: -1,
        max_drawdown_pct: 1,
        circuit_breaker_threshold: 1,
        circuit_breaker_cooldown: 1,
    };
    
    // Should be serializable
    let serialized = serde_json::to_string(&minimal_limits).unwrap();
    let deserialized: RiskLimits = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.max_position_size, 1);
    
    // Test with maximum limits
    let maximal_limits = RiskLimits {
        max_position_size: u64::MAX,
        max_position_value: u64::MAX,
        max_total_exposure: u64::MAX,
        max_order_size: u64::MAX,
        max_order_value: u64::MAX,
        max_orders_per_minute: u32::MAX,
        max_daily_loss: i64::MIN,
        max_drawdown_pct: i32::MAX,
        circuit_breaker_threshold: u32::MAX,
        circuit_breaker_cooldown: u64::MAX,
    };
    
    // Should handle extreme values
    let serialized = serde_json::to_string(&maximal_limits).unwrap();
    let deserialized: RiskLimits = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.max_position_size, u64::MAX);
}

#[test]
fn test_complex_custom_limits() {
    let mut custom_limits = FxHashMap::default();
    
    // Add various types of custom limits
    custom_limits.insert("max_correlation".to_string(), 8000); // 80%
    custom_limits.insert("var_limit".to_string(), -500_000); // $50k VaR
    custom_limits.insert("beta_limit".to_string(), 15000); // 1.5 beta
    custom_limits.insert("sector_concentration".to_string(), 3000); // 30%
    custom_limits.insert("turnover_limit".to_string(), 500); // 5% daily turnover
    
    let strategy_limits = StrategyLimits {
        strategy_id: "quant_strategy".to_string(),
        max_allocation: 50_000_000,
        max_positions: 100,
        allowed_symbols: vec![
            "SPY".to_string(), "QQQ".to_string(), "IWM".to_string(),
            "AAPL".to_string(), "GOOGL".to_string(), "MSFT".to_string(),
        ],
        custom_limits,
    };
    
    assert_eq!(strategy_limits.custom_limits.len(), 5);
    assert_eq!(*strategy_limits.custom_limits.get("max_correlation").unwrap(), 8000);
    assert_eq!(*strategy_limits.custom_limits.get("var_limit").unwrap(), -500_000);
}

#[test]
fn test_blackout_periods_handling() {
    let time_limits = TimeLimits {
        trading_hours: vec![(900, 1700)],
        blackout_periods: vec![
            "2024-01-01".to_string(),    // New Year
            "2024-07-04".to_string(),    // Independence Day
            "2024-11-28".to_string(),    // Thanksgiving
            "2024-12-25".to_string(),    // Christmas
            "2024-01-15".to_string(),    // MLK Day
            "2024-02-19".to_string(),    // Presidents Day
        ],
        eod_flatten_time: Some(1630),
    };
    
    assert_eq!(time_limits.blackout_periods.len(), 6);
    
    // Test serialization with multiple blackout periods
    let serialized = serde_json::to_string(&time_limits).unwrap();
    let deserialized: TimeLimits = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(deserialized.blackout_periods.len(), 6);
    assert!(deserialized.blackout_periods.contains(&"2024-07-04".to_string()));
}

#[test]
fn test_config_validation_rules() {
    let mut limits = RiskLimits::default();
    
    // Test logical relationships between limits
    limits.max_order_size = 1000;
    limits.max_position_size = 500; // Should be >= max_order_size typically
    
    // The struct allows this, but in practice we'd want validation
    assert!(limits.max_order_size > limits.max_position_size);
    
    // Test daily loss vs exposure relationships
    limits.max_total_exposure = 10_000_000;
    limits.max_daily_loss = -1_000_000; // 10% of exposure
    
    let loss_percentage = (limits.max_daily_loss.unsigned_abs() as f64) / (limits.max_total_exposure as f64);
    assert!(loss_percentage <= 1.0, "Daily loss should not exceed total exposure");
}