//! gRPC service implementation for Market Connector

use crate::{MarketConnectorService, MarketDataEvent as InternalEvent, MarketData, SubscriptionRequest as InternalSubRequest, MarketDataType};
use anyhow::Result;
use services_common::marketdata::v1::{
    market_data_service_server::MarketDataService,
    SubscribeRequest, UnsubscribeRequest, UnsubscribeResponse, GetSnapshotRequest, GetSnapshotResponse,
    GetHistoricalDataRequest, GetHistoricalDataResponse, MarketDataEvent, OrderBookUpdate,
    PriceLevel, Trade, Quote, MarketSnapshot
};
use std::pin::Pin;
use std::collections::{HashSet, HashMap};
use std::sync::Arc;
use tokio::sync::{mpsc, broadcast, RwLock};
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tonic::{Request, Response, Status, Result as TonicResult};
use tracing::{info, warn, error};
use futures_util::{SinkExt, StreamExt as FuturesStreamExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Fixed-point multiplier for price/quantity conversion (4 decimal places)
const FIXED_POINT_MULTIPLIER: f64 = 10000.0;

/// Channel capacity for market data streams
const MARKET_DATA_CHANNEL_CAPACITY: usize = 10000;

/// Binance WebSocket URLs (correct format per official docs)
const BINANCE_SPOT_WS_URL: &str = "wss://stream.binance.com:9443";
const BINANCE_FUTURES_WS_URL: &str = "wss://fstream.binance.com";

/// Reconnection parameters
const RECONNECT_DELAY_MS: u64 = 5000;
const MAX_RECONNECT_ATTEMPTS: u32 = 5;

/// Mock bid price for snapshots (100.00 in fixed point)
const MOCK_BID_PRICE: i64 = 1000000;

/// Mock ask price for snapshots (101.00 in fixed point) 
const MOCK_ASK_PRICE: i64 = 1010000;

/// Mock quantity for snapshots (10.00 in fixed point)
const MOCK_QUANTITY: i64 = 100000;

/// Safely convert floating-point price to fixed-point integer
/// NOTE: This is an API boundary conversion from external systems (exchanges)
/// to our internal fixed-point representation. External APIs use f64, internal uses i64.
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_precision_loss)]
#[allow(clippy::cast_possible_truncation)]
fn price_to_fixed_point(price: f64) -> i64 { // external API conversion
    let scaled = price * FIXED_POINT_MULTIPLIER;
    if scaled.is_finite() {
        // API boundary conversion - clamping to i64 range
        scaled.clamp(i64::MIN as f64, i64::MAX as f64) as i64
    } else {
        0
    }
}

/// Safely convert floating-point quantity to fixed-point integer
fn quantity_to_fixed_point(quantity: f64) -> i64 { // external API conversion
    let scaled = quantity * FIXED_POINT_MULTIPLIER;
    if scaled.is_finite() && scaled >= 0.0 {
        scaled.clamp(0.0, i64::MAX as f64) as i64
    } else {
        0
    }
}

/// Safely convert u64 sequence to i64 for proto
fn sequence_to_proto(sequence: u64) -> i64 {
    if sequence <= i64::MAX as u64 {
        sequence as i64
    } else {
        i64::MAX
    }
}

/// Get current timestamp in nanoseconds as i64 for proto
fn current_timestamp_nanos() -> Result<i64, Status> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| Status::internal("System time error"))?
        .as_nanos();
    
    if nanos <= i64::MAX as u128 {
        Ok(nanos as i64)
    } else {
        Ok(i64::MAX)
    }
}

/// Subscription filter for market data events
#[derive(Debug, Clone)]
struct SubscriptionFilter {
    symbols: HashSet<String>,
    exchanges: HashSet<String>,
    data_types: HashSet<MarketDataType>,
}

impl SubscriptionFilter {
    fn new() -> Self {
        Self {
            symbols: HashSet::new(),
            exchanges: HashSet::new(),
            data_types: HashSet::new(),
        }
    }
    
