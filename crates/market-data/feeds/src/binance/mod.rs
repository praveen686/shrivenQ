//! Binance feed adapter

pub mod config;
pub mod websocket;

pub use config::BinanceConfig;

use crate::common::adapter::{FeedAdapter, FeedConfig};
use auth::{BinanceAuth, BinanceMarket};
use common::{L2Update, Px, Qty, Side, Symbol, Ts};
use futures_util::{SinkExt, StreamExt};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info, warn};

/// Binance WebSocket feed
pub struct BinanceFeed {
    config: FeedConfig,
    auth: BinanceAuth,
    market: BinanceMarket,
    symbols: Vec<Symbol>,
    symbol_map: FxHashMap<String, Symbol>,
    stream_id: Option<String>,
}

impl BinanceFeed {
    /// Create new Binance feed
    pub fn new(config: FeedConfig, auth: BinanceAuth, market: BinanceMarket) -> Self {
        let symbol_map = config
            .symbol_map
            .iter()
            .map(|(k, v)| (v.to_lowercase(), *k))
            .collect();

        Self {
            config,
            auth,
            market,
            symbols: Vec::with_capacity(1000),
            symbol_map,
            stream_id: None,
        }
    }

    /// Parse Binance depth update
    fn parse_depth_update(&self, msg: &BinanceDepthUpdate) -> Vec<L2Update> {
        let symbol = match self.symbol_map.get(&msg.s.to_lowercase()) {
            Some(s) => *s,
            None => {
                warn!("Unknown symbol: {}", msg.s);
                return Vec::with_capacity(0);
            }
        };

        let ts = Ts::from_nanos(msg.event_time * 1_000_000);
        let mut updates = Vec::with_capacity(20);

        // Process bid updates
        for (i, bid) in msg.b.iter().enumerate() {
            let price = bid[0].parse::<f64>().unwrap_or(0.0);
            let qty = bid[1].parse::<f64>().unwrap_or(0.0);

            updates.push(L2Update::new(ts, symbol).with_level_data(
                Side::Bid,
                Px::new(price),
                Qty::new(qty),
                // SAFETY: i is explicitly bounded by u8::MAX check before cast
                if i <= u8::MAX as usize {
                    i as u8
                } else {
                    continue;
                },
            ));
        }

        // Process ask updates
        for (i, ask) in msg.a.iter().enumerate() {
            let price = ask[0].parse::<f64>().unwrap_or(0.0);
            let qty = ask[1].parse::<f64>().unwrap_or(0.0);

            updates.push(L2Update::new(ts, symbol).with_level_data(
                Side::Ask,
                Px::new(price),
                // SAFETY: Cast is safe within expected range
                Qty::new(qty),
                // SAFETY: i is explicitly bounded by u8::MAX check before cast
                if i <= u8::MAX as usize {
                    i as u8
                } else {
                    continue;
                },
            ));
        }

        updates
    }

    /// Create listen key for user data stream
    async fn create_listen_key(&self) -> anyhow::Result<String> {
        let client = reqwest::Client::new();
        let url = format!("{}/api/v3/userDataStream", self.config.api_url);

        let timestamp = chrono::Utc::now().timestamp_millis();
        let params = format!("timestamp={}", timestamp);
        let signature = self.auth.sign_query(self.market, &params)?;

        let response = client
            .post(&url)
            .header("X-MBX-APIKEY", self.auth.get_api_key(self.market)?)
            .query(&[
                ("timestamp", timestamp.to_string()),
                ("signature", signature),
            ])
            .send()
            .await?;

        let data: ListenKeyResponse = response.json().await?;
        Ok(data.listen_key)
    }
}

#[async_trait::async_trait]
impl FeedAdapter for BinanceFeed {
    async fn connect(&mut self) -> anyhow::Result<()> {
        info!("Connecting to Binance feed");

        // For public data, we don't need a listen key
        // For user data, create listen key
        if self.config.ws_url.contains("userData") {
            self.stream_id = Some(self.create_listen_key().await?);
        }

        Ok(())
    }

    async fn subscribe(&mut self, symbols: Vec<Symbol>) -> anyhow::Result<()> {
        self.symbols = symbols;
        info!("Subscribed to {} symbols", self.symbols.len());
        Ok(())
    }

    async fn run(&mut self, tx: mpsc::Sender<L2Update>) -> anyhow::Result<()> {
        // Build stream names
        let streams: Vec<String> = self
            .symbols
            .iter()
            .filter_map(|s| self.config.symbol_map.get(s))
            .map(|s| format!("{}@depth20@100ms", s.to_lowercase()))
            .collect();

        let ws_url = if streams.is_empty() {
            self.config.ws_url.clone()
        } else {
            format!(
                "{}/stream?streams={}",
                self.config.ws_url,
                streams.join("/")
            )
        };

        let (ws_stream, _) = connect_async(&ws_url).await?;
        let (mut write, mut read) = ws_stream.split();

        // Subscribe to combined streams
        let subscribe_msg = BinanceSubscribe {
            method: "SUBSCRIBE".to_string(),
            params: streams,
            id: 1,
        };

        write
            .send(Message::Text(serde_json::to_string(&subscribe_msg)?))
            .await?;

        info!("Binance feed connected and subscribed");

        // Process messages
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    // Try parsing as depth update
                    if let Ok(depth_msg) = serde_json::from_str::<BinanceStreamMessage>(&text) {
                        if depth_msg.stream.contains("depth") {
                            if let Ok(depth) =
                                serde_json::from_value::<BinanceDepthUpdate>(depth_msg.data)
                            {
                                for update in self.parse_depth_update(&depth) {
                                    if tx.send(update).await.is_err() {
                                        error!("Failed to send update");
                                        return Ok(());
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("WebSocket closed");
                    break;
                }
                Ok(Message::Ping(data)) => {
                    write.send(Message::Pong(data)).await?;
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

    async fn disconnect(&mut self) -> anyhow::Result<()> {
        info!("Disconnecting Binance feed");

        // Delete listen key if we have one
        if let Some(key) = &self.stream_id {
            let client = reqwest::Client::new();
            let url = format!("{}/api/v3/userDataStream", self.config.api_url);

            let timestamp = chrono::Utc::now().timestamp_millis();
            let params = format!("listenKey={}&timestamp={}", key, timestamp);
            let signature = self.auth.sign_query(self.market, &params)?;

            if let Err(e) = client
                .delete(&url)
                .header("X-MBX-APIKEY", self.auth.get_api_key(self.market)?)
                .query(&[
                    ("listenKey", key.clone()),
                    ("timestamp", timestamp.to_string()),
                    ("signature", signature),
                ])
                .send()
                .await
            {
                // Log but don't fail disconnect - best effort cleanup
                warn!("Failed to delete listen key during disconnect: {}", e);
            }
        }

        Ok(())
    }
}

// Binance message types

#[derive(Debug, Serialize)]
struct BinanceSubscribe {
    method: String,
    params: Vec<String>,
    id: u64,
}

#[derive(Debug, Deserialize)]
struct BinanceStreamMessage {
    stream: String,
    data: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct BinanceDepthUpdate {
    #[serde(rename = "E")]
    event_time: u64,
    #[serde(rename = "s")]
    s: String, // Symbol
    #[serde(rename = "b")]
    b: Vec<Vec<String>>, // Bids [price, quantity]
    #[serde(rename = "a")]
    a: Vec<Vec<String>>, // Asks [price, quantity]
}

#[derive(Debug, Deserialize)]
struct ListenKeyResponse {
    #[serde(rename = "listenKey")]
    listen_key: String,
}
