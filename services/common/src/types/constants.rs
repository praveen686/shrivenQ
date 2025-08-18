//! Core constants for the `ShrivenQuant` trading system.
//!
//! This module provides centralized constants to replace magic numbers
//! throughout the codebase, improving maintainability and clarity.

/// Fixed-point arithmetic constants
pub mod fixed_point {
    /// 4-decimal fixed-point scale factor (for quantities, weights)
    pub const SCALE_4: i64 = 10000;

    /// 2-decimal fixed-point scale factor (for prices, percentages)
    pub const SCALE_2: i64 = 100;

    /// 3-decimal fixed-point scale factor (for ratios)
    pub const SCALE_3: i64 = 1000;

    /// Conversion factor for basis points (1/100th of a percent)
    pub const BASIS_POINTS: i64 = 10000;
}

/// Time-related constants
pub mod time {
    /// Seconds per minute
    pub const SECS_PER_MINUTE: u64 = 60;

    /// Seconds per hour
    pub const SECS_PER_HOUR: u64 = 3600;

    /// Seconds per day
    pub const SECS_PER_DAY: u64 = 86400;

    /// Milliseconds per second
    pub const MILLIS_PER_SEC: u64 = 1000;

    /// Microseconds per second
    pub const MICROS_PER_SEC: u64 = 1_000_000;
    /// Unix epoch year for timestamp validation
    pub const UNIX_EPOCH_YEAR: i32 = 1970;

    /// Nanoseconds per second
    pub const NANOS_PER_SEC: u64 = 1_000_000_000;

    /// Nanoseconds per millisecond
    pub const NANOS_PER_MILLI: u64 = 1_000_000;

    /// Nanoseconds per microsecond
    pub const NANOS_PER_MICRO: u64 = 1_000;

    /// Default token expiry in seconds (1 hour)
    pub const DEFAULT_TOKEN_EXPIRY_SECS: u64 = 3600;

    /// Common time intervals in milliseconds
    pub const INTERVAL_100_MS: u64 = 100;
    /// 500 milliseconds interval
    pub const INTERVAL_500_MS: u64 = 500;
    /// 1 second in milliseconds
    pub const INTERVAL_1_SEC_MS: u64 = 1000;

    /// Common time intervals in seconds
    pub const INTERVAL_5_SECS: u64 = 5;
    /// 10 seconds interval
    pub const INTERVAL_10_SECS: u64 = 10;
    /// 30 seconds interval
    pub const INTERVAL_30_SECS: u64 = 30;
    /// 1 minute in seconds
    pub const INTERVAL_1_MIN: u64 = 60;
    /// 2 minutes in seconds
    pub const INTERVAL_2_MINS: u64 = 120;
    /// 5 minutes in seconds
    pub const INTERVAL_5_MINS: u64 = 300;
    /// 10 minutes in seconds
    pub const INTERVAL_10_MINS: u64 = 600;
    /// 15 minutes in seconds
    pub const INTERVAL_15_MINS: u64 = 900;
    /// 30 minutes in seconds
    pub const INTERVAL_30_MINS: u64 = 1800;
    /// 1 hour in seconds
    pub const INTERVAL_1_HOUR: u64 = 3600;
    /// 2 hours in seconds
    pub const INTERVAL_2_HOURS: u64 = 7200;

    /// Data staleness thresholds
    pub const DATA_STALE_THRESHOLD_SECS: u64 = 60;
    /// Data considered old after 5 minutes
    pub const DATA_OLD_THRESHOLD_SECS: u64 = 300;
}

/// Memory and buffer size constants
pub mod memory {
    /// Standard memory page size (1KB)
    pub const KB: usize = 1024;

    /// Megabyte in bytes
    pub const MB: usize = 1024 * 1024;

    /// Gigabyte in bytes
    pub const GB: usize = 1024 * 1024 * 1024;

