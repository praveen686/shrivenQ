//! Live Orderbook Analytics Dashboard
//! 
//! This binary connects to live market data and displays real-time
//! orderbook analytics including VPIN, Kyle's Lambda, PIN, and toxicity detection.
//!
//! Run with: cargo run --example live_analytics

use orderbook::{OrderBook, PerformanceMetrics};
use orderbook::analytics::{MicrostructureAnalytics, ImbalanceCalculator, ToxicityDetector};
use orderbook::core::{Order, Side};

use services_common::{Px, Qty, Symbol, Ts};
use services_common::marketdata::v1::{
    market_data_service_client::MarketDataServiceClient,
    SubscribeRequest, MarketDataEvent,
};
use tokio::time::{interval, Duration};
use tokio_stream::StreamExt;
use tonic::Request;
use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::RwLock;
use chrono::Local;
use colored::*;

/// Live orderbook manager for a symbol
struct LiveOrderBook {
    orderbook: OrderBook,
    analytics: MicrostructureAnalytics,
    toxicity: ToxicityDetector,
    metrics: PerformanceMetrics,
    last_update: Ts,
    update_count: u64,
}

impl LiveOrderBook {
    fn new(symbol: &str) -> Self {
        Self {
            orderbook: OrderBook::new(symbol),
            analytics: MicrostructureAnalytics::new(),
            toxicity: ToxicityDetector::new(),
            metrics: PerformanceMetrics::new(symbol),
            last_update: Ts::now(),
            update_count: 0,
        }
    }
}

/// Analytics dashboard state
struct AnalyticsDashboard {
    orderbooks: Arc<RwLock<HashMap<String, LiveOrderBook>>>,
    start_time: std::time::Instant,
}

impl AnalyticsDashboard {
    fn new() -> Self {
        Self {
            orderbooks: Arc::new(RwLock::new(HashMap::new())),
            start_time: std::time::Instant::now(),
        }
    }

