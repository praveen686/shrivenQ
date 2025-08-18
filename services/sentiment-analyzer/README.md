# Sentiment Analyzer Service

## Overview
Analyzes social media sentiment from Reddit for trading signals.

## Status: ✅ Compiles, ⚠️ Partially Implemented

### What's Implemented
- Reddit scraping framework
- Basic sentiment analysis
- gRPC service
- Proto definitions
- Python script for sentiment analysis

### What's Missing
- Reddit API authentication
- Rate limiting
- Multiple subreddit support
- Twitter integration
- News integration
- Sentiment aggregation
- Historical tracking
- Alert system

## Architecture

```
Reddit API → Scraper → Sentiment Analysis → Signal Generation
                            ↓
                     Python Script
```

## Data Sources

### Currently Supported
- Reddit (framework only, no API keys)

### Planned
- Twitter/X
- StockTwits
- Discord
- Telegram
- News APIs

## Sentiment Analysis

### Current Method
- VADER sentiment (via Python)
- Basic keyword matching
- Simple scoring (-1 to +1)

### Planned Improvements
- FinBERT for financial sentiment
- Custom trained models
- Sarcasm detection
- Spam filtering

## API

### gRPC Endpoints

#### `AnalyzeSentiment(SentimentRequest) → SentimentResponse`
Analyze sentiment for a symbol.

#### `StreamSentiment(StreamRequest) → stream SentimentUpdate`
Stream real-time sentiment updates.

#### `GetHistoricalSentiment(HistoricalRequest) → HistoricalResponse`
Get historical sentiment data (not implemented).

## Configuration

```yaml
# Not implemented - currently hardcoded
reddit:
  client_id: "YOUR_CLIENT_ID"
  client_secret: "YOUR_SECRET"
  user_agent: "ShrivenQuant/1.0"
  
subreddits:
  - wallstreetbets
  - stocks
  - options
```

## Python Script

Located at `/home/praveen/ShrivenQuant/scripts/sentiment_analyzer.py`

### Features
- VADER sentiment analysis
- Reddit comment parsing
- JSON output

### Issues
- No error handling
- No rate limiting
- Hardcoded credentials
- No caching

## Running

```bash
cargo run --release -p sentiment-analyzer
```

Service listens on port `50057`.

## Integration Status

| Component | Status | Notes |
|-----------|--------|-------|
| Reddit Scraping | ⚠️ | No API keys |
| Sentiment Analysis | ✅ | Basic VADER |
| Signal Generation | ❌ | Not implemented |
| Trading Integration | ❌ | Not connected |

## Sample Output

```json
{
  "symbol": "AAPL",
  "sentiment_score": 0.65,
  "confidence": 0.75,
  "volume": 1250,
  "sources": ["reddit"],
  "timestamp": "2025-01-15T10:30:00Z"
}
```

## Known Issues

1. No Reddit API authentication
2. No rate limiting
3. Single source only
4. No historical data
5. No entity recognition
6. No spam detection
7. Basic sentiment only
8. Not tested with real data

## TODO

- [ ] Add Reddit API keys
- [ ] Implement rate limiting
- [ ] Add Twitter support
- [ ] Improve sentiment model
- [ ] Add entity extraction
- [ ] Implement caching
- [ ] Add historical storage
- [ ] Create alert system
- [ ] Integration tests