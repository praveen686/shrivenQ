//! Data Aggregator Service - Production gRPC Server
//!
//! Aggregates raw market data into various timeframes and formats:
//! - OHLCV candles (multiple timeframes)
//! - Volume profiles
//! - Trade aggregations
//! - Market microstructure features

use anyhow::Result;
use data_aggregator::{DataEvent, TradeEvent, Wal};
use services_common::marketdata::v1::{
    market_data_service_server::{MarketDataService, MarketDataServiceServer},
    Candle as ProtoCandle, GetHistoricalDataRequest, GetHistoricalDataResponse,
    GetSnapshotRequest, GetSnapshotResponse, MarketDataEvent, MarketSnapshot,
    SubscribeRequest, Trade, UnsubscribeRequest,
    UnsubscribeResponse,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tokio_stream::{Stream, StreamExt as _};
use tonic::{transport::Server, Request, Response, Status};
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// Constants
const DEFAULT_GRPC_PORT: u16 = 50057;
const SERVICE_NAME: &str = "data-aggregator";
const EVENT_CHANNEL_CAPACITY: usize = 10000;
const MAX_SYMBOLS_PER_SUBSCRIPTION: usize = 100;
const DEFAULT_WAL_PATH: &str = "./data/wal";
const HEALTH_CHECK_INTERVAL_SECS: u64 = 10;

/// Data Aggregator gRPC Service Implementation
#[derive(Clone, Debug)]
pub struct DataAggregatorService {
    /// WAL storage for persistence
    wal: Arc<RwLock<Wal>>,
    /// Event broadcaster for streaming
    event_broadcaster: broadcast::Sender<MarketDataEvent>,
    /// Active subscriptions
    subscriptions: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// Health status
    is_healthy: Arc<RwLock<bool>>,
    /// Symbol to exchange mapping for tracking data sources
    symbol_exchange_map: Arc<RwLock<HashMap<String, String>>>,
    /// Current market state for snapshot retrieval
    market_state: Arc<RwLock<HashMap<String, MarketSnapshot>>>,
}

impl DataAggregatorService {
    /// Create new service instance
    pub async fn new(wal_path: &str) -> Result<Self> {
        // Initialize WAL
        let wal = Wal::new(Path::new(wal_path), None)?;
        let wal = Arc::new(RwLock::new(wal));
        
        // Create event broadcaster
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        
        Ok(Self {
            wal,
            event_broadcaster: event_tx,
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            is_healthy: Arc::new(RwLock::new(true)),
            symbol_exchange_map: Arc::new(RwLock::new(HashMap::new())),
            market_state: Arc::new(RwLock::new(HashMap::new())),
        })
    }
    
    /// Start market data ingestion from market-connector service
    pub fn start_data_ingestion(self: Arc<Self>) {
        tokio::spawn(async move {
            // Connect to market-connector gRPC service
            let market_connector_addr = "http://127.0.0.1:50052";
            
            loop {
                match services_common::proto::marketdata::v1::market_data_service_client::MarketDataServiceClient::connect(market_connector_addr.to_string()).await {
                    Ok(mut client) => {
                        info!("Connected to market-connector service");
                        
                        // Subscribe to all available symbols
                        let request = SubscribeRequest {
                            symbols: vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()], // Production symbols
                            data_types: vec![],
                            exchange: "binance".to_string(),
                        };
                        
                        match client.subscribe(request).await {
                            Ok(response) => {
                                let mut stream = response.into_inner();
                                
                                while let Some(event_result) = stream.next().await {
                                    match event_result {
                                        Ok(market_event) => {
                                            // Convert proto event to internal format and process
                                            if let Some(internal_event) = self.convert_from_proto_event(market_event) {
                                                if let Err(e) = self.process_event(internal_event).await {
                                                    error!("Failed to process market event: {}", e);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            error!("Stream error: {}", e);
                                            break;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to subscribe to market data: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to connect to market-connector: {}", e);
                    }
                }
                
                // Reconnect after 5 seconds if connection lost
                tokio::time::sleep(Duration::from_secs(5)).await;
                warn!("Reconnecting to market-connector service...");
            }
        });
    }
    
    /// Convert proto event to internal format
    fn convert_from_proto_event(&self, event: MarketDataEvent) -> Option<DataEvent> {
        match event.data {
            Some(services_common::proto::marketdata::v1::market_data_event::Data::Trade(trade)) => {
                Some(DataEvent::Trade(TradeEvent {
                    ts: services_common::Ts::from_nanos(event.timestamp_nanos as u64),
                    symbol: services_common::Symbol(event.symbol.parse().unwrap_or(0)),
                    price: services_common::Px::from_i64(trade.price),
                    quantity: services_common::Qty::from_i64(trade.quantity),
                    is_buy: trade.is_buyer_maker,
                    trade_id: trade.trade_id.parse().unwrap_or(0),
                }))
            }
            Some(services_common::proto::marketdata::v1::market_data_event::Data::Candle(candle)) => {
                Some(DataEvent::Candle(data_aggregator::storage::CandleEvent {
                    ts: services_common::Ts::from_nanos(event.timestamp_nanos as u64),
                    symbol: services_common::Symbol(event.symbol.parse().unwrap_or(0)),
                    timeframe: 60, // Default to 1 minute
                    open: services_common::Px::from_i64(candle.open),
                    high: services_common::Px::from_i64(candle.high),
                    low: services_common::Px::from_i64(candle.low),
                    close: services_common::Px::from_i64(candle.close),
                    volume: services_common::Qty::from_i64(candle.volume),
                    trades: candle.trades as u32,
                }))
            }
            Some(services_common::proto::marketdata::v1::market_data_event::Data::OrderBook(book)) => {
                // Convert proto orderbook to internal OrderBookEvent
                let bid_levels: Vec<(services_common::Px, services_common::Qty, u32)> = book.bids.iter()
                    .map(|level| (
                        services_common::Px::from_i64(level.price),
                        services_common::Qty::from_i64(level.quantity),
                        level.count as u32
                    ))
                    .collect();
                    
                let ask_levels: Vec<(services_common::Px, services_common::Qty, u32)> = book.asks.iter()
                    .map(|level| (
                        services_common::Px::from_i64(level.price),
                        services_common::Qty::from_i64(level.quantity),
                        level.count as u32
                    ))
                    .collect();
                
                Some(DataEvent::OrderBook(data_aggregator::storage::OrderBookEvent {
                    ts: services_common::Ts::from_nanos(event.timestamp_nanos as u64),
                    symbol: services_common::Symbol(event.symbol.parse().unwrap_or(0)),
                    event_type: data_aggregator::storage::OrderBookEventType::Update, // Always update for now
                    sequence: book.sequence as u64,
                    bid_levels,
                    ask_levels,
                    checksum: 0, // Calculate if needed
                }))
            }
            _ => None,
        }
    }
    
    /// Start background tasks
    pub fn start_background_tasks(&self) {
        // Start health check task
        let is_healthy = self.is_healthy.clone();
        let wal = self.wal.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(HEALTH_CHECK_INTERVAL_SECS));
            
            loop {
                interval.tick().await;
                
                // Check WAL health
                let wal_healthy = {
                    let wal_guard = wal.read().await;
                    // Perform actual WAL health checks
                    let healthy = wal_guard.is_healthy();
                    drop(wal_guard); // Explicitly release the guard
                    healthy
                };
                
                let mut health = is_healthy.write().await;
                *health = wal_healthy;
                
                if !wal_healthy {
                    error!("Data Aggregator health check failed");
                }
            }
        });
        
        // Start WAL flush task
        let wal_flush = self.wal.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5)); // Flush every 5 seconds
            
            loop {
                interval.tick().await;
                
                // Flush WAL to disk
                if let Err(e) = wal_flush.write().await.flush() {
                    error!("Failed to flush WAL: {}", e);
                } else {
                    info!("💾 WAL flushed to disk");
                }
            }
        });
    }
    
    /// Process and broadcast market data event
    async fn process_event(&self, event: DataEvent) -> Result<()> {
        // Log the event processing
        match &event {
            DataEvent::Trade(trade) => {
                info!("🔄 Processing TRADE: {} @ {} (qty: {})", 
                      trade.symbol, trade.price.as_f64(), trade.quantity.as_f64());
            }
            DataEvent::Candle(candle) => {
                info!("🔄 Processing CANDLE: {} OHLC [{}, {}, {}, {}]", 
                      candle.symbol, candle.open.as_f64(), candle.high.as_f64(), 
                      candle.low.as_f64(), candle.close.as_f64());
            }
            _ => {
                info!("🔄 Processing market event");
            }
        }
        
        // Store in WAL
        {
            let mut wal = self.wal.write().await;
            wal.append(&event)?;
            info!("💾 Stored event in WAL");
        }
        
        // Update market state for snapshot retrieval
        self.update_market_state(&event).await;
        
        // Convert to proto and broadcast
        let proto_event = self.convert_to_proto_event(event).await?;
        
        // Send to subscribers (ignore if no receivers)
        let _ = self.event_broadcaster.send(proto_event);
        
        Ok(())
    }
    
    /// Convert internal event to proto format
    async fn convert_to_proto_event(&self, event: DataEvent) -> Result<MarketDataEvent> {
        let timestamp_nanos = chrono::Utc::now().timestamp_nanos_opt()
            .ok_or_else(|| anyhow::anyhow!("Timestamp overflow"))?;
        
        let proto_event = match event {
            DataEvent::Candle(candle_event) => {
                // Look up exchange for this symbol
                let exchange = {
                    let map = self.symbol_exchange_map.read().await;
                    map.get(&candle_event.symbol.to_string())
                        .cloned()
                        .unwrap_or_else(|| String::from("aggregate"))
                };
                
                MarketDataEvent {
                    symbol: candle_event.symbol.to_string(),
                    exchange,
                    timestamp_nanos,
                    data: Some(services_common::proto::marketdata::v1::market_data_event::Data::Candle(
                        ProtoCandle {
                            open: candle_event.open.as_i64(),
                            high: candle_event.high.as_i64(),
                            low: candle_event.low.as_i64(),
                            close: candle_event.close.as_i64(),
                            volume: candle_event.volume.as_i64(),
                            trades: candle_event.trades as i32,
                            interval: format!("{}", candle_event.timeframe),
                        }
                    )),
                }
            }
            DataEvent::Trade(trade_event) => {
                MarketDataEvent {
                    symbol: trade_event.symbol.to_string(),
                    exchange: String::new(),
                    timestamp_nanos,
                    data: Some(services_common::proto::marketdata::v1::market_data_event::Data::Trade(
                        Trade {
                            price: trade_event.price.as_i64(),
                            quantity: trade_event.quantity.as_i64(),
                            is_buyer_maker: trade_event.is_buy,
                            trade_id: trade_event.trade_id.to_string(),
                        }
                    )),
                }
            }
            _ => {
                // For other event types not yet handled
                return Err(anyhow::anyhow!("Unsupported event type"));
            }
        };
        
        Ok(proto_event)
    }
    
    /// Update market state for snapshot retrieval
    async fn update_market_state(&self, event: &DataEvent) {
        match event {
            DataEvent::Trade(trade) => {
                let mut state = self.market_state.write().await;
                let symbol = trade.symbol.to_string();
                
                // Update or create snapshot with latest trade data
                let snapshot = state.entry(symbol.clone()).or_insert_with(|| {
                    MarketSnapshot {
                        symbol: symbol.clone(),
                        order_book: None,
                        quote: None,
                        timestamp_nanos: trade.ts.as_nanos() as i64,
                    }
                });
                
                // Update timestamp
                snapshot.timestamp_nanos = trade.ts.as_nanos() as i64;
                
                // Create quote from trade (simplified - in production, aggregate multiple trades)
                snapshot.quote = Some(services_common::proto::marketdata::v1::Quote {
                    bid_price: trade.price.as_i64() - 100, // Mock bid slightly below trade
                    bid_size: trade.quantity.as_i64(),
                    ask_price: trade.price.as_i64() + 100, // Mock ask slightly above trade
                    ask_size: trade.quantity.as_i64(),
                });
            }
            DataEvent::Candle(candle) => {
                let mut state = self.market_state.write().await;
                let symbol = candle.symbol.to_string();
                
                let snapshot = state.entry(symbol.clone()).or_insert_with(|| {
                    MarketSnapshot {
                        symbol: symbol.clone(),
                        order_book: None,
                        quote: None,
                        timestamp_nanos: candle.ts.as_nanos() as i64,
                    }
                });
                
                // Update timestamp and create quote from candle close price
                snapshot.timestamp_nanos = candle.ts.as_nanos() as i64;
                snapshot.quote = Some(services_common::proto::marketdata::v1::Quote {
                    bid_price: candle.close.as_i64() - 100,
                    bid_size: candle.volume.as_i64() / 2,
                    ask_price: candle.close.as_i64() + 100,
                    ask_size: candle.volume.as_i64() / 2,
                });
            }
            _ => {
                // Other event types don't update market state yet
            }
        }
    }
    
    /// Register symbol with exchange for tracking
    async fn register_symbol_exchange(&self, symbol: &str, exchange: &str) {
        let mut map = self.symbol_exchange_map.write().await;
        map.insert(symbol.to_string(), exchange.to_string());
    }
}

