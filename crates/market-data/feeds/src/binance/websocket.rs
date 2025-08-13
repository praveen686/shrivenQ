//! Binance WebSocket implementation with testnet support

use crate::common::adapter::{FeedAdapter, FeedConfig};
use auth::{BinanceAuth, BinanceMarket};
use common::{L2Update, Px, Qty, Side, Symbol, Ts};
use futures_util::{SinkExt, StreamExt};
use rustc_hash::{FxBuildHasher, FxHashMap};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

/// Binance depth update message
#[derive(Debug, Deserialize)]
pub struct DepthUpdate {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "U")]
    pub first_update_id: u64,
    #[serde(rename = "u")]
    pub final_update_id: u64,
    #[serde(rename = "b")]
    pub bids: Vec<[String; 2]>,
    #[serde(rename = "a")]
    pub asks: Vec<[String; 2]>,
}

/// Binance depth snapshot (REST API)
#[derive(Debug, Deserialize)]
pub struct DepthSnapshot {
    #[serde(rename = "lastUpdateId")]
    pub last_update_id: u64,
    pub bids: Vec<[String; 2]>,
    pub asks: Vec<[String; 2]>,
}

/// Binance trade message
#[derive(Debug, Deserialize)]
pub struct TradeUpdate {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "p")]
    pub price: String,
    #[serde(rename = "q")]
    pub quantity: String,
    #[serde(rename = "m")]
    pub is_buyer_maker: bool,
}

/// Binance 24hr ticker
#[derive(Debug, Deserialize)]
pub struct TickerUpdate {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "c")]
    pub last_price: String,
    #[serde(rename = "b")]
    pub best_bid: String,
    #[serde(rename = "B")]
    pub best_bid_qty: String,
    #[serde(rename = "a")]
    pub best_ask: String,
    #[serde(rename = "A")]
    pub best_ask_qty: String,
}

/// Combined stream message
#[derive(Debug, Deserialize)]
pub struct StreamMessage {
    pub stream: String,
    pub data: serde_json::Value,
}

/// Order book manager for each symbol
struct OrderBookManager {
    symbol: Symbol,
    bids: Vec<(Px, Qty)>, // Using fixed-point types
    asks: Vec<(Px, Qty)>,
    last_update_id: u64,
    snapshot_received: bool,
}

impl OrderBookManager {
    fn new(symbol: Symbol) -> Self {
        Self {
            symbol,
            bids: Vec::with_capacity(20), // Typical depth
            asks: Vec::with_capacity(20),
            last_update_id: 0,
            snapshot_received: false,
        }
    }

    /// Apply depth snapshot
    fn apply_snapshot(&mut self, snapshot: DepthSnapshot) {
        self.bids.clear();
        self.asks.clear();

        // Parse bids
        for [price, qty] in snapshot.bids.iter().take(20) {
            if let (Ok(p), Ok(q)) = (price.parse::<f64>(), qty.parse::<f64>()) {
                if q > 0.0 {
                    self.bids.push((Px::new(p), Qty::new(q)));
                }
            }
        }

        // Parse asks
        for [price, qty] in snapshot.asks.iter().take(20) {
            if let (Ok(p), Ok(q)) = (price.parse::<f64>(), qty.parse::<f64>()) {
                if q > 0.0 {
                    self.asks.push((Px::new(p), Qty::new(q)));
                }
            }
        }

        // Sort books (Px implements Ord so we can use regular comparison)
        self.bids.sort_by(|a, b| b.0.cmp(&a.0)); // Descending
        self.asks.sort_by(|a, b| a.0.cmp(&b.0)); // Ascending

        self.last_update_id = snapshot.last_update_id;
        self.snapshot_received = true;

        debug!(
            "Applied snapshot with {} bids and {} asks",
            self.bids.len(),
            self.asks.len()
        );
    }

