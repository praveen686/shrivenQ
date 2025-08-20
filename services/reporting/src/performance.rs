//! Performance analytics and reporting
//!
//! Advanced performance metrics with statistical analysis

use services_common::{Px, Qty, Symbol, Ts};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Performance analyzer for detailed trading analytics
pub struct PerformanceAnalyzer {
    /// Trade history buffer
    trade_history: VecDeque<TradeRecord>,
    /// Daily `PnL` tracking
    daily_pnl: VecDeque<DailyPnL>,
    /// Market price history by symbol
    price_history: FxHashMap<Symbol, VecDeque<PriceRecord>>,
    /// Buffer capacity
    capacity: usize,
}

impl std::fmt::Debug for PerformanceAnalyzer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PerformanceAnalyzer")
            .field("trade_history_len", &self.trade_history.len())
            .field("daily_pnl_len", &self.daily_pnl.len())
            .field("price_history_symbols", &self.price_history.len())
            .field("capacity", &self.capacity)
            .finish()
    }
}

/// Individual trade record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    /// Timestamp when the trade was executed (nanoseconds)
    pub timestamp: u64,
    /// Symbol that was traded
    pub symbol: Symbol,
    /// Quantity traded (fixed-point, negative for sells)
    pub quantity: i64, // Fixed-point
    /// Price at which the trade was executed (fixed-point)
    pub price: i64,    // Fixed-point
    /// Total volume of the trade (quantity * price, fixed-point)
    pub volume: i64,   // Fixed-point (quantity * price)
    /// Side of the trade (buy or sell)
    pub side: TradeSide,
}

/// Trade side enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeSide {
    /// Buy side trade (long position)
    Buy,
    /// Sell side trade (short position)
    Sell,
}

/// Daily `PnL` record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyPnL {
    /// Date in YYYY-MM-DD format
    pub date: String, // YYYY-MM-DD format
    /// Realized profit and loss for the day
    pub realized_pnl: i64,
    /// Unrealized profit and loss for the day
    pub unrealized_pnl: i64,
    /// Total profit and loss for the day
    pub total_pnl: i64,
    /// Number of trades executed during the day
    pub trades_count: u32,
    /// Total trading volume for the day
    pub volume: u64,
}

/// Price record for market analysis
#[derive(Debug, Clone)]
pub struct PriceRecord {
    /// Timestamp when the price was recorded (nanoseconds)
    pub timestamp: u64,
    /// Best bid price (fixed-point)
    pub bid: i64,
    /// Best ask price (fixed-point)
    pub ask: i64,
    /// Mid price calculated from bid and ask (fixed-point)
    pub mid: i64,
}

/// Comprehensive performance report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceReport {
    /// Report generation timestamp (nanoseconds)
    pub timestamp: u64,

    /// Basic metrics
    /// Total number of trades in the analysis period
    pub total_trades: u32,
    /// Number of trading days in the analysis period
    pub trading_days: u32,
    /// Average number of trades executed per day
    pub avg_trades_per_day: f64,

    /// `PnL` metrics
    /// Total profit and loss over the analysis period
    pub total_pnl: i64,
    /// Profit and loss for the current day
    pub daily_pnl: i64,
    /// Realized profit and loss from closed positions
    pub realized_pnl: i64,
    /// Unrealized profit and loss from open positions
    pub unrealized_pnl: i64,

    /// Performance ratios
    /// Risk-adjusted return measure (Sharpe ratio)
    pub sharpe_ratio: f64,
    /// Downside risk-adjusted return measure (Sortino ratio)
    pub sortino_ratio: f64,
    /// Return to maximum drawdown ratio (Calmar ratio)
    pub calmar_ratio: f64,
    /// Ratio of gross profit to gross loss
    pub profit_factor: f64,

    /// Risk metrics
    /// Maximum peak-to-trough decline in portfolio value
    pub max_drawdown: i64,
    /// Maximum drawdown expressed as percentage in basis points
    pub max_drawdown_pct: i32, // Basis points
    /// Value at Risk at 95% confidence level
    pub var_95: i64,           // Value at Risk (95%)
    /// Expected loss beyond the VaR threshold
    pub expected_shortfall: i64,

    /// Win/Loss statistics
    /// Percentage of profitable trades
    pub win_rate: f64, // Percentage
    /// Average profit from winning trades
    pub avg_win: i64,
    /// Average loss from losing trades
    pub avg_loss: i64,
    /// Largest single winning trade
    pub largest_win: i64,
    /// Largest single losing trade
    pub largest_loss: i64,

    /// Volume statistics
    /// Total trading volume across all trades
    pub total_volume: u64,
    /// Average size per trade
    pub avg_trade_size: f64,

    /// Time-based analysis
    /// Average time between trades in minutes
    pub avg_holding_period: f64, // Minutes
    /// Number of trades executed per hour
    pub trading_frequency: f64, // Trades per hour

    /// Symbol breakdown
    /// Performance metrics broken down by trading symbol
    pub symbol_performance: FxHashMap<Symbol, SymbolPerformance>,
}