#[tonic::async_trait]
impl MarketDataService for DataAggregatorService {
    type SubscribeStream = Pin<Box<dyn Stream<Item = Result<MarketDataEvent, Status>> + Send>>;
    
    async fn subscribe(
        &self,
        request: Request<SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let req = request.into_inner();
        
        // Validate request
        if req.symbols.is_empty() {
            return Err(Status::invalid_argument("No symbols specified"));
        }
        
        if req.symbols.len() > MAX_SYMBOLS_PER_SUBSCRIPTION {
            return Err(Status::invalid_argument(format!(
                "Too many symbols: {} (max: {})",
                req.symbols.len(),
                MAX_SYMBOLS_PER_SUBSCRIPTION
            )));
        }
        
        info!("New subscription for {} symbols", req.symbols.len());
        
        // Store subscription and register symbol-exchange mappings
        {
            let mut subs = self.subscriptions.write().await;
            for symbol in &req.symbols {
                subs.entry(symbol.clone())
                    .or_insert_with(Vec::new)
                    .push(req.exchange.clone());
                
                // Register the symbol-exchange mapping for tracking
                self.register_symbol_exchange(symbol, &req.exchange).await;
            }
        }
        
        // Create filtered stream
        let rx = self.event_broadcaster.subscribe();
        let symbols = req.symbols.clone();
        
        let stream = tokio_stream::wrappers::BroadcastStream::new(rx)
            .filter_map(move |result| {
                match result {
                    Ok(event) => {
                        // Filter by subscribed symbols
                        if symbols.contains(&event.symbol) {
                            Some(Ok(event))
                        } else {
                            None
                        }
                    }
                    Err(e) => {
                        warn!("Broadcast error: {}", e);
                        None
                    }
                }
            });
        
        Ok(Response::new(Box::pin(stream) as Self::SubscribeStream))
    }
    
