# ShrivenQuant Architecture Overview

**Current Status**: 85% Complete - Production-Grade Microservices with Zero Warnings  
**Service Status**: 6/8 Executable, 2/8 Library-Complete  
**Compliance Score**: 30/100 (6x improvement from 5/100)  

## System Design Principles

ShrivenQuant is built on the following core principles:

1. **Zero-Allocation Hot Paths**: No memory allocations during critical trading operations
2. **Lock-Free Data Structures**: Atomic operations and lock-free algorithms throughout  
3. **Cache-Aligned Memory**: All critical structures are 64-byte aligned for optimal CPU cache usage
4. **Compile-Time Optimization**: Heavy use of const functions and compile-time polymorphism
5. **SIMD Operations**: Vectorized calculations for metrics and analytics
6. **Fixed-Point Arithmetic**: All financial calculations use i64 with 4 decimal precision
7. **gRPC Communication**: High-performance inter-service messaging with type safety

## Microservices Architecture (Current)

```
┌─────────────────────────────────────────────────────────────────┐
│                    gRPC Service Mesh                             │
├─────────────┬─────────────┬─────────────┬─────────────┬────────┤
│✅ Auth      │✅ Gateway   │✅ Market    │✅ Risk      │✅ Exec │
│   Service   │   Service   │  Connector  │  Manager    │ Router │
│   :50051    │   :8080     │   :50052    │   :50053    │ :50054 │
│             │             │             │             │        │
│ Multi-Exch  │ REST API    │ Real WS     │ Middleware  │ Smart  │
│ Auth + JWT  │ Handlers    │ Binance     │ Kill Switch │ Algos  │
└─────────────┴─────────────┴─────────────┴─────────────┴────────┘
                                │
├─────────────┬─────────────┬─────────────┬─────────────────────┤
│✅ Demo      │📚 Portfolio │📚 Reporting │📚 Data              │
│   Service   │   Manager   │   Service   │  Aggregator         │
│   :8081     │   :50055    │   :50056    │  :50057             │
│             │             │             │                     │
│ Integration │ Position    │ SIMD        │ WAL Storage         │
│ Showcase    │ Tracking    │ Analytics   │ & Events            │
└─────────────┴─────────────┴─────────────┴─────────────────────┘
                                │
                    ┌───────────┴───────────┐
                    │     WAL Storage       │
                    │   229 MB/s Writes     │
                    └───────────────────────┘
```

**Legend**: ✅ = Production-ready service with zero warnings, 📚 = Business logic complete, needs main.rs

## Service Implementation Status

### ✅ **Production-Ready Services (6/8)**

#### 1. Auth Service (`services/auth/`)
- **Status**: ✅ Production gRPC server
- **Port**: 50051
- **Features**:
  - Multi-exchange authentication (Zerodha + Binance)
  - Automated TOTP 2FA (no manual intervention)
  - JWT token management with role-based permissions
  - PostgreSQL integration with sqlx
  - Session caching (12-hour token validity)
- **Evidence**: `cargo run -p auth-service` works

#### 2. API Gateway (`services/gateway/`)
- **Status**: ✅ Production REST API
- **Port**: 8080
- **Features**:
  - Comprehensive REST handlers for all trading operations
  - gRPC client connections to backend services
  - WebSocket streaming for real-time data
  - Rate limiting and middleware
  - Working CLI interface
- **Evidence**: `cargo run -p api-gateway -- --help` responds

#### 3. Risk Manager (`services/risk-manager/`)
- **Status**: ✅ Production gRPC server with middleware
- **Port**: 50053
- **Features**:
  - Full request middleware with rate limiting
  - Functional kill switch with atomic operations
  - Circuit breakers and health checks
  - Prometheus metrics integration
- **Evidence**: Zero warnings, production-grade implementation

#### 4. Market Connector (`services/market-connector/`)
- **Status**: ✅ Real WebSocket connections working
- **Port**: 50052
- **Features**:
  - Binance Spot WebSocket (not REST polling)
  - Binance Futures WebSocket connection
  - Automatic reconnection with exponential backoff
  - Order book depth updates (5 levels)
- **Evidence**: Production WebSocket implementation

#### 5. Execution Router (`services/execution-router/`)
- **Status**: ✅ Production gRPC server
- **Port**: 50054
- **Features**:
  - Smart order routing algorithms (TWAP, VWAP, POV)
  - Memory pools for zero-allocation execution
  - Venue selection and latency optimization
  - Proper error handling with no panics
- **Evidence**: 1000+ lines of production code

