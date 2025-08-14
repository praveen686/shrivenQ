//! Common constants used across all services
//!
//! COMPLIANCE: Single source of truth for all magic numbers

// Fixed-point arithmetic constants
/// Fixed-point scale factor (4 decimal places)
pub const FIXED_POINT_SCALE: i64 = 10000;
pub const FIXED_POINT_SCALE_U64: u64 = 10000;
pub const FIXED_POINT_SCALE_I32: i32 = 10000;
pub const FIXED_POINT_SCALE_F64: f64 = 10000.0;

// Percentage/basis points constants
pub const PERCENT_TO_BP: i64 = 100; // 1% = 100 basis points
pub const BP_SCALE: i64 = FIXED_POINT_SCALE; // Basis points use same scale

// Time constants
pub const MILLIS_PER_SEC: u64 = 1000;
pub const MICROS_PER_SEC: u64 = 1_000_000;
pub const NANOS_PER_SEC: u64 = 1_000_000_000;
pub const NANOS_PER_MILLI: u64 = 1_000_000;
pub const NANOS_PER_MICRO: u64 = 1000;
pub const SECS_PER_MIN: u64 = 60;
pub const MINS_PER_HOUR: u64 = 60;
pub const HOURS_PER_DAY: u64 = 24;
pub const SECS_PER_HOUR: u64 = SECS_PER_MIN * MINS_PER_HOUR;
pub const SECS_PER_DAY: u64 = SECS_PER_HOUR * HOURS_PER_DAY;

// Size constants
pub const BYTES_PER_KB: u64 = 1024;
pub const BYTES_PER_MB: u64 = BYTES_PER_KB * 1024;
pub const BYTES_PER_GB: u64 = BYTES_PER_MB * 1024;

// Market constants
pub const DEFAULT_LOT_SIZE: i64 = 1;
pub const DEFAULT_TICK_SIZE: i64 = 5; // 0.0005 in fixed-point

// Risk limits (in fixed-point)
pub const DEFAULT_MAX_ORDER_VALUE: i64 = 1000000 * FIXED_POINT_SCALE; // 1M
pub const DEFAULT_MAX_POSITION_VALUE: i64 = 10000000 * FIXED_POINT_SCALE; // 10M
pub const DEFAULT_MAX_DAILY_LOSS: i64 = 100000 * FIXED_POINT_SCALE; // 100K

// Performance thresholds
pub const MAX_LATENCY_MICROS: u64 = 100; // 100μs max latency
pub const TARGET_THROUGHPUT: u64 = 10000; // 10K ops/sec

// Retry constants
pub const DEFAULT_MAX_RETRIES: u32 = 3;
pub const DEFAULT_RETRY_DELAY_MS: u64 = 100;
pub const MAX_RETRY_DELAY_MS: u64 = 5000;

// Buffer sizes
pub const DEFAULT_BUFFER_SIZE: usize = 1024;
pub const DEFAULT_CHANNEL_SIZE: usize = 1000;
pub const DEFAULT_POOL_SIZE: usize = 10000;

// Numeric limits for validation
pub const MAX_PRICE: i64 = i64::MAX / FIXED_POINT_SCALE; // Prevent overflow
pub const MAX_QUANTITY: i64 = 1_000_000_000; // 1B units max
pub const MIN_QUANTITY: i64 = 1; // Minimum 1 unit
