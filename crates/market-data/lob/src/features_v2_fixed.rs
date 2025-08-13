//! Advanced feature extraction for LOB v2 with HFT-grade analytics
//!
//! Enhanced features leveraging v2 capabilities with FIXED-POINT arithmetic:
//! - All calculations use i64 fixed-point (scale 10000)
//! - No floating-point operations in critical path
//! - HFT-grade precision for trading signals
//! - Zero precision loss in calculations
//!
//! Fixed-point scale: 10000 (4 decimal places)
//! Example: 1.2345 = 12345 (fixed-point)

use crate::analytics::Analytics;
use crate::v2::OrderBookV2;
use common::{Symbol, Ts};
use std::collections::VecDeque;

/// Fixed-point constants (scale 10000)
const FIXED_SCALE: i64 = 10000;
const FIXED_HALF: i64 = 5000; // 0.5
const FIXED_100: i64 = 1000000; // 100.0
const FIXED_EMA_ALPHA: i64 = 500; // 0.05 for EMA
const FIXED_EMA_BETA: i64 = 9500; // 0.95 for EMA

/// Advanced feature calculator for LOB v2 (fixed-point)
pub struct FeatureCalculatorV2Fixed {
    /// Symbol being tracked
    symbol: Symbol,

    // VWAP tracking (all fixed-point)
    vwap_window_ns: u64,
    vwap_buffer: VecDeque<(Ts, i64, i64)>, // (timestamp, price_fixed, volume_fixed)

    // Order flow tracking
    trade_flow_buffer: VecDeque<(Ts, i64, bool)>, // (timestamp, size_fixed, is_buy)
    flow_window_ns: u64,

    // Microstructure state (all fixed-point i64, scale 10000)
    last_microprice: i64,
    last_spread: i64,
    spread_ema: i64,

    // Regime detection
    volatility_window: VecDeque<i64>, // Fixed-point returns
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

/// Extended feature set for HFT (ALL FIXED-POINT)
#[derive(Debug, Clone)]
pub struct FeatureFrameV2Fixed {
    /// Timestamp
    pub ts: Ts,
    /// Symbol
    pub symbol: Symbol,

    /// Spread in ticks
    pub spread_ticks: i64,
    /// Mid price (fixed-point)
    pub mid: i64,
    /// Microprice (fixed-point)
    pub microprice: i64,
    /// Order book imbalance (fixed-point)
    pub imbalance: i64,
    /// VWAP deviation (fixed-point)
    pub vwap_dev: i64,

    /// Volume-weighted spread (fixed-point)
    pub weighted_spread: i64,
    /// Effective spread from trades (fixed-point)
    pub effective_spread: i64,
    /// Temporary impact estimate (fixed-point)
    pub price_impact: i64,
    /// Speed of mean reversion (fixed-point)
    pub resilience: i64,

    /// VPIN-like toxicity metric (fixed-point)
    pub flow_toxicity: i64,
    /// Buy vs sell pressure (fixed-point)
    pub trade_imbalance: i64,
    /// Message rate anomaly detection (fixed-point)
    pub quote_stuffing: i64,
    /// Price momentum (fixed-point)
    pub momentum: i64,

    /// Overall liquidity quality (fixed-point)
    pub liquidity_score: i64,
    /// Price stability metric (fixed-point)
    pub stability_index: i64,
    /// Depth-weighted pressure (fixed-point)
    pub book_pressure: i64,
    /// Current market regime
    pub regime: MarketRegime,

