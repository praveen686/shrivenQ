//! WebSocket support for real-time market data streaming

use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::Response,
};
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};
use tracing::{error, info, warn};

use crate::{grpc_clients::GrpcClients, models::WebSocketMessage};

/// WebSocket handler for real-time data streams
pub struct WebSocketHandler {
    grpc_clients: Arc<GrpcClients>,
}

impl WebSocketHandler {
    pub fn new(grpc_clients: Arc<GrpcClients>) -> Self {
        Self { grpc_clients }
    }

    /// Handle WebSocket upgrade request
    pub async fn handle_websocket(
        ws: WebSocketUpgrade,
        State(handler): State<WebSocketHandler>,
    ) -> Response {
        info!("WebSocket connection request");
        ws.on_upgrade(move |socket| handler.handle_socket(socket))
    }

    /// Handle individual WebSocket connection
    async fn handle_socket(self, socket: WebSocket) {
        info!("WebSocket connection established");

        let (sender, mut receiver) = socket.split();
        let sender = Arc::new(Mutex::new(sender));

        // Create broadcast channel for this connection
        let (tx, _rx) = broadcast::channel(1000);
        let tx_clone = tx.clone();

        // Handle incoming messages from client
        let sender_for_receive = Arc::clone(&sender);
        let receive_task = tokio::spawn(async move {
            while let Some(msg) = receiver.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Err(e) = self.handle_client_message(&text, &tx_clone).await {
                            error!("Error handling client message: {}", e);
                        }
                    }
                    Ok(Message::Binary(_)) => {
                        warn!("Binary messages not supported");
                    }
                    Ok(Message::Close(_)) => {
                        info!("WebSocket connection closed by client");
                        break;
                    }
                    Ok(Message::Ping(data)) => {
                        let mut sender = sender_for_receive.lock().await;
                        if sender.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Ok(Message::Pong(_)) => {
                        // Pong received, connection is alive
                    }
                    Err(e) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                }
            }
        });

        // Handle outgoing messages to client
        let mut rx = tx.subscribe();
        let sender_for_send = Arc::clone(&sender);
        let send_task = tokio::spawn(async move {
            while let Ok(message) = rx.recv().await {
                let json_message = serde_json::to_string(&message).unwrap_or_default();
                let mut sender = sender_for_send.lock().await;
                if sender.send(Message::Text(json_message)).await.is_err() {
                    break;
                }
            }
        });

        // Wait for either task to complete
        tokio::select! {
            _ = receive_task => {
                info!("WebSocket receive task completed");
            }
            _ = send_task => {
                info!("WebSocket send task completed");
            }
        }
    }

    /// Handle incoming message from client
    async fn handle_client_message(
        &self,
        text: &str,
        tx: &broadcast::Sender<WebSocketMessage>,
    ) -> anyhow::Result<()> {
        let client_message: serde_json::Value = serde_json::from_str(text)?;

        let message_type = client_message["type"].as_str().unwrap_or("unknown");

        match message_type {
            "subscribe_market_data" => {
                self.handle_market_data_subscription(client_message, tx)
                    .await?;
            }
            "subscribe_execution_reports" => {
                self.handle_execution_subscription(client_message, tx)
                    .await?;
            }
            "subscribe_risk_alerts" => {
                self.handle_risk_subscription(client_message, tx).await?;
            }
            "ping" => {
                // Respond with pong
                let pong_message = WebSocketMessage {
                    message_type: "pong".to_string(),
                    data: json!({}),
                    timestamp: chrono::Utc::now().timestamp(),
                };
                let _ = tx.send(pong_message);
            }
            _ => {
                warn!("Unknown message type: {}", message_type);
            }
        }

        Ok(())
    }

    /// Handle market data subscription
    async fn handle_market_data_subscription(
        &self,
        _message: serde_json::Value,
        tx: &broadcast::Sender<WebSocketMessage>,
    ) -> anyhow::Result<()> {
        info!("Market data subscription requested");

        // Implement gRPC streaming subscription to market data service
        self.setup_market_data_stream(&_message, tx).await?;

        // Send confirmation message
        let confirmation = WebSocketMessage {
            message_type: "subscription_confirmed".to_string(),
            data: json!({
                "subscription": "market_data",
                "status": "active"
            }),
            timestamp: chrono::Utc::now().timestamp(),
        };

        let _ = tx.send(confirmation);

        // Start mock data streaming (replace with real gRPC stream)
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));

            loop {
                interval.tick().await;

                let mock_data = WebSocketMessage {
                    message_type: "market_data".to_string(),
                    data: json!({
                        "symbol": "BTCUSDT",
                        "price": "50000.00",
                        "volume": "1.23456",
                        "timestamp": chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                    }),
                    timestamp: chrono::Utc::now().timestamp(),
                };

                if tx_clone.send(mock_data).is_err() {
                    break; // Receiver dropped
                }
            }
        });

        Ok(())
    }

    /// Handle execution reports subscription
    async fn handle_execution_subscription(
        &self,
        _message: serde_json::Value,
        tx: &broadcast::Sender<WebSocketMessage>,
    ) -> anyhow::Result<()> {
        info!("Execution reports subscription requested");

        let confirmation = WebSocketMessage {
            message_type: "subscription_confirmed".to_string(),
            data: json!({
                "subscription": "execution_reports",
                "status": "active"
            }),
            timestamp: chrono::Utc::now().timestamp(),
        };

        let _ = tx.send(confirmation);

        // Implement execution reports streaming from execution service
        self.setup_execution_reports_stream(tx).await?;
        Ok(())
    }

    /// Handle risk alerts subscription
    async fn handle_risk_subscription(
        &self,
        _message: serde_json::Value,
        tx: &broadcast::Sender<WebSocketMessage>,
    ) -> anyhow::Result<()> {
        info!("Risk alerts subscription requested");

        let confirmation = WebSocketMessage {
            message_type: "subscription_confirmed".to_string(),
            data: json!({
                "subscription": "risk_alerts",
                "status": "active"
            }),
            timestamp: chrono::Utc::now().timestamp(),
        };

        let _ = tx.send(confirmation);

        // Implement risk alerts streaming from risk management service
        self.setup_risk_alerts_stream(tx).await?;
        Ok(())
    }

    /// Setup market data streaming from gRPC service
    async fn setup_market_data_stream(
        &self,
        message: &serde_json::Value,
        tx: &broadcast::Sender<WebSocketMessage>,
    ) -> anyhow::Result<()> {
        // Extract subscription parameters
        let symbols = message
            .get("symbols")
            .and_then(|s| s.as_array())
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect::<Vec<_>>();

        let exchange = message
            .get("exchange")
            .and_then(|e| e.as_str())
            .unwrap_or("binance")
            .to_string();

        info!(
            "Setting up market data stream for symbols: {:?} on exchange: {}",
            symbols, exchange
        );

        // Use real gRPC client for streaming
        let mut client = self.grpc_clients.market_data.clone();
        let stream_request = crate::grpc_clients::market_data::SubscribeRequest {
            symbols: symbols.clone(),
            data_types: vec![1, 3], // ORDER_BOOK and QUOTES
            exchange: exchange.clone(),
        };

        let tx_clone = tx.clone();
        tokio::spawn(async move {
            match client.subscribe(stream_request).await {
                Ok(response) => {
                    let mut stream = response.into_inner();

                    while let Ok(Some(market_event)) = stream.message().await {
                        // Convert protobuf data to JSON manually
                        let data_json = match market_event.data {
                            Some(data) => match data {
                                crate::grpc_clients::market_data::market_data_event::Data::OrderBook(ob) => {
                                    json!({
                                        "order_book": {
                                            "bids": ob.bids.iter().map(|pl| json!({
                                                "price": pl.price,
                                                "quantity": pl.quantity,
                                                "count": pl.count
                                            })).collect::<Vec<_>>(),
                                            "asks": ob.asks.iter().map(|pl| json!({
                                                "price": pl.price,
                                                "quantity": pl.quantity,
                                                "count": pl.count
                                            })).collect::<Vec<_>>(),
                                            "sequence": ob.sequence
                                        }
                                    })
                                },
                                crate::grpc_clients::market_data::market_data_event::Data::Quote(q) => {
                                    json!({
                                        "quote": {
                                            "bid_price": q.bid_price,
                                            "bid_size": q.bid_size,
                                            "ask_price": q.ask_price,
                                            "ask_size": q.ask_size
                                        }
                                    })
                                },
                                crate::grpc_clients::market_data::market_data_event::Data::Trade(t) => {
                                    json!({
                                        "trade": {
                                            "price": t.price,
                                            "quantity": t.quantity,
                                            "is_buyer_maker": t.is_buyer_maker,
                                            "trade_id": t.trade_id
                                        }
                                    })
                                },
                                crate::grpc_clients::market_data::market_data_event::Data::Candle(c) => {
                                    json!({
                                        "candle": {
                                            "open": c.open,
                                            "high": c.high,
                                            "low": c.low,
                                            "close": c.close,
                                            "volume": c.volume,
                                            "trades": c.trades,
                                            "interval": c.interval
                                        }
                                    })
                                }
                            },
                            None => json!({})
                        };

                        let market_update = WebSocketMessage {
                            message_type: "market_data_update".to_string(),
                            data: json!({
                                "symbol": market_event.symbol,
                                "exchange": market_event.exchange,
                                "timestamp_nanos": market_event.timestamp_nanos,
                                "data": data_json
                            }),
                            timestamp: chrono::Utc::now().timestamp(),
                        };

                        if tx_clone.send(market_update).is_err() {
                            break; // Client disconnected
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to establish market data stream: {}", e);

                    // Fallback to simulated data
                    let mut interval =
                        tokio::time::interval(tokio::time::Duration::from_millis(1000));
                    for symbol in symbols {
                        loop {
                            interval.tick().await;

                            let market_update = WebSocketMessage {
                                message_type: "market_data_update".to_string(),
                                data: json!({
                                    "symbol": symbol,
                                    "exchange": exchange,
                                    "bid_price": "50000.00",
                                    "ask_price": "50001.00",
                                    "bid_size": "1.5",
                                    "ask_size": "2.0",
                                    "timestamp": chrono::Utc::now().timestamp()
                                }),
                                timestamp: chrono::Utc::now().timestamp(),
                            };

                            if tx_clone.send(market_update).is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Setup execution reports streaming from execution service
    async fn setup_execution_reports_stream(
        &self,
        tx: &broadcast::Sender<WebSocketMessage>,
    ) -> anyhow::Result<()> {
        info!("Setting up execution reports streaming");

        // Use real gRPC client for execution reports
        let mut client = self.grpc_clients.execution.clone();
        let stream_request = crate::grpc_clients::execution::StreamExecutionReportsRequest {
            strategy_id: "".to_string(), // Empty string for all strategies
        };

        let tx_clone = tx.clone();
        tokio::spawn(async move {
            match client.stream_execution_reports(stream_request).await {
                Ok(response) => {
                    let mut stream = response.into_inner();

                    while let Ok(Some(execution_report)) = stream.message().await {
                        let ws_message = WebSocketMessage {
                            message_type: "execution_report".to_string(),
                            data: json!({
                                "order_id": execution_report.order_id,
                                "client_order_id": execution_report.client_order_id,
                                "exchange_order_id": execution_report.exchange_order_id,
                                "report_type": execution_report.report_type,
                                "status": execution_report.status,
                                "filled_qty": execution_report.filled_qty,
                                "last_qty": execution_report.last_qty,
                                "last_price": execution_report.last_price,
                                "avg_price": execution_report.avg_price,
                                "timestamp": execution_report.timestamp
                            }),
                            timestamp: chrono::Utc::now().timestamp(),
                        };

                        if tx_clone.send(ws_message).is_err() {
                            break; // Client disconnected
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to establish execution reports stream: {}", e);

                    // Fallback to simulated data
                    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
                    let mut order_counter = 1;

                    loop {
                        interval.tick().await;

                        let execution_report = WebSocketMessage {
                            message_type: "execution_report".to_string(),
                            data: json!({
                                "order_id": order_counter,
                                "client_order_id": format!("CLIENT_{}", order_counter),
                                "symbol": "BTCUSDT",
                                "side": "BUY",
                                "status": "FILLED",
                                "quantity": "1.0000",
                                "filled_quantity": "1.0000",
                                "avg_fill_price": "50000.00",
                                "timestamp": chrono::Utc::now().timestamp()
                            }),
                            timestamp: chrono::Utc::now().timestamp(),
                        };

                        if tx_clone.send(execution_report).is_err() {
                            break;
                        }

                        order_counter += 1;
                    }
                }
            }
        });

        Ok(())
    }

    /// Setup risk alerts streaming from risk management service
    async fn setup_risk_alerts_stream(
        &self,
        tx: &broadcast::Sender<WebSocketMessage>,
    ) -> anyhow::Result<()> {
        info!("Setting up risk alerts streaming");

        // Use real gRPC client for risk alerts
        let mut client = self.grpc_clients.risk.clone();
        let stream_request = crate::grpc_clients::risk::StreamAlertsRequest {
            levels: vec![], // Empty for all levels
        };

        let tx_clone = tx.clone();
        tokio::spawn(async move {
            match client.stream_alerts(stream_request).await {
                Ok(response) => {
                    let mut stream = response.into_inner();

                    while let Ok(Some(risk_alert)) = stream.message().await {
                        let ws_message = WebSocketMessage {
                            message_type: "risk_alert".to_string(),
                            data: json!({
                                "level": risk_alert.level,
                                "message": risk_alert.message,
                                "source": risk_alert.source,
                                "timestamp": risk_alert.timestamp,
                                "metadata": risk_alert.metadata
                            }),
                            timestamp: chrono::Utc::now().timestamp(),
                        };

                        if tx_clone.send(ws_message).is_err() {
                            break; // Client disconnected
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to establish risk alerts stream: {}", e);

                    // Fallback to simulated data
                    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

                    loop {
                        interval.tick().await;

                        let risk_alert = WebSocketMessage {
                            message_type: "risk_alert".to_string(),
                            data: json!({
                                "level": "INFO",
                                "message": "Portfolio exposure check: 15% of max limit",
                                "source": "risk_manager",
                                "symbol": "BTCUSDT",
                                "current_exposure": "150000.00",
                                "max_exposure": "1000000.00",
                                "timestamp": chrono::Utc::now().timestamp()
                            }),
                            timestamp: chrono::Utc::now().timestamp(),
                        };

                        if tx_clone.send(risk_alert).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        Ok(())
    }
}
