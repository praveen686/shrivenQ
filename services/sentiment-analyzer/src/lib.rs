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
use tracing::{info, warn, error, debug};

/// Sentiment signal for trading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentimentSignal {
    /// Trading symbol (e.g., "BTC", "ETH")
    pub symbol: String,
    /// Sentiment score from -1.0 (bearish) to 1.0 (bullish)
    pub sentiment: f64,
    /// Trading signal derived from sentiment
    pub signal: SignalType,
    /// Confidence level from 0.0 to 1.0
    pub confidence: f64,
    /// Timestamp of the sentiment analysis
    pub timestamp: DateTime<Utc>,
    /// Source of the sentiment data
    pub source: String,
}

/// Trading signal types based on sentiment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignalType {
    /// Strong buy signal (sentiment > 0.8)
    StrongBuy,
    /// Buy signal (sentiment > 0.3)
    Buy,
    /// Neutral signal (-0.3 to 0.3)
    Neutral,
    /// Sell signal (sentiment < -0.3)
    Sell,
    /// Strong sell signal (sentiment < -0.8)
    StrongSell,
}

/// Analysis configuration shared with the service
#[derive(Debug, Clone)]
pub struct AnalysisConfig {
    /// Minimum confidence threshold for signals
    pub min_confidence: f64,
    /// Maximum age of posts to analyze (hours)
    pub max_post_age_hours: u64,
    /// Enable experimental analysis features
    pub experimental_mode: bool,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.7,
            max_post_age_hours: 24,
            experimental_mode: false,
        }
    }
}

/// Reddit sentiment analyzer
#[derive(Debug)]
pub struct RedditAnalyzer {
    client: Client,
    sentiment_cache: Arc<DashMap<String, SentimentSignal>>,
    sentiment_config: SentimentConfig,
    analysis_config: AnalysisConfig,
    rate_limit: Arc<RwLock<RateLimiter>>,
}

/// Rate limiter for API calls
#[derive(Debug)]
pub struct RateLimiter {
    requests_per_minute: u32,
    last_requests: Vec<DateTime<Utc>>,
}

