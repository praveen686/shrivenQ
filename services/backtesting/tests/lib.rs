//! Main test entry point for backtesting service
//! 
//! This module brings together all unit and integration tests for comprehensive
//! testing of the backtesting service functionality.

// Import all test modules
pub mod test_utils;
pub mod unit;
pub mod integration;

// Re-export test utilities for use in tests
pub use test_utils::*;

#[cfg(test)]
mod tests {
    use super::*;
    
    /// Basic test to ensure the test framework is working
    #[test]
    fn test_framework_sanity_check() {
        assert_eq!(2 + 2, 4);
    }
    
    /// Test that our test utilities are working
    #[test]
    fn test_utilities_working() {
        let config = TestConfigFactory::basic_config();
        assert!(config.initial_capital > 0.0);
        
        let data = TestDataFactory::trending_up_data(5, 100.0);
        assert_eq!(data.len(), 5);
        
        TestRandom::reset();
        let val1 = TestRandom::next();
        TestRandom::reset();
        let val2 = TestRandom::next();
        assert_eq!(val1, val2, "Random should be deterministic after reset");
    }
}