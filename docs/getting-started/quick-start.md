# ShrivenQuant Quick Start Guide

## 🚨 Current Status
- **40% Complete** - Strong libraries, missing production infrastructure
- **Critical Issue:** Gateway won't compile (see fix below)
- **Timeline:** 4-6 weeks to MVP

---

## 🔧 Fix Gateway Compilation (Priority 1)

```rust
// File: services/gateway/src/server.rs:227
// Error: use of partially moved value

// FIX: Clone before moving
let request_clone = request.clone();
let quantity = request.quantity;
match ExecutionHandlers::submit_order(
    State(state.execution_handlers), 
    headers, 
    Json(request_clone)  // Use clone here
).await {
    // ...
}
```

---

## 🏗️ Create Missing Services (Priority 2)

### Template for 5 Missing main.rs Files

```rust
// services/{service-name}/src/main.rs
use anyhow::Result;
use tonic::transport::Server;
use {service_name}::{create_service, ServiceGrpc};
use shrivenquant_proto::{service_name_server::ServiceNameServer};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    let service = create_service().await?;
    let grpc_service = ServiceGrpc::new(service);
    
    let addr = "[::1]:5005X".parse()?; // X = 2,3,4,5,6
    println!("Starting {} on {}", env!("CARGO_PKG_NAME"), addr);
    
    Server::builder()
        .add_service(ServiceNameServer::new(grpc_service))
        .serve(addr)
        .await?;
    
    Ok(())
}
```

### Port Assignments
- market-connector: 50052
- risk-manager: 50053  
- execution-router: 50054
- portfolio-manager: 50055
- reporting: 50056

---

## ✅ Verify Setup

```bash
# 1. Fix compilation
cargo check --workspace

# 2. Run what works
cargo run --bin auth-service &
cargo run --bin api-gateway &  # After fixing

# 3. Test health
curl http://localhost:8080/health
```

---

## 📋 Development Checklist

### Before Coding
- [ ] Read [Quantitative Best Practices](developer-guide/QUANTITATIVE_DEVELOPMENT_BEST_PRACTICES.md)
- [ ] Understand fixed-point math (Px/Qty types)
- [ ] Review existing service patterns

### While Coding  
- [ ] No f32/f64 for money
- [ ] No allocations in hot paths
- [ ] No unwrap() or panic!()
- [ ] Use FxHashMap not HashMap
- [ ] Pre-allocate collections

### Before Committing
- [ ] `cargo fmt`
- [ ] `cargo clippy -- -D warnings`
- [ ] `cargo test`
- [ ] Verify performance targets

---

## 🎯 MVP Milestones

### Week 1
- [x] Fix gateway compilation
- [ ] Create market-connector main.rs
- [ ] Create risk-manager main.rs

### Week 2  
- [ ] Create remaining 3 main.rs files
- [ ] Test inter-service communication
- [ ] Basic integration test

### Week 3-4
- [ ] Docker containers
- [ ] Basic deployment
- [ ] End-to-end testing

---

## 🆘 Common Issues

### "cannot find crate"
```bash
# Update dependencies
cargo update
cargo build --workspace
```

### "use of moved value"  
```rust
// Clone before moving
let data_clone = data.clone();
```

### "floating point in money calc"
```rust
// Wrong
let price: f64 = 100.50;

// Right  
let price = Px::from_i64(1005000); // Fixed-point
```

---

## 📞 Help

1. Check compilation errors
2. Review this guide
3. Email: praveenkumar.avln@gmail.com