    fn add_subscription(&mut self, symbol: String, exchange: String, data_types: Vec<MarketDataType>) {
        self.symbols.insert(symbol);
        self.exchanges.insert(exchange);
        for data_type in data_types {
            self.data_types.insert(data_type);
        }
    }
    
    fn matches(&self, event: &InternalEvent) -> bool {
        self.symbols.contains(&event.symbol) && 
        self.exchanges.contains(&event.exchange) &&
        self.matches_data_type(&event.data)
    }
    
    fn matches_data_type(&self, data: &MarketData) -> bool {
        // If no specific data types requested, accept all data types
        if self.data_types.is_empty() {
            return true;
        }
        
        match data {
            MarketData::OrderBook { .. } => self.data_types.contains(&MarketDataType::OrderBook),
            MarketData::Trade { .. } => self.data_types.contains(&MarketDataType::Trades),
            MarketData::Quote { .. } => self.data_types.contains(&MarketDataType::Quotes),
        }
    }
}

/// Active WebSocket connection info
#[derive(Debug, Clone)]
struct WebSocketConnection {
    exchange: String,
    symbols: Vec<String>,
    connected: bool,
    market_type: String, // "spot" or "futures"
}

impl WebSocketConnection {
    /// Check if this connection handles the given symbol and exchange
    fn handles_symbol(&self, symbol: &str, exchange: &str) -> bool {
        self.symbols.contains(&symbol.to_string()) && self.exchange == exchange
    }
    
    /// Get connection info for monitoring
    fn connection_info(&self) -> String {
        format!("{} {} connection for symbols: {:?}", 
                self.exchange, self.market_type, self.symbols)
    }
    
    /// Check if connection is active
    fn is_connected(&self) -> bool {
        self.connected
    }
}

/// gRPC Market Data Service wrapper
impl std::fmt::Debug for MarketDataGrpcService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarketDataGrpcService")
            .field("inner", &self.inner)
            .field("event_broadcaster", &"Arc<broadcast::Sender<InternalEvent>>")
            .field("active_connections", &"Arc<RwLock<HashMap<String, WebSocketConnection>>>")
            .finish()
    }
}

/// gRPC service wrapper for market connector functionality
///
/// This service provides a gRPC interface to the market connector capabilities,
/// including real-time market data streaming, instrument management, and
/// WebSocket connection handling. It acts as a bridge between the internal
/// market connector service and external gRPC clients.
///
/// # Features
/// - **Market Data Streaming**: Real-time market data via gRPC streams
/// - **Instrument Services**: Comprehensive instrument lookup and management
/// - **Connection Management**: WebSocket connection lifecycle management
/// - **Event Broadcasting**: Multi-client event distribution
///
/// # Architecture
/// The service wraps the core MarketConnectorService and adds:
/// - gRPC protocol handling and serialization
/// - Client connection management and tracking
/// - Event broadcasting for multiple subscribers
/// - WebSocket connection state management
///
/// # Thread Safety
/// All components are designed for concurrent access with appropriate
/// synchronization mechanisms for multi-client scenarios.
pub struct MarketDataGrpcService {
    inner: MarketConnectorService,
    event_broadcaster: Arc<broadcast::Sender<InternalEvent>>,
    active_connections: Arc<RwLock<HashMap<String, WebSocketConnection>>>,
}

impl MarketDataGrpcService {
    /// Create new gRPC service wrapper
    pub fn new() -> (Self, mpsc::Sender<InternalEvent>) {
        let (event_tx, _rx) = mpsc::channel(MARKET_DATA_CHANNEL_CAPACITY);
        let (broadcaster, _) = broadcast::channel(MARKET_DATA_CHANNEL_CAPACITY);
        let broadcaster = Arc::new(broadcaster);
        
        let service = MarketConnectorService::new(event_tx.clone());
        
        // Spawn task to forward events from internal service to broadcaster
        let broadcaster_clone = broadcaster.clone();
        tokio::spawn(async move {
            let mut rx = _rx;
            while let Some(event) = rx.recv().await {
                if let Err(e) = broadcaster_clone.send(event) {
                    error!("Failed to broadcast market data event: {}", e);
                }
            }
        });
        
        (Self {
            inner: service,
            event_broadcaster: broadcaster,
            active_connections: Arc::new(RwLock::new(HashMap::new())),
        }, event_tx)
    }
    
