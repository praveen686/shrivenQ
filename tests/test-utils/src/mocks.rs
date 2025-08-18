//! Mock services and components for testing

use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use anyhow::Result;
use std::collections::HashMap;
use uuid::Uuid;

/// Mock exchange connector for testing
pub struct MockExchangeConnector {
    orders: Arc<Mutex<HashMap<Uuid, MockOrder>>>,
    market_data: Arc<RwLock<HashMap<String, f64>>>,
    connection_status: Arc<RwLock<bool>>,
    fail_next_order: Arc<RwLock<bool>>,
}

impl MockExchangeConnector {
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
    
    pub async fn place_order(&self, order: MockOrder) -> Result<Uuid> {
        if *self.fail_next_order.read().await {
            *self.fail_next_order.write().await = false;
            return Err(anyhow::anyhow!("Order placement failed"));
        }
        
        let order_id = order.id;
        self.orders.lock().await.insert(order_id, order);
        Ok(order_id)
    }
    
    pub async fn cancel_order(&self, order_id: Uuid) -> Result<()> {
        self.orders
            .lock()
            .await
            .remove(&order_id)
            .ok_or_else(|| anyhow::anyhow!("Order not found"))?;
        Ok(())
    }
    
    pub async fn get_market_price(&self, symbol: &str) -> Result<f64> {
        self.market_data
            .read()
            .await
            .get(symbol)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("Symbol not found"))
    }
    
    pub async fn set_market_price(&self, symbol: String, price: f64) {
        self.market_data.write().await.insert(symbol, price);
    }
    
    pub async fn set_fail_next_order(&self, fail: bool) {
        *self.fail_next_order.write().await = fail;
    }
    
    pub async fn is_connected(&self) -> bool {
        *self.connection_status.read().await
    }
    
    pub async fn disconnect(&self) {
        *self.connection_status.write().await = false;
    }
    
    pub async fn connect(&self) {
        *self.connection_status.write().await = true;
    }
}

#[derive(Debug, Clone)]
pub struct MockOrder {
    pub id: Uuid,
    pub symbol: String,
    pub side: String,
    pub quantity: f64,
    pub price: Option<f64>,
    pub order_type: String,
}

/// Mock risk manager for testing
pub struct MockRiskManager {
    max_position_size: f64,
    max_daily_loss: f64,
    current_positions: Arc<RwLock<HashMap<String, f64>>>,
    daily_pnl: Arc<RwLock<f64>>,
    risk_checks_enabled: Arc<RwLock<bool>>,
}

impl MockRiskManager {
    pub fn new() -> Self {
        Self {
            max_position_size: 100000.0,
            max_daily_loss: 5000.0,
            current_positions: Arc::new(RwLock::new(HashMap::new())),
            daily_pnl: Arc::new(RwLock::new(0.0)),
            risk_checks_enabled: Arc::new(RwLock::new(true)),
        }
    }
    
    pub async fn check_order_risk(&self, symbol: &str, quantity: f64, price: f64) -> Result<bool> {
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
    
    pub async fn update_position(&self, symbol: String, quantity: f64) {
        self.current_positions.write().await.insert(symbol, quantity);
    }
    
    pub async fn update_pnl(&self, pnl: f64) {
        *self.daily_pnl.write().await += pnl;
    }
    
    pub async fn disable_risk_checks(&self) {
        *self.risk_checks_enabled.write().await = false;
    }
    
    pub async fn enable_risk_checks(&self) {
        *self.risk_checks_enabled.write().await = true;
    }
    
    pub async fn reset(&self) {
        self.current_positions.write().await.clear();
        *self.daily_pnl.write().await = 0.0;
        *self.risk_checks_enabled.write().await = true;
    }
}

/// Mock database for testing
pub struct MockDatabase {
    data: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    fail_next: Arc<RwLock<bool>>,
}

impl MockDatabase {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
            fail_next: Arc::new(RwLock::new(false)),
        }
    }
    
    pub async fn insert(&self, key: String, value: Vec<u8>) -> Result<()> {
        if *self.fail_next.read().await {
            *self.fail_next.write().await = false;
            return Err(anyhow::anyhow!("Database insert failed"));
        }
        
        self.data.write().await.insert(key, value);
        Ok(())
    }
    
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.data.read().await.get(key).cloned())
    }
    
    pub async fn delete(&self, key: &str) -> Result<()> {
        self.data.write().await.remove(key);
        Ok(())
    }
    
    pub async fn clear(&self) {
        self.data.write().await.clear();
    }
    
    pub async fn set_fail_next(&self, fail: bool) {
        *self.fail_next.write().await = fail;
    }
}

/// Mock WebSocket client for testing
pub struct MockWebSocketClient {
    messages: Arc<Mutex<Vec<String>>>,
    is_connected: Arc<RwLock<bool>>,
    auto_reconnect: Arc<RwLock<bool>>,
}

impl MockWebSocketClient {
    pub fn new() -> Self {
        Self {
            messages: Arc::new(Mutex::new(Vec::new())),
            is_connected: Arc::new(RwLock::new(false)),
            auto_reconnect: Arc::new(RwLock::new(true)),
        }
    }
    
    pub async fn connect(&self, _url: &str) -> Result<()> {
        *self.is_connected.write().await = true;
        Ok(())
    }
    
    pub async fn disconnect(&self) {
        *self.is_connected.write().await = false;
    }
    
    pub async fn send(&self, message: String) -> Result<()> {
        if !*self.is_connected.read().await {
            return Err(anyhow::anyhow!("Not connected"));
        }
        
        self.messages.lock().await.push(message);
        Ok(())
    }
    
    pub async fn receive(&self) -> Result<Option<String>> {
        if !*self.is_connected.read().await {
            return Err(anyhow::anyhow!("Not connected"));
        }
        
        Ok(self.messages.lock().await.pop())
    }
    
    pub async fn inject_message(&self, message: String) {
        self.messages.lock().await.push(message);
    }
    
    pub async fn is_connected(&self) -> bool {
        *self.is_connected.read().await
    }
}

/// Mock gRPC service for testing
pub struct MockGrpcService<T> {
    responses: Arc<Mutex<Vec<T>>>,
    call_count: Arc<Mutex<usize>>,
    fail_next: Arc<RwLock<bool>>,
}

impl<T: Clone> MockGrpcService<T> {
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(Vec::new())),
            call_count: Arc::new(Mutex::new(0)),
            fail_next: Arc::new(RwLock::new(false)),
        }
    }
    
    pub async fn set_response(&self, response: T) {
        self.responses.lock().await.push(response);
    }
    
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
    
    pub async fn get_call_count(&self) -> usize {
        *self.call_count.lock().await
    }
    
    pub async fn reset(&self) {
        self.responses.lock().await.clear();
        *self.call_count.lock().await = 0;
        *self.fail_next.write().await = false;
    }
    
    pub async fn set_fail_next(&self, fail: bool) {
        *self.fail_next.write().await = fail;
    }
}