//! Mock services and components for testing

use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use anyhow::Result;
use std::collections::HashMap;
use uuid::Uuid;

/// Mock exchange connector for simulating exchange interactions in tests
/// 
/// This struct provides a complete simulation of an exchange connector, allowing
/// tests to mock order placement, market data retrieval, and connection management
/// without requiring actual exchange connections.
pub struct MockExchangeConnector {
    orders: Arc<Mutex<HashMap<Uuid, MockOrder>>>,
    market_data: Arc<RwLock<HashMap<String, f64>>>,
    connection_status: Arc<RwLock<bool>>,
    fail_next_order: Arc<RwLock<bool>>,
}

impl std::fmt::Debug for MockExchangeConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockExchangeConnector")
            .field("orders", &"Arc<Mutex<HashMap<Uuid, MockOrder>>>")
            .field("market_data", &"Arc<RwLock<HashMap<String, f64>>>")
            .field("connection_status", &"Arc<RwLock<bool>>")
            .field("fail_next_order", &"Arc<RwLock<bool>>")
            .finish()
    }
}

impl MockExchangeConnector {
    /// Creates a new mock exchange connector with default market data
    /// 
    /// Initializes with sample prices for BTCUSDT (45000.0) and ETHUSDT (2800.0)
    pub fn new() -> Self {
        let mut market_data = HashMap::new();
        market_data.insert("BTCUSDT".to_string(), 45000.0);
        market_data.insert("ETHUSDT".to_string(), 2800.0);
        
        Self {
            orders: Arc::new(Mutex::new(HashMap::new())),
            market_data: Arc::new(RwLock::new(market_data)),
            connection_status: Arc::new(RwLock::new(true)),
            fail_next_order: Arc::new(RwLock::new(false)),
        }
    }
    
    /// Places an order in the mock exchange
    /// 
    /// # Arguments
    /// * `order` - The order to place
    /// 
    /// # Returns
    /// * `Ok(Uuid)` - The order ID if successful
    /// * `Err` - If order placement fails (when fail_next_order is set)
    pub async fn place_order(&self, order: MockOrder) -> Result<Uuid> {
        if *self.fail_next_order.read().await {
            *self.fail_next_order.write().await = false;
            return Err(anyhow::anyhow!("Order placement failed"));
        }
        
        let order_id = order.id;
        self.orders.lock().await.insert(order_id, order);
        Ok(order_id)
    }
    
    /// Cancels an existing order
    /// 
    /// # Arguments
    /// * `order_id` - The ID of the order to cancel
    /// 
    /// # Returns
    /// * `Ok(())` - If the order was successfully cancelled
    /// * `Err` - If the order was not found
    pub async fn cancel_order(&self, order_id: Uuid) -> Result<()> {
        self.orders
            .lock()
            .await
            .remove(&order_id)
            .ok_or_else(|| anyhow::anyhow!("Order not found"))?;
        Ok(())
    }
    
    /// Retrieves the current market price for a symbol
    /// 
    /// # Arguments
    /// * `symbol` - The trading symbol (e.g., "BTCUSDT")
    /// 
    /// # Returns
    /// * `Ok(f64)` - The current price if the symbol exists
    /// * `Err` - If the symbol is not found
    pub async fn get_market_price(&self, symbol: &str) -> Result<f64> {
        self.market_data
            .read()
            .await
            .get(symbol)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("Symbol not found"))
    }
    
    /// Updates the market price for a symbol
    /// 
    /// # Arguments
    /// * `symbol` - The trading symbol to update
    /// * `price` - The new price value
    pub async fn set_market_price(&self, symbol: String, price: f64) {
        self.market_data.write().await.insert(symbol, price);
    }
    
    /// Configures whether the next order placement should fail
    /// 
    /// # Arguments
    /// * `fail` - If true, the next order placement will fail
    pub async fn set_fail_next_order(&self, fail: bool) {
        *self.fail_next_order.write().await = fail;
    }
    
    /// Checks if the mock exchange is connected
    /// 
    /// # Returns
    /// * `true` if connected, `false` otherwise
    pub async fn is_connected(&self) -> bool {
        *self.connection_status.read().await
    }
    
    /// Simulates disconnecting from the exchange
    pub async fn disconnect(&self) {
        *self.connection_status.write().await = false;
    }
    
    /// Simulates connecting to the exchange
    pub async fn connect(&self) {
        *self.connection_status.write().await = true;
    }
}

