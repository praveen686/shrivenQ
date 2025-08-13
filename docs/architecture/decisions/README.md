# Architecture Decision Records (ADRs)

This directory contains Architecture Decision Records (ADRs) documenting important design decisions made in the ShrivenQuant trading system.

## Index

| ADR | Title | Status | Date |
|-----|-------|--------|------|
| [ADR-0003](0003-fixed-point-arithmetic.md) | Fixed-Point Arithmetic | Accepted | 2024 |
| [ADR-0004](0004-display-utilities.md) | Display Utilities Pattern | Accepted | 2024 |
| [ADR-0005](0005-feature-calculations.md) | Numeric Policy for Feature Calculations | Accepted | 2024 |
| [ADR-0006](0006-memory-pool-design.md) | Lock-Free Memory Pool Design | Accepted | 2024 |

## ADR Format

Each ADR follows this structure:
- **Status**: Proposed, Accepted, Deprecated, Superseded
- **Context**: The issue motivating this decision
- **Decision**: The change we're making
- **Consequences**: What becomes easier or harder
- **Alternatives**: Other options considered

## Key Design Principles

### 1. Performance First
All decisions prioritize ultra-low latency and predictable performance.

### 2. Safety Without Compromise
Memory safety and correctness are enforced at compile time where possible.

### 3. Zero-Cost Abstractions
Abstractions should have no runtime overhead compared to hand-written code.

### 4. Explicit Over Implicit
Make performance costs visible and explicit in the API.

## Critical Path Decisions

### Numeric Precision (ADR-0003, ADR-0005)
- **Core Path**: Fixed-point arithmetic for prices and quantities
- **Analytics**: Controlled f64 conversions at boundaries
- **Display**: Centralized conversion utilities

### Memory Management (ADR-0006)
- **Object Pools**: Lock-free pools for zero-allocation
- **RAII**: Automatic resource management
- **ABA Prevention**: Tagged pointers with generation counters

## Performance Impact

| Decision | Latency Impact | Memory Impact | Safety Impact |
|----------|---------------|---------------|---------------|
| Fixed-Point Math | ✅ Predictable | ✅ Compact | ✅ No precision loss |
| Memory Pools | ✅ ~20ns acquire | ✅ Pre-allocated | ✅ RAII cleanup |
| Display Utils | ⚠️ Boundary only | → Neutral | ✅ Centralized |

## Future Considerations

- **ADR-0007**: SIMD Optimizations for batch operations
- **ADR-0008**: NUMA-aware memory allocation
- **ADR-0009**: Zero-copy networking with io_uring
- **ADR-0010**: Custom allocator for small objects