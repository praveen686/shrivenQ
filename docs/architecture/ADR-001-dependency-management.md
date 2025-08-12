# ADR-001: Dependency Version Management Strategy

## Status
Accepted

## Date
2025-01-12

## Context
ShrivenQ is a high-frequency trading system where dependency management is critical for:
- Security (no unexpected behavior from dependencies)
- Performance (no bloat from duplicate dependencies)
- Reliability (deterministic builds)
- Compliance (auditable dependency tree)

We encountered multiple versions of transitive dependencies:
- `regex-automata` (v0.1.10 and v0.4.9)
- `regex-syntax` (v0.6.29 and v0.8.5)
- `getrandom` (v0.2.16 and v0.3.3)
- Various Windows-specific dependencies

These duplicates come from:
- `tracing-subscriber` using older `regex-automata` through its `matchers` dependency
- Different crates having different version requirements

## Decision
We will implement a **three-tier dependency management strategy**:

### 1. Explicit Dependency Control
- Use workspace dependencies in `Cargo.toml` for all direct dependencies
- Pin versions for critical dependencies (security, performance-critical)
- Regular dependency audits using `cargo audit`

### 2. Duplicate Version Management
- Use `cargo-deny` with explicit configuration in `deny.toml`
- Document and explicitly approve known safe duplicates
- Block any new unexpected duplicates
- Add `#![allow(clippy::multiple_crate_versions)]` with reference to cargo-deny

### 3. Continuous Monitoring
- Pre-commit hooks to check for new duplicates
- CI/CD pipeline includes dependency audit
- Quarterly dependency update reviews

## Consequences

### Positive
- **Explicit control**: We know exactly what duplicates exist and why
- **Security**: New unexpected dependencies will be caught
- **Auditability**: Clear documentation for compliance
- **Performance**: We can measure impact of any duplicates
- **Industry standard**: Using cargo-deny is Rust ecosystem best practice

### Negative
- **Maintenance overhead**: Need to update deny.toml when dependencies change
- **Some duplicates remain**: We accept certain duplicates as safe
- **Potential conflicts**: Future updates might require resolution

## Alternatives Considered

### 1. Force Single Versions (Rejected)
- Use `[patch]` section to force single versions
- **Rejected because**: Can break dependencies that require specific versions

### 2. Fork and Update Dependencies (Rejected)
- Fork `tracing-subscriber` and update its dependencies
- **Rejected because**: High maintenance burden, divergence from ecosystem

### 3. Ignore the Warning (Rejected)
- Simply allow multiple versions without tracking
- **Rejected because**: Unacceptable for production trading system

## Implementation Details

### deny.toml Configuration
```toml
[bans]
multiple-versions = "warn"
skip = [
    # Approved duplicates with justification
    { name = "regex-automata", version = "0.1" },  # Used by tracing-subscriber
    { name = "regex-syntax", version = "0.6" },     # Transitive from regex-automata
]
```

### Code Changes
```rust
// In lib.rs files with strict clippy
#![deny(clippy::cargo)]
#![allow(clippy::multiple_crate_versions)] // Handled by cargo-deny configuration
```

### Pre-commit Configuration
```yaml
# In .pre-commit-config.yaml
args: [clippy, ..., -D, "clippy::cargo", -A, "clippy::multiple_crate_versions"]
```

### Monitoring Process
1. Run `cargo deny check` in CI/CD
2. Review `cargo tree --duplicates` monthly
3. Update deny.toml when adding new dependencies

## References
- [cargo-deny documentation](https://github.com/EmbarkStudios/cargo-deny)
- [Rust API Guidelines on Dependencies](https://rust-lang.github.io/api-guidelines/dependencies.html)
- [HFT System Requirements for Dependency Management](https://www.hftreview.com/pg/blog/mike/read/19022/)

## Review History
- 2025-01-12: Initial decision and implementation