/// Represents a mock order for testing purposes
/// 
/// This struct contains all the essential fields needed to simulate
/// trading orders in test scenarios.
#[derive(Debug, Clone)]
pub struct MockOrder {
    /// Unique identifier for the order
    pub id: Uuid,
    /// Trading symbol (e.g., "BTCUSDT")
    pub symbol: String,
    /// Order side ("BUY" or "SELL")
    pub side: String,
    /// Order quantity
    pub quantity: f64,
    /// Order price (None for market orders)
    pub price: Option<f64>,
    /// Type of order ("MARKET", "LIMIT", etc.)
    pub order_type: String,
}

/// Mock risk manager for simulating risk management in tests
/// 
/// This struct provides risk checking functionality including position size limits,
/// daily loss limits, and the ability to enable/disable risk checks for testing
/// various scenarios.
pub struct MockRiskManager {
    max_position_size: f64,
    max_daily_loss: f64,
    current_positions: Arc<RwLock<HashMap<String, f64>>>,
    daily_pnl: Arc<RwLock<f64>>,
    risk_checks_enabled: Arc<RwLock<bool>>,
}

impl std::fmt::Debug for MockRiskManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockRiskManager")
            .field("max_position_size", &self.max_position_size)
            .field("max_daily_loss", &self.max_daily_loss)
            .field("current_positions", &"Arc<RwLock<HashMap<String, f64>>>")
            .field("daily_pnl", &"Arc<RwLock<f64>>")
            .field("risk_checks_enabled", &"Arc<RwLock<bool>>")
            .finish()
    }
}

impl MockRiskManager {
    /// Creates a new mock risk manager with default limits
    /// 
    /// Default settings:
    /// - Max position size: 100,000
    /// - Max daily loss: 5,000
    /// - Risk checks enabled
    pub fn new() -> Self {
        Self {
            max_position_size: 100000.0,
            max_daily_loss: 5000.0,
            current_positions: Arc::new(RwLock::new(HashMap::new())),
            daily_pnl: Arc::new(RwLock::new(0.0)),
            risk_checks_enabled: Arc::new(RwLock::new(true)),
        }
    }
    
    /// Performs risk checks on an order
    /// 
    /// # Arguments
    /// * `symbol` - Trading symbol
    /// * `quantity` - Order quantity
    /// * `price` - Order price
    /// 
    /// # Returns
    /// * `Ok(true)` - If the order passes risk checks
    /// * `Ok(false)` - If the order violates risk limits
    /// * `Err` - If an error occurs during risk checking
    pub async fn check_order_risk(&self, _symbol: &str, quantity: f64, price: f64) -> Result<bool> {
        if !*self.risk_checks_enabled.read().await {
            return Ok(true);
        }
        
        let position_value = quantity * price;
        
        // Check position size limit
        if position_value > self.max_position_size {
            return Ok(false);
        }
        
        // Check daily loss limit
        if *self.daily_pnl.read().await < -self.max_daily_loss {
            return Ok(false);
        }
        
        Ok(true)
    }
    
    /// Updates the position for a symbol
    /// 
    /// # Arguments
    /// * `symbol` - Trading symbol
    /// * `quantity` - New position quantity
    pub async fn update_position(&self, symbol: String, quantity: f64) {
        self.current_positions.write().await.insert(symbol, quantity);
    }
    
    /// Updates the daily P&L
    /// 
    /// # Arguments
    /// * `pnl` - P&L amount to add to the daily total
    pub async fn update_pnl(&self, pnl: f64) {
        *self.daily_pnl.write().await += pnl;
    }
    
    /// Disables all risk checks for testing scenarios
    pub async fn disable_risk_checks(&self) {
        *self.risk_checks_enabled.write().await = false;
    }
    
    /// Enables risk checks (default state)
    pub async fn enable_risk_checks(&self) {
        *self.risk_checks_enabled.write().await = true;
    }
    
    /// Resets the risk manager to its initial state
    /// 
    /// Clears all positions, resets daily P&L to zero, and enables risk checks
    pub async fn reset(&self) {
        self.current_positions.write().await.clear();
        *self.daily_pnl.write().await = 0.0;
        *self.risk_checks_enabled.write().await = true;
    }
}

