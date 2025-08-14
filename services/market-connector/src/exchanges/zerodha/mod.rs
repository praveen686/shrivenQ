//! Zerodha KiteConnect feed adapter

pub mod config;
pub mod market_data_pipeline;
pub mod websocket;

pub use config::ZerodhaConfig;
pub use market_data_pipeline::{MarketDataPipeline, PipelineConfig};

use crate::connectors::adapter::{FeedAdapter, FeedConfig};
use auth::ZerodhaAuth;
use common::{L2Update, Px, Qty, Side, Symbol, Ts};
use futures_util::{SinkExt, StreamExt};
use rustc_hash::FxHashMap;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

/// Zerodha WebSocket feed
pub struct ZerodhaFeed {
    config: FeedConfig,
    auth: ZerodhaAuth,
    symbols: Vec<Symbol>,
    #[allow(dead_code)] // Reserved for potential symbol remapping functionality
    symbol_map: FxHashMap<String, Symbol>,
}

impl ZerodhaFeed {
    /// Create new Zerodha feed
    pub fn new(config: FeedConfig, auth: ZerodhaAuth) -> Self {
        let symbol_map = config
            .symbol_map
            .iter()
            .map(|(k, v)| (v.clone(), *k))
            .collect();

        Self {
            config,
            auth,
            symbols: Vec::with_capacity(1000),
            symbol_map,
        }
    }

    /// Parse Zerodha market depth message from JSON (reserved for text message support)
    #[allow(dead_code)] // Reserved for potential JSON text message parsing
    fn parse_depth(&self, msg: &ZerodhaDepth) -> Vec<L2Update> {
        let symbol = match self.symbol_map.get(&msg.token) {
            Some(s) => *s,
            None => {
                warn!("Unknown token: {}", msg.token);
                return Vec::with_capacity(0); // Empty case
            }
        };

        let ts = Ts::from_nanos(msg.timestamp * 1_000_000);
        let mut updates = Vec::with_capacity(20);

        // Process bid levels
        for (i, level) in msg.depth.buy.iter().enumerate() {
            if level.quantity > 0.0 {
                updates.push(L2Update::new(ts, symbol).with_level_data(
                    Side::Bid,
                    Px::new(level.price),
                    Qty::new(level.quantity),
                    u8::try_from(i).unwrap_or(255),
                ));
            }
        }

        // Process ask levels
        for (i, level) in msg.depth.sell.iter().enumerate() {
            if level.quantity > 0.0 {
                updates.push(L2Update::new(ts, symbol).with_level_data(
                    Side::Ask,
                    Px::new(level.price),
                    Qty::new(level.quantity),
                    u8::try_from(i).unwrap_or(255),
                ));
            }
        }

        updates
    }
}

#[async_trait::async_trait]
impl FeedAdapter for ZerodhaFeed {
    async fn connect(&mut self) -> anyhow::Result<()> {
        info!("Connecting to Zerodha feed");
        // Connection will be established in run()
        Ok(())
    }

    async fn subscribe(&mut self, symbols: Vec<Symbol>) -> anyhow::Result<()> {
        self.symbols = symbols;
        info!("Subscribed to {} symbols", self.symbols.len());
        Ok(())
    }