    /// Get reference to internal service
    pub fn inner(&self) -> &MarketConnectorService {
        &self.inner
    }
    
    /// Get mutable reference to internal service
    pub fn inner_mut(&mut self) -> &mut MarketConnectorService {
        &mut self.inner
    }
    
    /// Get connection status for monitoring
    pub async fn get_connection_status(&self) -> Vec<String> {
        let connections = self.active_connections.read().await;
        connections.values()
            .map(|conn| {
                let status = if conn.is_connected() { "CONNECTED" } else { "DISCONNECTED" };
                format!("{} - {}", conn.connection_info(), status)
            })
            .collect()
    }
    
    /// Check if we have an active connection for a symbol and exchange
    pub async fn has_connection(&self, symbol: &str, exchange: &str) -> bool {
        let connections = self.active_connections.read().await;
        connections.values()
            .any(|conn| conn.handles_symbol(symbol, exchange) && conn.is_connected())
    }
    
    /// Start Binance WebSocket connection for real market data
    async fn start_binance_websocket(&self, symbols: Vec<String>, exchange: String, market_type: &str) {
        let connection_id = format!("binance_{}_{}", market_type, uuid::Uuid::new_v4());
        
        // Store connection info
        {
            let mut connections = self.active_connections.write().await;
            connections.insert(connection_id.clone(), WebSocketConnection {
                exchange: exchange.clone(),
                symbols: symbols.clone(),
                connected: false,
                market_type: market_type.to_string(),
            });
        }
        
        let event_broadcaster = self.event_broadcaster.clone();
        let active_connections = self.active_connections.clone();
        let connection_id_clone = connection_id.clone();
        let market_type_owned = market_type.to_string();
        
        // Spawn WebSocket connection task
        tokio::spawn(async move {
            let mut reconnect_attempts = 0u32;
            
            loop {
                // Build WebSocket URL with correct format per Binance API docs
                let streams: Vec<String> = symbols.iter()
                    .flat_map(|s| {
                        let symbol = s.to_lowercase();
                        match market_type_owned.as_str() {
                            "futures" => vec![
                                format!("{}@ticker", symbol),    // 24hr ticker
                                format!("{}@depth@100ms", symbol), // Order book depth  
                                format!("{}@aggTrade", symbol),   // Aggregated trades (futures)
                            ],
                            _ => vec![
                                format!("{}@ticker", symbol),    // 24hr ticker
                                format!("{}@depth@100ms", symbol), // Order book depth
                                format!("{}@trade", symbol),     // Individual trades (spot)
                            ]
                        }
                    })
                    .collect();
                    
                let stream_path = streams.join("/");
                let base_url = match market_type_owned.as_str() {
                    "futures" => BINANCE_FUTURES_WS_URL,
                    _ => BINANCE_SPOT_WS_URL, // Default to spot
                };
                let ws_url = format!("{}/stream?streams={}", base_url, stream_path);
                
                info!("Connecting to Binance {} WebSocket: {}", market_type_owned, ws_url);
                
                match connect_async(&ws_url).await {
                    Ok((ws_stream, _)) => {
                        info!("✅ Binance {} WebSocket connected successfully", market_type_owned);
                        
                        // Update connection status
                        {
                            let mut connections = active_connections.write().await;
                            if let Some(conn) = connections.get_mut(&connection_id_clone) {
                                conn.connected = true;
                            }
                        }
                        
                        reconnect_attempts = 0;
                        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
                        
                        // Handle WebSocket messages
                        while let Some(msg_result) = FuturesStreamExt::next(&mut ws_receiver).await {
                            match msg_result {
                                Ok(Message::Text(text)) => {
                                    // Parse Binance message and convert to internal event
                                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                                        if let Some(stream) = json.get("stream").and_then(|s| s.as_str()) {
                                            if let Some(data) = json.get("data") {
                                                // Process different stream types
                                                if stream.contains("@depth") {
                                                    Self::process_binance_depth(data, &event_broadcaster);
                                                } else if stream.contains("@trade") {
                                                    Self::process_binance_trade(data, &event_broadcaster);
                                                } else if stream.contains("@ticker") {
                                                    Self::process_binance_ticker(data, &event_broadcaster);
                                                }
                                            }
                                        }
                                    }
                                }
                                Ok(Message::Ping(data)) => {
                                    // Respond to ping with pong
                                    let _ = ws_sender.send(Message::Pong(data)).await;
                                }
                                Ok(Message::Close(_)) => {
                                    warn!("Binance WebSocket closed");
                                    break;
                                }
                                Err(e) => {
                                    error!("WebSocket error: {}", e);
                                    break;
                                }
                                _ => {}
                            }
                        }
                        
                        // Update connection status
                        {
                            let mut connections = active_connections.write().await;
                            if let Some(conn) = connections.get_mut(&connection_id_clone) {
                                conn.connected = false;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to connect to Binance WebSocket: {}", e);
                        reconnect_attempts += 1;
                    }
                }
                
                // Check if we should stop reconnecting
                if reconnect_attempts >= MAX_RECONNECT_ATTEMPTS {
                    error!("Max reconnection attempts reached for Binance WebSocket");
                    break;
                }
                
                // Wait before reconnecting
                tokio::time::sleep(tokio::time::Duration::from_millis(RECONNECT_DELAY_MS)).await;
                info!("Reconnecting to Binance WebSocket... (attempt {})", reconnect_attempts + 1);
            }
        });
    }
    
    /// Process Binance depth update
    fn process_binance_depth(data: &serde_json::Value, broadcaster: &broadcast::Sender<InternalEvent>) {
        if let Ok(symbol) = data.get("s").and_then(|s| s.as_str()).ok_or(()) {
            let mut bids = Vec::new();
            let mut asks = Vec::new();
            
            // Parse bids
            if let Some(bid_array) = data.get("b").and_then(|b| b.as_array()) {
                for bid in bid_array.iter().take(5) {
                    if let Some(bid_pair) = bid.as_array() {
                        if bid_pair.len() >= 2 {
                            if let (Some(price_str), Some(qty_str)) = 
                                (bid_pair[0].as_str(), bid_pair[1].as_str()) {
                                if let (Ok(price), Ok(qty)) = 
                                    (price_str.parse::<f64>(), qty_str.parse::<f64>()) {
                                    bids.push((price, qty));
                                }
                            }
                        }
                    }
                }
            }
            
            // Parse asks
            if let Some(ask_array) = data.get("a").and_then(|a| a.as_array()) {
                for ask in ask_array.iter().take(5) {
                    if let Some(ask_pair) = ask.as_array() {
                        if ask_pair.len() >= 2 {
                            if let (Some(price_str), Some(qty_str)) = 
                                (ask_pair[0].as_str(), ask_pair[1].as_str()) {
                                if let (Ok(price), Ok(qty)) = 
                                    (price_str.parse::<f64>(), qty_str.parse::<f64>()) {
                                    asks.push((price, qty));
                                }
                            }
                        }
                    }
                }
            }
            
            // Create and broadcast event
            let event = InternalEvent {
                symbol: symbol.to_string(),
                exchange: "binance".to_string(),
                timestamp: chrono::Utc::now().timestamp_millis() as u64,
                data: MarketData::OrderBook {
                    bids: bids.clone(),
                    asks: asks.clone(),
                    sequence: data.get("u").and_then(|u| u.as_u64()).unwrap_or(0),
                },
            };
            
            info!("📊 DEPTH: {} - {} bids, {} asks", symbol, bids.len(), asks.len());
            
            // Display orderbook every 10th update to avoid spam
            static mut DEPTH_COUNTER: u32 = 0;
            unsafe {
                DEPTH_COUNTER += 1;
                if DEPTH_COUNTER % 10 == 0 {
                    Self::display_orderbook(&bids, &asks, symbol);
                }
            }
            
            if let Err(e) = broadcaster.send(event) {
                error!("Failed to broadcast depth event: {}", e);
            }
        }
    }
    
    /// Process Binance trade
    fn process_binance_trade(data: &serde_json::Value, broadcaster: &broadcast::Sender<InternalEvent>) {
        if let (Ok(symbol), Ok(price_str), Ok(qty_str), Ok(is_buyer_maker)) = (
            data.get("s").and_then(|s| s.as_str()).ok_or(()),
            data.get("p").and_then(|p| p.as_str()).ok_or(()),
            data.get("q").and_then(|q| q.as_str()).ok_or(()),
            data.get("m").and_then(|m| m.as_bool()).ok_or(()),
        ) {
            if let (Ok(price), Ok(quantity)) = (price_str.parse::<f64>(), qty_str.parse::<f64>()) {
                let event = InternalEvent {
                    symbol: symbol.to_string(),
                    exchange: "binance".to_string(),
                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                    data: MarketData::Trade {
                        price,
                        quantity,
                        side: if is_buyer_maker { "sell".to_string() } else { "buy".to_string() },
                        trade_id: data.get("t").and_then(|t| t.as_u64()).map(|id| id.to_string()).unwrap_or_else(|| "0".to_string()),
                    },
                };
                
                info!("📈 TRADE: {} {} @ {} (qty: {})", symbol, 
                      if is_buyer_maker { "SELL" } else { "BUY" }, price, quantity);
                
                if let Err(e) = broadcaster.send(event) {
                    error!("Failed to broadcast trade event: {}", e);
                }
            }
        }
    }
    
    /// Process Binance ticker
    fn process_binance_ticker(data: &serde_json::Value, broadcaster: &broadcast::Sender<InternalEvent>) {
        if let (Ok(symbol), Ok(bid_str), Ok(ask_str)) = (
            data.get("s").and_then(|s| s.as_str()).ok_or(()),
            data.get("b").and_then(|b| b.as_str()).ok_or(()),
            data.get("a").and_then(|a| a.as_str()).ok_or(()),
        ) {
            if let (Ok(bid_price), Ok(ask_price)) = (bid_str.parse::<f64>(), ask_str.parse::<f64>()) {
                let bid_qty = data.get("B").and_then(|b| b.as_str())
                    .and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                let ask_qty = data.get("A").and_then(|a| a.as_str())
                    .and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                    
                let event = InternalEvent {
                    symbol: symbol.to_string(),
                    exchange: "binance".to_string(),
                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                    data: MarketData::Quote {
                        bid_price,
                        bid_size: bid_qty,
                        ask_price,
                        ask_size: ask_qty,
                    },
                };
                
                info!("💰 QUOTE: {} - Bid: {} @ {} | Ask: {} @ {}", 
                      symbol, bid_qty, bid_price, ask_qty, ask_price);
                
                if let Err(e) = broadcaster.send(event) {
                    error!("Failed to broadcast ticker event: {}", e);
                }
            }
        }
    }
    
    /// Convert proto data type to internal data type
    fn convert_data_type(data_type: i32) -> MarketDataType {
        match data_type {
            1 => MarketDataType::OrderBook, // DATA_TYPE_ORDER_BOOK
            2 => MarketDataType::Trades,    // DATA_TYPE_TRADES  
            3 => MarketDataType::Quotes,    // DATA_TYPE_QUOTES
            4 => MarketDataType::Candles { interval: "1m".to_string() }, // DATA_TYPE_CANDLES
            _ => MarketDataType::Quotes, // Default fallback
        }
    }
    
    /// Convert internal market data event to proto event
    fn convert_event(event: InternalEvent) -> MarketDataEvent {
        let data = match event.data {
            MarketData::OrderBook { bids, asks, sequence } => {
                let bid_levels = bids.into_iter().map(|(price, qty)| PriceLevel {
                    price: price_to_fixed_point(price),
                    quantity: quantity_to_fixed_point(qty),
                    count: 1,
                }).collect();
                
                let ask_levels = asks.into_iter().map(|(price, qty)| PriceLevel {
                    price: price_to_fixed_point(price),
                    quantity: quantity_to_fixed_point(qty),
                    count: 1,
                }).collect();
                
                Some(services_common::proto::marketdata::v1::market_data_event::Data::OrderBook(
                    OrderBookUpdate {
                        bids: bid_levels,
                        asks: ask_levels,
                        sequence: sequence_to_proto(sequence),
                    }
                ))
            },
            MarketData::Trade { price, quantity, side: _, trade_id } => {
                Some(services_common::proto::marketdata::v1::market_data_event::Data::Trade(
                    Trade {
                        price: price_to_fixed_point(price),
                        quantity: quantity_to_fixed_point(quantity),
                        is_buyer_maker: false, // Could be derived from side
                        trade_id,
                    }
                ))
            },
            MarketData::Quote { bid_price, bid_size, ask_price, ask_size } => {
                Some(services_common::proto::marketdata::v1::market_data_event::Data::Quote(
                    Quote {
                        bid_price: price_to_fixed_point(bid_price),
                        bid_size: quantity_to_fixed_point(bid_size),
                        ask_price: price_to_fixed_point(ask_price),
                        ask_size: quantity_to_fixed_point(ask_size),
                    }
                ))
            },
        };
        
        MarketDataEvent {
            symbol: event.symbol,
            exchange: event.exchange,
            // Safe conversion: timestamp to i64 (proto-generated requirement)
            timestamp_nanos: match i64::try_from(event.timestamp) {
                Ok(val) => val,
                Err(_) => {
                    tracing::warn!("Event timestamp {} exceeds i64 range", event.timestamp);
                    i64::MAX
                }
            },
            data,
        }
    }
}

#[tonic::async_trait]
impl MarketDataService for MarketDataGrpcService {
    type SubscribeStream = Pin<Box<dyn Stream<Item = Result<MarketDataEvent, Status>> + Send>>;

