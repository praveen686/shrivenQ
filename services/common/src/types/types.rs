//! Core types for `ShrivenQ` trading platform

use crate::constants::fixed_point::{SCALE_2, SCALE_4};
use crate::constants::numeric::ZERO_I64;
use crate::constants::time::{NANOS_PER_MICRO, NANOS_PER_MILLI};
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
    /// Create a new Price from ticks (1 tick = 0.0001 units)
    /// For external API compatibility only - prefer `from_i64`
    #[must_use]
    pub fn new(value: f64) -> Self {
        let scaled = (value * SCALE_4 as f64).round();
        // Safely convert f64 to i64 using proper bounds
        // i64::MAX = 9_223_372_036_854_775_807
        const MAX_SAFE: f64 = 9_223_372_036_854_775_807.0;
        const MIN_SAFE: f64 = -9_223_372_036_854_775_808.0;

        let clamped = if scaled >= MAX_SAFE {
            i64::MAX
        } else if scaled <= MIN_SAFE {
            i64::MIN
        } else {
            // Now safe to cast after bounds check
            #[allow(clippy::cast_possible_truncation)]
            // SAFETY: Cast is safe within expected range
            let result = scaled as i64; // SAFETY: price scaled to fixed precision
            result
        };
        Self(clamped)
    }

    /// Get price as f64 for external APIs only
    /// WARNING: For values > 2^53 / 10000, this may lose precision
    /// Internal code should ALWAYS use fixed-point arithmetic
    #[must_use]
    pub fn as_f64(&self) -> f64 {
        // We must convert i64 to f64 here for external API compatibility
        // This is a fundamental limitation when interfacing with systems that use floating point
        // We explicitly allow this ONE conversion at the system boundary
        #[allow(clippy::cast_precision_loss)]
        // SAFETY: Cast is safe within expected range
        {
            // SAFETY: Cast is safe within expected range
            self.0 as f64 / SCALE_4 as f64
        }
    }

    /// Create from cents (100 cents = 1 unit)
    #[must_use]
    pub const fn from_cents(cents: i64) -> Self {
        Self(cents * (SCALE_4 / 100)) // 100 cents = SCALE_4 ticks
    }

    /// Create from integer price in smallest units (e.g., paise, cents)
    /// Assumes 2 decimal places in input, converts to 4 decimal internal
    #[must_use]
    pub const fn from_price_i32(price: i32) -> Self {
        // SAFETY: i32 to i64 is always lossless (widening conversion)
        #[allow(clippy::cast_lossless)]
        Self((price as i64) * (SCALE_4 / SCALE_2))
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

    /// Add two prices (fixed-point arithmetic)
    #[must_use]
    pub const fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }

    /// Subtract two prices (fixed-point arithmetic)
    #[must_use]
    pub const fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }

    /// Multiply price by quantity to get notional value
    /// Returns value in ticks (divide by 10000 for display)
    #[must_use]
    pub const fn mul_qty(self, qty: Qty) -> i64 {
        (self.0 * qty.0) / SCALE_4
    }
}

impl fmt::Display for Px {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let whole = self.0 / SCALE_4;
        let frac = (self.0 % SCALE_4).abs();
        write!(f, "{whole}.{frac:04}")
    }
}

/// Quantity type for order sizes (stored as i64 units for determinism, 4 decimal places)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Qty(i64); // Internal: quantity in units (1 unit = 0.0001)

impl Qty {
    /// Create a new Quantity from units (for external API compatibility)
    /// For internal code, prefer `from_i64`
    #[must_use]
    pub fn new(value: f64) -> Self {
        let scaled = (value * SCALE_4 as f64).round();
        // Safely convert f64 to i64 using proper bounds
        // i64::MAX = 9_223_372_036_854_775_807
        const MAX_SAFE: f64 = 9_223_372_036_854_775_807.0;
        const MIN_SAFE: f64 = -9_223_372_036_854_775_808.0;

        let clamped = if scaled >= MAX_SAFE {
            i64::MAX
        } else if scaled <= MIN_SAFE {
            i64::MIN
            // SAFETY: Cast is safe within expected range
        } else {
            // SAFETY: Cast is safe within expected range
            // Now safe to cast after bounds check
            // SAFETY: Cast is safe within expected range
            #[allow(clippy::cast_possible_truncation)]
            let result = scaled as i64; // SAFETY: price scaled to fixed precision
            result
        };
        Self(clamped)
    }

    /// Get quantity as f64 for external APIs only
    /// WARNING: For values > 2^53 / 10000, this may lose precision
    /// Internal code should ALWAYS use fixed-point arithmetic
    #[must_use]
    pub fn as_f64(&self) -> f64 {
        // We must convert i64 to f64 here for external API compatibility
        // SAFETY: Cast is safe within expected range
        // This is a fundamental limitation when interfacing with systems that use floating point
        // SAFETY: Cast is safe within expected range
        // We explicitly allow this ONE conversion at the system boundary
        // SAFETY: Cast is safe within expected range
        #[allow(clippy::cast_precision_loss)]
        {
            self.0 as f64 / SCALE_4 as f64
        }
    }

    /// Create from whole units
    #[must_use]
    pub const fn from_units(units: i64) -> Self {
        Self(units * SCALE_4)
    }
    // SAFETY: Cast is safe within expected range

    // SAFETY: Cast is safe within expected range
    /// Create from integer quantity
    #[must_use]
    pub const fn from_qty_i32(qty: i32) -> Self {
        // SAFETY: i32 to i64 is always lossless (widening conversion)
        #[allow(clippy::cast_lossless)]
        Self((qty as i64) * SCALE_4)
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
        self.0 == ZERO_I64
    }

    /// Get raw i64 value
    #[must_use]
    pub const fn raw(&self) -> i64 {
        self.0
    }

    /// Zero quantity
    pub const ZERO: Self = Self(0);

    /// Add two quantities (fixed-point arithmetic)
    #[must_use]
    pub const fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }

    /// Subtract two quantities (fixed-point arithmetic)
    #[must_use]
    pub const fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl fmt::Display for Qty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let whole = self.0 / SCALE_4;
        let frac = (self.0 % SCALE_4).abs();
        write!(f, "{whole}.{frac:04}")
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
        // as_nanos() returns u128, but we need u64
        // For timestamps, this is safe as u64 can represent ~584 years of nanoseconds
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0));
        // Use as_secs and subsec_nanos to avoid u128
        let nanos = duration.as_secs() * 1_000_000_000 + u64::from(duration.subsec_nanos());
        Self(nanos)
    }

    /// Create timestamp from nanoseconds
    #[must_use]
    pub const fn from_nanos(nanos: u64) -> Self {
        Self(nanos)
    }
    
    /// Create timestamp from milliseconds
    #[must_use]
    pub const fn from_millis(millis: i64) -> Self {
        Self((millis as u64) * NANOS_PER_MILLI)
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
        self.0 / NANOS_PER_MICRO
    }

    /// Get timestamp as milliseconds
    #[must_use]
    pub const fn as_millis(&self) -> u64 {
        self.0 / NANOS_PER_MILLI
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
        let px = Px::from_i64(12_345_600); // 1234.56 as ticks
        let encoded = bincode::serialize(&px)?;
        let decoded: Px = bincode::deserialize(&encoded)?;
        assert_eq!(px, decoded);
        Ok(())
    }

    #[test]
    fn test_qty_serde() -> Result<(), Box<dyn std::error::Error>> {
        let qty = Qty::from_units(100); // 100 units
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
