# ML Inference Service

## Overview
Machine learning inference service providing real-time predictions for trading strategies.

## Status: ✅ Compiles, ❌ No Models

### What's Implemented
- Feature engineering framework
- Technical indicators (RSI, MACD, Bollinger Bands)
- Feature store with buffering
- gRPC service structure
- Proto definitions

### What's Missing
- Actual ML models
- Model loading/serving
- Model versioning
- A/B testing
- Feature validation
- Model monitoring
- Performance optimization

## Features

### Technical Indicators
```rust
- RSI (Relative Strength Index)
- MACD (Moving Average Convergence Divergence)
- Bollinger Bands
- Volume indicators
- Price momentum
```

### Feature Store
- Real-time feature computation
- Sliding window buffers
- Feature caching

## Architecture

```
Market Data → Feature Engineering → Model Inference → Predictions
                    ↓
              Feature Store
```

## Prediction Types

1. **Price Direction** - Probability of price going up
2. **Price Target** - Predicted price level
3. **Volatility** - Predicted volatility
4. **Market Regime** - Market state classification
5. **Anomaly Detection** - Unusual pattern detection

## Models

### Currently Implemented
None - only framework exists

### Planned Models
- LSTM for price prediction
- Random Forest for regime detection
- Autoencoder for anomaly detection
- Reinforcement Learning for execution

## API

### gRPC Endpoints

#### `Predict(PredictRequest) → PredictResponse`
Get prediction for a symbol.

#### `BatchPredict(BatchPredictRequest) → BatchPredictResponse`
Get predictions for multiple symbols.

#### `GetFeatures(GetFeaturesRequest) → GetFeaturesResponse`
Get computed features for a symbol.

## Configuration

Currently hardcoded. Needs:
- Model paths
- Feature windows
- Update frequencies
- Model selection

## Running

```bash
cargo run --release -p ml-inference
```

Service listens on port `50056`.

## Performance

Not measured. Framework only, no actual inference.

## Integration Status

| Component | Status | Notes |
|-----------|--------|-------|
| Feature Engineering | ✅ | Basic implementation |
| Model Serving | ❌ | No models |
| Market Data Integration | ❌ | Not connected |
| Trading Gateway Integration | ❌ | Not connected |

## Known Issues

1. No actual models
2. No model management
3. No feature validation
4. No performance metrics
5. Features not tested with real data
6. Memory unbounded
7. No backtesting integration

## TODO

- [ ] Implement actual models
- [ ] Add model loading
- [ ] Connect to market data
- [ ] Add feature validation
- [ ] Implement model monitoring
- [ ] Add performance metrics
- [ ] Integration tests
- [ ] Backtesting support