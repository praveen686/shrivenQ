//! Zerodha WebSocket implementation with synthetic/real data switching

use crate::common::adapter::{FeedAdapter, FeedConfig};
use auth::ZerodhaAuth;
use common::{L2Update, Px, Qty, Side, Symbol, Ts};
use futures_util::{SinkExt, StreamExt};
use rustc_hash::{FxBuildHasher, FxHashMap};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};
use url::Url;

/// Zerodha WebSocket message types
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum KiteMessage {
    #[serde(rename = "order")]
    Order(OrderUpdate),
    #[serde(rename = "quote")]
    Quote(QuoteUpdate),
    #[serde(rename = "message")]
    Message { data: String },
}

#[derive(Debug, Deserialize)]
pub struct OrderUpdate {
    pub data: OrderData,
}

#[derive(Debug, Deserialize)]
pub struct OrderData {
    pub instrument_token: u32,
    pub timestamp: i64,
    pub last_price: f64,
    pub depth: Depth,
}

#[derive(Debug, Deserialize)]
pub struct Depth {
    pub buy: Vec<DepthLevel>,
    pub sell: Vec<DepthLevel>,
}

#[derive(Debug, Deserialize)]
pub struct DepthLevel {
    pub price: f64,
    pub quantity: u32,
    pub orders: u32,
}

#[derive(Debug, Deserialize)]
pub struct QuoteUpdate {
    pub data: FxHashMap<String, QuoteData>,
}

#[derive(Debug, Deserialize)]
pub struct QuoteData {
    pub instrument_token: u32,
    pub timestamp: String,
    pub last_price: f64,
    pub volume: u32,
    pub buy_quantity: u32,
    pub sell_quantity: u32,
    pub ohlc: OHLC,
}

#[derive(Debug, Deserialize)]
pub struct OHLC {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

/// Subscription message for Kite
#[derive(Debug, Serialize)]
pub struct KiteSubscribe {
    pub a: String,   // action: "subscribe"
    pub v: Vec<u32>, // instrument tokens
}

/// Mode change message
#[derive(Debug, Serialize)]
pub struct KiteModeChange {
    pub a: String,             // action: "mode"
    pub v: Vec<(String, u32)>, // [(mode, token)]
}

/// Zerodha WebSocket feed
pub struct ZerodhaWebSocketFeed {
    config: FeedConfig,
    auth: ZerodhaAuth,
    symbols: Vec<Symbol>,
    symbol_map: FxHashMap<u32, Symbol>, // token -> Symbol
    token_map: FxHashMap<Symbol, u32>,  // Symbol -> token
}

impl ZerodhaWebSocketFeed {
    /// Create new Zerodha WebSocket feed
    pub fn new(config: FeedConfig, auth: ZerodhaAuth) -> Self {
        // Create bidirectional mappings
        let mut symbol_map = FxHashMap::with_capacity_and_hasher(1000, FxBuildHasher);
        let mut token_map = FxHashMap::with_capacity_and_hasher(1000, FxBuildHasher);

        // Assuming symbol_map in config is Symbol -> instrument_token (as string)
        for (symbol, token_str) in &config.symbol_map {
            if let Ok(token) = token_str.parse::<u32>() {
                symbol_map.insert(token, *symbol);
                token_map.insert(*symbol, token);
            }
        }

        Self {
            config,
            auth,
            symbols: Vec::with_capacity(1000),
            symbol_map,
            token_map,
        }
    }

    /// Get config
    pub fn config(&self) -> &FeedConfig {
        &self.config
    }

