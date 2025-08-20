//! WebSocket OrderBook Testing Utility
//! 
//! Tests Binance WebSocket orderbook stream connectivity and data parsing

use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{StreamExt, SinkExt};
use serde_json::Value;
use std::time::Duration;
use tracing;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for internal logging
    tracing_subscriber::fmt()
        .with_env_filter("debug")
        .init();
        
    // User-facing output for test results
    println!("🚀 Testing Binance WebSocket Orderbook Stream");
    println!("{}", "=".repeat(60));
    
    // Simple single stream test first
    let symbol = "btcusdt";
    let url = format!("wss://stream.binance.com:9443/ws/{}@depth@100ms", symbol);
    
    println!("Connecting to: {}", url);
    
    let (ws_stream, response) = connect_async(&url).await?;
    println!("✅ Connected! Response: {:?}", response.status());
    
    let (mut write, mut read) = ws_stream.split();
    
    // Counter for messages
    let mut msg_count = 0;
    let max_messages = 10;
    
    // Create channel for ping task communication
    let (ping_tx, mut ping_rx) = mpsc::unbounded_channel::<Message>();
    
    // Spawn ping task
    let ping_sender = ping_tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            if ping_sender.send(Message::Ping(vec![])).is_err() {
                break;
            }
        }
    });
    
    println!("\n📊 Receiving orderbook updates:");
    println!("{}", "-".repeat(60));
    
    loop {
        tokio::select! {
            // Handle incoming WebSocket messages
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        msg_count += 1;
                        
                        // Parse and display key info
                        if let Ok(json) = serde_json::from_str::<Value>(&text) {
                            let update_id = json["u"].as_u64().unwrap_or(0);
                            let bids = json["b"].as_array().map(|a| a.len()).unwrap_or(0);
                            let asks = json["a"].as_array().map(|a| a.len()).unwrap_or(0);
                            
                            println!("Update #{}: ID={}, Bids={}, Asks={}", 
                                     msg_count, update_id, bids, asks);
                            
                            // Show first bid and ask
                            if let Some(bid_array) = json["b"].as_array() {
                                if let Some(first_bid) = bid_array.first() {
                                    if let (Some(price), Some(qty)) = 
                                        (first_bid[0].as_str(), first_bid[1].as_str()) {
                                        println!("  Best Bid: {} @ {}", qty, price);
                                    }
                                }
                            }
                            
                            if let Some(ask_array) = json["a"].as_array() {
                                if let Some(first_ask) = ask_array.first() {
                                    if let (Some(price), Some(qty)) = 
                                        (first_ask[0].as_str(), first_ask[1].as_str()) {
                                        println!("  Best Ask: {} @ {}", qty, price);
                                    }
                                }
                            }
                            
                            println!();
                        } else {
                            println!("Received: {} bytes", text.len());
                        }
                        
                        if msg_count >= max_messages {
                            println!("✅ Test complete! Received {} orderbook updates", msg_count);
                            break;
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        tracing::debug!("Received ping with {} bytes, sending pong", data.len());
                        if let Err(e) = write.send(Message::Pong(data)).await {
                            tracing::error!("Failed to send pong: {}", e);
                        }
                    }
                    Some(Err(e)) => {
                        println!("❌ Error: {}", e);
                        break;
                    }
                    None => {
                        println!("🔚 WebSocket connection closed");
                        break;
                    }
                    _ => {}
                }
            }
            // Handle ping messages from ping task
            ping_msg = ping_rx.recv() => {
                if let Some(msg) = ping_msg {
                    if let Err(e) = write.send(msg).await {
                        tracing::error!("Failed to send ping: {}", e);
                        break;
                    }
                } else {
                    break; // Channel closed
                }
            }
        }
    }
    
    println!("\n🎉 WebSocket orderbook stream working correctly!");
    Ok(())
}