    /// Kilobyte in bytes (u64 for file operations)
    pub const BYTES_PER_KB: u64 = 1024;
    /// Megabyte in bytes (u64 for file operations)
    pub const BYTES_PER_MB: u64 = 1024 * 1024;
    /// Gigabyte in bytes (u64 for file operations)
    pub const BYTES_PER_GB: u64 = 1024 * 1024 * 1024;

    /// Default WAL segment size (128MB)
    pub const DEFAULT_WAL_SEGMENT_SIZE: usize = 128 * MB;

    /// Default WAL segment size in MB  
    pub const DEFAULT_WAL_SEGMENT_SIZE_MB: usize = 128;

    /// CPU cache line size (64 bytes for `x86_64`)
    pub const CACHE_LINE_SIZE: usize = 64;

    /// Half cache line size
    pub const HALF_CACHE_LINE: usize = 32;

    /// Default buffer capacity for collections
    pub const DEFAULT_BUFFER_CAPACITY: usize = 1000;

    /// Large buffer capacity
    pub const LARGE_BUFFER_CAPACITY: usize = 10000;

    /// Small buffer capacity
    pub const SMALL_BUFFER_CAPACITY: usize = 100;

    /// Ring buffer sizes
    pub const RING_BUFFER_SIZE_SMALL: usize = 1024;
    /// Medium ring buffer (8KB)
    pub const RING_BUFFER_SIZE_MEDIUM: usize = 8192;
    /// Large ring buffer (64KB)
    pub const RING_BUFFER_SIZE_LARGE: usize = 65536;

    /// WAL replay batch size - yield control every N events to prevent blocking
    pub const WAL_REPLAY_BATCH_SIZE: u64 = 10000;

    /// Arena allocator sizes
    /// Small arena allocator (1KB)
    pub const ARENA_SIZE_SMALL: usize = 1024;
    /// Medium arena allocator (8KB)
    pub const ARENA_SIZE_MEDIUM: usize = 8192;
    /// Large arena allocator (64KB)
    pub const ARENA_SIZE_LARGE: usize = 65536;
}

/// Numeric constants for common operations
pub mod numeric {
    /// Zero value for initialization
    pub const ZERO: usize = 0;
    /// One value for increments
    pub const ONE: usize = 1;
    /// Zero value for i64
    pub const ZERO_I64: i64 = 0;
    /// Zero value for u64
    pub const ZERO_U64: u64 = 0;
    /// Zero value for f64 comparisons
    pub const ZERO_F64: f64 = 0.0;
    /// Default initial counter value
    pub const INITIAL_COUNTER: usize = 0;
    /// Default increment value
    pub const INCREMENT: usize = 1;
    /// First index in arrays
    pub const FIRST_INDEX: usize = 0;
    /// Second index in arrays
    pub const SECOND_INDEX: usize = 1;
    /// Minimum positive value
    pub const MIN_POSITIVE: usize = 1;
}

/// Collection capacity constants
pub mod capacity {
    /// Standard collection pre-allocation sizes
    pub const TINY: usize = 8;
    /// Small collection capacity
    pub const SMALL: usize = 20;
    /// Medium collection capacity
    pub const MEDIUM: usize = 50;
    /// Large collection capacity
    pub const LARGE: usize = 100;
    /// Extra large collection capacity
    pub const XLARGE: usize = 200;
    /// Huge collection capacity
    pub const HUGE: usize = 500;
    /// Massive collection capacity
    pub const MASSIVE: usize = 1000;

    /// Progress report interval for long operations
    pub const PROGRESS_REPORT_INTERVAL: usize = 1000;