/// Mock database for simulating database operations in tests
/// 
/// This struct provides in-memory key-value storage functionality with
/// the ability to simulate database failures for error testing scenarios.
pub struct MockDatabase {
    data: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    fail_next: Arc<RwLock<bool>>,
}

impl std::fmt::Debug for MockDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockDatabase")
            .field("data", &"Arc<RwLock<HashMap<String, Vec<u8>>>>")
            .field("fail_next", &"Arc<RwLock<bool>>")
            .finish()
    }
}

impl MockDatabase {
    /// Creates a new empty mock database
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
            fail_next: Arc::new(RwLock::new(false)),
        }
    }
    
    /// Inserts a key-value pair into the database
    /// 
    /// # Arguments
    /// * `key` - The key to store
    /// * `value` - The value to store as bytes
    /// 
    /// # Returns
    /// * `Ok(())` - If insertion was successful
    /// * `Err` - If insertion fails (when fail_next is set)
    pub async fn insert(&self, key: String, value: Vec<u8>) -> Result<()> {
        if *self.fail_next.read().await {
            *self.fail_next.write().await = false;
            return Err(anyhow::anyhow!("Database insert failed"));
        }
        
        self.data.write().await.insert(key, value);
        Ok(())
    }
    
    /// Retrieves a value by key from the database
    /// 
    /// # Arguments
    /// * `key` - The key to look up
    /// 
    /// # Returns
    /// * `Ok(Some(Vec<u8>))` - If the key exists
    /// * `Ok(None)` - If the key doesn't exist
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.data.read().await.get(key).cloned())
    }
    
    /// Deletes a key-value pair from the database
    /// 
    /// # Arguments
    /// * `key` - The key to delete
    /// 
    /// # Returns
    /// * `Ok(())` - Always succeeds (no error if key doesn't exist)
    pub async fn delete(&self, key: &str) -> Result<()> {
        self.data.write().await.remove(key);
        Ok(())
    }
    
    /// Clears all data from the database
    pub async fn clear(&self) {
        self.data.write().await.clear();
    }
    
    /// Configures whether the next operation should fail
    /// 
    /// # Arguments
    /// * `fail` - If true, the next insert operation will fail
    pub async fn set_fail_next(&self, fail: bool) {
        *self.fail_next.write().await = fail;
    }
}

/// Mock WebSocket client for simulating WebSocket connections in tests
/// 
/// This struct provides WebSocket-like functionality including connection management,
/// message sending/receiving, and the ability to inject messages for testing.
pub struct MockWebSocketClient {
    messages: Arc<Mutex<Vec<String>>>,
    is_connected: Arc<RwLock<bool>>,
    auto_reconnect: Arc<RwLock<bool>>,
}

impl std::fmt::Debug for MockWebSocketClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockWebSocketClient")
            .field("messages", &"Arc<Mutex<Vec<String>>>")
            .field("is_connected", &"Arc<RwLock<bool>>")
            .field("auto_reconnect", &"Arc<RwLock<bool>>")
            .finish()
    }
}

impl MockWebSocketClient {
    /// Creates a new mock WebSocket client in disconnected state
    pub fn new() -> Self {
        Self {
            messages: Arc::new(Mutex::new(Vec::new())),
            is_connected: Arc::new(RwLock::new(false)),
            auto_reconnect: Arc::new(RwLock::new(true)),
        }
    }
    
    /// Simulates connecting to a WebSocket server
    /// 
    /// # Arguments
    /// * `_url` - The URL to connect to (ignored in mock)
    /// 
    /// # Returns
    /// * `Ok(())` - Always succeeds
    pub async fn connect(&self, _url: &str) -> Result<()> {
        *self.is_connected.write().await = true;
        Ok(())
    }
    
    /// Simulates disconnecting from the WebSocket server
    pub async fn disconnect(&self) {
        *self.is_connected.write().await = false;
        
        // If auto-reconnect is enabled, automatically reconnect
        if *self.auto_reconnect.read().await {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            *self.is_connected.write().await = true;
        }
    }
    
    /// Sets whether auto-reconnect is enabled
    pub async fn set_auto_reconnect(&self, enabled: bool) {
        *self.auto_reconnect.write().await = enabled;
    }
    