    /// Apply depth update
    fn apply_update(&mut self, update: &DepthUpdate) -> Vec<L2Update> {
        if !self.snapshot_received {
            return Vec::with_capacity(0);
        }

        // Check if update is in sequence
        if update.first_update_id > self.last_update_id + 1 {
            warn!(
                "Gap in updates: {} -> {}",
                self.last_update_id, update.first_update_id
            );
            self.snapshot_received = false;
            return Vec::with_capacity(0);
        }

        let ts = Ts::from_nanos(update.event_time * 1_000_000);
        let mut updates = Vec::with_capacity(20);

        // Update bids - parse directly to Px/Qty
        for [price_str, qty_str] in &update.bids {
            if let (Ok(price_f64), Ok(qty_f64)) = (price_str.parse::<f64>(), qty_str.parse::<f64>())
            {
                let price = Px::new(price_f64);
                let qty = Qty::new(qty_f64);
                Self::update_level(&mut self.bids, price, qty, false);
            }
        }

        // Update asks - parse directly to Px/Qty
        for [price_str, qty_str] in &update.asks {
            if let (Ok(price_f64), Ok(qty_f64)) = (price_str.parse::<f64>(), qty_str.parse::<f64>())
            {
                let price = Px::new(price_f64);
                let qty = Qty::new(qty_f64);
                Self::update_level(&mut self.asks, price, qty, true);
            }
        }

        // Generate L2Updates for top levels
        for (i, (price, qty)) in self.bids.iter().take(10).enumerate() {
            updates.push(L2Update::new(ts, self.symbol).with_level_data(
                Side::Bid,
                *price,
                *qty,
                u8::try_from(i).unwrap_or(0),
            ));
        }

        for (i, (price, qty)) in self.asks.iter().take(10).enumerate() {
            updates.push(L2Update::new(ts, self.symbol).with_level_data(
                Side::Ask,
                *price,
                *qty,
                u8::try_from(i).unwrap_or(0),
            ));
        }

        self.last_update_id = update.final_update_id;
        updates
    }

    /// Update a price level
    fn update_level(levels: &mut Vec<(Px, Qty)>, price: Px, qty: Qty, ascending: bool) {
        // Find existing level
        if let Some(pos) = levels.iter().position(|(p, _)| *p == price) {
            if qty == Qty::ZERO {
                levels.remove(pos);
            } else {
                levels[pos].1 = qty;
            }
        } else if qty > Qty::ZERO {
            // Add new level
            levels.push((price, qty));
            // Re-sort using Px's Ord implementation
            if ascending {
                levels.sort_by_key(|&(p, _)| p);
            } else {
                levels.sort_by_key(|&(p, _)| std::cmp::Reverse(p));
            }
            // Keep only top 20 levels
            levels.truncate(20);
        }
    }
}

/// Binance WebSocket feed with testnet support
pub struct BinanceWebSocketFeed {
    _config: FeedConfig,
    auth: BinanceAuth,
    market: BinanceMarket,
    testnet: bool,
    symbols: Vec<Symbol>,
    symbol_map: FxHashMap<String, Symbol>,
    symbol_names: FxHashMap<Symbol, String>,
    order_books: FxHashMap<Symbol, OrderBookManager>,
}

impl BinanceWebSocketFeed {
    /// Create new Binance WebSocket feed
    pub fn new(
        config: FeedConfig,
        auth: BinanceAuth,
        market: BinanceMarket,
        testnet: bool,
    ) -> Self {
        let mut symbol_map = FxHashMap::with_capacity_and_hasher(1000, FxBuildHasher);
        let mut symbol_names = FxHashMap::with_capacity_and_hasher(1000, FxBuildHasher);

        for (symbol, name) in &config.symbol_map {
            symbol_map.insert(name.to_lowercase(), *symbol);
            symbol_names.insert(*symbol, name.to_lowercase());
        }

        Self {
            _config: config,
            auth,
            market,
            testnet,
            symbols: Vec::with_capacity(1000),
            symbol_map,
            symbol_names,
            order_books: FxHashMap::with_capacity_and_hasher(1000, FxBuildHasher),
        }
    }

    /// Get WebSocket URL for market and testnet setting
    fn get_ws_url(&self) -> String {
        if self.testnet {
            match self.market {
                BinanceMarket::Spot => "wss://testnet.binance.vision/ws".to_string(),
                BinanceMarket::UsdFutures => "wss://stream.binancefuture.com/ws".to_string(),
                BinanceMarket::CoinFutures => "wss://dstream.binancefuture.com/ws".to_string(),
            }
        } else {
            self.market.ws_url(false).to_string()
        }
    }

    /// Get REST API base URL
    fn get_api_url(&self) -> String {
        if self.testnet {
            match self.market {
                BinanceMarket::Spot => "https://testnet.binance.vision".to_string(),
                BinanceMarket::UsdFutures => "https://testnet.binancefuture.com".to_string(),
                BinanceMarket::CoinFutures => "https://testnet.binancefuture.com".to_string(),
            }
        } else {
            self.market.api_url(false).to_string()
        }
    }

