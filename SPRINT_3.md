# ShrivenQ — Sprint 3 Plan

**Date:** 2025-08-11  
**Sprint:** 3 – Feed Adapters (Stub) & Limit Order Book Core  
**Status:** 🚀 **PLANNED** – Ready to start after Sprint 2 verification PASS

---

## 🎯 Goals

- **Normalize** live/file ticks from Zerodha & Binance into one canonical event type.
- Implement a **cache-friendly L2 Limit Order Book** (per symbol) with:
  - Best bid/ask
  - Mid price
  - Microprice
  - Spread
  - Depth imbalance
  - VWAP deviation
- Stream `LOBUpdate` + `FeatureFrame` onto the bus for downstream strategies.

---

## ✅ Definition of Done

- Replay NDJSON samples → LOB updates at ≥ **200k updates/sec** on dev laptop.
- `apply_update` p50 ≤ **200 ns**, p99 ≤ **900 ns** (DEPTH=32).
- Deterministic LOB state across replays (hash match).
- Unit tests:
  - Add/cancel/modify scenarios
  - Crossed/locked book prevention
  - Negative qty guardrails

---

## 📂 Repo Structure Updates

```text
shrivenQ/
  feed/
    zerodha/
      src/lib.rs
      src/file_ndjson.rs   # stub reader for replay
      src/ws.rs            # (Sprint 5) live WS
    binance/
      src/lib.rs
      src/file_ndjson.rs
  lob/
    src/lib.rs
    src/book.rs
    src/price_levels.rs
    benches/apply_update.rs
```

Add to root `Cargo.toml` workspace members:
```toml
"feed/zerodha", "feed/binance", "lob"
```

---

## 📜 Canonical Market Data Types

```rust
// common/src/market.rs
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Symbol(pub u32);          // interned ID

#[derive(Clone, Copy, Debug)]
pub enum Side { Bid, Ask }

#[derive(Clone, Copy, Debug)]
pub struct Px(pub i64);              // price in ticks
#[derive(Clone, Copy, Debug)]
pub struct Qty(pub i64);             // size in lots/units

#[derive(Clone, Copy, Debug)]
pub struct TsNanos(pub u64);

/// Normalized L2 update (absolute replace at price level)
#[derive(Clone, Debug)]
pub struct L2Update {
    pub ts: TsNanos,
    pub symbol: Symbol,
    pub side: Side,
    pub price: Px,
    pub qty: Qty,        // 0 => remove level
    pub level: u8,       // 0 = best
}
```

---

## 🔌 Feed Adapter Traits

```rust
// feed/zerodha/src/lib.rs (same pattern for binance)
use common::market::L2Update;

pub struct FeedConfig {
    pub symbol: common::market::Symbol,
    pub path: std::path::PathBuf,  // for file mode
}

#[async_trait::async_trait]
pub trait Feed: Send + Sync {
    async fn run(&mut self, tx: flume::Sender<L2Update>) -> anyhow::Result<()>;
}

pub struct FileNdjson { cfg: FeedConfig }
impl FileNdjson { pub fn new(cfg: FeedConfig) -> Self { Self { cfg } } }

#[async_trait::async_trait]
impl Feed for FileNdjson {
    async fn run(&mut self, tx: flume::Sender<L2Update>) -> anyhow::Result<()> {
        // read line-by-line, parse into L2Update, tx.send(update)?
        Ok(())
    }
}
```

---

## 📊 Limit Order Book Core

Design:
- Fixed depth array per side (`DEPTH=32`)
- Structure-of-arrays for cache friendliness
- Absolute replace semantics
- Guards for crossed/locked markets

```rust
// lob/src/book.rs
use common::market::*;

pub const DEPTH: usize = 32;

#[derive(Clone)]
pub struct SideBook {
    pub price: [Px; DEPTH],
    pub qty:   [Qty; DEPTH],
}
impl SideBook {
    #[inline] pub fn clear(&mut self) { /* zero qtys */ }
    #[inline] pub fn set(&mut self, level: u8, price: Px, qty: Qty) { /* write */ }
    #[inline] pub fn best(&self) -> Option<(Px, Qty)> { /* scan */ None }
}

pub struct OrderBook {
    pub ts: TsNanos,
    pub bids: SideBook,
    pub asks: SideBook,
}
impl OrderBook {
    #[inline] pub fn apply(&mut self, u: &L2Update) { /* update side */ }
    #[inline] pub fn mid(&self) -> Option<Px> { None }
    #[inline] pub fn microprice(&self) -> Option<Px> { None }
    #[inline] pub fn imbalance(&self) -> Option<f64> { None }
}
```

---

## 📈 Feature Emission

```rust
// common/src/features.rs
#[derive(Clone, Debug)]
pub struct FeatureFrame {
    pub ts: TsNanos,
    pub symbol: Symbol,
    pub spread_ticks: i64,
    pub mid: i64,
    pub micro: i64,
    pub imbalance: f64,
    pub vwap_dev: f64,
}
```

Data flow:
- `L2Update` from feed → `OrderBook.apply()` → compute features → publish `LOBUpdate` + `FeatureFrame` on bus.

---

## 🧪 Benchmarks & Tests

**Benches**
- `lob/benches/apply_update.rs`: 1M synthetic updates → measure p50/p99.

**Tests**
- LOB add/remove/update
- Crossed/locked guard
- Deterministic replay hash match

---

## 📏 Performance Targets

- `apply(update)` p50 ≤ **200 ns**, p99 ≤ **900 ns**
- LOB update throughput ≥ **200k/sec** on dev laptop
- Feature calculation p50 ≤ **300 ns**

---

## ▶ Run Instructions

Replay from file (Zerodha stub):
```bash
cargo run -p cli -- dev up
```

Benchmark LOB:
```bash
cargo bench -p lob
```

---

## 🗂 Deliverables

- [ ] Canonical `L2Update` struct in `common/market.rs`
- [ ] `Feed` trait + Zerodha/Binance file adapters
- [ ] LOB core with SideBook & OrderBook
- [ ] Feature calculation module
- [ ] Benchmarks and unit tests
- [ ] Documentation for all public API

---

*End of Sprint 3 Plan*