    async fn run(&mut self, tx: mpsc::Sender<L2Update>) -> anyhow::Result<()> {
        let token = self.auth.authenticate().await?;
        let api_key = self.auth.get_api_key();

        // Debug: Check token and API key format
        info!(
            "API key length: {}, starts with: {}",
            api_key.len(),
            &api_key[..std::cmp::min(4, api_key.len())]
        );
        info!(
            "Access token length: {}, starts with: {}",
            token.len(),
            &token[..std::cmp::min(8, token.len())]
        );

        // Create WebSocket URL (no URL encoding needed based on reference)
        let ws_url = format!(
            "{}?api_key={}&access_token={}",
            self.config.ws_url, api_key, token
        );

        let masked_url = format!(
            "{}?api_key={}...&access_token=***MASKED***",
            self.config.ws_url,
            &api_key[..std::cmp::min(8, api_key.len())]
        );
        info!("Connecting to Zerodha WebSocket: {}", masked_url);

        // Validate WebSocket URL
        let url = url::Url::parse(&ws_url)
            .map_err(|e| anyhow::anyhow!("Invalid WebSocket URL: {}", e))?;

        let (ws_stream, _response) = match connect_async(url).await {
            Ok((stream, resp)) => {
                info!(
                    "WebSocket connection established, response status: {:?}",
                    resp.status()
                );
                (stream, resp)
            }
            Err(e) => {
                error!("WebSocket connection failed: {:?}", e);
                return Err(anyhow::anyhow!("WebSocket connection failed: {}", e));
            }
        };
        let (mut write, mut read) = ws_stream.split();

        // Subscribe to market depth - Zerodha expects numeric tokens, not strings
        let tokens: Vec<u32> = self
            .symbols
            .iter()
            .filter_map(|s| {
                self.config
                    .symbol_map
                    .get(s)
                    .and_then(|token_str| token_str.parse::<u32>().ok())
            })
            .collect();

        info!("Subscribing to numeric tokens: {:?}", tokens);
        info!(
            "Expected tokens: NIFTY_SEP_FUT=13568258, BANKNIFTY_AUG_FUT=16409602, NIFTY_SPOT=256265"
        );

        let subscribe_msg = serde_json::json!({
            "a": "subscribe",
            "v": tokens
        });

        info!("Sending subscribe message: {}", subscribe_msg.to_string());
        write.send(Message::Text(subscribe_msg.to_string())).await?;

        // Set mode to full (includes depth)
        let mode_msg = serde_json::json!({
            "a": "mode",
            "v": ["full", tokens]
        });

        info!("Sending mode message: {}", mode_msg.to_string());
        write.send(Message::Text(mode_msg.to_string())).await?;

        info!("Zerodha feed connected and subscribed");

        // Process messages - Zerodha sends binary data primarily
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    debug!("Received text message: {}", text);
                    // Try to parse as JSON for control messages
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        debug!("Parsed JSON control message: {}", json);
                    }
                }
                Ok(Message::Binary(data)) => {
                    // Zerodha sends market data in binary format
                    debug!("Received binary message: {} bytes", data.len());

                    // Parse binary tick data and convert to L2Update
                    if let Ok(l2_updates) = Self::parse_binary_ticks(&data) {
                        for update in l2_updates {
                            if tx.send(update).await.is_err() {
                                error!("Failed to send L2Update");
                                return Ok(());
                            }
                        }
                    }
                }
                Ok(Message::Ping(data)) => {
                    debug!("Received ping: {} bytes", data.len());
                    // Respond to ping
                    if write.send(Message::Pong(data)).await.is_err() {
                        error!("Failed to send pong");
                        break;
                    }
                }
                Ok(Message::Pong(_)) => {
                    debug!("Received pong");
                }
                Ok(Message::Close(frame)) => {
                    info!("WebSocket closed: {:?}", frame);
                    break;
                }
                Ok(Message::Frame(_)) => {
                    // Raw frame, ignore
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    async fn disconnect(&mut self) -> anyhow::Result<()> {
        info!("Disconnecting Zerodha feed");
        Ok(())
    }
}