    /// Gets the current auto-reconnect setting
    pub async fn is_auto_reconnect_enabled(&self) -> bool {
        *self.auto_reconnect.read().await
    }
    
    /// Sends a message through the WebSocket connection
    /// 
    /// # Arguments
    /// * `message` - The message to send
    /// 
    /// # Returns
    /// * `Ok(())` - If connected and message was queued
    /// * `Err` - If not connected
    pub async fn send(&self, message: String) -> Result<()> {
        if !*self.is_connected.read().await {
            return Err(anyhow::anyhow!("Not connected"));
        }
        
        self.messages.lock().await.push(message);
        Ok(())
    }
    
    /// Receives a message from the WebSocket connection
    /// 
    /// # Returns
    /// * `Ok(Some(String))` - If a message is available
    /// * `Ok(None)` - If no messages are available
    /// * `Err` - If not connected
    pub async fn receive(&self) -> Result<Option<String>> {
        if !*self.is_connected.read().await {
            return Err(anyhow::anyhow!("Not connected"));
        }
        
        Ok(self.messages.lock().await.pop())
    }
    
    /// Injects a message into the receive queue for testing
    /// 
    /// # Arguments
    /// * `message` - The message to inject
    pub async fn inject_message(&self, message: String) {
        self.messages.lock().await.push(message);
    }
    
    /// Checks if the WebSocket is connected
    /// 
    /// # Returns
    /// * `true` if connected, `false` otherwise
    pub async fn is_connected(&self) -> bool {
        *self.is_connected.read().await
    }
}

/// Mock gRPC service for simulating gRPC service interactions in tests
/// 
/// This generic struct allows testing of gRPC client code by providing
/// configurable responses and failure simulation.
/// 
/// # Type Parameters
/// * `T` - The type of response this service returns
pub struct MockGrpcService<T> {
    responses: Arc<Mutex<Vec<T>>>,
    call_count: Arc<Mutex<usize>>,
    fail_next: Arc<RwLock<bool>>,
}

impl<T> std::fmt::Debug for MockGrpcService<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockGrpcService")
            .field("responses", &"Arc<Mutex<Vec<T>>>")
            .field("call_count", &"Arc<Mutex<usize>>")
            .field("fail_next", &"Arc<RwLock<bool>>")
            .finish()
    }
}

impl<T: Clone> MockGrpcService<T> {
    /// Creates a new mock gRPC service with no configured responses
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(Vec::new())),
            call_count: Arc::new(Mutex::new(0)),
            fail_next: Arc::new(RwLock::new(false)),
        }
    }
    
    /// Configures a response to be returned by the service
    /// 
    /// Responses are returned in LIFO order (last configured, first returned)
    /// 
    /// # Arguments
    /// * `response` - The response to queue for future calls
    pub async fn set_response(&self, response: T) {
        self.responses.lock().await.push(response);
    }
    
    /// Simulates a gRPC service call
    /// 
    /// # Returns
    /// * `Ok(T)` - A configured response if available
    /// * `Err` - If no response is configured or if fail_next is set
    pub async fn call(&self) -> Result<T> {
        if *self.fail_next.read().await {
            *self.fail_next.write().await = false;
            return Err(anyhow::anyhow!("Service call failed"));
        }
        
        *self.call_count.lock().await += 1;
        
        self.responses
            .lock()
            .await
            .pop()
            .ok_or_else(|| anyhow::anyhow!("No response configured"))
    }
    
    /// Returns the number of times the service has been called
    /// 
    /// # Returns
    /// * `usize` - The total number of calls made to this service
    pub async fn get_call_count(&self) -> usize {
        *self.call_count.lock().await
    }
    
    /// Resets the service to its initial state
    /// 
    /// Clears all configured responses, resets call count to zero,
    /// and disables failure mode
    pub async fn reset(&self) {
        self.responses.lock().await.clear();
        *self.call_count.lock().await = 0;
        *self.fail_next.write().await = false;
    }
    
    /// Configures whether the next service call should fail
    /// 
    /// # Arguments
    /// * `fail` - If true, the next call will return an error
    pub async fn set_fail_next(&self, fail: bool) {
        *self.fail_next.write().await = fail;
    }
}