    /// Run WebSocket feed
    async fn run_websocket(&mut self, tx: mpsc::Sender<L2Update>) -> anyhow::Result<()> {
        info!("Starting WebSocket feed for {} symbols", self.symbols.len());

        // Authenticate and get access token
        let access_token = self.auth.authenticate().await?;

        // Get API key from auth config
        let api_key = self.auth.get_api_key();

        info!("Using API key: {}", api_key);
        info!("Access token length: {}", access_token.len());

        // Construct WebSocket URL (no URL encoding needed)
        let ws_url = format!(
            "wss://ws.kite.trade?api_key={}&access_token={}",
            api_key.trim(),
            access_token.trim()
        );

        info!(
            "Connecting to WebSocket URL: {}...{}",
            &ws_url[..50],
            &ws_url[ws_url.len() - 20..]
        );

        debug!("Full URL: {}", ws_url);
        debug!(
            "API Key length: {}, Access token length: {}",
            api_key.len(),
            access_token.len()
        );

        // Parse URL properly
        let url = match Url::parse(&ws_url) {
            Ok(u) => u,
            Err(e) => {
                error!("Invalid WebSocket URL: {}", e);
                return Err(anyhow::anyhow!("Invalid WebSocket URL: {}", e));
            }
        };

        let (ws_stream, _response) = match connect_async(url).await {
            Ok((stream, resp)) => {
                info!("WebSocket connected successfully!");
                info!("Response status: {}", resp.status());
                debug!("Response headers: {:?}", resp.headers());
                (stream, resp)
            }
            Err(e) => {
                error!("WebSocket connection failed: {}", e);
                if let tokio_tungstenite::tungstenite::Error::Http(response) = &e {
                    error!("HTTP Status: {}", response.status());
                    error!("HTTP Body: {:?}", response.body());
                }
                return Err(anyhow::anyhow!("WebSocket connection failed: {}", e));
            }
        };
        let (mut write, mut read) = ws_stream.split();

        info!("Connected to Zerodha WebSocket");

        // Subscribe to instruments
        let tokens: Vec<u32> = self
            .symbols
            .iter()
            .filter_map(|s| self.token_map.get(s))
            .copied()
            .collect();

        if !tokens.is_empty() {
            // Subscribe message
            let subscribe_msg = KiteSubscribe {
                a: "subscribe".to_string(),
                v: tokens.clone(),
            };

            let msg_text = serde_json::to_string(&subscribe_msg)?;
            write.send(Message::Text(msg_text)).await?;

            // Set mode to "full" for order book data
            // Kite expects: {"a": "mode", "v": ["full", [token1, token2, ...]]}
            let mode_msg = serde_json::json!({
                "a": "mode",
                "v": ["full", tokens]
            });

            let mode_text = mode_msg.to_string();
            write.send(Message::Text(mode_text)).await?;

            info!("Subscribed to {} instruments in full mode", tokens.len());
        }

        // Process incoming messages
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    debug!("Received text message: {}", text);
                    if let Ok(kite_msg) = serde_json::from_str::<KiteMessage>(&text) {
                        match kite_msg {
                            KiteMessage::Order(order) => {
                                let updates = self.parse_order_update(order);
                                for update in updates {
                                    if tx.send(update).await.is_err() {
                                        warn!("Channel closed");
                                        return Ok(());
                                    }
                                }
                            }
                            KiteMessage::Quote(_quote) => {
                                debug!("Received quote update");
                            }
                            KiteMessage::Message { data } => {
                                info!("Server message: {}", data);
                            }
                        }
                    }
                }
                Ok(Message::Binary(data)) => {
                    // Check for heartbeat (1 byte)
                    if data.len() == 1 {
                        debug!("Received heartbeat");
                        // Send pong response
                        if write.send(Message::Pong(vec![])).await.is_err() {
                            error!("Failed to send pong");
                            break;
                        }
                        continue;
                    }

                    // Kite sends binary data for ticks
                    let updates = self.parse_binary_data(&data)?;
                    for update in updates {
                        if tx.send(update).await.is_err() {
                            warn!("Channel closed");
                            return Ok(());
                        }
                    }
                }
                Ok(Message::Ping(data)) => {
                    debug!("Received ping, sending pong");
                    if write.send(Message::Pong(data)).await.is_err() {
                        error!("Failed to send pong");
                        break;
                    }
                }
                Ok(Message::Pong(_)) => {
                    debug!("Received pong");
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

    /// Parse order update into L2Updates
    fn parse_order_update(&self, order: OrderUpdate) -> Vec<L2Update> {
        let mut updates = Vec::with_capacity(20); // Typical LOB depth

        if let Some(symbol) = self.symbol_map.get(&order.data.instrument_token) {
            // timestamp is i64 milliseconds, convert to nanoseconds
            let ts = if order.data.timestamp >= 0 {
                Ts::from_nanos(u64::try_from(order.data.timestamp).unwrap_or(0) * 1_000_000)
            } else {
                Ts::from_nanos(0) // Invalid negative timestamp
            };

            // Parse bid levels
            for (i, level) in order.data.depth.buy.iter().enumerate() {
                updates.push(L2Update::new(ts, *symbol).with_level_data(
                    Side::Bid,
                    Px::new(level.price),
                    Qty::from_units(i64::from(level.quantity)),
                    u8::try_from(i).unwrap_or(255),
                ));
            }

            // Parse ask levels
            for (i, level) in order.data.depth.sell.iter().enumerate() {
                updates.push(L2Update::new(ts, *symbol).with_level_data(
                    Side::Ask,
                    Px::new(level.price),
                    Qty::from_units(i64::from(level.quantity)),
                    u8::try_from(i).unwrap_or(255),
                ));
            }
        }

        updates
    }

    /// Parse binary tick data (Kite's compact format)
    fn parse_binary_data(&self, data: &[u8]) -> anyhow::Result<Vec<L2Update>> {
        let mut updates = Vec::with_capacity(20); // Typical LOB depth

        if data.len() < 2 {
            return Ok(updates);
        }

        // First 2 bytes = number of packets
        let num_packets = usize::from(u16::from_be_bytes([data[0], data[1]]));
        let mut offset = 2;

        for _ in 0..num_packets {
            if offset + 2 > data.len() {
                break;
            }

            // Next 2 bytes = packet length
            let packet_len = usize::from(u16::from_be_bytes([data[offset], data[offset + 1]]));
            offset += 2;

            if offset + packet_len > data.len() {
                break;
            }

            // Parse packet based on length
            let packet = &data[offset..offset + packet_len];

            // Different packet types based on size:
            // 8 bytes = LTP mode
            // 28 bytes = Indices quote
            // 32 bytes = Indices full
            // 44 bytes = Quote mode
            // 184 bytes = Full mode (with depth)

            if packet_len >= 4 {
                // First 4 bytes = instrument token
                let token = u32::from_be_bytes([packet[0], packet[1], packet[2], packet[3]]);

                if let Some(symbol) = self.symbol_map.get(&token) {
                    // For full mode (184 bytes), parse market depth
                    if packet_len == 184 {
                        let ts = Ts::now();

                        // Skip to market depth section (after basic quote data)
                        // Depth starts at byte 44
                        let depth_offset = 44;

                        // Parse 5 bid levels (each level = 12 bytes)
                        for i in 0..5 {
                            let level_offset = depth_offset + i * 12;
                            if level_offset + 12 <= packet.len() {
                                let qty = u32::from_be_bytes([
                                    packet[level_offset],
                                    packet[level_offset + 1],
                                    packet[level_offset + 2],
                                    packet[level_offset + 3],
                                ]);
                                let price = u32::from_be_bytes([
                                    packet[level_offset + 4],
                                    packet[level_offset + 5],
                                    packet[level_offset + 6],
                                    packet[level_offset + 7],
                                ]);

                                if price > 0 && qty > 0 {
                                    // Safe conversion: u32 to i32 checked
                                    if let (Ok(price_i32), Ok(qty_i32)) =
                                        (i32::try_from(price), i32::try_from(qty))
                                    {
                                        updates.push(L2Update::new(ts, *symbol).with_level_data(
                                            Side::Bid,
                                            Px::from_price_i32(price_i32),
                                            Qty::from_qty_i32(qty_i32),
                                            u8::try_from(i).unwrap_or(255),
                                        ));
                                    }
                                }
                            }
                        }

                        // Parse 5 ask levels
                        let ask_offset = depth_offset + 60; // After 5 bid levels
                        for i in 0..5 {
                            let level_offset = ask_offset + i * 12;
                            if level_offset + 12 <= packet.len() {
                                let qty = u32::from_be_bytes([
                                    packet[level_offset],
                                    packet[level_offset + 1],
                                    packet[level_offset + 2],
                                    packet[level_offset + 3],
                                ]);
                                let price = u32::from_be_bytes([
                                    packet[level_offset + 4],
                                    packet[level_offset + 5],
                                    packet[level_offset + 6],
                                    packet[level_offset + 7],
                                ]);

                                if price > 0 && qty > 0 {
                                    // Safe conversion: u32 to i32 checked
                                    if let (Ok(price_i32), Ok(qty_i32)) =
                                        (i32::try_from(price), i32::try_from(qty))
                                    {
                                        updates.push(L2Update::new(ts, *symbol).with_level_data(
                                            Side::Ask,
                                            Px::from_price_i32(price_i32),
                                            Qty::from_qty_i32(qty_i32),
                                            u8::try_from(i).unwrap_or(255),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }

            offset += packet_len;
        }

        Ok(updates)
    }
}

#[async_trait::async_trait]
impl FeedAdapter for ZerodhaWebSocketFeed {
    async fn disconnect(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn connect(&mut self) -> anyhow::Result<()> {
        info!("Connecting to Zerodha WebSocket");
        Ok(())
    }

    async fn subscribe(&mut self, symbols: Vec<Symbol>) -> anyhow::Result<()> {
        self.symbols = symbols;
        info!("Subscribed to {} symbols", self.symbols.len());
        Ok(())
    }

    async fn run(&mut self, tx: mpsc::Sender<L2Update>) -> anyhow::Result<()> {
        self.run_websocket(tx).await
    }
}