    /// Specific collection sizes
    pub const ORDER_FILLS_CAPACITY: usize = 8;
    /// Symbol risk tracking capacity
    pub const SYMBOL_RISKS_CAPACITY: usize = 100;
    /// Order book depth levels
    pub const ORDER_BOOK_DEPTH_CAPACITY: usize = 20;
    /// Instrument tokens capacity
    pub const TOKENS_CAPACITY: usize = 200;
    /// Options chain capacity
    pub const OPTIONS_CAPACITY: usize = 50;
    /// Update buffer capacity
    pub const UPDATE_BUFFER_CAPACITY: usize = 100;
    /// Events buffer capacity
    pub const EVENTS_CAPACITY: usize = 1000;
    /// Snapshot storage capacity
    pub const SNAPSHOTS_CAPACITY: usize = 1000;
    /// Venue string collection capacity
    pub const VENUE_STRING_CAPACITY: usize = 100;
    /// ROI data buffer capacity
    pub const ROI_DATA_CAPACITY: usize = 1000;
    /// VWAP calculation buffer capacity
    pub const VWAP_BUFFER_CAPACITY: usize = 1000;
    /// Volatility window data capacity
    pub const VOLATILITY_WINDOW_CAPACITY: usize = 100;
}

/// Trading and risk management constants
pub mod trading {
    use super::fixed_point::SCALE_4;

    /// Side constants for efficient representation
    /// Buy side represented as 0
    pub const SIDE_BUY: u8 = 0;
    /// Sell side represented as 1
    pub const SIDE_SELL: u8 = 1;

    /// Maximum position size in ticks (10000 * 10000)
    pub const MAX_POSITION_SIZE_TICKS: i64 = 100_000_000;

    /// Maximum order size in ticks (1000 * 10000)
    pub const MAX_ORDER_SIZE_TICKS: i64 = 10_000_000;

    /// Rate limiting window in milliseconds (60 seconds)
    pub const RATE_LIMIT_WINDOW_MS: u64 = 60_000;

    /// Default order book depth levels
    pub const DEFAULT_BOOK_DEPTH: usize = 32;

    /// Maximum order book depth levels
    pub const MAX_BOOK_DEPTH: usize = 100;

    /// Minimum order quantity (1 unit in fixed-point)
    pub const MIN_ORDER_QTY: i64 = SCALE_4;

    /// Maker fee in basis points (10 bp = 0.1%)
    pub const MAKER_FEE_BP: i64 = 10;

    /// Taker fee in basis points (20 bp = 0.2%)
    pub const TAKER_FEE_BP: i64 = 20;

    /// Maximum orders per symbol
    pub const MAX_ORDERS_PER_SYMBOL: usize = 100;

    /// Default maximum positions for trading engine
    pub const DEFAULT_MAX_POSITIONS: usize = 1000;

    /// Default maximum orders per second rate limit
    pub const DEFAULT_MAX_ORDERS_PER_SEC: u32 = 1000;
}

/// Financial precision constants
pub mod financial {
    /// Standard tick size for equities (1 paisa)
    pub const EQUITY_TICK_SIZE: f64 = 0.01;

    /// NIFTY tick size (5 paise)
    pub const NIFTY_TICK_SIZE: f64 = 0.05;

    /// Standard crypto tick size
    pub const CRYPTO_TICK_SIZE: f64 = 0.01;

    /// EMA beta coefficient (default)
    pub const DEFAULT_EMA_BETA: f64 = 0.95;

    /// Percentage scale factor
    pub const PERCENT_SCALE: f64 = 100.0;

    /// Basis point scale factor
    pub const BASIS_POINT_SCALE: f64 = 10000.0;

    /// Default center price for ROI calculations (NIFTY approximate)
    pub const DEFAULT_ROI_CENTER_PRICE: f64 = 25000.0;
    /// Strike price scale for fixed-point representation (2 decimal places)
    pub const STRIKE_PRICE_SCALE: f64 = 100.0;

    /// Default ROI width for order book
    pub const DEFAULT_ROI_WIDTH: f64 = 1000.0;

    /// Default quantity for synthetic LOB updates
    pub const DEFAULT_SYNTHETIC_QTY: f64 = 1.0;
}

