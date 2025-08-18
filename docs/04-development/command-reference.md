# 🤖 Agent Quick Reference Card
## ShrivenQuant Compliance Essentials

> **CRITICAL**: Keep this reference visible while coding. Violations = Instant Rejection.

---

## 🚫 INSTANT REJECTION PATTERNS

```rust
// ❌ NEVER DO THESE:
let _unused = expensive_call();        // Underscore abuse
Vec::new()                            // Hot path allocation  
panic!("error")                       // Panic in production
order.price().unwrap()                // Unwrap usage
let price: f64 = 123.45;              // Float for money
match result { Err(_) => {} }         // Ignore errors
// TODO: implement this                // Unfinished work
std::collections::HashMap::new()      // Slow hashmap
unimplemented!()                      // No context
return 0; // placeholder              // Fake returns
```

## ✅ CORRECT PATTERNS

```rust
// ✅ ALWAYS DO THESE:
Vec::with_capacity(1000)              // Pre-allocate
Price::from_fixed_point(12345)        // Fixed-point money
Result<T, SpecificError>              // Specific errors
FxHashMap::with_capacity(100)         // Fast hashmap
match result {                        // Handle all cases
    Ok(val) => process(val),
    Err(OrderError::Invalid) => reject(),
    Err(e) => log_and_return(e),
}
#[inline(always)]                     // Hot path functions
const MAX_SIZE: usize = 1024;         // Named constants
```

---

## ⚡ PERFORMANCE RULES

- **Latency Budget**: 10μs maximum for hot paths
- **Memory**: No allocations in hot paths
- **Numbers**: i64 for prices, NOT f64
- **Collections**: Pre-allocate with capacity
- **Strings**: Use &str, avoid String::new()
- **Errors**: Handle explicitly, never ignore

---

## 🔧 BEFORE COMMITTING

```bash
# 1. Run comprehensive compliance check
./scripts/compliance/run-compliance.sh --details

# 2. Run with strict thresholds
./scripts/compliance/run-compliance.sh --strict

# 3. Check risk limits
./scripts/compliance/validate-risk-limits.sh
```

---

## 🎯 SCORING SYSTEM

- **Critical Violations**: -25 points each (Instant rejection)
- **High Priority**: -10 points each  
- **Medium Priority**: -3 points each
- **Low Priority**: -1 point each
- **Passing Score**: 90+ (Excellent), 70+ (Good)

---

## 📚 DOCUMENTATION

- **Master Guide**: `docs/developer-guide/QUANTITATIVE_DEVELOPMENT_BEST_PRACTICES.md`
- **Compliance Framework**: `docs/developer-guide/AGENT_COMPLIANCE_FRAMEWORK.md`
- **This Reference**: Keep open while coding

---

## 🆘 EMERGENCY FIXES

**Found violations?**

1. **Hot path allocations** → Use `Vec::with_capacity(n)`
2. **Panic/unwrap** → Use `Result<T, E>` and `match`
3. **Float money** → Use `i64` fixed-point arithmetic
4. **std::HashMap** → Replace with `FxHashMap`
5. **Underscore vars** → Use proper names or `#[allow(unused)]`
6. **TODO/FIXME** → Complete implementation or create issues
7. **Err(_)** → Handle specific error types
8. **clone()** → Use borrowing with `&` or `iter()`

---

**🚀 Ready to Code Compliantly!**
