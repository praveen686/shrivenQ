//! Feed manager that orchestrates authentication, feeds, LOB, and event bus

use super::adapter::{FeedAdapter, FeedConfig};
use super::event::MarketEvent;
use crate::binance::BinanceFeed;
use crate::zerodha::ZerodhaFeed;
use auth::{ZerodhaAuth, ZerodhaConfig as ZerodhaAuthConfig, BinanceAuth, BinanceConfig as BinanceAuthConfig, BinanceMarket};
use bus::{Bus, Publisher};
use common::{L2Update, Symbol};
use lob::OrderBook;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info, warn};

/// Feed manager configuration
#[derive(Debug, Clone)]
pub struct FeedManagerConfig {
    /// Zerodha configuration
    pub zerodha: Option<ZerodhaConfig>,
    /// Binance configuration  
    pub binance: Option<BinanceConfig>,
    /// Buffer size for update channels
    pub buffer_size: usize,
}

#[derive(Debug, Clone)]
pub struct ZerodhaConfig {
    /// API key
    pub api_key: String,
    /// API secret
    pub api_secret: String,
    /// Token file path
    pub token_file: String,
    /// WebSocket URL
    pub ws_url: String,
    /// REST API URL
    pub api_url: String,
    /// Symbol mappings
    pub symbols: HashMap<Symbol, String>,
}

#[derive(Debug, Clone)]
pub struct BinanceConfig {
    /// API key
    pub api_key: String,
    /// API secret
    pub api_secret: String,
    /// WebSocket URL
    pub ws_url: String,
    /// REST API URL
    pub api_url: String,
    /// Symbol mappings
    pub symbols: HashMap<Symbol, String>,
}

/// Manages all feed connections with authentication
pub struct FeedManager {
    config: FeedManagerConfig,
    books: Arc<RwLock<HashMap<Symbol, OrderBook>>>,
    bus: Arc<Bus<MarketEvent>>,
}

impl FeedManager {
    /// Create new feed manager
    pub fn new(config: FeedManagerConfig, bus: Arc<Bus<MarketEvent>>) -> Self {
        Self {
            config,
            books: Arc::new(RwLock::new(HashMap::new())),
            bus,
        }
    }
    
    /// Initialize order books for all symbols
    pub async fn init_books(&self, symbols: Vec<Symbol>) {
        let mut books = self.books.write().await;
        for symbol in symbols {
            books.insert(symbol, OrderBook::new(symbol));
        }
        info!("Initialized {} order books", books.len());
    }
    
    /// Start all configured feeds with authentication
    pub async fn start(&self) -> anyhow::Result<()> {
        let (tx, mut rx) = mpsc::channel::<L2Update>(self.config.buffer_size);
        
        // Start Zerodha feed if configured
        if let Some(zerodha_config) = &self.config.zerodha {
            let tx_clone = tx.clone();
            let config_clone = zerodha_config.clone();
            
            tokio::spawn(async move {
                if let Err(e) = Self::run_zerodha_feed(config_clone, tx_clone).await {
                    error!("Zerodha feed error: {}", e);
                }
            });
        }
        
        // Start Binance feed if configured
        if let Some(binance_config) = &self.config.binance {
            let tx_clone = tx.clone();
            let config_clone = binance_config.clone();
            
            tokio::spawn(async move {
                if let Err(e) = Self::run_binance_feed(config_clone, tx_clone).await {
                    error!("Binance feed error: {}", e);
                }
            });
        }
        
        // Process updates
        let books = self.books.clone();
        let bus = self.bus.clone();
        
        tokio::spawn(async move {
            while let Some(update) = rx.recv().await {
                Self::process_update(update, &books, &bus).await;
            }
        });
        
        info!("Feed manager started");
        Ok(())
    }
    
