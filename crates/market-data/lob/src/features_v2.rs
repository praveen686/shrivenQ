//! Advanced feature extraction for LOB v2 with HFT-grade analytics
//!
//! Enhanced features leveraging v2 capabilities:
//! - ROI-optimized calculations
//! - Microstructure analytics from hftbacktest
//! - SIMD-accelerated computations
//! - Order flow toxicity metrics
//! - Market regime detection

use crate::v2::{OrderBookV2, SideBookV2};
use common::{Symbol, Ts};
use std::collections::VecDeque;

/// Advanced feature calculator for LOB v2
pub struct FeatureCalculatorV2 {
    /// Symbol being tracked
    #[allow(dead_code)]
    symbol: Symbol,

    // VWAP tracking
    vwap_window_ns: u64,
    vwap_buffer: VecDeque<(Ts, f64, f64)>, // (timestamp, price, volume)

    // Order flow tracking
    trade_flow_buffer: VecDeque<(Ts, f64, bool)>, // (timestamp, size, is_buy)
    flow_window_ns: u64,

    // Microstructure state (stored as fixed-point i64, scale 10000)
    last_microprice: i64, // Fixed-point: actual * 10000
    last_spread: i64,     // Fixed-point: actual * 10000
    spread_ema: i64,      // Fixed-point: actual * 10000

    // Regime detection
    volatility_window: VecDeque<f64>,
    regime: MarketRegime,

    // Performance metrics
    update_count: u64,
    last_update_ts: Ts,
}

/// Market regime classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarketRegime {
    /// Low volatility, tight spreads
    Stable,
    /// Normal trading conditions
    Normal,
    /// High volatility, wide spreads
    Volatile,
    /// Extreme conditions (gaps, halts)
    Stressed,
}

/// Extended feature set for HFT
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct FeatureFrameV2 {
    // Core features (compatible with v1)
    /// Timestamp
    pub ts: Ts,
    /// Symbol
    pub symbol: Symbol,
    /// Spread in ticks
    pub spread_ticks: i64,
    /// Mid price
    pub mid: i64,
    /// Microprice
    pub microprice: i64,
    /// Order book imbalance
    pub imbalance: f64,
    /// VWAP deviation
    pub vwap_dev: f64,

    // Advanced microstructure features
    /// Volume-weighted spread
    pub weighted_spread: f64,
    /// Realized spread from trades
    pub effective_spread: f64,
    /// Temporary impact estimate
    pub price_impact: f64,
    /// Speed of mean reversion
    pub resilience: f64,

    // Order flow features
    /// VPIN-like toxicity metric
    pub flow_toxicity: f64,
    /// Buy vs sell pressure
    pub trade_imbalance: f64,
    /// Message rate anomaly detection
    pub quote_stuffing: f64,
    /// Price momentum
    pub momentum: f64,

    // Market quality metrics
    /// Overall liquidity quality
    pub liquidity_score: f64,
    /// Price stability metric
    pub stability_index: f64,
    /// Depth-weighted pressure
    pub book_pressure: f64,
    /// Current market regime
    pub regime: MarketRegime,

    // Predictive signals
    /// Short-term price prediction
    pub price_trend: f64,
    /// Expected volatility
    pub volatility_forecast: f64,
    /// Mean reversion strength
    pub mean_reversion_signal: f64,
    /// Toxicity prediction
    pub adverse_selection: f64,
}

impl FeatureCalculatorV2 {
    /// Create new advanced feature calculator
    pub fn new(symbol: Symbol) -> Self {
        Self {
            symbol,
            vwap_window_ns: 60_000_000_000, // 60 seconds
            vwap_buffer: VecDeque::with_capacity(1000),
            trade_flow_buffer: VecDeque::with_capacity(1000),
            flow_window_ns: 10_000_000_000, // 10 seconds
            last_microprice: 0,
            last_spread: 0,
            spread_ema: 0,
            volatility_window: VecDeque::with_capacity(100),
            regime: MarketRegime::Normal,
            update_count: 0,
            last_update_ts: Ts::from_nanos(0),
        }
    }