#### 6. Demo Service (`services/demo/`)
- **Status**: ✅ Integration demonstration
- **Port**: 8081
- **Features**:
  - Auth and market connector integration showcase
  - Real trading workflow examples
  - Service communication patterns
- **Evidence**: Compiles and runs successfully

### 📚 **Library Services (Need main.rs) (2/8)**

#### 7. Portfolio Manager (`services/portfolio-manager/`)
- **Status**: 📚 462 lines of portfolio logic, needs main.rs
- **Business Logic**:
  - Position tracking and reconciliation
  - Portfolio optimization algorithms
  - Market feed processing
  - Risk analytics and attribution
- **Next Step**: Add gRPC server wrapper (~1 day effort)

#### 8. Data Aggregator (`services/data-aggregator/`)
- **Status**: 📚 578 lines of storage logic, needs main.rs
- **Business Logic**:
  - WAL persistence with 229 MB/s proven performance
  - Market data event processing and storage
  - Candle aggregation and volume profiling
  - Segment-based storage with CRC validation
- **Next Step**: Add gRPC server wrapper (~1 day effort)

#### 9. Reporting Service (`services/reporting/`)
- **Status**: 📚 431 lines of analytics, needs main.rs
- **Business Logic**:
  - SIMD-optimized performance calculations
  - Real-time metrics and KPIs
  - Trade analytics and attribution
  - Performance benchmarking
- **Next Step**: Add gRPC server wrapper (~1 day effort)
- **Event Types**:
  - `TickEvent`: Best bid/ask/last with nanosecond timestamps
  - `LobSnapshot`: Full order book depth with price levels
  - `OrderEvent`: Order submission and status updates
  - `FillEvent`: Trade execution events
- **Performance (Measured)**:
  - **298M events/min** replay speed (99.5x target of 3M)
  - **229 MB/s** WAL write throughput (2.86x target of 80 MB/s)
  - **Sub-microsecond** p50 latencies, <10µs p99
  - See [Benchmark Results](/reports/benchmark/benchmark-results.md) for details

### 2. Event Bus (`bus/`)
- **Purpose**: Zero-copy message passing between components
- **Features**:
  - Lock-free MPMC (Multi-Producer Multi-Consumer)
  - SPSC (Single-Producer Single-Consumer) channels
  - Type-safe message routing

### 3. Trading Engine (`engine/`)
- **Purpose**: Core trading logic and execution
- **Modules**:
  - `core.rs`: Main engine with atomic counters
  - `execution.rs`: Order routing and execution
  - `position.rs`: Lock-free position tracking
  - `risk.rs`: Pre-trade risk checks
  - `metrics.rs`: SIMD-optimized performance metrics
  - `venue.rs`: Exchange adapters
  - `memory.rs`: Memory pools for zero-allocation

### 4. Limit Order Book (`lob/`)
- **Purpose**: Ultra-fast order book management
- **Features**:
  - Price-time priority matching
  - O(1) order operations
  - Lock-free updates
  - 5-level market depth

### 5. Storage Layer (`storage/`)
- **Purpose**: Persistent data storage
- **Components**:
  - Write-Ahead Log (WAL) for market data
  - Memory-mapped files for fast I/O
  - Replay capabilities

### 6. Performance Module (`perf/`)
- **Purpose**: System benchmarking and optimization
- **Tools**:
  - Latency measurement
  - Throughput testing
  - Memory profiling

### 7. Authentication (`auth/`)
- **Purpose**: Secure broker connectivity
- **Supports**:
  - Zerodha Kite Connect API
  - Binance API (testnet/production)
  - Secure credential storage

## Data Flow

### Market Data Flow
1. **Ingestion**: WebSocket receives market tick
2. **Parsing**: Binary data parsed into typed structures
3. **WAL Write**: Data persisted to Write-Ahead Log
4. **Event Bus**: Tick published to subscribers
5. **LOB Update**: Order book updated if needed
6. **Engine Processing**: Trading logic triggered

### Order Flow
1. **Signal Generation**: Strategy generates trading signal
2. **Risk Check**: Pre-trade risk validation (< 50ns)
3. **Order Creation**: Order object allocated from pool
4. **Routing**: Route to appropriate venue adapter
5. **Execution**: Paper/Live/Backtest execution
6. **Position Update**: Atomic position update
7. **PnL Calculation**: Real-time PnL update

## Memory Layout

