//! Lock-free position tracking with cache-aligned data structures

use common::{Px, Qty, Side, Symbol, Ts};
use dashmap::DashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

/// Single position - cache-line aligned
#[repr(C, align(64))]
pub struct Position {
    pub symbol: Symbol,
    pub quantity: AtomicI64,     // Positive = long, Negative = short
    pub avg_price: AtomicU64,    // Fixed point (multiply by 100)
    pub realized_pnl: AtomicI64, // In smallest unit (paise/satoshi)
    pub unrealized_pnl: AtomicI64,
    pub last_update: AtomicU64, // Timestamp nanos

    // Market data for PnL calculation
    pub last_bid: AtomicU64,
    pub last_ask: AtomicU64,

    _padding: [u8; 8],
}

impl Position {
    /// Create new position
    pub fn new(symbol: Symbol) -> Self {
        Self {
            symbol,
            quantity: AtomicI64::new(0),
            avg_price: AtomicU64::new(0),
            realized_pnl: AtomicI64::new(0),
            unrealized_pnl: AtomicI64::new(0),
            last_update: AtomicU64::new(0),
            last_bid: AtomicU64::new(0),
            last_ask: AtomicU64::new(0),
            _padding: [0; 8],
        }
    }

    /// Update position with fill - LOCK-FREE
    #[inline(always)]
    pub fn apply_fill(&self, side: u8, qty: Qty, price: Px, ts: Ts) {
        let qty_raw = qty.raw() as i64;
        let price_raw = (price.as_f64() * 100.0) as u64;

        // Determine quantity delta
        let qty_delta = if side == 0 { qty_raw } else { -qty_raw }; // Buy = 0, Sell = 1

        // Update quantity
        let old_qty = self.quantity.fetch_add(qty_delta, Ordering::AcqRel);
        let new_qty = old_qty + qty_delta;

        // Update average price (lock-free weighted average)
        if new_qty != 0 && old_qty * new_qty >= 0 {
            // Adding to position (or opening new position when old_qty == 0)
            let old_avg = self.avg_price.load(Ordering::Acquire);
            let new_avg = if old_qty == 0 {
                price_raw
            } else {
                ((old_avg * old_qty.abs() as u64) + (price_raw * qty_raw.abs() as u64))
                    / (old_qty.abs() + qty_raw.abs()) as u64
            };
            self.avg_price.store(new_avg, Ordering::Release);
        } else if old_qty != 0 && new_qty * old_qty <= 0 {
            // Closing or flipping position
            let old_avg = self.avg_price.load(Ordering::Acquire);
            let closed_qty = old_qty.abs().min(qty_raw.abs());
            let pnl = if old_qty > 0 {
                // Was long, now selling
                (price_raw as i64 - old_avg as i64) * closed_qty
            } else {
                // Was short, now buying
                (old_avg as i64 - price_raw as i64) * closed_qty
            };

            // Add to realized PnL
            self.realized_pnl.fetch_add(pnl / 100, Ordering::AcqRel);

            // Reset avg price if position flipped
            if new_qty != 0 {
                self.avg_price.store(price_raw, Ordering::Release);
            }
        }

        // Update timestamp
        self.last_update.store(ts.nanos(), Ordering::Release);
    }

    /// Update market prices for unrealized PnL - LOCK-FREE
    #[inline(always)]
    pub fn update_market(&self, bid: Px, ask: Px, ts: Ts) {
        let bid_raw = (bid.as_f64() * 100.0) as u64;
        let ask_raw = (ask.as_f64() * 100.0) as u64;

        self.last_bid.store(bid_raw, Ordering::Relaxed);
        self.last_ask.store(ask_raw, Ordering::Relaxed);

        // Calculate unrealized PnL
        let qty = self.quantity.load(Ordering::Acquire);
        if qty != 0 {
            let avg_price = self.avg_price.load(Ordering::Acquire);
            let mark_price = if qty > 0 { bid_raw } else { ask_raw };

            let unrealized = if qty > 0 {
                (mark_price as i64 - avg_price as i64) * qty
            } else {
                (avg_price as i64 - mark_price as i64) * qty.abs()
            };

            self.unrealized_pnl
                .store(unrealized / 100, Ordering::Release);
        } else {
            self.unrealized_pnl.store(0, Ordering::Release);
        }

        self.last_update.store(ts.nanos(), Ordering::Release);
    }

    /// Get total PnL
    #[inline(always)]
    pub fn total_pnl(&self) -> i64 {
        self.realized_pnl.load(Ordering::Acquire) + self.unrealized_pnl.load(Ordering::Acquire)
    }
}

/// Position tracker - manages all positions
pub struct PositionTracker {
    positions: DashMap<Symbol, Position>,
    pending_orders: DashMap<u64, (Symbol, Side, Qty)>, // order_id -> (symbol, side, qty)

    // Global PnL tracking
    total_realized: AtomicI64,
    total_unrealized: AtomicI64,

    // Reconciliation tracking
    update_counter: AtomicU64,    // Count updates since last reconciliation
    last_reconcile_ts: AtomicU64, // Timestamp of last reconciliation
}