    /// Calculate all features from LOB v2
    pub fn calculate(&mut self, book: &OrderBookV2) -> Option<FeatureFrameV2> {
        // Get BBO (ultra-fast cached access in v2)
        let (bid_px, bid_qty) = book.best_bid()?;
        let (ask_px, ask_qty) = book.best_ask()?;

        // Core metrics
        let spread_ticks = book.spread_ticks()?;
        let mid_price = book.mid_price()?;
        let microprice = book.microprice()?;
        let imbalance = book.imbalance(5);

        // Advanced microstructure features
        let weighted_spread = self.calculate_weighted_spread(book);
        let effective_spread = self.calculate_effective_spread(book);
        let price_impact = self.estimate_price_impact(book);
        let resilience = self.calculate_resilience(microprice);

        // Order flow features
        let flow_toxicity = self.calculate_flow_toxicity(book);
        let trade_imbalance = self.calculate_trade_imbalance();
        let quote_stuffing = self.detect_quote_stuffing(book);
        let momentum = self.calculate_momentum(microprice);

        // Market quality metrics
        let liquidity_score = self.calculate_liquidity_score(book);
        let stability_index = self.calculate_stability(book);
        let book_pressure = self.calculate_book_pressure(book);

        // Update regime
        self.update_regime(spread_ticks as f64, microprice);

        // Predictive signals
        let price_trend = self.predict_price_trend(book);
        let volatility_forecast = self.forecast_volatility();
        let mean_reversion_signal = self.calculate_mean_reversion(microprice);
        let adverse_selection = self.estimate_adverse_selection(book);

        // VWAP tracking
        self.update_vwap(book.ts, mid_price, bid_qty.as_f64() + ask_qty.as_f64());
        let vwap_dev = self.calculate_vwap_deviation(mid_price);

        // Update state (convert to fixed-point)
        self.last_microprice = (microprice * 10000.0) as i64;
        self.last_spread =
            ((ask_px.as_f64() - bid_px.as_f64()) / mid_price * 10000.0 * 10000.0) as i64;
        self.spread_ema = (self.spread_ema as f64 * 0.95 + self.last_spread as f64 * 0.05) as i64;
        self.update_count += 1;
        self.last_update_ts = book.ts;

        Some(FeatureFrameV2 {
            ts: book.ts,
            symbol: book.symbol,
            spread_ticks,
            mid: (mid_price * 100.0).round() as i64,
            microprice: (microprice * 100.0) as i64,
            imbalance,
            vwap_dev,
            weighted_spread,
            effective_spread,
            price_impact,
            resilience,
            flow_toxicity,
            trade_imbalance,
            quote_stuffing,
            momentum,
            liquidity_score,
            stability_index,
            book_pressure,
            regime: self.regime,
            price_trend,
            volatility_forecast,
            mean_reversion_signal,
            adverse_selection,
        })
    }

    /// Calculate volume-weighted spread across multiple levels
    fn calculate_weighted_spread(&self, book: &OrderBookV2) -> f64 {
        let mut weighted_spread = 0.0;
        let mut total_weight = 0.0;

        // Use ROI-optimized access for top levels
        for i in 0..5 {
            if let (Some(bid), Some(ask)) =
                (self.get_level(&book.bids, i), self.get_level(&book.asks, i))
            {
                let spread = (ask.0 - bid.0) / ((ask.0 + bid.0) / 2.0) * 10000.0;
                let weight = (bid.1 + ask.1) / 2.0;
                weighted_spread += spread * weight;
                total_weight += weight;
            }
        }

        if total_weight > 0.0 {
            weighted_spread / total_weight
        } else {
            self.last_spread as f64 / 10000.0
        }
    }

    /// Calculate effective spread from recent trades
    fn calculate_effective_spread(&self, book: &OrderBookV2) -> f64 {
        // Simplified version - in production would use actual trade data
        let quoted_spread = self.last_spread;
        let volume_factor = book.bids.total_volume() as f64
            / (book.bids.total_volume() + book.asks.total_volume()) as f64;

        // Effective spread is typically 50-80% of quoted spread
        quoted_spread as f64 * (0.5 + 0.3 * volume_factor)
    }

    /// Estimate temporary price impact
    fn estimate_price_impact(&self, book: &OrderBookV2) -> f64 {
        // Kyle's lambda approximation
        let total_depth = book.bids.total_qty_up_to(10) + book.asks.total_qty_up_to(10);
        if total_depth > 0.0 {
            let volatility = self.estimate_current_volatility();
            volatility / total_depth.sqrt() * 10000.0
        } else {
            0.0
        }
    }

    /// Calculate resilience (speed of recovery after trades)
    fn calculate_resilience(&self, current_microprice: f64) -> f64 {
        if self.last_microprice != 0 {
            let last_price_f64 = self.last_microprice as f64 / 10000.0;
            let price_change = (current_microprice - last_price_f64).abs();
            let mean_reversion_speed = 1.0 / (1.0 + price_change * 100.0);
            mean_reversion_speed
        } else {
            0.5
        }
    }