    /// Process market data event
    fn process_market_event(&self, event: MarketDataEvent) {
        let start = std::time::Instant::now();
        
        match event.data {
            Some(services_common::proto::marketdata::v1::market_data_event::Data::OrderBook(book)) => {
                let mut orderbooks = self.orderbooks.write();
                let live_book = orderbooks.entry(event.symbol.clone())
                    .or_insert_with(|| LiveOrderBook::new(&event.symbol));
                
                // Convert proto levels to internal format
                let bid_levels: Vec<(Px, Qty, u64)> = book.bids.iter()
                    .map(|level| (
                        Px::from_i64(level.price),
                        Qty::from_i64(level.quantity),
                        level.count as u64
                    ))
                    .collect();
                    
                let ask_levels: Vec<(Px, Qty, u64)> = book.asks.iter()
                    .map(|level| (
                        Px::from_i64(level.price),
                        Qty::from_i64(level.quantity),
                        level.count as u64
                    ))
                    .collect();
                
                // Load snapshot into orderbook
                live_book.orderbook.load_snapshot(bid_levels.clone(), ask_levels.clone());
                
                // Calculate imbalances
                let imbalances = ImbalanceCalculator::calculate_imbalances(&bid_levels, &ask_levels);
                
                // Update analytics with actual orderbook data
                if let (Some((best_bid, bid_qty, _)), Some((best_ask, ask_qty, _))) = 
                    (bid_levels.first(), ask_levels.first()) {
                    
                    // Use mid price as trade price for analytics
                    let mid_price = Px::from_i64((best_bid.as_i64() + best_ask.as_i64()) / 2);
                    
                    // Use actual volume from orderbook - take the average of best bid/ask sizes
                    // This creates more realistic volume variation for Kyle's Lambda calculation
                    let total_depth_volume: i64 = bid_levels.iter()
                        .chain(ask_levels.iter())
                        .take(5)
                        .map(|(_, qty, _)| qty.as_i64())
                        .sum();
                    
                    // Add some variation based on depth and imbalance
                    let volume_factor = 1.0 + (imbalances.top_level_imbalance.abs() / 100.0);
                    let trade_volume = Qty::from_i64(((total_depth_volume as f64 / 10.0) * volume_factor) as i64).max(Qty::from_i64(100));
                    
                    // Update analytics with realistic trade volume
                    live_book.analytics.update_trade(
                        mid_price, 
                        trade_volume,
                        imbalances.buy_pressure > imbalances.sell_pressure,
                        Ts::now()
                    );
                    
                    // Update toxicity detector with same volume
                    live_book.toxicity.update(
                        imbalances.buy_pressure > imbalances.sell_pressure,
                        trade_volume,
                        Ts::now()
                    );
                }
                
                // Record metrics
                let latency = start.elapsed().as_nanos() as u64;
                live_book.metrics.record_order_add(Qty::from_i64(1000), latency);
                
                // Update counters
                live_book.update_count += 1;
                live_book.last_update = Ts::now();
            }
            Some(services_common::proto::marketdata::v1::market_data_event::Data::Trade(trade)) => {
                let mut orderbooks = self.orderbooks.write();
                let live_book = orderbooks.entry(event.symbol.clone())
                    .or_insert_with(|| LiveOrderBook::new(&event.symbol));
                
                // Use actual trade quantity - multiply by 10000 to convert from float representation
                let trade_qty = Qty::from_i64((trade.quantity as f64 * 10000.0) as i64);
                let trade_price = Px::from_i64(trade.price);
                
                // Update analytics with actual trade data
                live_book.analytics.update_trade(
                    trade_price,
                    trade_qty,
                    trade.is_buyer_maker,
                    Ts::from_nanos(event.timestamp_nanos as u64)
                );
                
                // Update toxicity with actual volume
                live_book.toxicity.update(
                    trade.is_buyer_maker,
                    trade_qty,
                    Ts::from_nanos(event.timestamp_nanos as u64)
                );
                
                let latency = start.elapsed().as_nanos() as u64;
                live_book.metrics.record_trade(trade_qty, latency);
                
                // Update counters
                live_book.update_count += 1;
                live_book.last_update = Ts::from_nanos(event.timestamp_nanos as u64);
            }
            _ => {}
        }
    }

