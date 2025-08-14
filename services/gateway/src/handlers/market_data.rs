//! Market data handlers

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{error, info};

use crate::{
    grpc_clients::{GrpcClients, market_data},
    middleware::{check_permission, get_user_context},
    models::{
        ApiResponse, ErrorResponse, MarketSnapshot, OrderBookSnapshot, PriceLevel, QuoteSnapshot,
    },
};

/// Query parameters for market data
#[derive(Deserialize)]
pub struct SnapshotQuery {
    pub symbols: String, // Comma-separated symbols
    pub exchange: String,
}

#[derive(Deserialize)]
pub struct HistoricalQuery {
    pub symbol: String,
    pub exchange: String,
    pub start_time: i64,
    pub end_time: i64,
    pub data_type: String,
    pub interval: Option<String>,
}

/// Market data handlers
#[derive(Clone)]
pub struct MarketDataHandlers {
    grpc_clients: Arc<GrpcClients>,
}

impl MarketDataHandlers {
    pub fn new(grpc_clients: Arc<GrpcClients>) -> Self {
        Self { grpc_clients }
    }

    /// Get market data snapshot
    pub async fn get_snapshot(
        State(handlers): State<MarketDataHandlers>,
        request: axum::extract::Request,
        Query(query): Query<SnapshotQuery>,
    ) -> Result<Json<ApiResponse<Vec<MarketSnapshot>>>, StatusCode> {
        // Check permissions
        let user_context = get_user_context(&request);
        if let Some(user) = user_context {
            if !check_permission(user, "READ_MARKET_DATA") {
                let error_response = ErrorResponse {
                    error: "PERMISSION_DENIED".to_string(),
                    message: "Insufficient permissions to read market data".to_string(),
                    details: None,
                };
                return Ok(Json(ApiResponse::error(error_response)));
            }
        }

        info!("Get snapshot request for symbols: {}", query.symbols);

        let mut client = handlers.grpc_clients.market_data.clone();

        let symbols: Vec<String> = query
            .symbols
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();

        let grpc_request = market_data::GetSnapshotRequest {
            symbols,
            exchange: query.exchange,
        };

        match client.get_snapshot(grpc_request).await {
            Ok(response) => {
                let grpc_response = response.into_inner();

                let snapshots: Vec<MarketSnapshot> = grpc_response
                    .snapshots
                    .into_iter()
                    .map(|snapshot| {
                        let order_book = snapshot.order_book.map(|ob| OrderBookSnapshot {
                            bids: ob.bids.into_iter().map(price_level_from_grpc).collect(),
                            asks: ob.asks.into_iter().map(price_level_from_grpc).collect(),
                            sequence: ob.sequence,
                        });

                        let quote = snapshot.quote.map(|q| QuoteSnapshot {
                            bid_price: fixed_point_to_string(q.bid_price),
                            bid_size: fixed_point_to_string(q.bid_size),
                            ask_price: fixed_point_to_string(q.ask_price),
                            ask_size: fixed_point_to_string(q.ask_size),
                        });

                        MarketSnapshot {
                            symbol: snapshot.symbol,
                            timestamp_nanos: snapshot.timestamp_nanos,
                            order_book,
                            quote,
                        }
                    })
                    .collect();

                Ok(Json(ApiResponse::success(snapshots)))
            }
            Err(e) => {
                error!("Get snapshot failed: {}", e);
                let error_response = ErrorResponse {
                    error: "SNAPSHOT_FAILED".to_string(),
                    message: "Failed to get market data snapshot".to_string(),
                    details: None,
                };
                Ok(Json(ApiResponse::error(error_response)))
            }
        }
    }

    /// Get historical market data
    pub async fn get_historical_data(
        State(handlers): State<MarketDataHandlers>,
        request: axum::extract::Request,
        Query(query): Query<HistoricalQuery>,
    ) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
        // Check permissions
        let user_context = get_user_context(&request);
        if let Some(user) = user_context {
            if !check_permission(user, "READ_MARKET_DATA") {
                let error_response = ErrorResponse {
                    error: "PERMISSION_DENIED".to_string(),
                    message: "Insufficient permissions to read market data".to_string(),
                    details: None,
                };
                return Ok(Json(ApiResponse::error(error_response)));
            }
        }

        info!("Get historical data request for symbol: {}", query.symbol);

        let mut client = handlers.grpc_clients.market_data.clone();

