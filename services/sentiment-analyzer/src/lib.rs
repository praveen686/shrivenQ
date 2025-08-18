//! Sentiment Analysis Service for Trading Signals
//! 
//! Analyzes social media sentiment for trading decisions

use anyhow::{Result, Context};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

/// Sentiment signal for trading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentimentSignal {
    pub symbol: String,
    pub sentiment: f64,      // -1.0 to 1.0
    pub signal: SignalType,
    pub confidence: f64,      // 0.0 to 1.0
    pub timestamp: DateTime<Utc>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignalType {
    StrongBuy,
    Buy,
    Neutral,
    Sell,
    StrongSell,
}

/// Reddit sentiment analyzer
pub struct RedditAnalyzer {
    client: Client,
    sentiment_cache: Arc<DashMap<String, SentimentSignal>>,
    config: SentimentConfig,
    rate_limit: Arc<RwLock<RateLimiter>>,
}

/// Rate limiter for API calls
pub struct RateLimiter {
    requests_per_minute: u32,
    last_requests: Vec<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct SentimentConfig {
    pub subreddits: Vec<String>,
    pub keywords_bullish: Vec<String>,
    pub keywords_bearish: Vec<String>,
    pub update_interval_secs: u64,
}

impl Default for SentimentConfig {
    fn default() -> Self {
        Self {
            subreddits: vec![
                "wallstreetbets".to_string(),
                "cryptocurrency".to_string(),
                "CryptoMarkets".to_string(),
                "stocks".to_string(),
            ],
            keywords_bullish: vec![
                "moon".to_string(), "pump".to_string(), "bullish".to_string(),
                "buy".to_string(), "long".to_string(), "rocket".to_string(),
                "breakout".to_string(), "calls".to_string(), "hodl".to_string(),
            ],
            keywords_bearish: vec![
                "dump".to_string(), "bearish".to_string(), "sell".to_string(),
                "short".to_string(), "crash".to_string(), "puts".to_string(),
                "drop".to_string(), "dead".to_string(), "rip".to_string(),
            ],
            update_interval_secs: 300, // 5 minutes
        }
    }
}

impl RedditAnalyzer {
    pub fn new(config: SentimentConfig) -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .user_agent("ShrivenQuant/1.0")
                .build()
                .context("Failed to create HTTP client")?,
            sentiment_cache: Arc::new(DashMap::new()),
            config,
            rate_limit: Arc::new(RwLock::new(RateLimiter {
                requests_per_minute: 60,
                last_requests: Vec::new(),
            })),
        })
    }
    
    /// Check rate limit before making API call
    pub async fn check_rate_limit(&self) -> Result<()> {
        let mut limiter = self.rate_limit.write().await;
        let now = Utc::now();
        
        // Remove requests older than 1 minute
        limiter.last_requests.retain(|&req_time| {
            (now - req_time).num_seconds() < 60
        });
        
        if limiter.last_requests.len() >= limiter.requests_per_minute as usize {
            let oldest = limiter.last_requests[0];
            let wait_time = 60 - (now - oldest).num_seconds();
            if wait_time > 0 {
                warn!("Rate limit reached, waiting {} seconds", wait_time);
                tokio::time::sleep(tokio::time::Duration::from_secs(wait_time as u64)).await;
            }
        }
        
        limiter.last_requests.push(now);
        Ok(())
    }
    
    /// Fetch and analyze posts from Reddit
    pub async fn analyze_subreddit(&self, subreddit: &str) -> Result<Vec<SentimentSignal>> {
        // Check rate limit before making request
        self.check_rate_limit().await?;
        
        let url = format!("https://www.reddit.com/r/{}/hot.json?limit=25", subreddit);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch Reddit data")?;
        
        if !response.status().is_success() {
            warn!("Reddit API returned status: {}", response.status());
            return Ok(Vec::new());
        }
        
        let data: RedditResponse = response.json().await?;
        let mut signals = Vec::new();
        
        for child in data.data.children {
            let post = child.data;
            if let Some(signal) = self.analyze_post(&post) {
                signals.push(signal);
            }
        }
        
        Ok(signals)
    }
    
    /// Analyze a single Reddit post for sentiment
    fn analyze_post(&self, post: &RedditPost) -> Option<SentimentSignal> {
        let text = format!("{} {}", post.title, post.selftext.as_deref().unwrap_or(""));
        let text_lower = text.to_lowercase();
        
        // Extract ticker symbols (basic pattern)
        let symbols = self.extract_symbols(&text);
        if symbols.is_empty() {
            return None;
        }
        
        // Calculate sentiment score
        let mut bullish_score = 0.0;
        let mut bearish_score = 0.0;
        
        for keyword in &self.config.keywords_bullish {
            if text_lower.contains(keyword) {
                bullish_score += 1.0;
            }
        }
        
        for keyword in &self.config.keywords_bearish {
            if text_lower.contains(keyword) {
                bearish_score += 1.0;
            }
        }
        
        // Weight by post score and comments
        let engagement_weight = (post.score as f64).log10().max(1.0);
        bullish_score *= engagement_weight;
        bearish_score *= engagement_weight;
        
        let total_score = bullish_score + bearish_score;
        if total_score == 0.0 {
            return None;
        }
        
        let sentiment = (bullish_score - bearish_score) / total_score;
        let confidence = (total_score / 10.0).min(1.0);
        
        let signal = match sentiment {
            s if s > 0.5 => SignalType::StrongBuy,
            s if s > 0.2 => SignalType::Buy,
            s if s < -0.5 => SignalType::StrongSell,
            s if s < -0.2 => SignalType::Sell,
            _ => SignalType::Neutral,
        };
        
        Some(SentimentSignal {
            symbol: symbols[0].clone(),
            sentiment,
            signal,
            confidence,
            timestamp: Utc::now(),
            source: format!("reddit_{}", post.subreddit),
        })
    }
    
    /// Extract ticker symbols from text
    fn extract_symbols(&self, text: &str) -> Vec<String> {
        let mut symbols = Vec::new();
        
        // Look for $SYMBOL or common crypto symbols
        let words: Vec<&str> = text.split_whitespace().collect();
        for word in words {
            if word.starts_with('$') && word.len() > 1 {
                symbols.push(word[1..].to_uppercase());
            } else if self.is_known_symbol(word) {
                symbols.push(word.to_uppercase());
            }
        }
        
        symbols
    }
    
    /// Check if a word is a known trading symbol
    fn is_known_symbol(&self, word: &str) -> bool {
        let known = ["BTC", "ETH", "BTCUSDT", "ETHUSDT", "SPY", "QQQ", "TSLA", "AAPL"];
        let word_upper = word.to_uppercase();
        known.contains(&word_upper.as_str())
    }
    
    /// Run continuous sentiment analysis
    pub async fn run(&self) -> Result<()> {
        let mut interval = tokio::time::interval(
            tokio::time::Duration::from_secs(self.config.update_interval_secs)
        );
        
        loop {
            interval.tick().await;
            
            for subreddit in &self.config.subreddits {
                match self.analyze_subreddit(subreddit).await {
                    Ok(signals) => {
                        for signal in signals {
                            info!("Sentiment signal: {} {:?} (confidence: {:.2})", 
                                signal.symbol, signal.signal, signal.confidence);
                            
                            // Cache the signal
                            self.sentiment_cache.insert(signal.symbol.clone(), signal);
                        }
                    }
                    Err(e) => {
                        error!("Failed to analyze {}: {}", subreddit, e);
                    }
                }
            }
        }
    }
    
    /// Get current sentiment for a symbol
    pub fn get_sentiment(&self, symbol: &str) -> Option<SentimentSignal> {
        self.sentiment_cache.get(symbol).map(|entry| entry.clone())
    }
    
    /// Get all current signals
    pub fn get_all_signals(&self) -> Vec<SentimentSignal> {
        self.sentiment_cache
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }
}

/// Reddit API response structures
#[derive(Debug, Deserialize)]
struct RedditResponse {
    data: RedditData,
}

#[derive(Debug, Deserialize)]
struct RedditData {
    children: Vec<RedditChild>,
}

#[derive(Debug, Deserialize)]
struct RedditChild {
    data: RedditPost,
}

#[derive(Debug, Deserialize)]
struct RedditPost {
    title: String,
    selftext: Option<String>,
    score: i32,
    num_comments: i32,
    subreddit: String,
    created_utc: f64,
}