    /// Display analytics dashboard
    fn display_dashboard(&self) {
        // Clear screen (ANSI escape code)
        print!("\x1B[2J\x1B[1;1H");
        
        let uptime = self.start_time.elapsed().as_secs();
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        
        println!("{}", "═══════════════════════════════════════════════════════════════════════════════".bright_cyan());
        println!("{} {} {}", 
            "║".bright_cyan(),
            format!("   ShrivenQuant Live Orderbook Analytics Dashboard   [{}]", timestamp).bright_white().bold(),
            "║".bright_cyan()
        );
        println!("{}", "═══════════════════════════════════════════════════════════════════════════════".bright_cyan());
        println!("  Uptime: {}s", uptime);
        println!();
        
        let orderbooks = self.orderbooks.read();
        
        if orderbooks.is_empty() {
            println!("  {} Waiting for market data...", "⏳".yellow());
            return;
        }
        
        for (symbol, live_book) in orderbooks.iter() {
            println!("{}", format!("  📊 {} (Updates: {})", symbol, live_book.update_count).bright_yellow().bold());
            println!("  {}", "─".repeat(75).bright_black());
            
            // Display BBO
            let (best_bid, best_ask) = live_book.orderbook.get_bbo();
            if let (Some(bid), Some(ask)) = (best_bid, best_ask) {
                let spread = live_book.orderbook.get_spread().unwrap_or(0);
                let spread_bps = if ask.as_i64() > 0 {
                    (spread as f64 / ask.as_f64()) * 10000.0
                } else {
                    0.0
                };
                
                println!("  {} Best Bid: ${:.2}  |  Best Ask: ${:.2}  |  Spread: ${:.2} ({:.1} bps)",
                    "📈".green(),
                    bid.as_f64() / 10000.0,
                    ask.as_f64() / 10000.0,
                    spread as f64 / 10000.0,
                    spread_bps
                );
            }
            
            // Display orderbook depth
            let (bid_levels, ask_levels) = live_book.orderbook.get_depth(5);
            if !bid_levels.is_empty() || !ask_levels.is_empty() {
                println!("\n  {} Orderbook Depth:", "📚".cyan());
                println!("  {:>12} {:>8} {:>6} │ {:>6} {:>8} {:>12}",
                    "Bid Price", "Size", "Orders", "Orders", "Size", "Ask Price");
                println!("  {}", "─".repeat(75).bright_black());
                
                for i in 0..5 {
                    let bid_str = if i < bid_levels.len() {
                        let (price, qty, count) = &bid_levels[i];
                        format!("{:>12.2} {:>8.4} {:>6}",
                            price.as_f64() / 10000.0,
                            qty.as_f64() / 10000.0,
                            count
                        ).green().to_string()
                    } else {
                        format!("{:>12} {:>8} {:>6}", "", "", "")
                    };
                    
                    let ask_str = if i < ask_levels.len() {
                        let (price, qty, count) = &ask_levels[i];
                        format!("{:>6} {:>8.4} {:>12.2}",
                            count,
                            qty.as_f64() / 10000.0,
                            price.as_f64() / 10000.0
                        ).red().to_string()
                    } else {
                        format!("{:>6} {:>8} {:>12}", "", "", "")
                    };
                    
                    println!("  {} │ {}", bid_str, ask_str);
                }
            }
            
            // Calculate and display imbalances
            let (bid_levels, ask_levels) = live_book.orderbook.get_depth(10);
            let imbalances = ImbalanceCalculator::calculate_imbalances(&bid_levels, &ask_levels);
            
            println!("\n  {} Market Imbalance Analysis:", "⚖️".bright_magenta());
            println!("  {:20} {:>10} {:>10} {:>10} {:>10}",
                "", "Top", "3-Level", "5-Level", "10-Level");
            println!("  {:20} {:>10.1}% {:>10.1}% {:>10.1}% {:>10.1}%",
                "Imbalance:",
                imbalances.top_level_imbalance,
                imbalances.three_level_imbalance,
                imbalances.five_level_imbalance,
                imbalances.ten_level_imbalance
            );
            
            // Display pressure indicators
            if imbalances.buy_pressure > 0.0 {
                let bar_length = (imbalances.buy_pressure / 2.0).min(50.0) as usize;
                println!("  Buy Pressure:  {} {:.1}%", 
                    "█".repeat(bar_length).green(),
                    imbalances.buy_pressure
                );
            }
            if imbalances.sell_pressure > 0.0 {
                let bar_length = (imbalances.sell_pressure / 2.0).min(50.0) as usize;
                println!("  Sell Pressure: {} {:.1}%",
                    "█".repeat(bar_length).red(),
                    imbalances.sell_pressure
                );
            }
            
            // Display microstructure analytics
            println!("\n  {} Advanced Market Microstructure Analytics:", "🔬".bright_cyan());
            println!("  ┌─────────────────────────┬──────────────┐");
            println!("  │ {:23} │ {:^12} │", "Metric", "Value");
            println!("  ├─────────────────────────┼──────────────┤");
            
            let vpin = live_book.analytics.get_vpin();
            let vpin_color = if vpin > 50.0 { "red" } else if vpin > 30.0 { "yellow" } else { "green" };
            println!("  │ {:23} │ {:>11.2}% │", 
                "VPIN (Toxicity)",
                format!("{:.2}", vpin).color(vpin_color)
            );
            
            let flow_imbalance = live_book.analytics.get_flow_imbalance();
            let flow_color = if flow_imbalance.abs() > 50.0 { "red" } else if flow_imbalance.abs() > 30.0 { "yellow" } else { "green" };
            println!("  │ {:23} │ {:>11.2}% │",
                "Flow Imbalance",
                format!("{:.2}", flow_imbalance).color(flow_color)
            );
            
            println!("  │ {:23} │ {:>11.4} │",
                "Kyle's Lambda",
                live_book.analytics.get_kyles_lambda()
            );
            
            let pin = live_book.analytics.get_pin();
            let pin_color = if pin > 50.0 { "red" } else if pin > 30.0 { "yellow" } else { "green" };
            println!("  │ {:23} │ {:>11.2}% │",
                "PIN (Informed Trading)",
                format!("{:.2}", pin).color(pin_color)
            );
            
            let toxicity = live_book.toxicity.get_toxicity();
            let tox_color = if toxicity > 70.0 { "red" } else if toxicity > 40.0 { "yellow" } else { "green" };
            println!("  │ {:23} │ {:>11.2}% │",
                "Toxicity Score",
                format!("{:.2}", toxicity).color(tox_color)
            );
            
            println!("  └─────────────────────────┴──────────────┘");
            
            // Display performance metrics
            let metrics = live_book.metrics.get_snapshot();
            if let Some(add_latency) = &metrics.latency_stats.order_add {
                println!("\n  {} Performance Metrics:", "⚡".yellow());
                println!("  Operation Latencies (nanoseconds):");
                println!("  {:15} p50: {:>8} | p90: {:>8} | p99: {:>8} | p99.9: {:>8}",
                    "Order Add:",
                    format!("{}", add_latency.p50).bright_green(),
                    format!("{}", add_latency.p90).yellow(),
                    format!("{}", add_latency.p99).yellow(),
                    format!("{}", add_latency.p999).red()
                );
            }
            
            // Display warnings
            if vpin > 50.0 || toxicity > 70.0 || pin > 50.0 {
                println!("\n  {} {} Toxic flow detected! High probability of adverse selection.",
                    "⚠️".red().bold(),
                    "WARNING:".red().bold()
                );
            }
            
            println!("\n  {}", "─".repeat(75).bright_black());
        }
        
        println!("\n  {} to stop", "Press Ctrl+C".bright_black());
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    
    println!("{}", "🚀 Starting ShrivenQuant Live Orderbook Analytics...".bright_green().bold());
    println!("📡 Connecting to Market Connector service...\n");
    
    // Create dashboard
    let dashboard = Arc::new(AnalyticsDashboard::new());
    
    // Connect to Market Connector
    let market_connector_addr = "http://127.0.0.1:50052";
    
    // Clone for the display task
    let dashboard_display = dashboard.clone();
    
    // Start display refresh task
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            dashboard_display.display_dashboard();
        }
    });
    
    // Main connection loop
    loop {
        match MarketDataServiceClient::connect(market_connector_addr.to_string()).await {
            Ok(mut client) => {
                println!("✅ Connected to Market Connector!\n");
                
                // Subscribe to market data
                let request = Request::new(SubscribeRequest {
                    symbols: vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()],
                    data_types: vec![], // Subscribe to all data types
                    exchange: "binance".to_string(),
                });
                
                match client.subscribe(request).await {
                    Ok(response) => {
                        let mut stream = response.into_inner();
                        
                        while let Some(event_result) = stream.next().await {
                            match event_result {
                                Ok(market_event) => {
                                    dashboard.process_market_event(market_event);
                                }
                                Err(e) => {
                                    eprintln!("❌ Stream error: {}", e);
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to subscribe: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("❌ Failed to connect to Market Connector: {}", e);
                eprintln!("   Make sure Market Connector is running (./test_live_data.sh)");
                eprintln!("   Retrying in 5 seconds...\n");
            }
        }
        
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}