/// Lot size constants for different instruments
pub mod lot_sizes {
    /// NIFTY lot size
    pub const NIFTY_LOT_SIZE: u32 = 25;

    /// BANK NIFTY lot size  
    pub const BANK_NIFTY_LOT_SIZE: u32 = 15;

    /// Default lot size for equity
    pub const DEFAULT_LOT_SIZE: u32 = 1;
}

/// Benchmarking constants
pub mod bench {
    use super::memory::MB;

    /// Default pool size for benchmarks
    pub const BENCH_POOL_SIZE: usize = 1000;

    /// Default arena size for benchmarks (1MB)
    pub const BENCH_ARENA_SIZE: usize = MB;

    /// Number of benchmark iterations
    pub const BENCH_ITERATIONS: usize = 1000;

    /// Sample size for benchmarks
    pub const BENCH_SAMPLE_SIZE: usize = 100;
}

/// Demo and testing constants
pub mod demo {
    /// Default demo rate limit per user
    pub const DEFAULT_DEMO_RATE_LIMIT: u32 = 100;

    /// Demo event buffer size for UI display
    pub const DEMO_EVENT_BUFFER_SIZE: usize = 100;

    /// Demo market data channel capacity
    pub const DEMO_CHANNEL_CAPACITY: usize = 1000;

    /// Demo display limit for recent items
    pub const DEMO_DISPLAY_LIMIT: usize = 10;
}

/// Network and service configuration constants
pub mod network {
    /// Default reconnection delay in milliseconds
    pub const DEFAULT_RECONNECT_DELAY_MS: u64 = 5000;

    /// Default connection timeout in milliseconds (30 seconds)
    pub const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 30000;

    /// Default request timeout in milliseconds (10 seconds)
    pub const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 10000;

    /// Maximum retry attempts
    pub const MAX_RETRY_ATTEMPTS: u32 = 3;

    /// Initial retry delay in milliseconds
    pub const INITIAL_RETRY_DELAY_MS: u64 = 100;

    /// Maximum retry delay in milliseconds
    pub const MAX_RETRY_DELAY_MS: u64 = 5000;

    /// Default gRPC server port
    pub const DEFAULT_GRPC_PORT: u16 = 50051;

    /// Default HTTP server port
    pub const DEFAULT_HTTP_PORT: u16 = 8080;

    /// Alternative HTTP port
    pub const ALT_HTTP_PORT: u16 = 9000;

    /// Development HTTP port
    pub const DEV_HTTP_PORT: u16 = 3000;

    /// Maximum concurrent connections
    pub const MAX_CONCURRENT_CONNECTIONS: usize = 1000;

    /// WebSocket ping interval in seconds
    pub const WS_PING_INTERVAL_SECS: u64 = 30;

    /// HTTP request timeout in seconds (short)
    pub const HTTP_SHORT_TIMEOUT_SECS: u64 = 10;

    /// HTTP request timeout in seconds (medium)
    pub const HTTP_MEDIUM_TIMEOUT_SECS: u64 = 30;

    /// Default rate limit per minute
    pub const DEFAULT_RATE_LIMIT_PER_MINUTE: u32 = 60;

    /// Maximum body size (1MB)
    pub const MAX_HTTP_BODY_SIZE: usize = 1024 * 1024;

    /// WebSocket broadcast channel capacity
    pub const WS_BROADCAST_CHANNEL_SIZE: usize = 1000;

    /// Maximum rate limiters to track
    pub const MAX_RATE_LIMITERS: usize = 10000;

    /// Default connection timeout in seconds
    pub const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 30;

    /// Default request timeout in seconds  
    pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 10;

    /// Maximum reconnection attempts
    pub const MAX_RECONNECT_ATTEMPTS: u32 = 3;

    /// Reconnection backoff in milliseconds
    pub const RECONNECT_BACKOFF_MS: u64 = 1000;