impl PositionTracker {
    /// Create new tracker with pre-allocated capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            positions: DashMap::with_capacity(capacity),
            pending_orders: DashMap::with_capacity(capacity * 2),
            total_realized: AtomicI64::new(0),
            total_unrealized: AtomicI64::new(0),
            update_counter: AtomicU64::new(0),
            last_reconcile_ts: AtomicU64::new(0),
        }
    }

    /// Add pending order
    #[inline(always)]
    pub fn add_pending(&self, order_id: u64, symbol: Symbol, side: Side, qty: Qty) {
        self.pending_orders.insert(order_id, (symbol, side, qty));
    }

    /// Apply fill to position
    #[inline(always)]
    pub fn apply_fill(&self, order_id: u64, fill_qty: Qty, fill_price: Px, ts: Ts) {
        if let Some((_, (symbol, side, _))) = self.pending_orders.remove(&order_id) {
            let position = self
                .positions
                .entry(symbol)
                .or_insert_with(|| Position::new(symbol));

            let side_u8 = if side == Side::Bid { 0 } else { 1 };

            // Store old PnL values
            let old_realized = position.realized_pnl.load(Ordering::Acquire);
            let old_unrealized = position.unrealized_pnl.load(Ordering::Acquire);

            // Apply the fill
            position.apply_fill(side_u8, fill_qty, fill_price, ts);

            // Update global PnL incrementally (no iteration needed)
            let new_realized = position.realized_pnl.load(Ordering::Acquire);
            let new_unrealized = position.unrealized_pnl.load(Ordering::Acquire);

            let realized_delta = new_realized - old_realized;
            let unrealized_delta = new_unrealized - old_unrealized;

            self.total_realized
                .fetch_add(realized_delta, Ordering::AcqRel);
            self.total_unrealized
                .fetch_add(unrealized_delta, Ordering::AcqRel);

            // Check if we need to reconcile (every 100 updates or every second)
            let updates = self.update_counter.fetch_add(1, Ordering::AcqRel);
            if updates > 100 {
                let now = Ts::now().nanos();
                let last = self.last_reconcile_ts.load(Ordering::Acquire);
                if now - last > 1_000_000_000 {
                    // 1 second
                    self.reconcile_global_pnl();
                    self.update_counter.store(0, Ordering::Release);
                    self.last_reconcile_ts.store(now, Ordering::Release);
                }
            }
        }
    }

    /// Update market prices for all positions
    #[inline(always)]
    pub fn update_market(&self, symbol: Symbol, bid: Px, ask: Px, ts: Ts) {
        if let Some(position) = self.positions.get(&symbol) {
            // Store old unrealized PnL
            let old_unrealized = position.unrealized_pnl.load(Ordering::Acquire);

            // Update market prices
            position.update_market(bid, ask, ts);

            // Update global unrealized PnL incrementally
            let new_unrealized = position.unrealized_pnl.load(Ordering::Acquire);
            let unrealized_delta = new_unrealized - old_unrealized;

            self.total_unrealized
                .fetch_add(unrealized_delta, Ordering::AcqRel);
        }
    }

    /// Reconcile global PnL - call periodically (e.g., every second or after N updates)
    /// This is NOT called on hot path, only for periodic reconciliation
    pub fn reconcile_global_pnl(&self) {
        let mut total_realized = 0i64;
        let mut total_unrealized = 0i64;

        // Iterate over all positions for accurate totals
        for entry in self.positions.iter() {
            let position = entry.value();
            total_realized += position.realized_pnl.load(Ordering::Relaxed);
            total_unrealized += position.unrealized_pnl.load(Ordering::Relaxed);
        }

        // Store reconciled values
        self.total_realized.store(total_realized, Ordering::Release);
        self.total_unrealized
            .store(total_unrealized, Ordering::Release);
    }

    /// Get position for symbol
    #[inline(always)]
    pub fn get_position(
        &self,
        symbol: Symbol,
    ) -> Option<dashmap::mapref::one::Ref<'_, Symbol, Position>> {
        self.positions.get(&symbol)
    }

    /// Get all positions
    pub fn get_all_positions(&self) -> Vec<(Symbol, i64, i64)> {
        self.positions
            .iter()
            .map(|entry| {
                let pos = entry.value();
                (
                    entry.key().clone(),
                    pos.quantity.load(Ordering::Acquire),
                    pos.total_pnl(),
                )
            })
            .collect()
    }

    /// Get global PnL
    #[inline(always)]
    pub fn get_global_pnl(&self) -> (i64, i64, i64) {
        let realized = self.total_realized.load(Ordering::Acquire);
        let unrealized = self.total_unrealized.load(Ordering::Acquire);
        (realized, unrealized, realized + unrealized)
    }
}

/// Position cache for fast lookups (pre-allocated)
pub struct PositionCache {
    cache: Vec<Position>,
    index: DashMap<Symbol, usize>,
}

impl PositionCache {
    pub fn new(capacity: usize) -> Self {
        let mut cache = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            cache.push(Position::new(Symbol::new(0)));
        }

        Self {
            cache,
            index: DashMap::with_capacity(capacity),
        }
    }

    #[inline(always)]
    pub fn get(&self, symbol: Symbol) -> Option<&Position> {
        self.index.get(&symbol).map(|idx| &self.cache[*idx])
    }
}