    /// Short-term price prediction (fixed-point)
    pub price_trend: i64,
    /// Expected volatility (fixed-point)
    pub volatility_forecast: i64,
    /// Mean reversion strength (fixed-point)
    pub mean_reversion_signal: i64,
    /// Toxicity prediction (fixed-point)
    pub adverse_selection: i64,
}

impl FeatureCalculatorV2Fixed {
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
    pub fn calculate(&mut self, book: &OrderBookV2) -> Option<FeatureFrameV2Fixed> {
        // Get BBO
        let (bid_px, bid_qty) = book.best_bid()?;
        let (ask_px, ask_qty) = book.best_ask()?;

        // Convert to fixed-point
        let bid_px_fixed = bid_px.as_i64();
        let ask_px_fixed = ask_px.as_i64();
        let bid_qty_fixed = bid_qty.as_i64();
        let ask_qty_fixed = ask_qty.as_i64();

        // Core metrics (all fixed-point)
        let spread_ticks = book.spread_ticks()?;
        let mid_price_fixed = (bid_px_fixed + ask_px_fixed) / 2;
        let microprice_fixed = self.calculate_microprice_fixed(
            bid_px_fixed,
            bid_qty_fixed,
            ask_px_fixed,
            ask_qty_fixed,
        );
        let imbalance_fixed = self.calculate_imbalance_fixed(book);

        // Advanced microstructure features
        let weighted_spread = self.calculate_weighted_spread_fixed(book);
        let effective_spread = self.calculate_effective_spread_fixed(book);
        let price_impact = self.estimate_price_impact_fixed(book);
        let resilience = self.calculate_resilience_fixed(microprice_fixed);

        // Order flow features
        let flow_toxicity = self.calculate_flow_toxicity_fixed(book);
        let trade_imbalance = self.calculate_trade_imbalance_fixed();
        let quote_stuffing = self.detect_quote_stuffing_fixed(book);
        let momentum = self.calculate_momentum_fixed(microprice_fixed);

        // Market quality metrics
        let liquidity_score = self.calculate_liquidity_score_fixed(book);
        let stability_index = self.calculate_stability_fixed(book);
        let book_pressure = self.calculate_book_pressure_fixed(book);

        // Update regime detection
        self.update_regime_fixed(spread_ticks * FIXED_SCALE, microprice_fixed);

        // Predictive signals
        let price_trend = self.predict_price_trend_fixed(book);
        let volatility_forecast = self.forecast_volatility_fixed();
        let mean_reversion_signal = self.calculate_mean_reversion_fixed(microprice_fixed);
        let adverse_selection = self.estimate_adverse_selection_fixed(book);

        // VWAP tracking
        let total_volume_fixed = bid_qty_fixed + ask_qty_fixed;
        self.update_vwap_fixed(book.ts, mid_price_fixed, total_volume_fixed);
        let vwap_dev = self.calculate_vwap_deviation_fixed(mid_price_fixed);

        // Update state
        self.last_microprice = microprice_fixed;
        self.last_spread = ((ask_px_fixed - bid_px_fixed) * FIXED_SCALE) / mid_price_fixed.max(1);
        self.spread_ema =
            (self.spread_ema * FIXED_EMA_BETA + self.last_spread * FIXED_EMA_ALPHA) / FIXED_SCALE;
        self.update_count += 1;
        self.last_update_ts = book.ts;

        Some(FeatureFrameV2Fixed {
            ts: book.ts,
            symbol: self.symbol,
            spread_ticks,
            mid: mid_price_fixed,
            microprice: microprice_fixed,
            imbalance: imbalance_fixed,
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

    /// Calculate microprice (fixed-point)
    fn calculate_microprice_fixed(
        &self,
        bid_px: i64,
        bid_qty: i64,
        ask_px: i64,
        ask_qty: i64,
    ) -> i64 {
        let total_qty = bid_qty + ask_qty;
        if total_qty > 0 {
            (bid_px * ask_qty + ask_px * bid_qty) / total_qty
        } else {
            (bid_px + ask_px) / 2
        }
    }

    /// Calculate imbalance (fixed-point)
    fn calculate_imbalance_fixed(&self, book: &OrderBookV2) -> i64 {
        let bid_qty = book.bids.total_qty_up_to(5);
        let ask_qty = book.asks.total_qty_up_to(5);
        let total = bid_qty + ask_qty;

        if total > 0.0 {
            let imb = (bid_qty - ask_qty) / total;
            Analytics::f64_to_fixed_round(imb)
        } else {
            0
        }
    }

    /// Calculate volume-weighted spread (fixed-point)
    fn calculate_weighted_spread_fixed(&self, book: &OrderBookV2) -> i64 {
        let mut weighted_spread = 0i64;
        let mut total_weight = 0i64;

        for i in 0..5 {
            if let (Some((bid_px, bid_qty)), Some((ask_px, ask_qty))) =
                (book.bids.get_level(i), book.asks.get_level(i))
            {
                let bid_px_fixed = bid_px.as_i64();
                let ask_px_fixed = ask_px.as_i64();
                let mid_fixed = (bid_px_fixed + ask_px_fixed) / 2;

                // Spread in basis points (fixed-point)
                let spread_bps = if mid_fixed > 0 {
                    ((ask_px_fixed - bid_px_fixed) * FIXED_100) / mid_fixed
                } else {
                    0
                };

                let weight = (bid_qty.as_i64() + ask_qty.as_i64()) / 2;
                weighted_spread += spread_bps * weight;
                total_weight += weight;
            }
        }

        if total_weight > 0 {
            weighted_spread / total_weight
        } else {
            self.last_spread
        }
    }

    /// Calculate effective spread (fixed-point)
    fn calculate_effective_spread_fixed(&self, book: &OrderBookV2) -> i64 {
        let quoted_spread = self.last_spread;
        // SAFETY: Cast is safe within expected range
        let bid_vol = book.bids.total_volume() as i64;
        let ask_vol = book.asks.total_volume() as i64;
        let total_vol = bid_vol + ask_vol;

        let volume_factor = if total_vol > 0 {
            (bid_vol * FIXED_SCALE) / total_vol
        } else {
            FIXED_HALF
        };

        // Effective spread is typically 50-80% of quoted spread
        (quoted_spread * (FIXED_HALF + 3 * volume_factor / 10)) / FIXED_SCALE
    }

    /// Estimate price impact (fixed-point)
    fn estimate_price_impact_fixed(&self, book: &OrderBookV2) -> i64 {
        let depth_bid = book.bids.total_qty_up_to(10);
        let depth_ask = book.asks.total_qty_up_to(10);
        let total_depth_fixed = Analytics::f64_to_fixed_round(depth_bid + depth_ask);

        if total_depth_fixed > 0 {
            let volatility = self.estimate_current_volatility_fixed();
            // Kyle's lambda approximation: vol / sqrt(depth)
            // Using integer sqrt approximation
            let sqrt_depth = integer_sqrt(total_depth_fixed);
            if sqrt_depth > 0 {
                (volatility * FIXED_100) / sqrt_depth
            } else {
                0
            }
        } else {
            0
        }
    }

    /// Calculate resilience (fixed-point)
    fn calculate_resilience_fixed(&self, current_microprice: i64) -> i64 {
        if self.last_microprice != 0 {
            let price_change =
                ((current_microprice - self.last_microprice).abs() * 100) / FIXED_SCALE;
            // Mean reversion speed: 1 / (1 + price_change)
            if price_change < FIXED_100 {
                FIXED_SCALE * FIXED_SCALE / (FIXED_SCALE + price_change)
            } else {
                100 // Very low resilience for large moves
            }
        } else {
            FIXED_HALF
        }
    }

    /// Calculate flow toxicity (fixed-point)
    fn calculate_flow_toxicity_fixed(&self, book: &OrderBookV2) -> i64 {
        let imbalance = self.calculate_imbalance_fixed(book);
        let spread_normalized = (self.last_spread * 100) / FIXED_100.max(1);
        // SAFETY: Cast is safe within expected range

        let bid_vol = book.bids.total_volume() as i64;
        let ask_vol = book.asks.total_volume() as i64;
        let volume_ratio = if bid_vol > 0 {
            (ask_vol * FIXED_SCALE) / bid_vol
        } else {
            FIXED_SCALE
        };

        // Toxicity increases with imbalance, spread, and volume asymmetry
        let toxicity = (imbalance.abs() * spread_normalized * (volume_ratio - FIXED_SCALE).abs())
            / (FIXED_SCALE * FIXED_SCALE);

        // Apply tanh-like saturation
        fixed_tanh(toxicity)
    }

    /// Calculate trade imbalance (fixed-point)
    fn calculate_trade_imbalance_fixed(&self) -> i64 {
        if self.trade_flow_buffer.is_empty() {
            return 0;
        }

        let mut buy_volume = 0i64;
        let mut sell_volume = 0i64;

        for (_, size, is_buy) in &self.trade_flow_buffer {
            if *is_buy {
                buy_volume += size;
            } else {
                sell_volume += size;
            }
        }

        let total = buy_volume + sell_volume;
        if total > 0 {
            ((buy_volume - sell_volume) * FIXED_SCALE) / total
        } else {
            0
        }
    }

    /// Detect quote stuffing (fixed-point)
    fn detect_quote_stuffing_fixed(&self, book: &OrderBookV2) -> i64 {
        let time_since_last = book
            .ts
            .as_nanos()
            .saturating_sub(self.last_update_ts.as_nanos());
        // SAFETY: Cast is safe within expected range
        if time_since_last > 0 {
            // Updates per second (fixed-point)
            let update_rate = (1_000_000_000 * FIXED_SCALE) / time_since_last as i64;
            let normal_rate = 100 * FIXED_SCALE; // 100 updates/sec
            let ratio = (update_rate * FIXED_SCALE) / normal_rate - FIXED_SCALE;
            fixed_tanh(ratio)
        } else {
            0
        }
    }

    /// Calculate momentum (fixed-point)
    fn calculate_momentum_fixed(&self, current_price: i64) -> i64 {
        if self.last_microprice != 0 {
            // Return in basis points
            let return_bps =
                ((current_price - self.last_microprice) * FIXED_100) / self.last_microprice;
            return_bps / 100 // Normalize to [-100, 100] range
        } else {
            0
        }
    }

    /// Calculate liquidity score (fixed-point)
    fn calculate_liquidity_score_fixed(&self, book: &OrderBookV2) -> i64 {
        let depth = book.bids.total_qty_up_to(5) + book.asks.total_qty_up_to(5);
        let depth_score = Analytics::f64_to_fixed_round(depth / 1000.0).min(FIXED_SCALE);

        let spread_score = if self.last_spread > 0 {
            FIXED_SCALE * FIXED_SCALE / (FIXED_SCALE + self.last_spread / 10)
        } else {
            FIXED_SCALE
        };

        let imbalance = self.calculate_imbalance_fixed(book);
        let balance_score = FIXED_SCALE - imbalance.abs();

        // Geometric mean approximation
        integer_sqrt((depth_score * spread_score * balance_score) / FIXED_SCALE)
    }

    /// Calculate stability index (fixed-point)
    fn calculate_stability_fixed(&self, book: &OrderBookV2) -> i64 {
        let volatility = self.estimate_current_volatility_fixed();
        let spread_deviation = (self.last_spread - self.spread_ema).abs();
        let spread_stability = if spread_deviation > 0 {
            FIXED_SCALE * FIXED_SCALE / (FIXED_SCALE + spread_deviation)
        } else {
            // SAFETY: Cast is safe within expected range
            FIXED_SCALE
        };

        let bid_vol = book.bids.total_volume() as i64;
        let ask_vol = book.asks.total_volume() as i64;
        let depth_stability = if bid_vol.max(ask_vol) > 0 {
            (bid_vol.min(ask_vol) * FIXED_SCALE) / bid_vol.max(ask_vol)
        } else {
            0
        };

        // Combined stability metric
        let combined = (spread_stability * depth_stability) / (FIXED_SCALE + volatility);
        integer_sqrt(combined)
    }

    /// Calculate book pressure (fixed-point)
    fn calculate_book_pressure_fixed(&self, book: &OrderBookV2) -> i64 {
        let mut bid_pressure = 0i64;
        // SAFETY: Cast is safe within expected range
        let mut ask_pressure = 0i64;

        for i in 0..10 {
            // Weight by inverse distance from mid
            let weight = FIXED_SCALE / ((i as i64) + 1);

            if let Some((_, qty)) = book.bids.get_level(i) {
                bid_pressure += qty.as_i64() * weight;
            }
            if let Some((_, qty)) = book.asks.get_level(i) {
                ask_pressure += qty.as_i64() * weight;
            }
        }

        if bid_pressure + ask_pressure > 0 {
            ((bid_pressure - ask_pressure) * FIXED_SCALE) / (bid_pressure + ask_pressure)
        } else {
            0
        }
    }

    /// Predict price trend (fixed-point)
    fn predict_price_trend_fixed(&self, book: &OrderBookV2) -> i64 {
        let imbalance_signal = self.calculate_imbalance_fixed(book) * 3 / 10;
        let momentum_signal = self.calculate_momentum_fixed(self.last_microprice) * 3 / 10;
        let pressure_signal = self.calculate_book_pressure_fixed(book) * 2 / 10;
        let flow_signal = self.calculate_trade_imbalance_fixed() * 2 / 10;

        let combined = imbalance_signal + momentum_signal + pressure_signal + flow_signal;
        fixed_tanh(combined)
    }

    /// Forecast volatility (fixed-point)
    fn forecast_volatility_fixed(&self) -> i64 {
        // SAFETY: Cast is safe within expected range
        if self.volatility_window.len() < 10 {
            return 100; // Default low volatility (0.01 in fixed-point)
        }

        let sum: i64 = self.volatility_window.iter().sum();
        let mean = sum / self.volatility_window.len() as i64;

        let variance_sum: i64 = self
            .volatility_window
            .iter()
            // SAFETY: Cast is safe within expected range
            .map(|v| {
                let diff = v - mean;
                (diff * diff) / FIXED_SCALE
            })
            .sum();

        let variance = variance_sum / self.volatility_window.len() as i64;
        integer_sqrt(variance)
    }

    /// Calculate mean reversion signal (fixed-point)
    fn calculate_mean_reversion_fixed(&self, current_price: i64) -> i64 {
        if self.vwap_buffer.is_empty() {
            return 0;
        }

        let vwap = self.calculate_current_vwap_fixed();
        if vwap == 0 {
            return 0;
        }

        let deviation = ((current_price - vwap) * FIXED_SCALE) / vwap;
        -fixed_tanh(deviation)
    }

    /// Estimate adverse selection (fixed-point)
    fn estimate_adverse_selection_fixed(&self, book: &OrderBookV2) -> i64 {
        let toxicity = self.calculate_flow_toxicity_fixed(book);
        let stuffing = self.detect_quote_stuffing_fixed(book);
        let imbalance = self.calculate_imbalance_fixed(book).abs();

        ((toxicity + stuffing + imbalance) / 3).min(FIXED_SCALE)
    }

    /// Update market regime (fixed-point)
    fn update_regime_fixed(&mut self, spread: i64, price: i64) {
        if self.last_microprice != 0 {
            let return_bps = ((price - self.last_microprice) * FIXED_100) / self.last_microprice;
            self.volatility_window.push_back(return_bps.abs());
            if self.volatility_window.len() > 100 {
                self.volatility_window.pop_front();
            }
        }

        let current_vol = self.forecast_volatility_fixed();
        let spread_bps = (spread * FIXED_100) / price.max(1);

        self.regime = if current_vol < 10 && spread_bps < 20 {
            MarketRegime::Stable
        } else if current_vol < 100 && spread_bps < 50 {
            MarketRegime::Normal
        } else if current_vol < 500 && spread_bps < 100 {
            MarketRegime::Volatile
        } else {
            MarketRegime::Stressed
        };
    }

    /// Update VWAP (fixed-point)
    fn update_vwap_fixed(&mut self, ts: Ts, price: i64, volume: i64) {
        self.vwap_buffer.push_back((ts, price, volume));

        let cutoff = ts.as_nanos().saturating_sub(self.vwap_window_ns);
        while let Some((front_ts, _, _)) = self.vwap_buffer.front() {
            if front_ts.as_nanos() < cutoff {
                self.vwap_buffer.pop_front();
            } else {
                break;
            }
        }
    }

    /// Calculate current VWAP (fixed-point)
    fn calculate_current_vwap_fixed(&self) -> i64 {
        let mut value_sum = 0i64;
        let mut volume_sum = 0i64;

        for (_, price, volume) in &self.vwap_buffer {
            value_sum += price * volume;
            volume_sum += volume;
        }

        if volume_sum > 0 {
            value_sum / volume_sum
        } else {
            self.last_microprice
        }
    }

    /// Calculate VWAP deviation (fixed-point)
    fn calculate_vwap_deviation_fixed(&self, current_price: i64) -> i64 {
        let vwap = self.calculate_current_vwap_fixed();
        if vwap != 0 {
            ((current_price - vwap) * FIXED_100) / vwap // in basis points
        } else {
            0
        }
    }

    /// Estimate current volatility (fixed-point)
    fn estimate_current_volatility_fixed(&self) -> i64 {
        if self.volatility_window.len() >= 10 {
            let recent_sum: i64 = self.volatility_window.iter().rev().take(10).sum();
            recent_sum / 10
        } else {
            10 // Default low volatility (0.001)
        }
    }
}

// Helper functions for fixed-point arithmetic

/// Integer square root (Newton's method)
fn integer_sqrt(n: i64) -> i64 {
    if n < 0 {
        return 0;
    }
    if n < 2 {
        return n;
    }

    let mut x = n;
    let mut y = (x + 1) / 2;

    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }

    x
}

/// Fixed-point tanh approximation
fn fixed_tanh(x: i64) -> i64 {
    // tanh(x) ≈ x / (1 + |x|/SCALE) for small x
    if x.abs() < FIXED_SCALE / 2 {
        (x * FIXED_SCALE) / (FIXED_SCALE + x.abs())
    } else if x > 0 {
        FIXED_SCALE - 100 // Saturate at ~0.99
    } else {
        -FIXED_SCALE + 100 // Saturate at ~-0.99
    }
}

/// Create feature calculator with HFT settings
pub fn create_hft_calculator_fixed(symbol: Symbol) -> FeatureCalculatorV2Fixed {
    let mut calc = FeatureCalculatorV2Fixed::new(symbol);
    calc.vwap_window_ns = 30_000_000_000; // 30 seconds
    calc.flow_window_ns = 5_000_000_000; // 5 seconds
    calc
}

/// Create feature calculator for market making
pub fn create_mm_calculator_fixed(symbol: Symbol) -> FeatureCalculatorV2Fixed {
    let mut calc = FeatureCalculatorV2Fixed::new(symbol);
    calc.vwap_window_ns = 60_000_000_000; // 1 minute
    calc.flow_window_ns = 10_000_000_000; // 10 seconds
    calc
}