    async fn subscribe(
        &self,
        request: Request<SubscribeRequest>,
    ) -> TonicResult<Response<Self::SubscribeStream>> {
        let req = request.into_inner();
        info!("Subscribe request: symbols={:?}, exchange={}", req.symbols, req.exchange);
        
        // Create a new receiver for this subscription
        let (tx, rx) = mpsc::channel(MARKET_DATA_CHANNEL_CAPACITY);
        
        // Build subscription filter
        let mut filter = SubscriptionFilter::new();
        
        // Process subscription for each symbol
        for symbol in req.symbols.clone() {
            let data_types = req.data_types.iter()
                .map(|dt| Self::convert_data_type(*dt))
                .collect::<Vec<_>>();
                
            let sub_request = InternalSubRequest {
                exchange: req.exchange.clone(),
                symbol: symbol.clone(),
                data_types: data_types.clone(),
            };
            
            // Add to subscription filter
            filter.add_subscription(symbol, req.exchange.clone(), data_types);
            
            // Log the subscription
            info!("Subscribed to {} on {} for data types: {:?}", 
                  sub_request.symbol, sub_request.exchange, sub_request.data_types);
        }
        
        // Start WebSocket connections for the requested exchange
        if req.exchange.to_lowercase() == "binance" {
            // Start both spot and futures connections for comprehensive market coverage
            self.start_binance_websocket(req.symbols.clone(), req.exchange.clone(), "spot").await;
            self.start_binance_websocket(req.symbols.clone(), req.exchange.clone(), "futures").await;
        }
        
        // Subscribe to broadcast events and filter for this subscription
        let broadcaster = self.event_broadcaster.clone();
        tokio::spawn(async move {
            let mut event_receiver = broadcaster.subscribe();
            
            while let Ok(event) = event_receiver.recv().await {
                // Filter events based on subscription criteria
                if filter.matches(&event) {
                    if let Err(e) = tx.send(event).await {
                        error!("Failed to send filtered market data event: {}", e);
                        break;
                    }
                }
            }
        });
        
        // Create stream from receiver
        let stream = ReceiverStream::new(rx)
            .map(|event| Ok(Self::convert_event(event)));
        
        Ok(Response::new(Box::pin(stream)))
    }