impl ZerodhaFeed {
    /// Parse Zerodha's binary tick format into L2Updates
    /// CORRECT Binary format based on reference implementation:
    /// Each message is ONE tick (no packet count header)
    /// LTP mode (44 bytes):
    /// - 0-3: instrument_token (i32, big-endian)  
    /// - 4-7: last_price (i32, big-endian, divide by 100)
    /// - 8-11: last_quantity (i32, big-endian)
    /// - 12-15: average_price (i32, big-endian, divide by 100)
    /// - 16-19: volume (i32, big-endian)
    /// - 20-23: buy_quantity (i32, big-endian)
    /// - 24-27: sell_quantity (i32, big-endian)  
    /// - 28-31: open (i32, big-endian, divide by 100)
    /// - 32-35: high (i32, big-endian, divide by 100)
    /// - 36-39: low (i32, big-endian, divide by 100)
    /// - 40-43: close (i32, big-endian, divide by 100)
    fn parse_binary_ticks(data: &[u8]) -> anyhow::Result<Vec<L2Update>> {
        let mut updates = Vec::new();

        // Handle multiple ticks in one message - Zerodha can send concatenated packets
        let mut offset = 0;

        while offset < data.len() {
            // Determine packet size at current offset
            let remaining = data.len() - offset;

            let packet_size = if remaining >= 164 {
                // Could be full mode (164 bytes)
                164
            } else if remaining >= 44 {
                // Could be quote mode (44 bytes)
                44
            } else if remaining >= 8 {
                // Could be LTP mode (8 bytes)
                8
            } else {
                if remaining == 1 {
                    debug!("Received heartbeat message");
                } else {
                    warn!("Unknown packet fragment: {} bytes", remaining);
                }
                break;
            };

            // Validate we have enough data for this packet
            if offset + packet_size > data.len() {
                warn!(
                    "Incomplete packet: need {} bytes, have {} at offset {}",
                    packet_size, remaining, offset
                );
                break;
            }

            let packet_data = &data[offset..offset + packet_size];

            // Parse this individual packet
            if let Ok(mut packet_updates) = Self::parse_single_tick(packet_data) {
                updates.append(&mut packet_updates);
            }

            offset += packet_size;
        }

        Ok(updates)
    }