    /// Fetch depth snapshot for a symbol
    async fn fetch_snapshot(&self, symbol_name: &str) -> anyhow::Result<DepthSnapshot> {
        let client = reqwest::Client::new();
        let url = format!("{}/api/v3/depth", self.get_api_url());

        let response = client
            .get(&url)
            .query(&[
                ("symbol", symbol_name.to_uppercase()),
                ("limit", "20".to_string()),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Failed to fetch snapshot: {}", error_text));
        }

        let snapshot = response.json::<DepthSnapshot>().await?;
        Ok(snapshot)
    }
}

#[async_trait::async_trait]
impl FeedAdapter for BinanceWebSocketFeed {
    async fn disconnect(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn connect(&mut self) -> anyhow::Result<()> {
        let network = if self.testnet { "TESTNET" } else { "MAINNET" };
        info!("Connecting to Binance {} {:?} feed", network, self.market);

        // Validate credentials if available
        if self.auth.has_market(self.market) {
            match self.auth.validate_credentials(self.market).await {
                Ok(true) => info!("✓ Binance credentials validated"),
                Ok(false) => warn!("✗ Binance credentials invalid - will use public streams only"),
                Err(e) => warn!("Failed to validate credentials: {}", e),
            }
        }

        Ok(())
    }

    async fn subscribe(&mut self, symbols: Vec<Symbol>) -> anyhow::Result<()> {
        self.symbols = symbols.clone();

        // Initialize order book managers
        for symbol in &symbols {
            self.order_books
                .insert(*symbol, OrderBookManager::new(*symbol));
        }

        info!("Subscribed to {} symbols on Binance", symbols.len());
        Ok(())
    }

    async fn run(&mut self, tx: mpsc::Sender<L2Update>) -> anyhow::Result<()> {
        let ws_url = self.get_ws_url();

        // Build combined stream URL
        let mut streams = Vec::with_capacity(self.symbols.len());
        for symbol in &self.symbols {
            if let Some(name) = self.symbol_names.get(symbol) {
                // Subscribe to depth and trade streams
                streams.push(format!("{}@depth@100ms", name));
                streams.push(format!("{}@trade", name));
            }
        }

        if streams.is_empty() {
            warn!("No streams to subscribe to");
            return Ok(());
        }

        let combined_url = format!("{}/stream?streams={}", ws_url, streams.join("/"));
        info!("Connecting to: {}", combined_url);

        // Connect to WebSocket
        let (ws_stream, _) = connect_async(&combined_url).await?;
        let (mut write, mut read) = ws_stream.split();

        info!("Connected to Binance WebSocket");

        // Fetch initial snapshots
        for symbol in &self.symbols {
            if let Some(name) = self.symbol_names.get(symbol) {
                match self.fetch_snapshot(name).await {
                    Ok(snapshot) => {
                        if let Some(book) = self.order_books.get_mut(symbol) {
                            book.apply_snapshot(snapshot);
                            info!("Fetched snapshot for {}", name);
                        }
                    }
                    Err(e) => {
                        error!("Failed to fetch snapshot for {}: {}", name, e);
                    }
                }
            }
        }

        // Process incoming messages
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(stream_msg) = serde_json::from_str::<StreamMessage>(&text) {
                        // Parse stream name to determine message type
                        if stream_msg.stream.contains("@depth") {
                            if let Ok(depth) =
                                serde_json::from_value::<DepthUpdate>(stream_msg.data)
                            {
                                if let Some(symbol) =
                                    self.symbol_map.get(&depth.symbol.to_lowercase())
                                {
                                    if let Some(book) = self.order_books.get_mut(symbol) {
                                        let updates = book.apply_update(&depth);
                                        for update in updates {
                                            if tx.send(update).await.is_err() {
                                                warn!("Channel closed");
                                                return Ok(());
                                            }
                                        }
                                    }
                                }
                            }
                        } else if stream_msg.stream.contains("@trade") {
                            // Could process trades here if needed
                            debug!("Trade update received");
                        }
                    }
                }
                Ok(Message::Ping(data)) => {
                    write.send(Message::Pong(data)).await?;
                }
                Ok(Message::Close(_)) => {
                    info!("WebSocket closed by server");
                    break;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }
}