    async fn unsubscribe(
        &self,
        request: Request<UnsubscribeRequest>,
    ) -> TonicResult<Response<UnsubscribeResponse>> {
        let req = request.into_inner();
        info!("Unsubscribe request: symbols={:?}, exchange={}", req.symbols, req.exchange);
        
        // In a real implementation, we would unsubscribe from the internal service
        for symbol in req.symbols {
            info!("Would unsubscribe from {} on {}", symbol, req.exchange);
        }
        
        Ok(Response::new(UnsubscribeResponse { success: true }))
    }

    async fn get_snapshot(
        &self,
        request: Request<GetSnapshotRequest>,
    ) -> TonicResult<Response<GetSnapshotResponse>> {
        let req = request.into_inner();
        info!("Snapshot request: symbols={:?}, exchange={}", req.symbols, req.exchange);
        
        // In a real implementation, we would get current snapshots
        let timestamp = current_timestamp_nanos()?;
        let snapshots = req.symbols.into_iter().map(|symbol| {
            MarketSnapshot {
                symbol: symbol.clone(),
                order_book: Some(OrderBookUpdate {
                    bids: vec![PriceLevel { price: MOCK_BID_PRICE, quantity: MOCK_QUANTITY, count: 1 }],
                    asks: vec![PriceLevel { price: MOCK_ASK_PRICE, quantity: MOCK_QUANTITY, count: 1 }],
                    sequence: 1,
                }),
                quote: Some(Quote {
                    bid_price: MOCK_BID_PRICE,
                    bid_size: MOCK_QUANTITY, 
                    ask_price: MOCK_ASK_PRICE,
                    ask_size: MOCK_QUANTITY,
                }),
                timestamp_nanos: timestamp,
            }
        }).collect();
        
        Ok(Response::new(GetSnapshotResponse { snapshots }))
    }

