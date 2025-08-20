//! Options Engine Service
//! 
//! Mathematical options trading system for Indian index options

use anyhow::Result;
use tonic::transport::Server;
use tracing::{info, error};
use tracing_subscriber;
use chrono::Utc;

mod grpc_service;

/// Protocol buffer definitions for options service
pub mod pb {
    tonic::include_proto!("options");
}

use pb::options_engine_server::OptionsEngineServer;
use grpc_service::OptionsEngineService;
use options_engine::*;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("options_engine=debug,info")
        .init();
    
    // Initialize and test various options engine components
    initialize_options_engine().await?;
    
    let addr = "[::1]:50060".parse()?;
    let service = OptionsEngineService::new();
    
    info!("🚀 Options Engine Service starting on {}", addr);
    info!("📊 Features: Black-Scholes, Greeks, IV, Strategies");
    info!("🇮🇳 Supporting: NIFTY50, BANKNIFTY, FINNIFTY, MIDCAPNIFTY");
    
    match Server::builder()
        .add_service(OptionsEngineServer::new(service))
        .serve(addr)
        .await 
    {
        Ok(_) => info!("Options Engine shut down gracefully"),
        Err(e) => error!("Options Engine error: {}", e),
    }
    
    Ok(())
}

/// Initialize options engine components and demonstrate usage
async fn initialize_options_engine() -> Result<()> {
    info!("Initializing Options Engine components...");
    
    // Test IndexOption methods
    test_index_options().await?;
    
    // Test volatility surface functionality
    test_volatility_surface().await?;
    
    // Test option strategies
    test_option_strategies().await?;
    
    // Test exotic options
    test_exotic_options().await?;
    
    // Test different execution modes
    test_execution_modes().await?;
    
    info!("Options Engine initialization complete");
    Ok(())
}

/// Test IndexOption methods
async fn test_index_options() -> Result<()> {
    info!("Testing IndexOption functionality...");
    
    for index in &[IndexOption::Nifty50, IndexOption::BankNifty, IndexOption::FinNifty, IndexOption::MidCapNifty] {
        let tick_size = index.tick_size();
        let lot_size = index.lot_size();
        
        info!("Index: {:?}, Tick Size: {}, Lot Size: {}", index, tick_size, lot_size);
        
        // Get expiry dates
        let current_date = Utc::now();
        match index.get_expiry_dates(current_date) {
            Ok(expiries) => {
                info!("Next 5 expiries for {:?}:", index);
                for (i, expiry) in expiries.iter().take(5).enumerate() {
                    info!("  {}: {}", i + 1, expiry.format("%Y-%m-%d"));
                }
            }
            Err(e) => error!("Failed to get expiries for {:?}: {}", index, e),
        }
        
        // Test symbol parsing
        let test_symbols = [
            "NIFTY24JAN18000CE",
            "BANKNIFTY24JAN45000PE",
            "FINNIFTY24JAN20000CE",
        ];
        
        for symbol in &test_symbols {
            match IndexOption::parse_symbol(symbol) {
                Ok((parsed_index, option_type, strike, expiry)) => {
                    info!("Parsed {}: {:?} {:?} strike={} expiry={}", 
                          symbol, parsed_index, option_type, strike, expiry.format("%Y-%m-%d"));
                }
                Err(e) => info!("Could not parse {}: {}", symbol, e),
            }
        }
    }
    
    Ok(())
}

/// Test volatility surface functionality
async fn test_volatility_surface() -> Result<()> {
    info!("Testing VolatilitySurface functionality...");
    
    let mut surface = VolatilitySurface::new();
    
    // Set volatility surface properties
    surface.atm_volatility = 0.20;
    surface.skew = 0.05;
    surface.term_structure = vec![0.18, 0.20, 0.22, 0.25];
    
    // Test SABR volatility calculation
    let sabr_vol = surface.sabr_volatility(18000.0, 18500.0, 0.25, 0.2, 0.8, -0.3, 0.4);
    info!("SABR volatility for F=18000, K=18500, T=0.25: {:.4}", sabr_vol);
    
    // Test implied volatility lookup
    let iv = surface.get_iv(18000.0, 18500.0, 0.25);
    info!("Implied volatility for S=18000, K=18500, T=0.25: {:.4}", iv);
    
    // Test with different scenarios - the volatility surface will use its internal methods
    // We just test the get_iv method which handles the calculations internally
    info!("Testing volatility surface interpolation with different scenarios:");
    
    let interpolated_iv = surface.get_iv(18000.0, 19800.0, 0.25); // Moneyness = 1.1
    info!("Interpolated IV for moneyness 1.1: {:.4}", interpolated_iv);
    
    Ok(())
}