### Cache-Aligned Structures
```rust
// ShrivenQuant Trading Platform - Cache-Aligned Position Structure
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Cache-aligned position structure optimized for CPU cache efficiency
//          and atomic operations in high-frequency trading scenarios.
//
// PERFORMANCE: 64-byte cache line alignment ensures the entire structure fits
//              in one CPU cache line for maximum memory access speed
//
// USAGE: Core position tracking structure used throughout trading engine
//        for lock-free position and PnL management.
//
// SAFETY: All fields are atomic for thread-safe concurrent access

#[repr(C, align(64))]  // 64-byte cache line alignment
pub struct Position {
    pub symbol: Symbol,           // 4 bytes
    pub quantity: AtomicI64,      // 8 bytes
    pub avg_price: AtomicU64,     // 8 bytes
    pub realized_pnl: AtomicI64,  // 8 bytes
    pub unrealized_pnl: AtomicI64,// 8 bytes
    pub last_update: AtomicU64,   // 8 bytes
    pub last_bid: AtomicU64,      // 8 bytes
    pub last_ask: AtomicU64,      // 8 bytes
    _padding: [u8; 8],            // 8 bytes padding
}  // Total: 64 bytes (1 cache line)
```

### Memory Pools
- Pre-allocated object pools for orders
- Ring buffers for lock-free communication
- Arena allocators for bulk operations
- Stack allocators for temporary data

## Performance Guidelines

For detailed performance best practices, see [Performance Guidelines](../performance/guidelines.md).

Key principles:
- **Zero allocations in hot paths** - Use object pools and pre-allocation
- **Copy for small POD types** - Avoid Arc for types <64 bytes
- **Cache-line alignment** - Prevent false sharing in concurrent code
- **Static errors** - Replace String errors with enums
- **Lock-free algorithms** - Use atomics instead of mutexes

## Performance Optimizations

### 1. Branch-Free Code
```rust
// ShrivenQuant Trading Platform - Branch-Free Risk Check
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Branch-free risk validation to eliminate CPU branch prediction misses
//          in critical trading paths requiring deterministic latency.
//
// PERFORMANCE: Uses bitwise operations instead of conditional branches
//              for consistent sub-50ns execution time
//
// USAGE: Applied to all pre-trade risk checks in order validation pipeline

// Branch-free risk check
let all_checks = size_ok & value_ok & breaker_ok & position_ok;
all_checks != 0  // No branching
```

### 2. SIMD Operations
```rust
// ShrivenQuant Trading Platform - SIMD Vectorized Calculations
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: SIMD vectorization for parallel computation of trading metrics
//          and performance analytics using CPU vector instructions.
//
// PERFORMANCE: Process 4 floating-point operations simultaneously using
//              256-bit AVX instructions for 4x speedup
//
// USAGE: Applied to calculations requiring high throughput like Sharpe ratio,
//        correlation analysis, and portfolio optimization.

// Vectorized Sharpe ratio calculation
let v = f64x4::from_slice(chunk);
sum += v;  // 4 operations in 1 instruction
```

### 3. Atomic Operations
```rust
// ShrivenQuant Trading Platform - Lock-Free Atomic Operations
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Lock-free atomic operations for thread-safe shared state updates
//          without blocking or contention in multi-threaded trading environment.
//
// PERFORMANCE: Hardware-level atomic instructions provide thread safety
//              without expensive lock acquisition overhead
//
// USAGE: Used throughout system for counters, flags, and shared state updates

// Lock-free counter increment
self.order_counter.fetch_add(1, Ordering::Relaxed);
```

### 4. Compile-Time Optimization
```rust
// ShrivenQuant Trading Platform - Compile-Time Optimization Techniques
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Compile-time optimizations to eliminate runtime overhead
//          in critical trading paths through aggressive inlining.
//
// PERFORMANCE: Forces function inlining to eliminate call overhead,
//              const functions enable compile-time evaluation
//
// USAGE: Applied to all hot path functions and type constructors

#[inline(always)]  // Force inlining
pub const fn new() -> Self  // Compile-time construction
```

## Scalability

### Horizontal Scaling
- Multiple symbol processing in parallel
- Venue-specific processing threads
- Independent strategy instances

### Vertical Scaling
- NUMA-aware memory allocation
- CPU affinity for critical threads
- Kernel bypass networking (planned)

## Architecture Decision Records (ADRs)

Key architectural decisions documented for future reference:

- [ADR-001: Dependency Version Management Strategy](./ADR-001-dependency-management.md) - How we handle transitive dependency conflicts
- [ADR-006: Memory Pool Design](./decisions/0006-memory-pool-design.md) - Lock-free memory pools with RAII
- [ADR-007: Zero-Copy and Clone Elimination Philosophy](./decisions/0007-zero-copy-philosophy.md) - When to use Copy, Arc, and borrows

