# ShrivenQuant Documentation

**Status:** ~40% Complete | **Timeline:** 4-6 weeks to MVP

---

## 📁 Documentation Structure

```
docs/
├── getting-started/
│   ├── quick-start.md       # Fix compilation, create services
│   └── project-status.md    # Ground truth: 40% complete
│
├── development/
│   ├── best-practices.md    # Critical DO's and DON'Ts
│   └── command-reference.md # Command cheat sheet
│
├── architecture/
│   ├── overview.md          # System design & performance
│   ├── trading-engine-plan.md # Component integration strategy
│   ├── fixed-point-design.md # Financial precision (Px/Qty)
│   └── memory-pool-design.md # Zero-allocation patterns
│
└── integrations/
    ├── zerodha-setup.md     # NSE/BSE connectivity
    └── binance-setup.md     # Crypto API setup
```

---

## 🚨 Start Here

1. **[getting-started/project-status.md](getting-started/project-status.md)** - Understand real status (40% not 70%)
2. **[getting-started/quick-start.md](getting-started/quick-start.md)** - Fix gateway compilation error first

---

## 🎯 Critical Issues

```rust
// Fix gateway compilation at services/gateway/src/server.rs:227
let request_clone = request.clone();
let quantity = request.quantity;
// Use request_clone in submit_order
```

**Missing:** 5 service executables (main.rs files)  
**Broken:** Gateway compilation  
**Timeline:** 4-6 weeks to MVP, 8-10 weeks to production

---

## 📞 Contact

**Email:** praveenkumar.avln@gmail.com