//! Zerodha Integration for Options Trading
//! Complete integration with Kite Connect API for Indian markets

use anyhow::{Result, Context};
use chrono::{DateTime, Utc, Local, NaiveDate};
use reqwest::{Client, header};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::RwLock;
use std::sync::Arc;

// ============================================================================
// ZERODHA API STRUCTURES
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerodhaConfig {
    pub api_key: String,
    pub api_secret: String,
    pub access_token: String,
    pub user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionChainRequest {
    pub symbol: String,      // e.g., "NIFTY", "BANKNIFTY"
    pub expiry: String,      // e.g., "2024-01-25"
    pub strike_range: (f64, f64), // Min and max strikes
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionQuote {
    pub instrument_token: u32,
    pub timestamp: DateTime<Utc>,
    pub last_price: f64,
    pub volume: u64,
    pub buy_quantity: u64,
    pub sell_quantity: u64,
    pub open_interest: u64,
    pub bid: f64,
    pub ask: f64,
    pub bid_quantity: u64,
    pub ask_quantity: u64,
    pub change: f64,
    pub change_percent: f64,
    pub greeks: OptionGreeks,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionGreeks {
    pub iv: f64,
    pub delta: f64,
    pub gamma: f64,
    pub theta: f64,
    pub vega: f64,
    pub rho: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRequest {
    pub tradingsymbol: String,
    pub exchange: String,
    pub transaction_type: String, // "BUY" or "SELL"
    pub order_type: String,       // "MARKET", "LIMIT", "SL", "SL-M"
    pub quantity: u32,
    pub product: String,          // "MIS", "NRML", "CNC"
    pub validity: String,         // "DAY", "IOC"
    pub price: Option<f64>,
    pub trigger_price: Option<f64>,
    pub tag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub tradingsymbol: String,
    pub exchange: String,
    pub instrument_token: u32,
    pub product: String,
    pub quantity: i32,
    pub average_price: f64,
    pub last_price: f64,
    pub pnl: f64,
    pub unrealised: f64,
    pub realised: f64,
    pub m2m: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarginRequirement {
    pub total: f64,
    pub span: f64,
    pub exposure: f64,
    pub option_premium: f64,
    pub additional: f64,
    pub bo_freeze: f64,
}

// ============================================================================
// ZERODHA API CLIENT
// ============================================================================

pub struct ZerodhaOptionsClient {
    config: ZerodhaConfig,
    client: Client,
    base_url: String,
    instruments: Arc<RwLock<HashMap<String, InstrumentInfo>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentInfo {
    pub instrument_token: u32,
    pub exchange_token: u32,
    pub tradingsymbol: String,
    pub name: String,
    pub expiry: Option<NaiveDate>,
    pub strike: f64,
    pub tick_size: f64,
    pub lot_size: u32,
    pub instrument_type: String, // "CE", "PE", "FUT"
    pub segment: String,
    pub exchange: String,
}

impl ZerodhaOptionsClient {
    pub fn new(config: ZerodhaConfig) -> Self {
        let client = Client::new();
        Self {
            config,
            client,
            base_url: "https://api.kite.trade".to_string(),
            instruments: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Fetch and cache all instruments
    pub async fn fetch_instruments(&self) -> Result<()> {
        let url = format!("{}/instruments", self.base_url);
        
        let response = self.client
            .get(&url)
            .header("X-Kite-Version", "3")
            .header("Authorization", format!("token {}:{}", 
                self.config.api_key, self.config.access_token))
            .send()
            .await?;
        
        let csv_data = response.text().await?;
        let mut instruments = self.instruments.write().await;
        
        // Parse CSV (simplified - use csv crate in production)
        for line in csv_data.lines().skip(1) {
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() >= 12 {
                let instrument = InstrumentInfo {
                    instrument_token: parts[0].parse().unwrap_or(0),
                    exchange_token: parts[1].parse().unwrap_or(0),
                    tradingsymbol: parts[2].to_string(),
                    name: parts[3].to_string(),
                    expiry: None, // Parse date if present
                    strike: parts[6].parse().unwrap_or(0.0),
                    tick_size: parts[7].parse().unwrap_or(0.05),
                    lot_size: parts[8].parse().unwrap_or(1),
                    instrument_type: parts[9].to_string(),
                    segment: parts[10].to_string(),
                    exchange: parts[11].to_string(),
                };
                
                instruments.insert(instrument.tradingsymbol.clone(), instrument);
            }
        }
        
        println!("📊 Loaded {} instruments", instruments.len());
        Ok(())
    }
    
    /// Get option chain for an index
    pub async fn get_option_chain(&self, request: OptionChainRequest) -> Result<OptionChain> {
        let instruments = self.instruments.read().await;
        
        let mut calls = Vec::new();
        let mut puts = Vec::new();
        
        // Filter instruments for the requested symbol and expiry
        for (symbol, info) in instruments.iter() {
            if symbol.contains(&request.symbol) && symbol.contains(&request.expiry) {
                if info.strike >= request.strike_range.0 && info.strike <= request.strike_range.1 {
                    let quote = self.get_quote(&info.instrument_token.to_string()).await?;
                    
                    if symbol.ends_with("CE") {
                        calls.push(OptionData {
                            strike: info.strike,
                            quote: quote.clone(),
                            info: info.clone(),
                        });
                    } else if symbol.ends_with("PE") {
                        puts.push(OptionData {
                            strike: info.strike,
                            quote: quote.clone(),
                            info: info.clone(),
                        });
                    }
                }
            }
        }
        
        // Sort by strike
        calls.sort_by(|a, b| a.strike.partial_cmp(&b.strike).unwrap());
        puts.sort_by(|a, b| a.strike.partial_cmp(&b.strike).unwrap());
        
        Ok(OptionChain {
            symbol: request.symbol,
            expiry: request.expiry,
            calls,
            puts,
            spot_price: self.get_spot_price(&request.symbol).await?,
            timestamp: Utc::now(),
        })
    }
    
    /// Get real-time quote for an instrument
    pub async fn get_quote(&self, instrument_token: &str) -> Result<OptionQuote> {
        let url = format!("{}/quote?i={}", self.base_url, instrument_token);
        
        let response = self.client
            .get(&url)
            .header("X-Kite-Version", "3")
            .header("Authorization", format!("token {}:{}", 
                self.config.api_key, self.config.access_token))
            .send()
            .await?;
        
        let data: Value = response.json().await?;
        
        // Parse the response (simplified)
        Ok(OptionQuote {
            instrument_token: instrument_token.parse().unwrap_or(0),
            timestamp: Utc::now(),
            last_price: data["data"][instrument_token]["last_price"].as_f64().unwrap_or(0.0),
            volume: data["data"][instrument_token]["volume"].as_u64().unwrap_or(0),
            buy_quantity: data["data"][instrument_token]["buy_quantity"].as_u64().unwrap_or(0),
            sell_quantity: data["data"][instrument_token]["sell_quantity"].as_u64().unwrap_or(0),
            open_interest: data["data"][instrument_token]["oi"].as_u64().unwrap_or(0),
            bid: data["data"][instrument_token]["depth"]["buy"][0]["price"].as_f64().unwrap_or(0.0),
            ask: data["data"][instrument_token]["depth"]["sell"][0]["price"].as_f64().unwrap_or(0.0),
            bid_quantity: data["data"][instrument_token]["depth"]["buy"][0]["quantity"].as_u64().unwrap_or(0),
            ask_quantity: data["data"][instrument_token]["depth"]["sell"][0]["quantity"].as_u64().unwrap_or(0),
            change: data["data"][instrument_token]["change"].as_f64().unwrap_or(0.0),
            change_percent: data["data"][instrument_token]["change_percent"].as_f64().unwrap_or(0.0),
            greeks: OptionGreeks {
                iv: 0.0, // Calculate using Black-Scholes
                delta: 0.0,
                gamma: 0.0,
                theta: 0.0,
                vega: 0.0,
                rho: 0.0,
            },
        })
    }
    
    /// Get spot price for an index
    pub async fn get_spot_price(&self, symbol: &str) -> Result<f64> {
        let index_token = match symbol {
            "NIFTY" => "256265", // Nifty 50
            "BANKNIFTY" => "260105", // Bank Nifty
            "FINNIFTY" => "257801", // Fin Nifty
            _ => return Ok(0.0),
        };
        
        let quote = self.get_quote(index_token).await?;
        Ok(quote.last_price)
    }
    
    /// Place an order
    pub async fn place_order(&self, order: OrderRequest) -> Result<String> {
        let url = format!("{}/orders/regular", self.base_url);
        
        let response = self.client
            .post(&url)
            .header("X-Kite-Version", "3")
            .header("Authorization", format!("token {}:{}", 
                self.config.api_key, self.config.access_token))
            .json(&order)
            .send()
            .await?;
        
        let data: Value = response.json().await?;
        
        if data["status"] == "success" {
            Ok(data["data"]["order_id"].as_str().unwrap_or("").to_string())
        } else {
            Err(anyhow::anyhow!("Order placement failed: {:?}", data))
        }
    }
    
    /// Get positions
    pub async fn get_positions(&self) -> Result<Vec<Position>> {
        let url = format!("{}/portfolio/positions", self.base_url);
        
        let response = self.client
            .get(&url)
            .header("X-Kite-Version", "3")
            .header("Authorization", format!("token {}:{}", 
                self.config.api_key, self.config.access_token))
            .send()
            .await?;
        
        let data: Value = response.json().await?;
        
        // Parse positions
        let mut positions = Vec::new();
        if let Some(net_positions) = data["data"]["net"].as_array() {
            for pos in net_positions {
                positions.push(Position {
                    tradingsymbol: pos["tradingsymbol"].as_str().unwrap_or("").to_string(),
                    exchange: pos["exchange"].as_str().unwrap_or("").to_string(),
                    instrument_token: pos["instrument_token"].as_u64().unwrap_or(0) as u32,
                    product: pos["product"].as_str().unwrap_or("").to_string(),
                    quantity: pos["quantity"].as_i64().unwrap_or(0) as i32,
                    average_price: pos["average_price"].as_f64().unwrap_or(0.0),
                    last_price: pos["last_price"].as_f64().unwrap_or(0.0),
                    pnl: pos["pnl"].as_f64().unwrap_or(0.0),
                    unrealised: pos["unrealised"].as_f64().unwrap_or(0.0),
                    realised: pos["realised"].as_f64().unwrap_or(0.0),
                    m2m: pos["m2m"].as_f64().unwrap_or(0.0),
                });
            }
        }
        
        Ok(positions)
    }
    
    /// Calculate margin requirements
    pub async fn calculate_margin(&self, positions: Vec<OrderRequest>) -> Result<MarginRequirement> {
        let url = format!("{}/margins/basket", self.base_url);
        
        let response = self.client
            .post(&url)
            .header("X-Kite-Version", "3")
            .header("Authorization", format!("token {}:{}", 
                self.config.api_key, self.config.access_token))
            .json(&positions)
            .send()
            .await?;
        
        let data: Value = response.json().await?;
        
        Ok(MarginRequirement {
            total: data["data"]["initial"]["total"].as_f64().unwrap_or(0.0),
            span: data["data"]["initial"]["span"].as_f64().unwrap_or(0.0),
            exposure: data["data"]["initial"]["exposure"].as_f64().unwrap_or(0.0),
            option_premium: data["data"]["initial"]["option_premium"].as_f64().unwrap_or(0.0),
            additional: data["data"]["initial"]["additional"].as_f64().unwrap_or(0.0),
            bo_freeze: data["data"]["initial"]["bo_freeze"].as_f64().unwrap_or(0.0),
        })
    }
}

// ============================================================================
// OPTION CHAIN STRUCTURE
// ============================================================================

#[derive(Debug, Clone)]
pub struct OptionChain {
    pub symbol: String,
    pub expiry: String,
    pub calls: Vec<OptionData>,
    pub puts: Vec<OptionData>,
    pub spot_price: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct OptionData {
    pub strike: f64,
    pub quote: OptionQuote,
    pub info: InstrumentInfo,
}

impl OptionChain {
    /// Find ATM (At The Money) strike
    pub fn get_atm_strike(&self) -> f64 {
        let strikes: Vec<f64> = self.calls.iter().map(|c| c.strike).collect();
        
        strikes.iter()
            .min_by_key(|&&strike| ((strike - self.spot_price).abs() * 100.0) as i64)
            .copied()
            .unwrap_or(self.spot_price)
    }
    
    /// Get ITM (In The Money) options
    pub fn get_itm_options(&self) -> (Vec<&OptionData>, Vec<&OptionData>) {
        let itm_calls: Vec<&OptionData> = self.calls.iter()
            .filter(|c| c.strike < self.spot_price)
            .collect();
        
        let itm_puts: Vec<&OptionData> = self.puts.iter()
            .filter(|p| p.strike > self.spot_price)
            .collect();
        
        (itm_calls, itm_puts)
    }
    
    /// Get OTM (Out of The Money) options
    pub fn get_otm_options(&self) -> (Vec<&OptionData>, Vec<&OptionData>) {
        let otm_calls: Vec<&OptionData> = self.calls.iter()
            .filter(|c| c.strike > self.spot_price)
            .collect();
        
        let otm_puts: Vec<&OptionData> = self.puts.iter()
            .filter(|p| p.strike < self.spot_price)
            .collect();
        
        (otm_calls, otm_puts)
    }
    
    /// Calculate Put-Call Ratio
    pub fn calculate_pcr(&self) -> PCRMetrics {
        let put_oi: u64 = self.puts.iter().map(|p| p.quote.open_interest).sum();
        let call_oi: u64 = self.calls.iter().map(|c| c.quote.open_interest).sum();
        
        let put_volume: u64 = self.puts.iter().map(|p| p.quote.volume).sum();
        let call_volume: u64 = self.calls.iter().map(|c| c.quote.volume).sum();
        
        PCRMetrics {
            oi_pcr: if call_oi > 0 { put_oi as f64 / call_oi as f64 } else { 0.0 },
            volume_pcr: if call_volume > 0 { put_volume as f64 / call_volume as f64 } else { 0.0 },
            interpretation: Self::interpret_pcr(put_oi as f64 / call_oi.max(1) as f64),
        }
    }
    
    fn interpret_pcr(pcr: f64) -> String {
        match pcr {
            x if x > 1.5 => "Extremely Bullish (Contrarian: Bearish)".to_string(),
            x if x > 1.2 => "Bullish".to_string(),
            x if x > 0.8 => "Neutral".to_string(),
            x if x > 0.5 => "Bearish".to_string(),
            _ => "Extremely Bearish (Contrarian: Bullish)".to_string(),
        }
    }
    
    /// Find max pain strike
    pub fn calculate_max_pain(&self) -> f64 {
        let mut pain_map: HashMap<i64, f64> = HashMap::new();
        
        let strikes: Vec<f64> = self.calls.iter().map(|c| c.strike).collect();
        
        for &expiry_price in &strikes {
            let mut total_pain = 0.0;
            
            // Calculate pain for calls
            for call in &self.calls {
                if expiry_price > call.strike {
                    total_pain += (expiry_price - call.strike) * call.quote.open_interest as f64;
                }
            }
            
            // Calculate pain for puts
            for put in &self.puts {
                if expiry_price < put.strike {
                    total_pain += (put.strike - expiry_price) * put.quote.open_interest as f64;
                }
            }
            
            pain_map.insert((expiry_price * 100.0) as i64, total_pain);
        }
        
        // Find strike with minimum pain
        pain_map.iter()
            .min_by_key(|(_, &pain)| (pain * 100.0) as i64)
            .map(|(&strike, _)| strike as f64 / 100.0)
            .unwrap_or(self.spot_price)
    }
}

#[derive(Debug, Clone)]
pub struct PCRMetrics {
    pub oi_pcr: f64,
    pub volume_pcr: f64,
    pub interpretation: String,
}

// ============================================================================
// STRATEGY EXECUTION
// ============================================================================

pub struct StrategyExecutor {
    client: Arc<ZerodhaOptionsClient>,
    risk_limits: RiskLimits,
}

#[derive(Debug, Clone)]
pub struct RiskLimits {
    pub max_position_size: u32,
    pub max_loss_per_trade: f64,
    pub max_daily_loss: f64,
    pub max_open_positions: usize,
    pub min_margin_buffer: f64,
}

impl Default for RiskLimits {
    fn default() -> Self {
        Self {
            max_position_size: 10, // lots
            max_loss_per_trade: 10000.0, // ₹10,000
            max_daily_loss: 25000.0, // ₹25,000
            max_open_positions: 5,
            min_margin_buffer: 50000.0, // ₹50,000
        }
    }
}

impl StrategyExecutor {
    pub fn new(client: Arc<ZerodhaOptionsClient>) -> Self {
        Self {
            client,
            risk_limits: RiskLimits::default(),
        }
    }
    
    /// Execute Iron Condor strategy
    pub async fn execute_iron_condor(
        &self,
        symbol: &str,
        expiry: &str,
        wing_width: f64,
        body_width: f64,
    ) -> Result<Vec<String>> {
        // Get option chain
        let chain = self.client.get_option_chain(OptionChainRequest {
            symbol: symbol.to_string(),
            expiry: expiry.to_string(),
            strike_range: (chain.spot_price - 500.0, chain.spot_price + 500.0),
        }).await?;
        
        let spot = chain.spot_price;
        let atm = chain.get_atm_strike();
        
        // Calculate strikes
        let put_short = atm - body_width / 2.0;
        let put_long = put_short - wing_width;
        let call_short = atm + body_width / 2.0;
        let call_long = call_short + wing_width;
        
        // Find the actual strikes from option chain
        let put_short_option = chain.puts.iter()
            .min_by_key(|p| ((p.strike - put_short).abs() * 100.0) as i64)
            .context("Put short strike not found")?;
            
        let put_long_option = chain.puts.iter()
            .min_by_key(|p| ((p.strike - put_long).abs() * 100.0) as i64)
            .context("Put long strike not found")?;
            
        let call_short_option = chain.calls.iter()
            .min_by_key(|c| ((c.strike - call_short).abs() * 100.0) as i64)
            .context("Call short strike not found")?;
            
        let call_long_option = chain.calls.iter()
            .min_by_key(|c| ((c.strike - call_long).abs() * 100.0) as i64)
            .context("Call long strike not found")?;
        
        // Create orders
        let orders = vec![
            OrderRequest {
                tradingsymbol: put_long_option.info.tradingsymbol.clone(),
                exchange: "NFO".to_string(),
                transaction_type: "BUY".to_string(),
                order_type: "LIMIT".to_string(),
                quantity: put_long_option.info.lot_size,
                product: "NRML".to_string(),
                validity: "DAY".to_string(),
                price: Some(put_long_option.quote.ask),
                trigger_price: None,
                tag: Some("IRON_CONDOR_PUT_LONG".to_string()),
            },
            OrderRequest {
                tradingsymbol: put_short_option.info.tradingsymbol.clone(),
                exchange: "NFO".to_string(),
                transaction_type: "SELL".to_string(),
                order_type: "LIMIT".to_string(),
                quantity: put_short_option.info.lot_size,
                product: "NRML".to_string(),
                validity: "DAY".to_string(),
                price: Some(put_short_option.quote.bid),
                trigger_price: None,
                tag: Some("IRON_CONDOR_PUT_SHORT".to_string()),
            },
            OrderRequest {
                tradingsymbol: call_short_option.info.tradingsymbol.clone(),
                exchange: "NFO".to_string(),
                transaction_type: "SELL".to_string(),
                order_type: "LIMIT".to_string(),
                quantity: call_short_option.info.lot_size,
                product: "NRML".to_string(),
                validity: "DAY".to_string(),
                price: Some(call_short_option.quote.bid),
                trigger_price: None,
                tag: Some("IRON_CONDOR_CALL_SHORT".to_string()),
            },
            OrderRequest {
                tradingsymbol: call_long_option.info.tradingsymbol.clone(),
                exchange: "NFO".to_string(),
                transaction_type: "BUY".to_string(),
                order_type: "LIMIT".to_string(),
                quantity: call_long_option.info.lot_size,
                product: "NRML".to_string(),
                validity: "DAY".to_string(),
                price: Some(call_long_option.quote.ask),
                trigger_price: None,
                tag: Some("IRON_CONDOR_CALL_LONG".to_string()),
            },
        ];
        
        // Check margin requirements
        let margin = self.client.calculate_margin(orders.clone()).await?;
        println!("💰 Margin Required: ₹{:.2}", margin.total);
        
        // Place orders
        let mut order_ids = Vec::new();
        for order in orders {
            println!("📝 Placing order: {} {} @ {:.2}", 
                order.transaction_type, order.tradingsymbol, order.price.unwrap_or(0.0));
            let order_id = self.client.place_order(order).await?;
            order_ids.push(order_id);
        }
        
        Ok(order_ids)
    }
}