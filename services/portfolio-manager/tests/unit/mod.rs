//! Unit tests for portfolio manager components
//! 
//! This module contains comprehensive unit tests for all portfolio manager components:
//! - Position tracking and atomic operations
//! - Portfolio analytics and performance metrics
//! - Optimization algorithms (equal weight, minimum variance, max Sharpe, risk parity)
//! - Rebalancing logic and order generation
//! - Market feed integration and price updates

pub mod position_tests;
pub mod portfolio_tests;
pub mod optimization_tests;
pub mod rebalancer_tests;
pub mod market_feed_tests;