    /// Parse a single tick packet based on OFFICIAL Zerodha documentation
    /// 8 bytes: LTP mode (Last Traded Price only)
    /// 44 bytes: Quote mode (OHLC + volume, no depth)
    /// 164 bytes: Full mode (OHLC + volume + market depth)
    fn parse_single_tick(data: &[u8]) -> anyhow::Result<Vec<L2Update>> {
        let mut updates = Vec::new();

        if data.len() < 8 {
            return Err(anyhow::anyhow!("Packet too small: {} bytes", data.len()));
        }

        // Parse common fields (available in all modes)
        // SAFETY: Instrument tokens from Zerodha are always positive
        let instrument_token_i32 = i32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let instrument_token = u32::try_from(instrument_token_i32)
            .map_err(|_| anyhow::anyhow!("Invalid instrument token: {}", instrument_token_i32))?;

        let symbol = Symbol::new(instrument_token);
        let ts = Ts::now();

        match data.len() {
            8 => {
                // LTP mode: only last traded price
                // Protocol conversion: Zerodha sends prices as i32 (paise), convert to f64 (rupees)
                #[allow(clippy::cast_precision_loss)] // i32 fits in f64 without precision loss
                let last_price =
                    i32::from_be_bytes([data[4], data[5], data[6], data[7]]) as f64 / 100.0;

                let token_name = match instrument_token {
                    13568258 => "NIFTY_SEP_FUT",
                    16409602 => "BANKNIFTY_AUG_FUT",
                    256265 => "NIFTY_SPOT",
                    _ => "UNKNOWN",
                };

                info!(
                    "📊 LTP tick for token {} ({}): price={:.2}",
                    instrument_token, token_name, last_price
                );

                if last_price > 0.0 {
                    // Create a simple L2Update with last price (no bid/ask in LTP mode)
                    let price_px = Px::new(last_price);
                    let qty = Qty::new(1.0); // Minimal quantity since we don't have real bid/ask

                    let update =
                        L2Update::new(ts, symbol).with_level_data(Side::Bid, price_px, qty, 0);
                    updates.push(update);
                }
            }

            44 => {
                // Quote mode: OHLC + volume but no market depth
                // Protocol conversion: Zerodha sends prices as i32 (paise), convert to f64 (rupees)
                #[allow(clippy::cast_precision_loss)] // i32 fits in f64 without precision loss
                let last_price =
                    i32::from_be_bytes([data[4], data[5], data[6], data[7]]) as f64 / 100.0;
                // SAFETY: Quantities are always non-negative in exchange data
                let last_quantity = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
                #[allow(clippy::cast_precision_loss)] // Protocol conversion from paise to rupees
                let average_price =
                    i32::from_be_bytes([data[12], data[13], data[14], data[15]]) as f64 / 100.0;
                // SAFETY: Volumes are always non-negative
                let volume = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
                let buy_quantity = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
                let sell_quantity = u32::from_be_bytes([data[24], data[25], data[26], data[27]]);
                #[allow(clippy::cast_precision_loss)] // Protocol OHLC conversion
                let open =
                    i32::from_be_bytes([data[28], data[29], data[30], data[31]]) as f64 / 100.0;
                #[allow(clippy::cast_precision_loss)] // Protocol OHLC conversion
                let high =
                    i32::from_be_bytes([data[32], data[33], data[34], data[35]]) as f64 / 100.0;
                #[allow(clippy::cast_precision_loss)] // Protocol OHLC conversion
                let low =
                    i32::from_be_bytes([data[36], data[37], data[38], data[39]]) as f64 / 100.0;
                #[allow(clippy::cast_precision_loss)] // Protocol OHLC conversion
                let close =
                    i32::from_be_bytes([data[40], data[41], data[42], data[43]]) as f64 / 100.0;

                info!(
                    "📈 Quote tick for token {}: OHLC({:.2}/{:.2}/{:.2}/{:.2}) LTP={:.2} Vol={} AvgPx={:.2} LastQty={}",
                    instrument_token,
                    open,
                    high,
                    low,
                    close,
                    last_price,
                    volume,
                    average_price,
                    last_quantity
                );

                // Create synthetic bid/ask from buy/sell quantities
                if last_price > 0.0 {
                    let price_px = Px::new(last_price);

                    if buy_quantity > 0 {
                        #[allow(clippy::cast_precision_loss)] // u32 quantity to f64
                        let buy_qty = Qty::new(buy_quantity as f64);
                        let bid_update = L2Update::new(ts, symbol).with_level_data(
                            Side::Bid,
                            price_px,
                            buy_qty,
                            0,
                        );
                        updates.push(bid_update);
                    }

                    if sell_quantity > 0 {
                        #[allow(clippy::cast_precision_loss)] // u32 quantity to f64
                        let sell_qty = Qty::new(sell_quantity as f64);
                        let ask_update = L2Update::new(ts, symbol).with_level_data(
                            Side::Ask,
                            price_px,
                            sell_qty,
                            0,
                        );
                        updates.push(ask_update);
                    }
                }
            }

            164 => {
                // Full mode: includes market depth (5 bid + 5 ask levels)
                // Protocol conversion: Zerodha sends prices as i32 (paise), convert to f64 (rupees)
                #[allow(clippy::cast_precision_loss)] // i32 fits in f64 without precision loss
                let last_price =
                    i32::from_be_bytes([data[4], data[5], data[6], data[7]]) as f64 / 100.0;
                // SAFETY: Quantities are always non-negative in exchange data
                let last_quantity = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
                #[allow(clippy::cast_precision_loss)] // Protocol conversion from paise to rupees
                let average_price =
                    i32::from_be_bytes([data[12], data[13], data[14], data[15]]) as f64 / 100.0;
                // SAFETY: Volumes are always non-negative
                let volume = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
                let buy_quantity = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
                let sell_quantity = u32::from_be_bytes([data[24], data[25], data[26], data[27]]);
                #[allow(clippy::cast_precision_loss)] // Protocol OHLC conversion
                let open =
                    i32::from_be_bytes([data[28], data[29], data[30], data[31]]) as f64 / 100.0;
                #[allow(clippy::cast_precision_loss)] // Protocol OHLC conversion
                let high =
                    i32::from_be_bytes([data[32], data[33], data[34], data[35]]) as f64 / 100.0;
                #[allow(clippy::cast_precision_loss)] // Protocol OHLC conversion
                let low =
                    i32::from_be_bytes([data[36], data[37], data[38], data[39]]) as f64 / 100.0;
                #[allow(clippy::cast_precision_loss)] // Protocol OHLC conversion
                let close =
                    i32::from_be_bytes([data[40], data[41], data[42], data[43]]) as f64 / 100.0;

                info!(
                    "🔥 Full tick for token {}: OHLC({:.2}/{:.2}/{:.2}/{:.2}) LTP={:.2} Vol={} AvgPx={:.2} BuyQty={} SellQty={} LastQty={} +depth",
                    instrument_token,
                    open,
                    high,
                    low,
                    close,
                    last_price,
                    volume,
                    average_price,
                    buy_quantity,
                    sell_quantity,
                    last_quantity
                );

                // Parse market depth (starts at byte 44)
                let mut depth_offset = 44;

                // Parse 5 bid levels (each 12 bytes: quantity(4) + price(4) + orders(2) + padding(2))
                for level in 0..5 {
                    if depth_offset + 12 <= data.len() {
                        let quantity = u32::from_be_bytes([
                            data[depth_offset],
                            data[depth_offset + 1],
                            data[depth_offset + 2],
                            data[depth_offset + 3],
                        ]);
                        let price = i32::from_be_bytes([
                            data[depth_offset + 4],
                            data[depth_offset + 5],
                            data[depth_offset + 6],
                            data[depth_offset + 7],
                        ]) as f64
                            / 100.0;
                        let orders =
                            u16::from_be_bytes([data[depth_offset + 8], data[depth_offset + 9]]);
                        // Skip 2 bytes padding

                        if price > 0.0 && quantity > 0 {
                            let bid_px = Px::new(price);
                            let bid_qty = Qty::new(quantity as f64);
                            let bid_update = L2Update::new(ts, symbol).with_level_data(
                                Side::Bid,
                                bid_px,
                                bid_qty,
                                level as u8,
                            );
                            // Log order count for bid level
                            debug!("Bid level {}: {} orders", level, orders);
                            updates.push(bid_update);
                        }

                        depth_offset += 12;
                    }
                }

                // Parse 5 ask levels (each 12 bytes)
                for level in 0..5 {
                    if depth_offset + 12 <= data.len() {
                        let quantity = u32::from_be_bytes([
                            data[depth_offset],
                            data[depth_offset + 1],
                            data[depth_offset + 2],
                            data[depth_offset + 3],
                        ]);
                        let price = i32::from_be_bytes([
                            data[depth_offset + 4],
                            data[depth_offset + 5],
                            data[depth_offset + 6],
                            data[depth_offset + 7],
                        ]) as f64
                            / 100.0;
                        let orders =
                            u16::from_be_bytes([data[depth_offset + 8], data[depth_offset + 9]]);
                        // Skip 2 bytes padding

                        if price > 0.0 && quantity > 0 {
                            let ask_px = Px::new(price);
                            let ask_qty = Qty::new(quantity as f64);
                            let ask_update = L2Update::new(ts, symbol).with_level_data(
                                Side::Ask,
                                ask_px,
                                ask_qty,
                                level as u8,
                            );
                            // Log order count for ask level
                            debug!("Ask level {}: {} orders", level, orders);
                            updates.push(ask_update);
                        }

                        depth_offset += 12;
                    }
                }
            }

            _ => {
                warn!("Unknown tick packet size: {} bytes", data.len());
            }
        }

        Ok(updates)
    }
}

// Zerodha message types - Used for JSON text message parsing
// These are kept for potential future JSON message support alongside binary parsing

#[allow(dead_code)] // JSON message types - reserved for future text message support
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ZerodhaMessage {
    Depth(ZerodhaDepth),
}

#[allow(dead_code)] // JSON message types - reserved for future text message support
#[derive(Debug, Deserialize)]
struct ZerodhaDepth {
    token: String,
    timestamp: u64,
    depth: Depth,
}

#[allow(dead_code)] // JSON message types - reserved for future text message support
#[derive(Debug, Deserialize)]
struct Depth {
    buy: Vec<DepthLevel>,
    sell: Vec<DepthLevel>,
}

#[allow(dead_code)] // JSON message types - reserved for future text message support
#[derive(Debug, Deserialize)]
struct DepthLevel {
    price: f64,
    quantity: f64,
}
