# Error Handling Policy

## Core Principle
**ZERO PANICS IN PRODUCTION CODE**

## Rules by Code Type

### 1. Production Code (`src/`)
- **FORBIDDEN**: `unwrap()`, `expect()`, `panic!()`
- **ENFORCED BY**: `#![deny(clippy::unwrap_used, clippy::expect_used)]` in lib.rs
- **USE INSTEAD**: 
  - `?` operator with proper error types
  - `.ok_or_else(|| Error::...)?`
  - `thiserror` for custom errors
  - `anyhow::Context` for error context

#### Example: Production Code
```rust
// ❌ BAD
let config = load_config().unwrap();
let value = map.get(&key).expect("key should exist");

// ✅ GOOD
let config = load_config()
    .context("Failed to load trading config")?;
let value = map.get(&key)
    .ok_or_else(|| Error::MissingKey(key))?;
```

### 2. Test Code (`tests/`, `#[cfg(test)]`)
- **ALLOWED**: `unwrap()`, `expect()` - fail fast is desirable
- **ENFORCED BY**: `allow-unwrap-in-tests = true` in clippy.toml
- **RATIONALE**: Tests should panic on unexpected conditions

#### Example: Test Code
```rust
#[test]
fn test_order_execution() {
    // ✅ OK in tests - we WANT to panic if setup fails
    let engine = create_engine().expect("engine creation should not fail");
    let result = engine.send_order(...).unwrap();
    assert_eq!(result.status, OrderStatus::Filled);
}
```

### 3. Benchmark Code (`benches/`)
- **ALLOWED**: `unwrap()`, `expect()` with clear messages
- **ENFORCED BY**: `#![allow(clippy::unwrap_used, clippy::expect_used)]` at file level
- **RATIONALE**: Benchmarks are not production, but keep panics obvious

#### Example: Benchmark Code
```rust
// benches/engine_bench.rs
#![allow(clippy::unwrap_used, clippy::expect_used)]

fn bench_order_pool(c: &mut Criterion) {
    // ✅ OK in benchmarks - with descriptive message
    let pool = ObjectPool::new(1000)
        .expect("benchmark pool allocation should not fail");
}
```

### 4. Display/Diagnostic Utilities
- **ALLOWED**: In smallest scope with `#[allow(...)]`
- **ENFORCED BY**: Module-level or function-level `#[allow(...)]`
- **RATIONALE**: Display code often needs lossy conversions

#### Example: Display Utilities
```rust
// Module for display-only utilities
pub mod display {
    #![allow(clippy::cast_precision_loss)] // Display only
    
    pub fn format_bytes_gb(bytes: u64) -> f64 {
        bytes as f64 / 1_073_741_824.0  // OK for display
    }
}
```

## Enforcement

### CI/CD Pipeline
```yaml
# .github/workflows/dev-pipeline.yml
- name: Strict Clippy for Production
  run: |
    cargo clippy --lib --bins \
      -- -D clippy::unwrap_used \
         -D clippy::expect_used \
         -D clippy::panic

- name: Relaxed Clippy for Tests/Benches
  run: |
    cargo clippy --tests --benches \
      -- -D warnings  # But allow unwrap/expect
```

### Pre-commit Hook
```bash
# Check production code for panics
! grep -r "unwrap()\|expect(\|panic!" crates/*/src \
  --exclude-dir=tests \
  --exclude-dir=benches \
  --exclude="*_test.rs"
```

## Migration Guide

### Converting Existing Code
```rust
// Before (panics)
let port = env::var("PORT").unwrap();
let socket = TcpListener::bind(("0.0.0.0", port)).unwrap();

// After (proper errors)
let port = env::var("PORT")
    .context("PORT environment variable not set")?;
let socket = TcpListener::bind(("0.0.0.0", port))
    .with_context(|| format!("Failed to bind to port {}", port))?;
```

### When Truly Unreachable
If you have a genuine invariant that should never fail:

```rust
// Document WHY it's unreachable
match state {
    State::Ready => process(),
    State::Busy => wait(),
    _ => {
        // SAFETY: State enum only has 2 variants, enforced by type system
        unreachable!("Invalid state - this is a bug")
    }
}
```

## Exceptions

### Allowed `expect()` Cases
Only with explicit justification comment:

```rust
// SAFETY: Regex is compile-time constant and verified in tests
static PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\d{4}-\d{2}-\d{2}$")
        .expect("DATE_PATTERN regex is valid")
});
```

## Monitoring

### Detecting Violations
```bash
# Quick check for production violations
rg "unwrap\(\)|expect\(|panic!" crates/*/src --stats

# Detailed report
cargo clippy --lib --bins -- -D clippy::unwrap_used
```

### Metrics to Track
- Number of `Result<T, E>` vs raw `T` returns
- Error handling coverage
- Panic-free execution time in production

## Benefits

1. **Reliability**: No unexpected crashes in production
2. **Debuggability**: Proper error context instead of panic backtraces
3. **Composability**: Errors propagate cleanly through `?`
4. **Testing**: Clear distinction between test and production code
5. **Performance**: No unwinding overhead from panics

## Summary

| Code Type | `unwrap` | `expect` | `panic!` | Enforcement |
|-----------|----------|----------|----------|-------------|
| Production (`src/`) | ❌ | ❌ | ❌ | `#![deny(...)]` in lib.rs |
| Tests | ✅ | ✅ | ✅ | clippy.toml |
| Benchmarks | ✅ | ✅ | ⚠️ | File-level `#![allow(...)]` |
| Display utils | ⚠️ | ⚠️ | ❌ | Function-level `#[allow(...)]` |

**Remember**: If you're typing `.unwrap()` in production code, stop and think about the error case!