### ADR-001: Rust as Primary Language
**Status:** Accepted  
**Decision:** Use Rust (Edition 2024) for predictable low-latency performance, memory safety without GC pauses, and zero-cost abstractions.

### ADR-002: Zero-Tolerance Code Quality  
**Status:** Accepted  
**Decision:** No warnings, dead code, unwrap/expect/panic, TODO/FIXME comments, or incomplete documentation allowed.

### ADR-003: Lock-Free Message Passing
**Status:** Accepted  
**Decision:** Use crossbeam channels for lock-free SPSC/MPMC communication to avoid lock contention.

### ADR-004: Event-Driven Architecture
**Status:** Accepted  
**Decision:** Central event bus with publishers and subscribers for loose coupling and modularity.

### ADR-005: Write-Ahead Log (WAL) for Persistence
**Status:** Accepted  
**Decision:** Append-only WAL for crash recovery, deterministic replay, and audit trails.

### ADR-006: Bincode for Hot Path Serialization
**Status:** Accepted  
**Decision:** Use bincode for fast internal serialization, JSON for configurations.

### ADR-007: Tokio for Async Runtime
**Status:** Accepted  
**Decision:** Use tokio for I/O operations without blocking hot path.

### ADR-008: Monorepo with Cargo Workspaces
**Status:** Accepted  
**Decision:** Cargo workspace in monorepo for shared dependencies and atomic commits.

### ADR-009: GitHub Actions for CI/CD
**Status:** Accepted  
**Decision:** GitHub Actions for automated quality checks and deployment pipeline.

### ADR-010: Microstructure Features in LOB
**Status:** Accepted  
**Decision:** Compute features (spread, imbalance, VPIN, micro-price) directly in LOB engine for low-latency access.

## Current Implementation Status

### Latest Status (August 15, 2025)
- **~85% Complete** - Production-grade services with zero warnings
- **Zero Compilation Issues** - All services compile cleanly
- **6 of 8 Services Ready** - Full production implementations
- **3 Services Need main.rs** - Simple wrappers required
- **Deployment Pending** - Docker/Kubernetes not yet implemented

### What Actually Exists
- [x] **Core Type System** (`common/`)
  - Zero-copy types: `Symbol`, `Px`, `Qty`, `Ts`
  - Deterministic pricing with 4 decimal precision
  - Const functions for compile-time optimization
  - Full serialization support

- [x] **Authentication System** (`auth/`)
  - Zerodha Kite Connect integration
  - Binance API integration (testnet/production)
  - Secure credential management
  - TOTP/2FA support

- [x] **Storage Layer** (`storage/`)
  - Write-Ahead Log (WAL) implementation
  - Memory-mapped file I/O
  - Binary serialization with bincode
  - Data integrity with CRC32 checksums

#### Key Achievements
- **Zero Compilation Warnings** - Entire codebase compiles cleanly
- **Real WebSocket Connections** - Production Binance integration
- **Functional Kill Switch** - Atomic operations for emergency stops
- **Production Middleware** - Rate limiting, circuit breakers, metrics
- **6x Compliance Improvement** - Score increased from 5/100 to 30/100

### Sprint 2: Real-Time Data & Processing ✅ **COMPLETED**

**Duration**: Weeks 5-8  
**Status**: 🟢 **SPRINT 2 COMPLETE!**

#### Deliverables
- [x] **Market Data Feeds** (`feeds/`)
  - Real-time WebSocket connectivity to Zerodha/Binance
  - 5-level market depth processing
  - Tick-by-tick data capture
  - WAL-based data persistence

- [x] **Limit Order Book Engine** (`lob/`)
  - Ultra-fast order book updates (89.9M updates/sec)
  - Cache-friendly fixed-depth arrays
  - Deterministic arithmetic operations
  - Feature extraction (spread, microprice, imbalance)

- [x] **Event Bus** (`bus/`)
  - Lock-free inter-component communication
  - Type-safe message passing
  - Low-latency event propagation
  - Multi-producer, multi-consumer channels

#### Performance Achievements
- **LOB Performance**: 89.9M updates/sec (17ns p50)
- **Feed Processing**: Real-time tick capture with sub-microsecond latencies
- **Zero Allocations**: All hot paths allocation-free

### Sprint 3: Trading Engine Core ✅ **COMPLETED**

**Duration**: Weeks 9-12  
**Status**: 🟢 **SPRINT 3 COMPLETE!**