/// Configuration for sentiment analysis
#[derive(Debug, Clone)]
pub struct SentimentConfig {
    /// List of subreddits to monitor
    pub subreddits: Vec<String>,
    /// Keywords indicating bullish sentiment
    pub keywords_bullish: Vec<String>,
    /// Keywords indicating bearish sentiment
    pub keywords_bearish: Vec<String>,
    /// How often to update sentiment in seconds
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
    /// Create new Reddit sentiment analyzer with configuration
    pub fn new(sentiment_config: SentimentConfig, analysis_config: AnalysisConfig) -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .user_agent("ShrivenQuant/1.0")
                .build()
                .context("Failed to create HTTP client")?,
            sentiment_cache: Arc::new(DashMap::new()),
            sentiment_config,
            analysis_config,
            rate_limit: Arc::new(RwLock::new(RateLimiter {
                requests_per_minute: 60,
                last_requests: Vec::new(),
            })),
        })
    }
    
    /// Update analysis configuration
    pub fn update_analysis_config(&mut self, config: AnalysisConfig) {
        self.analysis_config = config;
        info!("Updated analysis config: max_post_age_hours={}, experimental_mode={}", 
              self.analysis_config.max_post_age_hours, self.analysis_config.experimental_mode);
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
        
        // Check if post is too old based on configured max_post_age_hours
        let now = Utc::now().timestamp() as f64;
        let post_age_hours = (now - post.created_utc) / 3600.0;
        let max_age = self.analysis_config.max_post_age_hours as f64;
        
        if post_age_hours > max_age {
            debug!("Skipping post '{}' - age {:.1}h exceeds max {:.1}h", 
                   post.title.chars().take(50).collect::<String>(), post_age_hours, max_age);
            return None;
        }
        
        // Extract ticker symbols (basic pattern)
        let symbols = self.extract_symbols(&text);
        if symbols.is_empty() {
            return None;
        }
        
        // Calculate sentiment score
        let mut bullish_score = 0.0;
        let mut bearish_score = 0.0;
        
        for keyword in &self.sentiment_config.keywords_bullish {
            if text_lower.contains(keyword) {
                let keyword_weight = if self.analysis_config.experimental_mode {
                    // In experimental mode, apply weighted scoring based on keyword strength
                    self.get_keyword_weight(keyword, true)
                } else {
                    1.0
                };
                bullish_score += keyword_weight;
            }
        }
        
        for keyword in &self.sentiment_config.keywords_bearish {
            if text_lower.contains(keyword) {
                let keyword_weight = if self.analysis_config.experimental_mode {
                    // In experimental mode, apply weighted scoring based on keyword strength
                    self.get_keyword_weight(keyword, false)
                } else {
                    1.0
                };
                bearish_score += keyword_weight;
            }
        }
        
        // Weight by post score and comment engagement
        let engagement_weight = (post.score as f64).log10().max(1.0);
        let comment_weight = (post.num_comments as f64).log10().max(1.0);
        let total_weight = engagement_weight * (1.0 + comment_weight * 0.5);
        
        bullish_score *= total_weight;
        bearish_score *= total_weight;
        
        let total_score = bullish_score + bearish_score;
        if total_score == 0.0 {
            return None;
        }
        
        let sentiment = (bullish_score - bearish_score) / total_score;
        
        // Adjust confidence based on comment activity and post freshness
        let base_confidence = (total_score / 10.0).min(1.0);
        let comment_confidence_boost = (post.num_comments as f64 / 100.0).min(0.2);
        let freshness_boost = (1.0 - (post_age_hours / max_age)).max(0.0) * 0.1;
        
        let confidence = if self.analysis_config.experimental_mode {
            // Enhanced confidence calculation in experimental mode
            let title_sentiment_boost = self.analyze_title_sentiment(&post.title) * 0.15;
            let length_penalty = if text.len() < 50 { -0.1 } else { 0.0 }; // Penalize very short posts
            let upvote_ratio_boost = if post.score > 10 { 0.1 } else { 0.0 };
            
            (base_confidence + comment_confidence_boost + freshness_boost + 
             title_sentiment_boost + length_penalty + upvote_ratio_boost).min(1.0).max(0.0)
        } else {
            (base_confidence + comment_confidence_boost + freshness_boost).min(1.0)
        };
        
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
    
    /// Get weighted score for keywords in experimental mode
    fn get_keyword_weight(&self, keyword: &str, is_bullish: bool) -> f64 {
        // Stronger sentiment keywords get higher weights
        match keyword.to_lowercase().as_str() {
            // High impact bullish keywords
            "moon" | "rocket" | "breakout" => if is_bullish { 2.0 } else { 1.0 },
            "pump" | "bullish" | "calls" => if is_bullish { 1.5 } else { 1.0 },
            "buy" | "long" | "hodl" => if is_bullish { 1.2 } else { 1.0 },
            
            // High impact bearish keywords  
            "crash" | "dump" | "dead" => if !is_bullish { 2.0 } else { 1.0 },
            "bearish" | "puts" | "short" => if !is_bullish { 1.5 } else { 1.0 },
            "sell" | "drop" | "rip" => if !is_bullish { 1.2 } else { 1.0 },
            
            // Default weight
            _ => 1.0,
        }
    }
    
    /// Analyze title sentiment for experimental mode confidence boost
    fn analyze_title_sentiment(&self, title: &str) -> f64 {
        let title_lower = title.to_lowercase();
        
        // Strong positive indicators in title
        if title_lower.contains("🚀") || title_lower.contains("💎") || 
           title_lower.contains("to the moon") || title_lower.contains("yolo") {
            return 0.3;
        }
        
        // Strong negative indicators in title
        if title_lower.contains("💀") || title_lower.contains("rip") ||
           title_lower.contains("crash") || title_lower.contains("dump") {
            return -0.3;
        }
        
        // Moderate positive
        if title_lower.contains("bullish") || title_lower.contains("buy") ||
           title_lower.contains("calls") {
            return 0.2;
        }
        
        // Moderate negative
        if title_lower.contains("bearish") || title_lower.contains("sell") ||
           title_lower.contains("puts") {
            return -0.2;
        }
        
        0.0
    }
    
    /// Run continuous sentiment analysis
    pub async fn run(&self) -> Result<()> {
        let mut interval = tokio::time::interval(
            tokio::time::Duration::from_secs(self.sentiment_config.update_interval_secs)
        );
        
        loop {
            interval.tick().await;
            
            for subreddit in &self.sentiment_config.subreddits {
                match self.analyze_subreddit(subreddit).await {
                    Ok(signals) => {
                        for signal in signals {
                            let log_msg = if self.analysis_config.experimental_mode {
                                format!("[EXPERIMENTAL] Sentiment signal: {} {:?} (confidence: {:.2}, sentiment: {:.2})", 
                                    signal.symbol, signal.signal, signal.confidence, signal.sentiment)
                            } else {
                                format!("Sentiment signal: {} {:?} (confidence: {:.2})", 
                                    signal.symbol, signal.signal, signal.confidence)
                            };
                            info!("{}", log_msg);
                            
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