/// Per-symbol performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolPerformance {
    /// Symbol identifier
    pub symbol: Symbol,
    /// Total number of trades for this symbol
    pub trades: u32,
    /// Total profit and loss for this symbol
    pub pnl: i64,
    /// Percentage of winning trades for this symbol
    pub win_rate: f64,
    /// Average bid-ask spread for this symbol
    pub avg_spread: f64,
    /// Total trading volume for this symbol
    pub volume: u64,
}

impl PerformanceAnalyzer {
    /// Create new performance analyzer
    #[must_use] pub fn new(capacity: usize) -> Self {
        Self {
            trade_history: VecDeque::with_capacity(capacity),
            daily_pnl: VecDeque::with_capacity(365), // ~1 year of daily data
            price_history: FxHashMap::default(),
            capacity,
        }
    }

    /// Record a trade and update daily `PnL`
    pub fn record_trade(&mut self, qty: Qty, price: Px, timestamp: Ts) {
        let side = if qty.raw() > 0 {
            TradeSide::Buy
        } else {
            TradeSide::Sell
        };

        let volume = qty.raw().saturating_mul(price.as_i64()) / 10000;

        let trade = TradeRecord {
            timestamp: timestamp.nanos(),
            symbol: Symbol::new(1), // Default symbol for now
            quantity: qty.raw(),
            price: price.as_i64(),
            volume,
            side,
        };

        // Update daily PnL
        self.update_daily_pnl(&trade);

        // Add to history with capacity management
        if self.trade_history.len() >= self.capacity {
            self.trade_history.pop_front();
        }
        self.trade_history.push_back(trade);
    }

    /// Update market price for a symbol
    pub fn update_market_price(&mut self, symbol: Symbol, bid: Px, ask: Px, timestamp: Ts) {
        let price_record = PriceRecord {
            timestamp: timestamp.nanos(),
            bid: bid.as_i64(),
            ask: ask.as_i64(),
            mid: i64::midpoint(bid.as_i64(), ask.as_i64()),
        };

        let price_history = self
            .price_history
            .entry(symbol)
            .or_insert_with(|| VecDeque::with_capacity(1000));

        if price_history.len() >= 1000 {
            price_history.pop_front();
        }
        price_history.push_back(price_record);
    }

    /// Update daily `PnL` tracking
    fn update_daily_pnl(&mut self, trade: &TradeRecord) {
        // Convert timestamp to date string (YYYY-MM-DD)
        let date_string = self.timestamp_to_date_string(trade.timestamp);

        // Find or create today's PnL record
        let today_pnl = self
            .daily_pnl
            .iter_mut()
            .find(|pnl| pnl.date == date_string);

        if let Some(pnl_record) = today_pnl {
            // Update existing record
            pnl_record.realized_pnl = pnl_record.realized_pnl.saturating_add(trade.volume);
            pnl_record.total_pnl = pnl_record
                .realized_pnl
                .saturating_add(pnl_record.unrealized_pnl);
            pnl_record.trades_count = pnl_record.trades_count.saturating_add(1);
            pnl_record.volume = pnl_record
                .volume
                .saturating_add(trade.volume.unsigned_abs());
        } else {
            // Create new daily record
            let new_pnl = DailyPnL {
                date: date_string,
                realized_pnl: trade.volume,
                unrealized_pnl: 0,
                total_pnl: trade.volume,
                trades_count: 1,
                volume: trade.volume.unsigned_abs(),
            };

            // Add with capacity management (keep ~1 year of data)
            if self.daily_pnl.len() >= 365 {
                self.daily_pnl.pop_front();
            }
            self.daily_pnl.push_back(new_pnl);
        }
    }

    /// Convert nanosecond timestamp to YYYY-MM-DD date string
    fn timestamp_to_date_string(&self, timestamp_nanos: u64) -> String {
        // Convert nanoseconds to seconds
        let timestamp_secs = timestamp_nanos / 1_000_000_000;

        // Simple date calculation (approximation)
        // This is a simplified version - in production, use chrono or time crate
        let days_since_epoch = timestamp_secs / 86400; // seconds per day

        // Simplified year calculation (not accounting for leap years properly)
        let year = 1970 + (days_since_epoch / 365);
        let day_of_year = days_since_epoch % 365;
        let month = std::cmp::min(12, (day_of_year / 30) + 1);
        let day = std::cmp::max(1, (day_of_year % 30) + 1);

        format!("{year:04}-{month:02}-{day:02}")
    }

    /// Get daily `PnL` history
    #[must_use] pub fn get_daily_pnl_history(&self) -> Vec<DailyPnL> {
        self.daily_pnl.iter().cloned().collect()
    }

    /// Get today's `PnL`
    #[must_use] pub fn get_todays_pnl(&self) -> Option<DailyPnL> {
        self.daily_pnl.back().cloned()
    }

    /// Get current daily `PnL` (real-time calculation)
    #[must_use] pub fn get_current_daily_pnl(&self) -> i64 {
        // Get today's date
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        let today = self.timestamp_to_date_string(now);

        // Sum all trades from today
        self.trade_history
            .iter()
            .filter(|trade| self.timestamp_to_date_string(trade.timestamp) == today)
            .map(|trade| trade.volume)
            .sum()
    }

    /// Generate comprehensive performance report
    #[allow(clippy::cast_precision_loss)]
    #[must_use] pub fn generate_report(&self) -> PerformanceReport {
        let now = Ts::now().nanos();
        let trades = &self.trade_history;

        if trades.is_empty() {
            return self.empty_report(now);
        }

        // Basic metrics
        let total_trades = u32::try_from(trades.len()).unwrap_or(u32::MAX);
        let total_volume = trades.iter().map(|t| t.volume.unsigned_abs()).sum::<u64>();
        let avg_trade_size = if total_trades > 0 {
            total_volume as f64 / f64::from(total_trades)
        } else {
            0.0
        };

        // PnL calculations
        let total_pnl: i64 = trades.iter().map(|t| t.volume).sum();
        let (wins, losses): (Vec<_>, Vec<_>) = trades.iter().partition(|t| t.volume > 0);

        let win_rate = if total_trades > 0 {
            wins.len() as f64 / f64::from(total_trades) * 100.0
        } else {
            0.0
        };

        let avg_win = if wins.is_empty() {
            0
        } else {
            wins.iter().map(|t| t.volume).sum::<i64>() / wins.len() as i64
        };

        let avg_loss = if losses.is_empty() {
            0
        } else {
            losses.iter().map(|t| t.volume.abs()).sum::<i64>() / losses.len() as i64
        };

        let largest_win = wins.iter().map(|t| t.volume).max().unwrap_or(0);
        let largest_loss = losses.iter().map(|t| t.volume.abs()).max().unwrap_or(0);

        // Profit factor
        let gross_profit: i64 = wins.iter().map(|t| t.volume).sum();
        let gross_loss: i64 = losses.iter().map(|t| t.volume.abs()).sum();
        let profit_factor = if gross_loss > 0 {
            gross_profit as f64 / gross_loss as f64
        } else {
            0.0
        };

        // Risk metrics
        let returns: Vec<f64> = trades.iter().map(|t| t.volume as f64 / 10000.0).collect();

        let (sharpe_ratio, sortino_ratio) = self.calculate_risk_ratios(&returns);
        let (max_drawdown, max_drawdown_pct) = self.calculate_drawdown(&returns);
        let (var_95, expected_shortfall) = self.calculate_var(&returns);

        // Time analysis
        let (avg_holding_period, trading_frequency) = self.calculate_time_metrics(trades);

        // Trading days estimation
        let trading_days = if let (Some(first_trade), Some(last_trade)) =
            (trades.front(), trades.back())
        {
            let first_timestamp = first_trade.timestamp;
            let last_timestamp = last_trade.timestamp;
            let duration_days = (last_timestamp - first_timestamp) / (24 * 60 * 60 * 1_000_000_000);
            std::cmp::max(1, u32::try_from(duration_days).unwrap_or(u32::MAX))
        } else {
            1
        };

        let avg_trades_per_day = f64::from(total_trades) / f64::from(trading_days);

        // Symbol breakdown
        let symbol_performance = self.calculate_symbol_performance(trades);

        PerformanceReport {
            timestamp: now,
            total_trades,
            trading_days,
            avg_trades_per_day,
            total_pnl,
            daily_pnl: self.get_current_daily_pnl(),
            realized_pnl: total_pnl,
            unrealized_pnl: 0,
            sharpe_ratio,
            sortino_ratio,
            calmar_ratio: if max_drawdown_pct > 0 {
                (total_pnl as f64 / 10000.0) / (f64::from(max_drawdown_pct) / 10000.0)
            } else {
                0.0
            },
            profit_factor,
            max_drawdown,
            max_drawdown_pct,
            var_95,
            expected_shortfall,
            win_rate,
            avg_win,
            avg_loss,
            largest_win,
            largest_loss,
            total_volume,
            avg_trade_size,
            avg_holding_period,
            trading_frequency,
            symbol_performance,
        }
    }

    /// Calculate Sharpe and Sortino ratios
    #[allow(clippy::cast_precision_loss)]
    fn calculate_risk_ratios(&self, returns: &[f64]) -> (f64, f64) {
        if returns.len() < 2 {
            return (0.0, 0.0);
        }

        let mean = returns.iter().sum::<f64>() / returns.len() as f64;

        // Standard deviation for Sharpe
        let variance =
            returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
        let std_dev = variance.sqrt();

        let sharpe_ratio = if std_dev > 0.0 {
            (mean / std_dev) * (252.0_f64).sqrt() // Annualized
        } else {
            0.0
        };

        // Downside deviation for Sortino
        let negative_returns: Vec<f64> = returns.iter().filter(|&&r| r < 0.0).copied().collect();

        let sortino_ratio = if negative_returns.is_empty() {
            sharpe_ratio // If no negative returns, Sortino = Sharpe
        } else {
            let downside_variance = negative_returns.iter().map(|r| r.powi(2)).sum::<f64>()
                / negative_returns.len() as f64;
            let downside_deviation = downside_variance.sqrt();

            if downside_deviation > 0.0 {
                (mean / downside_deviation) * (252.0_f64).sqrt()
            } else {
                0.0
            }
        };

        (sharpe_ratio, sortino_ratio)
    }

    /// Calculate maximum drawdown
    #[allow(clippy::cast_precision_loss)]
    fn calculate_drawdown(&self, returns: &[f64]) -> (i64, i32) {
        if returns.is_empty() {
            return (0, 0);
        }

        let mut peak = 0.0f64;
        let mut max_drawdown = 0.0f64;
        let mut cumulative = 0.0f64;

        for &ret in returns {
            cumulative += ret;
            if cumulative > peak {
                peak = cumulative;
            }
            let drawdown = peak - cumulative;
            if drawdown > max_drawdown {
                max_drawdown = drawdown;
            }
        }

        let max_drawdown_i64 = (max_drawdown * 10000.0) as i64;
        let max_drawdown_pct = if peak > 0.0 {
            ((max_drawdown / peak) * 10000.0) as i32 // Basis points
        } else {
            0
        };

        (max_drawdown_i64, max_drawdown_pct)
    }

    /// Calculate Value at Risk (95% confidence) and Expected Shortfall
    fn calculate_var(&self, returns: &[f64]) -> (i64, i64) {
        if returns.is_empty() {
            return (0, 0);
        }

        let mut sorted_returns = returns.to_vec();
        sorted_returns.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // VaR at 95% confidence (5th percentile)
        let var_idx = ((sorted_returns.len() as f64) * 0.05) as usize;
        let var_95 = if var_idx < sorted_returns.len() {
            (sorted_returns[var_idx] * 10000.0) as i64
        } else {
            0
        };

        // Expected Shortfall (average of losses beyond VaR)
        let expected_shortfall = if var_idx > 0 {
            let tail_losses: f64 = sorted_returns[..var_idx].iter().sum();
            (tail_losses / var_idx as f64 * 10000.0) as i64
        } else {
            var_95
        };

        (var_95, expected_shortfall)
    }

    /// Calculate time-based metrics
    #[allow(clippy::cast_precision_loss)]
    fn calculate_time_metrics(&self, trades: &VecDeque<TradeRecord>) -> (f64, f64) {
        if trades.len() < 2 {
            return (0.0, 0.0);
        }

        let (first_time, last_time) = match (trades.front(), trades.back()) {
            (Some(first), Some(last)) => (first.timestamp, last.timestamp),
            _ => return (0.0, 0.0), // Should not happen given the len check above
        };
        let total_duration_nanos = last_time - first_time;

        // Average holding period (simplified - time between trades)
        let avg_holding_period = if trades.len() > 1 {
            let trades_vec: Vec<&TradeRecord> = trades.iter().collect();
            let total_intervals: u64 = trades_vec
                .windows(2)
                .map(|window| window[1].timestamp - window[0].timestamp)
                .sum();
            (total_intervals / (trades.len() - 1) as u64) as f64 / 60_000_000_000.0 // Convert to minutes
        } else {
            0.0
        };

        // Trading frequency (trades per hour)
        let trading_frequency = if total_duration_nanos > 0 {
            let duration_hours = total_duration_nanos as f64 / 3_600_000_000_000.0;
            trades.len() as f64 / duration_hours
        } else {
            0.0
        };

        (avg_holding_period, trading_frequency)
    }

    /// Calculate per-symbol performance
    #[allow(clippy::cast_precision_loss)]
    fn calculate_symbol_performance(
        &self,
        trades: &VecDeque<TradeRecord>,
    ) -> FxHashMap<Symbol, SymbolPerformance> {
        let mut symbol_stats: FxHashMap<Symbol, (u32, i64, u64)> = FxHashMap::default();

        // Aggregate by symbol
        for trade in trades {
            let (trades_count, pnl, volume) = symbol_stats.entry(trade.symbol).or_insert((0, 0, 0));
            *trades_count += 1;
            *pnl += trade.volume;
            *volume += trade.volume.unsigned_abs();
        }

        // Convert to SymbolPerformance
        let mut result = FxHashMap::default();
        for (symbol, (trades_count, pnl, volume)) in symbol_stats {
            let wins = trades
                .iter()
                .filter(|t| t.symbol == symbol && t.volume > 0)
                .count();

            let win_rate = if trades_count > 0 {
                wins as f64 / f64::from(trades_count) * 100.0
            } else {
                0.0
            };

            // Get average spread from price history
            let avg_spread = self
                .price_history
                .get(&symbol)
                .map_or(0.0, |history| {
                    if history.is_empty() {
                        0.0
                    } else {
                        let total_spread: i64 = history.iter().map(|p| p.ask - p.bid).sum();
                        total_spread as f64 / history.len() as f64 / 10000.0
                    }
                });

            result.insert(
                symbol,
                SymbolPerformance {
                    symbol,
                    trades: trades_count,
                    pnl,
                    win_rate,
                    avg_spread,
                    volume,
                },
            );
        }

        result
    }

    /// Generate empty report for edge case
    fn empty_report(&self, timestamp: u64) -> PerformanceReport {
        PerformanceReport {
            timestamp,
            total_trades: 0,
            trading_days: 0,
            avg_trades_per_day: 0.0,
            total_pnl: 0,
            daily_pnl: 0,
            realized_pnl: 0,
            unrealized_pnl: 0,
            sharpe_ratio: 0.0,
            sortino_ratio: 0.0,
            calmar_ratio: 0.0,
            profit_factor: 0.0,
            max_drawdown: 0,
            max_drawdown_pct: 0,
            var_95: 0,
            expected_shortfall: 0,
            win_rate: 0.0,
            avg_win: 0,
            avg_loss: 0,
            largest_win: 0,
            largest_loss: 0,
            total_volume: 0,
            avg_trade_size: 0.0,
            avg_holding_period: 0.0,
            trading_frequency: 0.0,
            symbol_performance: FxHashMap::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_analyzer_creation() {
        let analyzer = PerformanceAnalyzer::new(1000);
        let report = analyzer.generate_report();
        assert_eq!(report.total_trades, 0);
    }

    #[test]
    fn test_trade_recording() {
        let mut analyzer = PerformanceAnalyzer::new(1000);

        analyzer.record_trade(
            Qty::from_i64(1000000), // 100 units
            Px::from_i64(1000000),  // $100
            Ts::now(),
        );

        let report = analyzer.generate_report();
        assert_eq!(report.total_trades, 1);
        assert!(report.total_volume > 0);
    }

    #[test]
    fn test_win_loss_calculation() {
        let mut analyzer = PerformanceAnalyzer::new(1000);

        // Record a winning trade
        analyzer.record_trade(
            Qty::from_i64(1000000),
            Px::from_i64(1100000), // Profitable
            Ts::now(),
        );

        // Record a losing trade
        analyzer.record_trade(
            Qty::from_i64(-1000000),
            Px::from_i64(900000), // Loss
            Ts::now(),
        );

        let report = analyzer.generate_report();
        assert_eq!(report.total_trades, 2);
        assert!(report.profit_factor > 0.0);
    }

    #[test]
    fn test_daily_pnl_tracking() {
        let mut analyzer = PerformanceAnalyzer::new(1000);
        let now = Ts::now();

        // Record multiple trades
        analyzer.record_trade(Qty::from_i64(1000000), Px::from_i64(1000000), now);
        analyzer.record_trade(Qty::from_i64(500000), Px::from_i64(1020000), now);

        // Check daily PnL was recorded
        let daily_history = analyzer.get_daily_pnl_history();
        assert!(!daily_history.is_empty());

        let today_pnl = analyzer.get_todays_pnl();
        assert!(today_pnl.is_some());

        let pnl_record = today_pnl.unwrap();
        assert_eq!(pnl_record.trades_count, 2);
        assert!(pnl_record.total_pnl != 0);

        // Check current daily PnL calculation
        let current_daily = analyzer.get_current_daily_pnl();
        assert!(current_daily != 0);
    }

    #[test]
    fn test_timestamp_to_date_conversion() {
        let analyzer = PerformanceAnalyzer::new(100);

        // Test with a known timestamp (roughly 2024)
        let timestamp_nanos = 1700000000000000000; // Approximately Nov 2023
        let date_string = analyzer.timestamp_to_date_string(timestamp_nanos);

        // Should be in YYYY-MM-DD format
        assert!(date_string.len() >= 10);
        assert!(date_string.contains('-'));
    }

    #[test]
    fn test_daily_pnl_capacity_management() {
        let mut analyzer = PerformanceAnalyzer::new(1000);

        // Create trades over many "days" by using different timestamps
        for i in 0..400 {
            // More than 365 days
            let timestamp = Ts::from_nanos((1700000000 + i * 86400) * 1_000_000_000); // Add 1 day each iteration
            analyzer.record_trade(Qty::from_i64(1000000), Px::from_i64(1000000), timestamp);
        }

        // Should not exceed capacity
        let daily_history = analyzer.get_daily_pnl_history();
        assert!(daily_history.len() <= 365);
    }
}