/// Test option strategy functionality
async fn test_option_strategies() -> Result<()> {
    info!("Testing OptionStrategy functionality...");
    
    // Create a bull call spread using Iron Condor as template
    let expiry = Utc::now() + chrono::Duration::days(30);
    let mut strategy = OptionStrategy::iron_condor(
        IndexOption::Nifty50,
        18000.0,
        expiry,
        500.0, // wing width
        250.0, // body width
    );
    
    // Modify to be a bull call spread (keep only call legs)
    strategy.name = "Bull Call Spread".to_string();
    strategy.legs = vec![
        // Buy 18000 call
        OptionLeg {
            contract: OptionContract {
                index: IndexOption::Nifty50,
                option_type: OptionType::Call,
                strike: 18000.0,
                expiry,
                lot_size: IndexOption::Nifty50.lot_size(),
                premium: 0.0,
                open_interest: 0,
                volume: 0,
                implied_volatility: 0.0,
                greeks: Greeks::default(),
            },
            quantity: 1,
            entry_price: 250.0,
        },
        // Sell 18500 call
        OptionLeg {
            contract: OptionContract {
                index: IndexOption::Nifty50,
                option_type: OptionType::Call,
                strike: 18500.0,
                expiry,
                lot_size: IndexOption::Nifty50.lot_size(),
                premium: 0.0,
                open_interest: 0,
                volume: 0,
                implied_volatility: 0.0,
                greeks: Greeks::default(),
            },
            quantity: -1,
            entry_price: 120.0,
        },
    ];
    
    // Test P&L calculation at different spot prices
    let test_spots = [17500.0, 18000.0, 18250.0, 18500.0, 19000.0];
    
    info!("Bull Call Spread P&L at different spot prices:");
    for spot in &test_spots {
        let pnl = strategy.calculate_pnl(*spot);
        info!("  Spot: {}, P&L: {:.2}", spot, pnl);
    }
    
    Ok(())
}

/// Test exotic options functionality
async fn test_exotic_options() -> Result<()> {
    info!("Testing ExoticOption functionality...");
    
    let mc_engine = MonteCarloEngine::new(10000, 100);
    let spot = 18000.0;
    let strike = 18000.0;
    let rate = 0.06;
    let volatility = 0.20;
    let time_to_expiry = 0.25;
    
    // Test barrier options
    let barrier_option_type = ExoticOptionType::Barrier { 
        barrier: 19000.0, 
        barrier_type: BarrierType::UpAndOut 
    };
    
    let barrier_price = mc_engine.price_exotic(
        &barrier_option_type,
        spot, strike, rate, volatility, time_to_expiry
    );
    
    info!("Created Up-and-Out barrier option:");
    info!("  Underlying: {}, Strike: {}, Barrier: 19000.0", spot, strike);
    info!("  Monte Carlo Price: {:.2}", barrier_price);
    
    // Test lookback option
    let lookback_option_type = ExoticOptionType::Lookback;
    
    let lookback_price = mc_engine.price_exotic(
        &lookback_option_type,
        spot, strike, rate, volatility, time_to_expiry
    );
    
    info!("Created Lookback option with underlying: {}", spot);
    info!("  Monte Carlo Price: {:.2}", lookback_price);
    
    // Test different barrier types
    for barrier_type in &[BarrierType::UpAndOut, BarrierType::DownAndOut, BarrierType::UpAndIn, BarrierType::DownAndIn] {
        let barrier_option_type = ExoticOptionType::Barrier { 
            barrier: 19000.0, 
            barrier_type: barrier_type.clone() 
        };
        
        let price = mc_engine.price_exotic(
            &barrier_option_type,
            spot, strike, rate, volatility, time_to_expiry
        );
        
        info!("Barrier option type: {:?}, Price: {:.2}", barrier_type, price);
    }
    
    // Test Asian option
    let asian_price = mc_engine.price_exotic(
        &ExoticOptionType::Asian,
        spot, strike, rate, volatility, time_to_expiry
    );
    info!("Asian option price: {:.2}", asian_price);
    
    Ok(())
}

/// Test execution modes
async fn test_execution_modes() -> Result<()> {
    info!("Testing ExecutionMode functionality...");
    
    let modes = [
        ExecutionMode::Backtest,
        ExecutionMode::Simulation,
        ExecutionMode::Paper,
        ExecutionMode::Live,
    ];
    
    for mode in &modes {
        info!("Testing execution mode: {:?}", mode);
        
        // Create an options engine for each mode
        let engine = OptionsEngine::new(mode.clone());
        
        // Create a simple test strategy
        let expiry = Utc::now() + chrono::Duration::days(7);
        let strategy = OptionStrategy::iron_condor(
            IndexOption::Nifty50,
            21500.0,
            expiry,
            100.0, // wing width
            200.0, // body width
        );
        
        // Test strategy execution in this mode
        match engine.execute_strategy(strategy).await {
            Ok(_) => info!("  ✅ Strategy executed successfully in {:?} mode", mode),
            Err(e) => info!("  ❌ Error executing strategy: {}", e),
        }
        
        match mode {
            ExecutionMode::Backtest => {
                info!("  - Historical backtesting mode using past market data");
                info!("  - Risk-free testing environment");
                info!("  - Full strategy validation");
            }
            ExecutionMode::Simulation => {
                info!("  - Real-time simulation with synthetic data");
                info!("  - Live market conditions without real money");
                info!("  - Performance testing");
            }
            ExecutionMode::Paper => {
                info!("  - Paper trading with live data but no real money");
                info!("  - Real market conditions simulation");
                info!("  - Strategy testing with live feeds");
            }
            ExecutionMode::Live => {
                info!("  - Live trading with real market data and execution");
                info!("  - Real money at risk");
                info!("  - Production trading environment");
            }
        }
    }
    
    Ok(())
}