    /// Run Zerodha feed with authentication
    async fn run_zerodha_feed(
        config: ZerodhaConfig,
        tx: mpsc::Sender<L2Update>
    ) -> anyhow::Result<()> {
        info!("Starting Zerodha feed with authentication");
        
        // Create authenticator
        let auth_config = ZerodhaAuthConfig::new(
            "user_id".to_string(), // This should come from config
            "password".to_string(), // This should come from config
            "totp_secret".to_string(), // This should come from config
            config.api_key.clone(),
            config.api_secret.clone(),
        ).with_cache_dir(config.token_file.clone());
        
        let auth = ZerodhaAuth::new(auth_config);
        
        // Verify authentication
        let token = auth.authenticate().await?;
        info!("Zerodha authentication successful, token: {}...", &token[..8]);
        
        // Create feed config
        let feed_config = FeedConfig {
            name: "zerodha".to_string(),
            ws_url: config.ws_url,
            api_url: config.api_url,
            symbol_map: config.symbols.clone(),
            max_reconnects: 5,
            reconnect_delay_ms: 1000,
        };
        
        // Create and run feed
        let mut feed = ZerodhaFeed::new(feed_config, auth);
        let symbols: Vec<Symbol> = config.symbols.keys().copied().collect();
        
        feed.connect().await?;
        feed.subscribe(symbols).await?;
        
        loop {
            match feed.run(tx.clone()).await {
                Ok(_) => {
                    warn!("Zerodha feed disconnected, reconnecting...");
                    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                }
                Err(e) => {
                    error!("Zerodha feed error: {}, reconnecting...", e);
                    tokio::time::sleep(tokio::time::Duration::from_millis(5000)).await;
                }
            }
        }
    }
    
    /// Run Binance feed with authentication
    async fn run_binance_feed(
        config: BinanceConfig,
        tx: mpsc::Sender<L2Update>
    ) -> anyhow::Result<()> {
        info!("Starting Binance feed with authentication");
        
        // Create authenticator
        let mut auth = BinanceAuth::new();
        auth.add_market(BinanceAuthConfig::new(
            config.api_key.clone(),
            config.api_secret.clone(),
            BinanceMarket::Spot,
        ));
        
        info!("Binance auth created for API key: {}...", &config.api_key[..8]);
        
        // Create feed config
        let feed_config = FeedConfig {
            name: "binance".to_string(),
            ws_url: config.ws_url,
            api_url: config.api_url,
            symbol_map: config.symbols.clone(),
            max_reconnects: 5,
            reconnect_delay_ms: 1000,
        };
        
        // Create and run feed
        let mut feed = BinanceFeed::new(feed_config, auth, BinanceMarket::Spot);
        let symbols: Vec<Symbol> = config.symbols.keys().copied().collect();
        
        feed.connect().await?;
        feed.subscribe(symbols).await?;
        
        loop {
            match feed.run(tx.clone()).await {
                Ok(_) => {
                    warn!("Binance feed disconnected, reconnecting...");
                    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                }
                Err(e) => {
                    error!("Binance feed error: {}, reconnecting...", e);
                    tokio::time::sleep(tokio::time::Duration::from_millis(5000)).await;
                }
            }
        }
    }
    
    /// Process L2 update through LOB and publish to event bus
    async fn process_update(
        update: L2Update,
        books: &Arc<RwLock<HashMap<Symbol, OrderBook>>>,
        bus: &Arc<Bus<MarketEvent>>,
    ) {
        let mut books = books.write().await;
        
        if let Some(book) = books.get_mut(&update.symbol) {
            // Apply update to LOB
            match book.apply(&update) {
                Ok(_) => {
                    // Publish update to event bus
                    let publisher = bus.publisher();
                    let event = MarketEvent::L2Update(update);
                    if let Err(e) = publisher.publish(event) {
                        error!("Failed to publish update: {}", e);
                    }
                    
                    // Also publish LOB snapshot
                    let lob_update = book.to_update();
                    let event = MarketEvent::LOBUpdate(lob_update);
                    if let Err(e) = publisher.publish(event) {
                        error!("Failed to publish LOB update: {}", e);
                    }
                }
                Err(e) => {
                    warn!("Failed to apply update to book: {}", e);
                }
            }
        } else {
            warn!("No book for symbol: {:?}", update.symbol);
        }
    }
    
    /// Get current state of an order book
    pub async fn get_book(&self, symbol: Symbol) -> Option<OrderBook> {
        let books = self.books.read().await;
        books.get(&symbol).cloned()
    }
    
    /// Get all active symbols
    pub async fn get_symbols(&self) -> Vec<Symbol> {
        let books = self.books.read().await;
        books.keys().copied().collect()
    }
}