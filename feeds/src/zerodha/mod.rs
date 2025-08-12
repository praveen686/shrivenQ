//! Zerodha KiteConnect feed adapter

pub mod websocket;
pub mod config;
pub mod market_data_pipeline;

pub use config::ZerodhaConfig;
pub use market_data_pipeline::{MarketDataPipeline, PipelineConfig};

use crate::common::adapter::{FeedAdapter, FeedConfig};
use auth::ZerodhaAuth;
use common::{L2Update, Px, Qty, Side, Symbol, Ts};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info, warn};

/// Zerodha WebSocket feed
pub struct ZerodhaFeed {
    config: FeedConfig,
    auth: ZerodhaAuth,
    symbols: Vec<Symbol>,
    symbol_map: HashMap<String, Symbol>,
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
            symbols: Vec::new(),
            symbol_map,
        }
    }
    
    /// Parse Zerodha market depth message
    fn parse_depth(&self, msg: &ZerodhaDepth) -> Vec<L2Update> {
        let symbol = match self.symbol_map.get(&msg.token) {
            Some(s) => *s,
            None => {
                warn!("Unknown token: {}", msg.token);
                return Vec::new();
            }
        };
        
        let ts = Ts::from_nanos(msg.timestamp * 1_000_000);
        let mut updates = Vec::new();
        
        // Process bid levels
        for (i, level) in msg.depth.buy.iter().enumerate() {
            if level.quantity > 0.0 {
                updates.push(L2Update::new(
                    ts,
                    symbol,
                    Side::Bid,
                    Px::new(level.price),
                    Qty::new(level.quantity as f64),
                    i as u8,
                ));
            }
        }
        
        // Process ask levels  
        for (i, level) in msg.depth.sell.iter().enumerate() {
            if level.quantity > 0.0 {
                updates.push(L2Update::new(
                    ts,
                    symbol,
                    Side::Ask,
                    Px::new(level.price),
                    Qty::new(level.quantity as f64),
                    i as u8,
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
        let api_key = self.config.api_url.split('/').last().unwrap_or("api_key");
        let ws_url = format!("{}?api_key={}&access_token={}", 
            self.config.ws_url,
            api_key,
            token
        );
        
        let (ws_stream, _) = connect_async(&ws_url).await?;
        let (mut write, mut read) = ws_stream.split();
        
        // Subscribe to market depth
        let tokens: Vec<String> = self.symbols
            .iter()
            .filter_map(|s| self.config.symbol_map.get(s))
            .cloned()
            .collect();
        
        let subscribe_msg = ZerodhaSubscribe {
            a: "subscribe".to_string(),
            v: tokens,
        };
        
        write.send(Message::Text(serde_json::to_string(&subscribe_msg)?)).await?;
        
        // Set mode to full (includes depth)
        let mode_msg = serde_json::json!({
            "a": "mode",
            "v": ["full", self.config.symbol_map.values().collect::<Vec<_>>()]
        });
        
        write.send(Message::Text(mode_msg.to_string())).await?;
        
        info!("Zerodha feed connected and subscribed");
        
        // Process messages
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    match serde_json::from_str::<ZerodhaMessage>(&text) {
                        Ok(ZerodhaMessage::Depth(depth)) => {
                            for update in self.parse_depth(&depth) {
                                if tx.send(update).await.is_err() {
                                    error!("Failed to send update");
                                    return Ok(());
                                }
                            }
                        }
                        Err(_) => {
                            // Ignore unparseable messages
                        }
                    }
                }
                Ok(Message::Binary(_)) => {
                    // Zerodha also sends binary format, parse if needed
                }
                Ok(Message::Close(_)) => {
                    info!("WebSocket closed");
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
    
    async fn disconnect(&mut self) -> anyhow::Result<()> {
        info!("Disconnecting Zerodha feed");
        Ok(())
    }
}

// Zerodha message types

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ZerodhaMessage {
    Depth(ZerodhaDepth),
}

#[derive(Debug, Deserialize)]
struct ZerodhaDepth {
    token: String,
    timestamp: u64,
    depth: Depth,
}

#[derive(Debug, Deserialize)]
struct Depth {
    buy: Vec<DepthLevel>,
    sell: Vec<DepthLevel>,
}

#[derive(Debug, Deserialize)]
struct DepthLevel {
    price: f64,
    quantity: f64,
}

#[derive(Debug, Serialize)]
struct ZerodhaSubscribe {
    a: String,
    v: Vec<String>,
}