    /// Event buffer size for streaming
    pub const EVENT_BUFFER_SIZE: usize = 1000;

    /// Default heartbeat interval in seconds
    pub const DEFAULT_HEARTBEAT_INTERVAL_SECS: u64 = 30;

    /// Maximum backoff in milliseconds
    pub const MAX_BACKOFF_MS: u64 = 30000;
}

/// Calendar constants
pub mod calendar {
    /// January month number
    pub const JANUARY: u32 = 1;
    /// February month number
    pub const FEBRUARY: u32 = 2;
    /// March month number
    pub const MARCH: u32 = 3;
    /// April month number
    pub const APRIL: u32 = 4;
    /// May month number
    pub const MAY: u32 = 5;
    /// June month number
    pub const JUNE: u32 = 6;
    /// July month number
    pub const JULY: u32 = 7;
    /// August month number
    pub const AUGUST: u32 = 8;
    /// September month number
    pub const SEPTEMBER: u32 = 9;
    /// October month number
    pub const OCTOBER: u32 = 10;
    /// November month number
    pub const NOVEMBER: u32 = 11;
    /// December month number
    pub const DECEMBER: u32 = 12;
}

/// Market hours and session constants
pub mod market {
    /// Market open hour (9 AM)
    pub const MARKET_OPEN_HOUR: u8 = 9;

    /// Market open minute (15 minutes)
    pub const MARKET_OPEN_MINUTE: u8 = 15;

    /// Market close hour (3 PM)
    pub const MARKET_CLOSE_HOUR: u8 = 15;

    /// Market close minute (30 minutes)
    pub const MARKET_CLOSE_MINUTE: u8 = 30;

    /// NSE/BSE market close time - hour component
    pub const NSE_CLOSE_HOUR: u32 = 15;

    /// NSE/BSE market close time - minute component  
    pub const NSE_CLOSE_MINUTE: u32 = 30;

    /// NSE/BSE market close time - second component
    pub const NSE_CLOSE_SECOND: u32 = 0;

    /// Pre-market duration in minutes
    pub const PRE_MARKET_DURATION_MINS: u16 = 15;

    /// Post-market duration in minutes
    pub const POST_MARKET_DURATION_MINS: u16 = 10;

    /// Default option strike range (number of strikes above/below spot)
    pub const DEFAULT_OPTION_STRIKE_RANGE: u32 = 10;

    /// NIFTY strike interval in points
    pub const NIFTY_STRIKE_INTERVAL: f64 = 50.0;
}

/// Statistical and mathematical constants
pub mod math {
    /// Square root of 252 (trading days) for annualized calculations
    pub const SQRT_TRADING_DAYS: f64 = 15.874_507_866_387_544;

    /// Number of trading days per year
    pub const TRADING_DAYS_PER_YEAR: u16 = 252;

    /// Number of trading minutes per day (375 for Indian markets)
    pub const TRADING_MINS_PER_DAY: u16 = 375;

    /// Default fetch interval in hours
    pub const DEFAULT_FETCH_INTERVAL_HOURS: u64 = 24;

    /// Default fetch hour (8 AM IST)
    pub const DEFAULT_FETCH_HOUR: u32 = 8;

    /// Default confidence level for `VaR` calculations
    pub const DEFAULT_VAR_CONFIDENCE: f64 = 0.95;

    /// Default window size for moving averages
    pub const DEFAULT_MA_WINDOW: usize = 20;

    /// Minimum data points for statistics
    pub const MIN_DATA_POINTS: usize = 2;

    /// Maximum error log entries before suppression
    pub const MAX_ERROR_LOG_ENTRIES: usize = 10;

    /// Fetch window in minutes for recent data
    pub const FETCH_WINDOW_MINUTES: u32 = 5;

    /// Bit shift for f64 precision check (2^53)
    pub const F64_PRECISION_BITS: u32 = 53;
}