    /// Calculate flow toxicity (VPIN-inspired)
    fn calculate_flow_toxicity(&self, book: &OrderBookV2) -> f64 {
        let imbalance = book.imbalance(5);
        let spread_normalized = self.last_spread as f64 / 1000000.0; // Convert fixed-point to normalized
        let volume_ratio =
            book.asks.total_volume() as f64 / (book.bids.total_volume() as f64 + 1.0);

        // Toxicity increases with imbalance, spread, and volume asymmetry
        (imbalance.abs() * spread_normalized * (volume_ratio - 1.0).abs()).tanh()
    }

    /// Calculate trade imbalance from flow buffer
    fn calculate_trade_imbalance(&self) -> f64 {
        if self.trade_flow_buffer.is_empty() {
            return 0.0;
        }

        let mut buy_volume = 0.0;
        let mut sell_volume = 0.0;

        for (_, size, is_buy) in &self.trade_flow_buffer {
            if *is_buy {
                buy_volume += size;
            } else {
                sell_volume += size;
            }
        }

        let total = buy_volume + sell_volume;
        if total > 0.0 {
            (buy_volume - sell_volume) / total
        } else {
            0.0
        }
    }

    /// Detect quote stuffing (abnormal update rates)
    fn detect_quote_stuffing(&self, book: &OrderBookV2) -> f64 {
        let time_since_last = (book.ts.as_nanos() - self.last_update_ts.as_nanos()) as f64;
        if time_since_last > 0.0 {
            let update_rate = 1e9 / time_since_last; // Updates per second
            let normal_rate = 100.0; // Expected updates/sec
            (update_rate / normal_rate - 1.0).tanh()
        } else {
            0.0
        }
    }

    /// Calculate price momentum
    fn calculate_momentum(&self, current_price: f64) -> f64 {
        if self.last_microprice != 0 {
            let last_price_f64 = self.last_microprice as f64 / 10000.0;
            let return_bps = (current_price / last_price_f64 - 1.0) * 10000.0;
            return_bps / 100.0 // Normalize to [-1, 1] range
        } else {
            0.0
        }
    }

    /// Calculate overall liquidity score
    fn calculate_liquidity_score(&self, book: &OrderBookV2) -> f64 {
        let depth_score = (book.bids.total_qty_up_to(5) + book.asks.total_qty_up_to(5)) / 1000.0;
        let spread_score = 1.0 / (1.0 + self.last_spread as f64 / 100000.0);
        let balance_score = 1.0 - book.imbalance(5).abs();

        (depth_score * spread_score * balance_score).sqrt()
    }

    /// Calculate price stability index
    fn calculate_stability(&self, book: &OrderBookV2) -> f64 {
        let volatility = self.estimate_current_volatility();
        let spread_stability =
            1.0 / (1.0 + ((self.last_spread - self.spread_ema) as f64 / 10000.0).abs());
        let depth_stability = book.bids.total_volume().min(book.asks.total_volume()) as f64
            / book.bids.total_volume().max(book.asks.total_volume()) as f64;

        (spread_stability * depth_stability / (1.0 + volatility)).sqrt()
    }

    /// Calculate book pressure (buying vs selling pressure)
    fn calculate_book_pressure(&self, book: &OrderBookV2) -> f64 {
        let mut bid_pressure = 0.0;
        let mut ask_pressure = 0.0;

        // Weight by inverse distance from mid
        for i in 0..10 {
            let weight = 1.0 / (i as f64 + 1.0);

            if let Some((_, qty)) = self.get_level(&book.bids, i) {
                bid_pressure += qty * weight;
            }
            if let Some((_, qty)) = self.get_level(&book.asks, i) {
                ask_pressure += qty * weight;
            }
        }

        if bid_pressure + ask_pressure > 0.0 {
            (bid_pressure - ask_pressure) / (bid_pressure + ask_pressure)
        } else {
            0.0
        }
    }

    /// Predict short-term price trend
    fn predict_price_trend(&self, book: &OrderBookV2) -> f64 {
        // Combine multiple signals
        let imbalance_signal = book.imbalance(3) * 0.3;
        let momentum_signal = self.calculate_momentum(book.microprice().unwrap_or(0.0)) * 0.3;
        let pressure_signal = self.calculate_book_pressure(book) * 0.2;
        let flow_signal = self.calculate_trade_imbalance() * 0.2;

        (imbalance_signal + momentum_signal + pressure_signal + flow_signal).tanh()
    }