        let data_type = match query.data_type.to_uppercase().as_str() {
            "ORDER_BOOK" => market_data::DataType::OrderBook.into(),
            "TRADES" => market_data::DataType::Trades.into(),
            "QUOTES" => market_data::DataType::Quotes.into(),
            "CANDLES" => market_data::DataType::Candles.into(),
            _ => market_data::DataType::Unspecified.into(),
        };

        let grpc_request = market_data::GetHistoricalDataRequest {
            symbol: query.symbol.clone(),
            exchange: query.exchange,
            start_time: query.start_time,
            end_time: query.end_time,
            data_type,
            interval: query.interval.unwrap_or_default(),
        };

        match client.get_historical_data(grpc_request).await {
            Ok(response) => {
                let grpc_response = response.into_inner();

                // Convert to flexible JSON response
                let events: Vec<serde_json::Value> = grpc_response
                    .events
                    .into_iter()
                    .map(|event| {
                        let mut json_event = serde_json::json!({
                            "symbol": event.symbol,
                            "exchange": event.exchange,
                            "timestamp_nanos": event.timestamp_nanos,
                        });

                        // Add specific data based on type
                        if let Some(data) = event.data {
                            match data {
                                market_data::market_data_event::Data::OrderBook(ob) => {
                                    json_event["order_book"] = serde_json::json!({
                                        "bids": ob.bids.into_iter().map(|pl| serde_json::json!({
                                            "price": fixed_point_to_string(pl.price),
                                            "quantity": fixed_point_to_string(pl.quantity),
                                            "count": pl.count
                                        })).collect::<Vec<_>>(),
                                        "asks": ob.asks.into_iter().map(|pl| serde_json::json!({
                                            "price": fixed_point_to_string(pl.price),
                                            "quantity": fixed_point_to_string(pl.quantity),
                                            "count": pl.count
                                        })).collect::<Vec<_>>(),
                                        "sequence": ob.sequence
                                    });
                                }
                                market_data::market_data_event::Data::Trade(trade) => {
                                    json_event["trade"] = serde_json::json!({
                                        "price": fixed_point_to_string(trade.price),
                                        "quantity": fixed_point_to_string(trade.quantity),
                                        "is_buyer_maker": trade.is_buyer_maker,
                                        "trade_id": trade.trade_id
                                    });
                                }
                                market_data::market_data_event::Data::Quote(quote) => {
                                    json_event["quote"] = serde_json::json!({
                                        "bid_price": fixed_point_to_string(quote.bid_price),
                                        "bid_size": fixed_point_to_string(quote.bid_size),
                                        "ask_price": fixed_point_to_string(quote.ask_price),
                                        "ask_size": fixed_point_to_string(quote.ask_size)
                                    });
                                }
                                market_data::market_data_event::Data::Candle(candle) => {
                                    json_event["candle"] = serde_json::json!({
                                        "open": fixed_point_to_string(candle.open),
                                        "high": fixed_point_to_string(candle.high),
                                        "low": fixed_point_to_string(candle.low),
                                        "close": fixed_point_to_string(candle.close),
                                        "volume": fixed_point_to_string(candle.volume),
                                        "trades": candle.trades,
                                        "interval": candle.interval
                                    });
                                }
                            }
                        }

                        json_event
                    })
                    .collect();

                let response_data = serde_json::json!({
                    "symbol": query.symbol,
                    "data_type": query.data_type,
                    "start_time": query.start_time,
                    "end_time": query.end_time,
                    "count": events.len(),
                    "events": events
                });

                Ok(Json(ApiResponse::success(response_data)))
            }
            Err(e) => {
                error!("Get historical data failed: {}", e);
                let error_response = ErrorResponse {
                    error: "HISTORICAL_DATA_FAILED".to_string(),
                    message: "Failed to get historical market data".to_string(),
                    details: None,
                };
                Ok(Json(ApiResponse::error(error_response)))
            }
        }
    }
}

/// Convert gRPC price level to REST model
fn price_level_from_grpc(pl: market_data::PriceLevel) -> PriceLevel {
    PriceLevel {
        price: fixed_point_to_string(pl.price),
        quantity: fixed_point_to_string(pl.quantity),
        count: pl.count,
    }
}

/// Convert fixed-point value to string with proper precision
fn fixed_point_to_string(value: i64) -> String {
    // For display only - using integer arithmetic to avoid casts
    let is_negative = value < 0;
    let abs_value = value.unsigned_abs();
    let whole = abs_value / 10000;
    let fraction = abs_value % 10000;

    if is_negative {
        format!("-{}.{:04}", whole, fraction)
    } else {
        format!("{}.{:04}", whole, fraction)
    }
}