    async fn get_historical_data(
        &self,
        request: Request<GetHistoricalDataRequest>,
    ) -> TonicResult<Response<GetHistoricalDataResponse>> {
        let req = request.into_inner();
        info!("Historical data request: symbol={}, exchange={}, start={}, end={}", 
              req.symbol, req.exchange, req.start_time, req.end_time);
        
        // In a real implementation, we would query historical data
        warn!("Historical data not implemented yet");
        
        Ok(Response::new(GetHistoricalDataResponse { events: vec![] }))
    }
}

impl MarketDataGrpcService {
    /// Display current orderbook in a nice format
    fn display_orderbook(bids: &[(f64, f64)], asks: &[(f64, f64)], symbol: &str) {
        info!("📋 ORDERBOOK: {} ============================", symbol);
        info!("       ASK PRICE    |    ASK QTY");
        for (i, (price, qty)) in asks.iter().enumerate() {
            if i < 5 {
                info!("    🔴 {:<12.2} | {:<10.4}", price, qty);
            }
        }
        info!("    --------------------------------");
        for (i, (price, qty)) in bids.iter().rev().enumerate() {
            if i < 5 {
                info!("    🟢 {:<12.2} | {:<10.4}", price, qty);
            }
        }
        info!("       BID PRICE    |    BID QTY");
        info!("===========================================");
    }
}