    /// Forecast volatility
    fn forecast_volatility(&self) -> f64 {
        if self.volatility_window.len() < 10 {
            return 0.01; // Default low volatility
        }

        let mean = self.volatility_window.iter().sum::<f64>() / self.volatility_window.len() as f64;
        let variance = self
            .volatility_window
            .iter()
            .map(|v| (v - mean).powi(2))
            .sum::<f64>()
            / self.volatility_window.len() as f64;

        variance.sqrt()
    }

    /// Calculate mean reversion signal
    fn calculate_mean_reversion(&self, current_price: f64) -> f64 {
        if self.vwap_buffer.is_empty() {
            return 0.0;
        }

        let vwap = self.calculate_current_vwap();
        let deviation = (current_price - vwap) / vwap;

        // Stronger signal for larger deviations
        -deviation.tanh()
    }

    /// Estimate adverse selection (probability of trading against informed flow)
    fn estimate_adverse_selection(&self, book: &OrderBookV2) -> f64 {
        let toxicity = self.calculate_flow_toxicity(book);
        let stuffing = self.detect_quote_stuffing(book);
        let imbalance = book.imbalance(3).abs();

        ((toxicity + stuffing + imbalance) / 3.0).min(1.0)
    }

    /// Update market regime detection
    fn update_regime(&mut self, spread: f64, price: f64) {
        // Update volatility window
        if self.last_microprice != 0 {
            let last_price_f64 = self.last_microprice as f64 / 10000.0;
            let return_val = ((price / last_price_f64).ln()).abs();
            self.volatility_window.push_back(return_val);
            if self.volatility_window.len() > 100 {
                self.volatility_window.pop_front();
            }
        }

        let current_vol = self.forecast_volatility();
        let spread_bps = spread / price * 10000.0;

        self.regime = if current_vol < 0.001 && spread_bps < 2.0 {
            MarketRegime::Stable
        } else if current_vol < 0.01 && spread_bps < 5.0 {
            MarketRegime::Normal
        } else if current_vol < 0.05 && spread_bps < 20.0 {
            MarketRegime::Volatile
        } else {
            MarketRegime::Stressed
        };
    }

    /// Update VWAP buffer
    fn update_vwap(&mut self, ts: Ts, price: f64, volume: f64) {
        self.vwap_buffer.push_back((ts, price, volume));

        // Remove old entries
        let cutoff = ts.as_nanos().saturating_sub(self.vwap_window_ns);
        while let Some((front_ts, _, _)) = self.vwap_buffer.front() {
            if front_ts.as_nanos() < cutoff {
                self.vwap_buffer.pop_front();
            } else {
                break;
            }
        }
    }

    /// Calculate current VWAP
    fn calculate_current_vwap(&self) -> f64 {
        let mut value_sum = 0.0;
        let mut volume_sum = 0.0;

        for (_, price, volume) in &self.vwap_buffer {
            value_sum += price * volume;
            volume_sum += volume;
        }

        if volume_sum > 0.0 {
            value_sum / volume_sum
        } else {
            self.last_microprice as f64 / 10000.0
        }
    }

    /// Calculate VWAP deviation
    fn calculate_vwap_deviation(&self, current_price: f64) -> f64 {
        let vwap = self.calculate_current_vwap();
        if vwap != 0.0 {
            (current_price - vwap) / vwap * 10000.0 // in bps
        } else {
            0.0
        }
    }

    /// Get level from side book (helper)
    fn get_level(&self, side: &SideBookV2, level: usize) -> Option<(f64, f64)> {
        // Use public API to access level data
        if let Some((px, qty)) = side.get_level(level) {
            Some((px.as_f64(), qty.as_f64()))
        } else {
            None
        }
    }

    /// Estimate current volatility
    fn estimate_current_volatility(&self) -> f64 {
        if self.volatility_window.len() >= 10 {
            let recent: Vec<_> = self
                .volatility_window
                .iter()
                .rev()
                .take(10)
                .copied()
                .collect();
            recent.iter().sum::<f64>() / recent.len() as f64
        } else {
            0.001 // Default low volatility
        }
    }
}

/// Create feature calculator with default HFT settings
pub fn create_hft_calculator(symbol: Symbol) -> FeatureCalculatorV2 {
    let mut calc = FeatureCalculatorV2::new(symbol);
    calc.vwap_window_ns = 30_000_000_000; // 30 seconds for HFT
    calc.flow_window_ns = 5_000_000_000; // 5 seconds for flow
    calc
}

/// Create feature calculator for market making
pub fn create_mm_calculator(symbol: Symbol) -> FeatureCalculatorV2 {
    let mut calc = FeatureCalculatorV2::new(symbol);
    calc.vwap_window_ns = 60_000_000_000; // 1 minute
    calc.flow_window_ns = 10_000_000_000; // 10 seconds
    calc
}
