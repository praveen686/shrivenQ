# Getting Started Guide

## 🎯 Purpose
Help you build and run ShrivenQuant for the first time.

## 📚 Documents in this Section

### For Quick Setup (15 minutes)
→ **[quick-start.md](quick-start.md)** - Minimal commands to get running

### For Detailed Understanding (1 hour)  
→ **[getting-started.md](getting-started.md)** - Complete setup with explanations

## ⚡ Fastest Path

```bash
# 1. Clone repo
git clone https://github.com/praveen686/shrivenQ.git
cd ShrivenQuant

# 2. Build everything
cargo build --release

# 3. Run a service
./target/release/api-gateway

# 4. Check it works
curl http://localhost:8080/health
```

## ⚠️ Before You Start

### Requirements
- **Rust**: 1.85+ (`rustup update`)
- **RAM**: 8GB minimum (16GB recommended)
- **Disk**: 10GB free space
- **OS**: Linux/Mac (Windows untested)
- **Protobuf**: `protoc` compiler installed

### This will NOT
- ❌ Connect to real exchanges
- ❌ Execute any trades  
- ❌ Make you money
- ❌ Work in production

## 🤔 Common Issues

| Problem | Solution |
|---------|----------|
| Won't compile | Update Rust: `rustup update` |
| Out of memory | Use fewer threads: `cargo build -j 2` |
| Port already in use | Kill process: `lsof -i :8080` then `kill -9 [PID]` |
| Missing protoc | Install: `apt install protobuf-compiler` or `brew install protobuf` |
| Slow compilation | Use `cargo build` instead of `cargo build --release` for development |

## 📖 Reading Path

1. **Start here** → You are here
2. **Quick build** → [quick-start.md](quick-start.md)
3. **Full setup** → [getting-started.md](getting-started.md)
4. **Understand** → [Architecture](../03-architecture/README.md)
5. **Contribute** → [Roadmap](../04-development/ROADMAP.md)

## ⚠️ Critical Warnings

This is a **development prototype** with:
- 🔴 **134 unwrap() calls** that will crash
- 🔴 **No error recovery** - cascading failures
- 🔴 **Never tested** with real exchanges
- 🔴 **No strategies** implemented

## 🚀 What You Can Do

✅ **Build** all services  
✅ **Run** individual services  
✅ **Test** Black-Scholes options pricing  
✅ **Learn** the architecture  
✅ **Contribute** improvements  

## ❌ What You Cannot Do

❌ **Trade** with real money  
❌ **Connect** to live exchanges  
❌ **Backtest** strategies (not implemented)  
❌ **Trust** it with money  

---

## 📧 Help & Support

- **Email**: praveenkumar.avln@gmail.com
- **GitHub**: [Issues](https://github.com/praveen686/shrivenQ/issues)
- **Status**: [Current State](../01-status-updates/SYSTEM_STATUS.md)

---

*Last Updated: August 18, 2025*