//! Sentiment Analyzer Service
//! 
//! Provides real-time sentiment analysis for trading signals

use anyhow::Result;
use sentiment_analyzer::{AnalysisConfig, RedditAnalyzer, SentimentConfig, SentimentSignal};
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{info, error};
use tracing_subscriber;

/// Protocol buffer definitions for sentiment service
pub mod pb {
    tonic::include_proto!("sentiment");
}

use pb::{
    sentiment_service_server::{SentimentService, SentimentServiceServer},
    GetSentimentRequest, GetSentimentResponse,
    GetAllSignalsRequest, GetAllSignalsResponse,
    Signal,
};


/// Sentiment analysis service implementation
pub struct SentimentServiceImpl {
    analyzer: Arc<RwLock<RedditAnalyzer>>,
    config: Arc<RwLock<AnalysisConfig>>,
}

impl std::fmt::Debug for SentimentServiceImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SentimentServiceImpl")
            .field("analyzer", &"RedditAnalyzer")
            .field("config", &"RwLock<AnalysisConfig>")
            .finish()
    }
}

impl SentimentServiceImpl {
    /// Create new sentiment service instance
    pub fn new() -> Result<Self> {
        let sentiment_config = SentimentConfig::default();
        let analysis_config = AnalysisConfig::default();
        let analyzer = Arc::new(RwLock::new(RedditAnalyzer::new(sentiment_config, analysis_config.clone())?));
        
        // Spawn background analysis task
        let analyzer_clone = analyzer.clone();
        tokio::spawn(async move {
            loop {
                if let Err(e) = analyzer_clone.read().await.run().await {
                    error!("Sentiment analyzer error: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
                }
            }
        });
        
        Ok(Self { 
            analyzer,
            config: Arc::new(RwLock::new(analysis_config)),
        })
    }
    
    /// Update analysis configuration and sync with analyzer
    pub async fn update_config(&self, new_config: AnalysisConfig) {
        // Update service config
        *self.config.write().await = new_config.clone();
        
        // Update analyzer config
        self.analyzer.write().await.update_analysis_config(new_config);
        
        info!("Analysis configuration updated");
    }
}

#[tonic::async_trait]
impl SentimentService for SentimentServiceImpl {
    async fn get_sentiment(
        &self,
        request: Request<GetSentimentRequest>,
    ) -> Result<Response<GetSentimentResponse>, Status> {
        let symbol = request.into_inner().symbol;
        
        // Read current configuration
        let config = self.config.read().await;
        let min_confidence = config.min_confidence;
        drop(config);
        
        let response = if let Some(signal) = self.analyzer.read().await.get_sentiment(&symbol) {
            // Filter by confidence threshold
            if signal.confidence < min_confidence {
                GetSentimentResponse {
                    found: false,
                    signal: None,
                }
            } else {
                GetSentimentResponse {
                    found: true,
                    signal: Some(convert_signal(signal)),
                }
            }
        } else {
            GetSentimentResponse {
                found: false,
                signal: None,
            }
        };
        
        Ok(Response::new(response))
    }
    
    async fn get_all_signals(
        &self,
        _request: Request<GetAllSignalsRequest>,
    ) -> Result<Response<GetAllSignalsResponse>, Status> {
        let signals = self.analyzer.read().await.get_all_signals();
        
        // Apply confidence filtering based on current config
        let config = self.config.read().await;
        let min_confidence = config.min_confidence;
        drop(config);
        
        let filtered_signals: Vec<Signal> = signals
            .into_iter()
            .filter(|signal| signal.confidence >= min_confidence)
            .map(convert_signal)
            .collect();
        
        let response = GetAllSignalsResponse {
            signals: filtered_signals,
        };
        
        Ok(Response::new(response))
    }
}

fn convert_signal(signal: SentimentSignal) -> Signal {
    Signal {
        symbol: signal.symbol,
        sentiment: signal.sentiment,
        signal_type: match signal.signal {
            sentiment_analyzer::SignalType::StrongBuy => "strong_buy".to_string(),
            sentiment_analyzer::SignalType::Buy => "buy".to_string(),
            sentiment_analyzer::SignalType::Neutral => "neutral".to_string(),
            sentiment_analyzer::SignalType::Sell => "sell".to_string(),
            sentiment_analyzer::SignalType::StrongSell => "strong_sell".to_string(),
        },
        confidence: signal.confidence,
        timestamp: signal.timestamp.timestamp(),
        source: signal.source,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("sentiment_analyzer=info")
        .init();
    
    let addr = "[::1]:50065".parse()?;
    let service = SentimentServiceImpl::new()?;
    
    // Log current configuration
    let config = service.config.read().await;
    info!("📊 Sentiment Analyzer Service starting on {}", addr);
    info!("🔍 Analyzing: Reddit (r/wallstreetbets, r/cryptocurrency)");
    info!("📈 Signals: Buy/Sell based on social sentiment");
    info!("⚙️  Configuration: min_confidence={:.2}, max_post_age_hours={}, experimental_mode={}", 
          config.min_confidence, config.max_post_age_hours, config.experimental_mode);
    
    if config.experimental_mode {
        info!("🧪 Experimental mode ENABLED - Enhanced sentiment analysis features active");
    }
    drop(config);
    
    Server::builder()
        .add_service(SentimentServiceServer::new(service))
        .serve(addr)
        .await?;
    
    Ok(())
}