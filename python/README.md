# ShrivenQuant Python Integration Layer

## Python Components

### 1. API Layer (`/api`)
- FastAPI REST endpoints
- GraphQL interface
- WebSocket servers
- gRPC service definitions

### 2. Analytics (`/analytics`)
- Performance analytics
- Risk metrics calculation
- Market impact analysis
- Transaction cost analysis
- Alpha factor research

### 3. Backtesting (`/backtest`)
- Strategy backtesting framework
- Event-driven backtester
- Vectorized backtesting
- Multi-asset portfolio simulation

### 4. Connectors (`/connectors`)
- Database connectors (TimescaleDB, ClickHouse)
- Message queue interfaces (Kafka, Redis)
- Cloud storage adapters (S3, GCS)
- External data providers (Bloomberg, Reuters)

### 5. Utilities (`/utils`)
- Data validation tools
- Time series utilities
- Statistical functions
- Plotting and visualization

### 6. Research Notebooks (`/notebooks`)
- Strategy development
- Feature exploration
- Model prototyping
- Performance analysis

## Integration with Rust Core

### PyO3 Bindings
```python
import shrivenquant_core

# Access Rust order book
book = shrivenquant_core.OrderBook()
book.add_order(price=100.5, quantity=1000, side="BID")

# Get features from Rust
features = shrivenquant_core.calculate_features(book)
```

### Shared Memory
- Zero-copy data sharing via Apache Arrow
- Memory-mapped files for large datasets
- Lock-free queues for real-time data

### gRPC Services
```python
from shrivenquant.grpc import TradingServiceStub

async with grpc.aio.insecure_channel('localhost:50051') as channel:
    stub = TradingServiceStub(channel)
    response = await stub.PlaceOrder(order_request)
```

## Development Setup

```bash
# Create virtual environment
python -m venv venv
source venv/bin/activate

# Install dependencies
pip install -r requirements.txt

# Install development dependencies
pip install -r requirements-dev.txt

# Run tests
pytest tests/

# Start Jupyter Lab
jupyter lab
```

## Key Libraries

- **Data Processing**: pandas, numpy, polars
- **ML/DL**: scikit-learn, pytorch, tensorflow, xgboost
- **Backtesting**: zipline, backtrader, vectorbt
- **Visualization**: plotly, matplotlib, seaborn
- **API**: fastapi, grpcio, websockets
- **Database**: sqlalchemy, pymongo, redis-py