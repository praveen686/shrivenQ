//! Sentiment Analyzer Service
//! 
//! Provides real-time sentiment analysis for trading signals

use anyhow::Result;
use sentiment_analyzer::{RedditAnalyzer, SentimentConfig, SentimentSignal};
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{info, error};
use tracing_subscriber;

pub mod pb {
    tonic::include_proto!("sentiment");
}

use pb::{
    sentiment_service_server::{SentimentService, SentimentServiceServer},
    GetSentimentRequest, GetSentimentResponse,
    GetAllSignalsRequest, GetAllSignalsResponse,
    Signal,
};

pub struct SentimentServiceImpl {
    analyzer: Arc<RedditAnalyzer>,
}

impl SentimentServiceImpl {
    pub fn new() -> Result<Self> {
        let config = SentimentConfig::default();
        let analyzer = Arc::new(RedditAnalyzer::new(config)?);
        
        // Spawn background analysis task
        let analyzer_clone = analyzer.clone();
        tokio::spawn(async move {
            if let Err(e) = analyzer_clone.run().await {
                error!("Sentiment analyzer error: {}", e);
            }
        });
        
        Ok(Self { analyzer })
    }
}

#[tonic::async_trait]
impl SentimentService for SentimentServiceImpl {
    async fn get_sentiment(
        &self,
        request: Request<GetSentimentRequest>,
    ) -> Result<Response<GetSentimentResponse>, Status> {
        let symbol = request.into_inner().symbol;
        
        let response = if let Some(signal) = self.analyzer.get_sentiment(&symbol) {
            GetSentimentResponse {
                found: true,
                signal: Some(convert_signal(signal)),
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
        let signals = self.analyzer.get_all_signals();
        
        let response = GetAllSignalsResponse {
            signals: signals.into_iter().map(convert_signal).collect(),
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
    
    info!("📊 Sentiment Analyzer Service starting on {}", addr);
    info!("🔍 Analyzing: Reddit (r/wallstreetbets, r/cryptocurrency)");
    info!("📈 Signals: Buy/Sell based on social sentiment");
    
    Server::builder()
        .add_service(SentimentServiceServer::new(service))
        .serve(addr)
        .await?;
    
    Ok(())
}