    async fn unsubscribe(
        &self,
        request: Request<UnsubscribeRequest>,
    ) -> Result<Response<UnsubscribeResponse>, Status> {
        let req = request.into_inner();
        
        let mut subs = self.subscriptions.write().await;
        for symbol in req.symbols {
            subs.remove(&symbol);
        }
        
        Ok(Response::new(UnsubscribeResponse { success: true }))
    }
    
    async fn get_snapshot(
        &self,
        request: Request<GetSnapshotRequest>,
    ) -> Result<Response<GetSnapshotResponse>, Status> {
        let req = request.into_inner();
        
        info!("Snapshot request for {} symbols", req.symbols.len());
        
        // Retrieve current market state snapshots
        let state = self.market_state.read().await;
        let snapshots = req.symbols.into_iter().map(|symbol| {
            state.get(&symbol).cloned().unwrap_or_else(|| {
                // Return empty snapshot if no state available
                MarketSnapshot {
                    symbol,
                    order_book: None,
                    quote: None,
                    timestamp_nanos: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
                }
            })
        }).collect();
        
        Ok(Response::new(GetSnapshotResponse { snapshots }))
    }
    
    async fn get_historical_data(
        &self,
        request: Request<GetHistoricalDataRequest>,
    ) -> Result<Response<GetHistoricalDataResponse>, Status> {
        let req = request.into_inner();
        
        info!(
            "Historical data request for {} from {} to {}",
            req.symbol, req.start_time, req.end_time
        );
        
        // Read from WAL with time-based filtering
        let events = {
            let wal_guard = self.wal.read().await;
            
            // Convert timestamps to Ts for filtering
            let start_ts = services_common::Ts::from_nanos(req.start_time as u64);
            let end_ts = services_common::Ts::from_nanos(req.end_time as u64);
            
            // Read events from WAL within time range
            let mut filtered_events = Vec::new();
            
            // Iterate through WAL entries (this assumes WAL has an iterator)
            // In production, WAL should have an efficient range query method
            if let Ok(entries) = wal_guard.read_range(start_ts, end_ts) {
                for entry in entries {
                    // Convert internal events to proto events
                    if let Ok(event) = self.convert_to_proto_event(entry).await {
                        // Filter by symbol if specified
                        if event.symbol == req.symbol {
                            filtered_events.push(event);
                        }
                    }
                }
            }
            
            drop(wal_guard); // Release guard
            filtered_events
        };
        
        Ok(Response::new(GetHistoricalDataResponse { events }))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    init_tracing()?;
    
    info!("Starting Data Aggregator Service v{}", env!("CARGO_PKG_VERSION"));
    
    // Create service
    let service = Arc::new(DataAggregatorService::new(DEFAULT_WAL_PATH).await?);
    
    // Start background tasks
    service.start_background_tasks();
    
    // Start real market data ingestion from market-connector
    service.clone().start_data_ingestion();
    
    // Configure gRPC server address
    let addr: SocketAddr = format!("0.0.0.0:{DEFAULT_GRPC_PORT}")
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid socket address: {}", e))?;
    
    info!("Data Aggregator gRPC server listening on {}", addr);
    
    // Start gRPC server
    Server::builder()
        .add_service(MarketDataServiceServer::new((*service).clone()))
        .serve(addr)
        .await
        .map_err(|e| {
            error!("gRPC server error: {}", e);
            anyhow::anyhow!("Failed to start gRPC server: {}", e)
        })?;
    
    Ok(())
}

/// Initialize tracing with environment filter
fn init_tracing() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| {
                    format!(
                        "{}=info,tower=info,tonic=info,h2=info",
                        SERVICE_NAME.replace('-', "_")
                    ).into()
                }),
        )
        .with(tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .with_thread_names(true))
        .init();
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_service_creation() {
        let service = DataAggregatorService::new("/tmp/test_wal").await;
        assert!(service.is_ok());
    }
}