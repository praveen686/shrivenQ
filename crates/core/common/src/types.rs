//! Core types for `ShrivenQ` trading platform

use serde::{Deserialize, Serialize};
use std::fmt;

/// Symbol identifier for trading instruments
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Symbol(pub u32);

impl Symbol {
    /// Create a new Symbol with given ID
    #[must_use]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SYM_{}", self.0)
    }
}

/// Price type (stored as i64 ticks for determinism, 4 decimal places)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Px(i64); // Internal: price in ticks (1 tick = 0.0001)

impl Px {
    /// Create a new Price from f64 (converts to ticks)
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn new(value: f64) -> Self {
        Self((value * 10000.0).round() as i64)
    }

    /// Get the price as f64
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn as_f64(&self) -> f64 {
        self.0 as f64 / 10000.0
    }

    /// Get price as i64 ticks
    #[must_use]
    pub const fn as_i64(&self) -> i64 {
        self.0
    }

    /// Create from i64 ticks
    #[must_use]
    pub const fn from_i64(ticks: i64) -> Self {
        Self(ticks)
    }

    /// Zero price
    pub const ZERO: Self = Self(0);
}

impl fmt::Display for Px {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.4}", self.as_f64())
    }
}

/// Quantity type for order sizes (stored as i64 units for determinism, 4 decimal places)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Qty(i64); // Internal: quantity in units (1 unit = 0.0001)

impl Qty {
    /// Create a new Quantity from f64
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn new(value: f64) -> Self {
        Self((value * 10000.0).round() as i64)
    }

    /// Get the quantity as f64
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn as_f64(&self) -> f64 {
        self.0 as f64 / 10000.0
    }

    /// Get quantity as i64 units
    #[must_use]
    pub const fn as_i64(&self) -> i64 {
        self.0
    }

    /// Create from i64 units
    #[must_use]
    pub const fn from_i64(units: i64) -> Self {
        Self(units)
    }

    /// Check if quantity is zero
    #[must_use]
    pub const fn is_zero(&self) -> bool {
        self.0 == 0
    }

    /// Get raw i64 value
    #[must_use]
    pub const fn raw(&self) -> i64 {
        self.0
    }

    /// Zero quantity
    pub const ZERO: Self = Self(0);
}

impl fmt::Display for Qty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.4}", self.as_f64())
    }
}

/// Timestamp in nanoseconds since UNIX epoch
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Ts(pub u64);

impl Ts {
    /// Get current timestamp
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn now() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_nanos() as u64;
        Self(nanos)
    }

    /// Create timestamp from nanoseconds
    #[must_use]
    pub const fn from_nanos(nanos: u64) -> Self {
        Self(nanos)
    }

    /// Get timestamp as nanoseconds
    #[must_use]
    pub const fn as_nanos(&self) -> u64 {
        self.0
    }

    /// Get timestamp as nanoseconds (alias for `as_nanos`)
    #[must_use]
    pub const fn nanos(&self) -> u64 {
        self.0
    }

    /// Get timestamp as microseconds
    #[must_use]
    pub const fn as_micros(&self) -> u64 {
        self.0 / 1000
    }

    /// Get timestamp as milliseconds
    #[must_use]
    pub const fn as_millis(&self) -> u64 {
        self.0 / 1_000_000
    }
}

impl fmt::Display for Ts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}ns", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bincode;

    #[test]
    fn test_symbol_serde() -> Result<(), Box<dyn std::error::Error>> {
        let sym = Symbol::new(42);
        let encoded = bincode::serialize(&sym)?;
        let decoded: Symbol = bincode::deserialize(&encoded)?;
        assert_eq!(sym, decoded);
        Ok(())
    }

    #[test]
    fn test_px_serde() -> Result<(), Box<dyn std::error::Error>> {
        let px = Px::new(1234.56);
        let encoded = bincode::serialize(&px)?;
        let decoded: Px = bincode::deserialize(&encoded)?;
        assert_eq!(px, decoded);
        Ok(())
    }

    #[test]
    fn test_qty_serde() -> Result<(), Box<dyn std::error::Error>> {
        let qty = Qty::new(100.0);
        let encoded = bincode::serialize(&qty)?;
        let decoded: Qty = bincode::deserialize(&encoded)?;
        assert_eq!(qty, decoded);
        Ok(())
    }

    #[test]
    fn test_ts_serde() -> Result<(), Box<dyn std::error::Error>> {
        let ts = Ts::from_nanos(1_234_567_890);
        let encoded = bincode::serialize(&ts)?;
        let decoded: Ts = bincode::deserialize(&encoded)?;
        assert_eq!(ts, decoded);
        Ok(())
    }

    #[test]
    fn test_ts_conversions() {
        let ts = Ts::from_nanos(1_234_567_890);
        assert_eq!(ts.as_nanos(), 1_234_567_890);
        assert_eq!(ts.as_micros(), 1_234_567);
        assert_eq!(ts.as_millis(), 1_234);
    }
}