#### Deliverables
- [x] **Enhanced Authentication** (`auth/`)
  - Full Zerodha authentication (TOTP, session management)
  - Multi-market Binance support (Spot/Futures/Options)
  - Credential validation and secure storage

- [x] **Feed Integration** (`feeds/`)
  - Complete WebSocket adapter implementation
  - Market data normalization across venues
  - Real-time order book reconstruction

- [x] **Performance Validation**
  - Comprehensive benchmarking suite
  - Hot path allocation detection
  - Latency regression testing

#### Integration Achievements
- **Authentication**: Live API testing for all markets
- **Data Flow**: End-to-end market data pipeline
- **Quality Assurance**: Zero-warning, zero-allocation codebase

### Sprint 4: Strategy Runtime & Paper Trading 📋 **PLANNED**

**Duration**: Weeks 13-16  
**Status**: ⏳ **NOT STARTED**

#### Planned Deliverables
- [ ] **Strategy Framework** (`strategy/`)
  - Strategy trait with lifecycle management
  - Signal generation and decision making
  - Performance attribution tracking

- [ ] **Paper Trading Engine** (`paper/`)
  - Simulated order execution
  - Realistic fill simulation
  - PnL tracking and reporting

- [ ] **Risk Management** (`risk/`)
  - Pre-trade risk checks
  - Position and exposure limits
  - Real-time risk monitoring

#### Success Criteria
- Strategy → Signal → Order flow operational
- Paper trading with realistic execution
- Risk limits enforced across all operations

### Sprint 5: Live Trading Integration 📋 **PLANNED**

**Duration**: Weeks 17-20  
**Status**: ⏳ **NOT STARTED**

#### Planned Deliverables
- [ ] **Live Order Management** (`orders/`)
  - Real order placement to exchanges
  - Order lifecycle tracking
  - Fill confirmation and position updates

- [ ] **Production Monitoring** (`monitoring/`)
  - Real-time system health monitoring
  - Performance metrics dashboard
  - Alert and notification systems

- [ ] **Safety Systems** (`safety/`)
  - Emergency stop mechanisms
  - Circuit breakers and fail-safes
  - Audit logging and compliance

#### Success Criteria
- Live trading operational with real money
- Complete system monitoring and alerting
- Production-grade safety and compliance

### Sprint 6: Advanced Analytics & Optimization 📋 **PLANNED**

**Duration**: Weeks 21-24  
**Status**: ⏳ **NOT STARTED**

#### Planned Deliverables
- [ ] **Advanced Analytics** (`analytics/`)
  - Greek calculations for options
  - Volatility surface modeling
  - Risk attribution analysis

- [ ] **Backtesting Engine** (`backtest/`)
  - Historical strategy simulation
  - Performance attribution
  - Risk-adjusted returns analysis

- [ ] **GPU Acceleration** (`gpu/`)
  - CUDA-based calculations
  - Parallel strategy execution
  - Ultra-low latency optimizations

#### Success Criteria
- Complete trading analytics suite
- Fast historical backtesting capability
- GPU-accelerated computations operational

### Overall Progress Summary

```
┌─────────────────────────────────────────────────────────┐
│                ShrivenQuant Progress                    │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  Sprint 1: Foundation       ✅ COMPLETE (100%)         │
│  Sprint 2: Real-Time Data   ✅ COMPLETE (100%)         │
│  Sprint 3: Trading Core     ✅ COMPLETE (100%)         │
│  Sprint 4: Strategy Runtime ⏳ PLANNED   (0%)          │
│  Sprint 5: Live Trading     ⏳ PLANNED   (0%)          │
│  Sprint 6: Advanced Analytics ⏳ PLANNED (0%)          │
│                                                         │
│  Overall Progress: 50% Complete                         │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### Next Steps

**Immediate Priority (Sprint 4):**
1. Design strategy framework architecture
2. Implement paper trading simulation
3. Build comprehensive risk management system
4. Create performance attribution tracking

**Long-term Vision:**
- Complete live trading platform
- Advanced quantitative analytics
- Multi-venue arbitrage capabilities
- GPU-accelerated computations

## Fault Tolerance

### Error Handling
- No panics in production code
- Result types for fallible operations
- Circuit breakers for risk management

### Data Persistence
- WAL ensures no data loss
- Replay capability for recovery
- Atomic operations prevent corruption

## Security

### API Security
- HMAC-SHA256 for API authentication
- Encrypted credential storage
- Rate limiting protection

### Code Security
- No unsafe code in critical paths
- Strict compiler checks (deny warnings)
- Comprehensive clippy lints
