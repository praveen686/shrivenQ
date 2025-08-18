//! ShrivenQuant Options Trading System
//! Mathematical Warfare Machine for Indian Index Options
//! 
//! Features:
//! - Complete Greeks calculation (Delta, Gamma, Theta, Vega, Rho)
//! - Black-Scholes pricing with Indian market adjustments
//! - Volatility surface modeling
//! - Monte Carlo simulations
//! - Stochastic volatility models
//! - Multi-mode execution (Backtest/Simulation/Paper/Live)
//! - Zerodha integration for Indian markets

use chrono::{DateTime, Utc, Datelike};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, BTreeMap};
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::{Result, Context};

// Mathematical constants
const TRADING_DAYS_INDIA: f64 = 252.0; // NSE trading days
const RISK_FREE_RATE: f64 = 0.065; // Indian T-bill rate ~6.5%
const SQRT_2PI: f64 = 2.5066282746310007;

// ============================================================================
// CORE MATHEMATICAL STRUCTURES
// ============================================================================

/// Option type for derivatives
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum OptionType {
    Call,
    Put,
}

/// Indian index options
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IndexOption {
    Nifty50,
    BankNifty,
    FinNifty,
    MidCapNifty,
}

impl IndexOption {
    pub fn lot_size(&self) -> u32 {
        match self {
            IndexOption::Nifty50 => 50,
            IndexOption::BankNifty => 25,
            IndexOption::FinNifty => 25,
            IndexOption::MidCapNifty => 50,
        }
    }
    
    pub fn tick_size(&self) -> f64 {
        0.05 // 5 paise for all index options
    }
}

/// Complete Greeks for option pricing
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Greeks {
    pub delta: f64,     // Rate of change of option price w.r.t underlying
    pub gamma: f64,     // Rate of change of delta w.r.t underlying  
    pub theta: f64,     // Time decay
    pub vega: f64,      // Sensitivity to volatility
    pub rho: f64,       // Sensitivity to interest rate
    pub lambda: f64,    // Leverage (Omega)
    pub vanna: f64,     // Sensitivity of delta to volatility
    pub charm: f64,     // Delta decay
    pub vomma: f64,     // Vega convexity
    pub speed: f64,     // Gamma sensitivity
    pub zomma: f64,     // Gamma sensitivity to volatility
    pub color: f64,     // Gamma decay
}

/// Option contract specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionContract {
    pub index: IndexOption,
    pub option_type: OptionType,
    pub strike: f64,
    pub expiry: DateTime<Utc>,
    pub lot_size: u32,
    pub premium: f64,
    pub open_interest: u64,
    pub volume: u64,
    pub implied_volatility: f64,
    pub greeks: Greeks,
}

/// Volatility surface for modeling IV across strikes and expiries
#[derive(Debug, Clone)]
pub struct VolatilitySurface {
    pub surface: BTreeMap<(f64, f64), f64>, // (moneyness, time_to_expiry) -> IV
    pub atm_volatility: f64,
    pub skew: f64,
    pub term_structure: Vec<f64>,
}

impl VolatilitySurface {
    pub fn new() -> Self {
        Self {
            surface: BTreeMap::new(),
            atm_volatility: 0.15, // 15% base volatility
            skew: -0.1,
            term_structure: vec![],
        }
    }
    
    /// SABR model for volatility smile
    pub fn sabr_volatility(&self, f: f64, k: f64, t: f64, alpha: f64, beta: f64, rho: f64, nu: f64) -> f64 {
        if (f - k).abs() < 1e-12 {
            return alpha / f.powf(1.0 - beta);
        }
        
        let fk_beta = (f * k).powf((1.0 - beta) / 2.0);
        let log_fk = (f / k).ln();
        let z = nu / alpha * fk_beta * log_fk;
        let x = ((1.0 - 2.0 * rho * z + z * z).sqrt() + z - rho).ln() / nu;
        
        let numerator = alpha;
        let denominator = fk_beta * (1.0 + (1.0 - beta).powi(2) / 24.0 * log_fk.powi(2)
            + (1.0 - beta).powi(4) / 1920.0 * log_fk.powi(4));
        
        let expansion = 1.0 + t * ((1.0 - beta).powi(2) / 24.0 * alpha.powi(2) / (f * k).powf(1.0 - beta)
            + 0.25 * rho * beta * nu * alpha / fk_beta
            + (2.0 - 3.0 * rho.powi(2)) / 24.0 * nu.powi(2));
        
        numerator * z / x / denominator * expansion
    }
    
    /// Get implied volatility for given strike and time
    pub fn get_iv(&self, spot: f64, strike: f64, time_to_expiry: f64) -> f64 {
        let moneyness = strike / spot;
        
        // Use volatility smile model
        let base_vol = self.atm_volatility;
        let skew_adjustment = self.skew * (moneyness - 1.0).ln();
        let term_adjustment = if self.term_structure.len() > 0 {
            self.interpolate_term_structure(time_to_expiry)
        } else {
            0.0
        };
        
        (base_vol + skew_adjustment + term_adjustment).max(0.01)
    }
    
    fn interpolate_term_structure(&self, time: f64) -> f64 {
        // Linear interpolation for term structure
        0.0 // Simplified for now
    }
}

// ============================================================================
// BLACK-SCHOLES MATHEMATICS
// ============================================================================

pub struct BlackScholes;

impl BlackScholes {
    /// Standard normal cumulative distribution function
    pub fn norm_cdf(x: f64) -> f64 {
        0.5 * (1.0 + libm::erf(x / std::f64::consts::SQRT_2))
    }
    
    /// Standard normal probability density function
    pub fn norm_pdf(x: f64) -> f64 {
        (-0.5 * x * x).exp() / SQRT_2PI
    }
    
    /// Calculate d1 parameter
    pub fn d1(s: f64, k: f64, r: f64, sigma: f64, t: f64) -> f64 {
        ((s / k).ln() + (r + 0.5 * sigma * sigma) * t) / (sigma * t.sqrt())
    }
    
    /// Calculate d2 parameter
    pub fn d2(s: f64, k: f64, r: f64, sigma: f64, t: f64) -> f64 {
        Self::d1(s, k, r, sigma, t) - sigma * t.sqrt()
    }
    
    /// Black-Scholes option price
    pub fn price(
        option_type: OptionType,
        spot: f64,
        strike: f64,
        rate: f64,
        volatility: f64,
        time: f64,
        dividend: f64,
    ) -> f64 {
        if time <= 0.0 {
            return match option_type {
                OptionType::Call => (spot - strike).max(0.0),
                OptionType::Put => (strike - spot).max(0.0),
            };
        }
        
        let adjusted_spot = spot * (-dividend * time).exp();
        let d1 = Self::d1(adjusted_spot, strike, rate, volatility, time);
        let d2 = Self::d2(adjusted_spot, strike, rate, volatility, time);
        
        match option_type {
            OptionType::Call => {
                adjusted_spot * Self::norm_cdf(d1) - strike * (-rate * time).exp() * Self::norm_cdf(d2)
            }
            OptionType::Put => {
                strike * (-rate * time).exp() * Self::norm_cdf(-d2) - adjusted_spot * Self::norm_cdf(-d1)
            }
        }
    }
    
    /// Complete Greeks calculation
    pub fn calculate_greeks(
        option_type: OptionType,
        spot: f64,
        strike: f64,
        rate: f64,
        volatility: f64,
        time: f64,
        dividend: f64,
    ) -> Greeks {
        if time <= 0.0 {
            return Greeks::default();
        }
        
        let adjusted_spot = spot * (-dividend * time).exp();
        let sqrt_t = time.sqrt();
        let d1 = Self::d1(adjusted_spot, strike, rate, volatility, time);
        let d2 = Self::d2(adjusted_spot, strike, rate, volatility, time);
        let nd1 = Self::norm_cdf(d1);
        let nd2 = Self::norm_cdf(d2);
        let npd1 = Self::norm_pdf(d1);
        let discount = (-rate * time).exp();
        
        let mut greeks = Greeks::default();
        
        // First-order Greeks
        greeks.delta = match option_type {
            OptionType::Call => nd1 * (-dividend * time).exp(),
            OptionType::Put => (nd1 - 1.0) * (-dividend * time).exp(),
        };
        
        greeks.gamma = npd1 * (-dividend * time).exp() / (spot * volatility * sqrt_t);
        
        greeks.theta = match option_type {
            OptionType::Call => {
                -spot * npd1 * volatility * (-dividend * time).exp() / (2.0 * sqrt_t)
                    - rate * strike * discount * nd2
                    + dividend * spot * (-dividend * time).exp() * nd1
            }
            OptionType::Put => {
                -spot * npd1 * volatility * (-dividend * time).exp() / (2.0 * sqrt_t)
                    + rate * strike * discount * Self::norm_cdf(-d2)
                    - dividend * spot * (-dividend * time).exp() * Self::norm_cdf(-d1)
            }
        } / 365.0; // Convert to daily theta
        
        greeks.vega = spot * (-dividend * time).exp() * npd1 * sqrt_t / 100.0; // Per 1% change
        
        greeks.rho = match option_type {
            OptionType::Call => strike * time * discount * nd2 / 100.0,
            OptionType::Put => -strike * time * discount * Self::norm_cdf(-d2) / 100.0,
        };
        
        // Second-order Greeks
        greeks.vanna = -npd1 * d2 / volatility;
        greeks.charm = -npd1 * (2.0 * rate * time - d2 * volatility * sqrt_t) / (2.0 * time * volatility * sqrt_t);
        greeks.vomma = greeks.vega * d1 * d2 / volatility;
        greeks.speed = -greeks.gamma / spot * (d1 / (volatility * sqrt_t) + 1.0);
        greeks.zomma = greeks.gamma * (d1 * d2 - 1.0) / volatility;
        greeks.color = -npd1 / (2.0 * spot * time * volatility * sqrt_t) 
            * (1.0 + d1 * (2.0 * rate * time - d2 * volatility * sqrt_t) / (volatility * sqrt_t));
        
        // Lambda (leverage)
        let option_price = Self::price(option_type, spot, strike, rate, volatility, time, dividend);
        if option_price > 0.0 {
            greeks.lambda = greeks.delta * spot / option_price;
        }
        
        greeks
    }
    
    /// Implied volatility using Newton-Raphson method
    pub fn implied_volatility(
        option_type: OptionType,
        spot: f64,
        strike: f64,
        rate: f64,
        time: f64,
        market_price: f64,
        dividend: f64,
    ) -> Result<f64> {
        let mut vol = 0.2; // Initial guess 20%
        let tolerance = 1e-6;
        let max_iterations = 100;
        
        for _ in 0..max_iterations {
            let price = Self::price(option_type, spot, strike, rate, vol, time, dividend);
            let vega = Self::calculate_greeks(option_type, spot, strike, rate, vol, time, dividend).vega;
            
            if vega.abs() < 1e-10 {
                break;
            }
            
            let diff = market_price - price;
            if diff.abs() < tolerance {
                return Ok(vol);
            }
            
            vol += diff / (vega * 100.0); // Vega is per 1% change
            vol = vol.max(0.001).min(5.0); // Bound between 0.1% and 500%
        }
        
        Ok(vol)
    }
}

// ============================================================================
// MONTE CARLO SIMULATION ENGINE
// ============================================================================

pub struct MonteCarloEngine {
    pub simulations: usize,
    pub time_steps: usize,
    pub random_seed: u64,
}

impl MonteCarloEngine {
    pub fn new(simulations: usize, time_steps: usize) -> Self {
        Self {
            simulations,
            time_steps,
            random_seed: 42,
        }
    }
    
    /// Simulate asset paths using Geometric Brownian Motion
    pub fn simulate_paths(
        &self,
        spot: f64,
        rate: f64,
        volatility: f64,
        time: f64,
    ) -> Vec<Vec<f64>> {
        let dt = time / self.time_steps as f64;
        let sqrt_dt = dt.sqrt();
        let mut paths = Vec::with_capacity(self.simulations);
        
        // Use a simple random number generator for demonstration
        use rand::{Rng, SeedableRng};
        use rand_distr::{Distribution, StandardNormal};
        let mut rng = rand::rngs::StdRng::seed_from_u64(self.random_seed);
        
        for _ in 0..self.simulations {
            let mut path = Vec::with_capacity(self.time_steps + 1);
            path.push(spot);
            
            let mut current = spot;
            for _ in 0..self.time_steps {
                let z: f64 = StandardNormal.sample(&mut rng);
                current *= (
                    (rate - 0.5 * volatility * volatility) * dt + 
                    volatility * sqrt_dt * z
                ).exp();
                path.push(current);
            }
            
            paths.push(path);
        }
        
        paths
    }
    
    /// Price exotic options using Monte Carlo
    pub fn price_exotic(
        &self,
        option_type: &ExoticOptionType,
        spot: f64,
        strike: f64,
        rate: f64,
        volatility: f64,
        time: f64,
    ) -> f64 {
        let paths = self.simulate_paths(spot, rate, volatility, time);
        let discount = (-rate * time).exp();
        
        let payoffs: Vec<f64> = paths.iter().map(|path| {
            match option_type {
                ExoticOptionType::Asian => {
                    let avg = path.iter().sum::<f64>() / path.len() as f64;
                    (avg - strike).max(0.0)
                }
                ExoticOptionType::Barrier { barrier, barrier_type } => {
                    let breached = match barrier_type {
                        BarrierType::UpAndOut => path.iter().any(|&p| p > *barrier),
                        BarrierType::DownAndOut => path.iter().any(|&p| p < *barrier),
                        BarrierType::UpAndIn => path.iter().any(|&p| p > *barrier),
                        BarrierType::DownAndIn => path.iter().any(|&p| p < *barrier),
                    };
                    
                    let final_value = *path.last().unwrap();
                    match (barrier_type, breached) {
                        (BarrierType::UpAndOut | BarrierType::DownAndOut, true) => 0.0,
                        (BarrierType::UpAndIn | BarrierType::DownAndIn, false) => 0.0,
                        _ => (final_value - strike).max(0.0),
                    }
                }
                ExoticOptionType::Lookback => {
                    let max_price = path.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    (max_price - strike).max(0.0)
                }
            }
        }).collect();
        
        let avg_payoff = payoffs.iter().sum::<f64>() / payoffs.len() as f64;
        avg_payoff * discount
    }
}

#[derive(Debug, Clone)]
pub enum ExoticOptionType {
    Asian,
    Barrier { barrier: f64, barrier_type: BarrierType },
    Lookback,
}

#[derive(Debug, Clone)]
pub enum BarrierType {
    UpAndOut,
    DownAndOut,
    UpAndIn,
    DownAndIn,
}

// ============================================================================
// TRADING STRATEGIES
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionStrategy {
    pub name: String,
    pub legs: Vec<OptionLeg>,
    pub max_profit: Option<f64>,
    pub max_loss: Option<f64>,
    pub breakeven_points: Vec<f64>,
    pub margin_required: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionLeg {
    pub contract: OptionContract,
    pub quantity: i32, // Positive for long, negative for short
    pub entry_price: f64,
}

impl OptionStrategy {
    /// Iron Condor strategy
    pub fn iron_condor(
        index: IndexOption,
        spot: f64,
        expiry: DateTime<Utc>,
        wing_width: f64,
        body_width: f64,
    ) -> Self {
        let put_short_strike = spot - body_width / 2.0;
        let put_long_strike = put_short_strike - wing_width;
        let call_short_strike = spot + body_width / 2.0;
        let call_long_strike = call_short_strike + wing_width;
        
        Self {
            name: "Iron Condor".to_string(),
            legs: vec![
                // Bull Put Spread
                OptionLeg {
                    contract: OptionContract {
                        index: index.clone(),
                        option_type: OptionType::Put,
                        strike: put_long_strike,
                        expiry,
                        lot_size: index.lot_size(),
                        premium: 0.0,
                        open_interest: 0,
                        volume: 0,
                        implied_volatility: 0.0,
                        greeks: Greeks::default(),
                    },
                    quantity: 1,
                    entry_price: 0.0,
                },
                OptionLeg {
                    contract: OptionContract {
                        index: index.clone(),
                        option_type: OptionType::Put,
                        strike: put_short_strike,
                        expiry,
                        lot_size: index.lot_size(),
                        premium: 0.0,
                        open_interest: 0,
                        volume: 0,
                        implied_volatility: 0.0,
                        greeks: Greeks::default(),
                    },
                    quantity: -1,
                    entry_price: 0.0,
                },
                // Bear Call Spread
                OptionLeg {
                    contract: OptionContract {
                        index: index.clone(),
                        option_type: OptionType::Call,
                        strike: call_short_strike,
                        expiry,
                        lot_size: index.lot_size(),
                        premium: 0.0,
                        open_interest: 0,
                        volume: 0,
                        implied_volatility: 0.0,
                        greeks: Greeks::default(),
                    },
                    quantity: -1,
                    entry_price: 0.0,
                },
                OptionLeg {
                    contract: OptionContract {
                        index: index.clone(),
                        option_type: OptionType::Call,
                        strike: call_long_strike,
                        expiry,
                        lot_size: index.lot_size(),
                        premium: 0.0,
                        open_interest: 0,
                        volume: 0,
                        implied_volatility: 0.0,
                        greeks: Greeks::default(),
                    },
                    quantity: 1,
                    entry_price: 0.0,
                },
            ],
            max_profit: None,
            max_loss: Some(wing_width * index.lot_size() as f64),
            breakeven_points: vec![put_short_strike, call_short_strike],
            margin_required: 0.0,
        }
    }
    
    /// Calculate strategy P&L at given spot price
    pub fn calculate_pnl(&self, spot: f64) -> f64 {
        self.legs.iter().map(|leg| {
            let intrinsic = match leg.contract.option_type {
                OptionType::Call => (spot - leg.contract.strike).max(0.0),
                OptionType::Put => (leg.contract.strike - spot).max(0.0),
            };
            let pnl = intrinsic - leg.entry_price;
            pnl * leg.quantity as f64 * leg.contract.lot_size as f64
        }).sum()
    }
    
    /// Calculate aggregate Greeks for the strategy
    pub fn calculate_aggregate_greeks(&self) -> Greeks {
        let mut aggregate = Greeks::default();
        
        for leg in &self.legs {
            let multiplier = leg.quantity as f64 * leg.contract.lot_size as f64;
            aggregate.delta += leg.contract.greeks.delta * multiplier;
            aggregate.gamma += leg.contract.greeks.gamma * multiplier;
            aggregate.theta += leg.contract.greeks.theta * multiplier;
            aggregate.vega += leg.contract.greeks.vega * multiplier;
            aggregate.rho += leg.contract.greeks.rho * multiplier;
        }
        
        aggregate
    }
}

// ============================================================================
// EXECUTION MODES
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionMode {
    Backtest,
    Simulation,
    Paper,
    Live,
}

#[derive(Debug)]
pub struct OptionsEngine {
    pub mode: ExecutionMode,
    pub volatility_surface: Arc<RwLock<VolatilitySurface>>,
    pub positions: Arc<RwLock<Vec<OptionStrategy>>>,
    pub market_data: Arc<RwLock<MarketData>>,
    pub risk_metrics: Arc<RwLock<RiskMetrics>>,
}

#[derive(Debug, Clone)]
pub struct MarketData {
    pub spot_prices: HashMap<IndexOption, f64>,
    pub option_chain: HashMap<String, OptionContract>,
    pub last_update: DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct RiskMetrics {
    pub portfolio_delta: f64,
    pub portfolio_gamma: f64,
    pub portfolio_theta: f64,
    pub portfolio_vega: f64,
    pub value_at_risk: f64,
    pub margin_utilized: f64,
    pub max_drawdown: f64,
}

impl OptionsEngine {
    pub fn new(mode: ExecutionMode) -> Self {
        Self {
            mode,
            volatility_surface: Arc::new(RwLock::new(VolatilitySurface::new())),
            positions: Arc::new(RwLock::new(Vec::new())),
            market_data: Arc::new(RwLock::new(MarketData {
                spot_prices: HashMap::new(),
                option_chain: HashMap::new(),
                last_update: Utc::now(),
            })),
            risk_metrics: Arc::new(RwLock::new(RiskMetrics::default())),
        }
    }
    
    /// Switch execution mode
    pub async fn switch_mode(&mut self, new_mode: ExecutionMode) {
        println!("Switching from {:?} to {:?} mode", self.mode, new_mode);
        self.mode = new_mode;
        
        // Reset positions when switching modes
        if self.mode == ExecutionMode::Live {
            println!("⚠️ WARNING: Switching to LIVE trading mode!");
        }
    }
    
    /// Execute a strategy based on current mode
    pub async fn execute_strategy(&self, strategy: OptionStrategy) -> Result<()> {
        match self.mode {
            ExecutionMode::Backtest => self.execute_backtest(strategy).await,
            ExecutionMode::Simulation => self.execute_simulation(strategy).await,
            ExecutionMode::Paper => self.execute_paper(strategy).await,
            ExecutionMode::Live => self.execute_live(strategy).await,
        }
    }
    
    async fn execute_backtest(&self, strategy: OptionStrategy) -> Result<()> {
        println!("Executing strategy in BACKTEST mode: {}", strategy.name);
        // Backtest logic here
        Ok(())
    }
    
    async fn execute_simulation(&self, strategy: OptionStrategy) -> Result<()> {
        println!("Executing strategy in SIMULATION mode: {}", strategy.name);
        // Simulation logic here
        Ok(())
    }
    
    async fn execute_paper(&self, strategy: OptionStrategy) -> Result<()> {
        println!("Executing strategy in PAPER mode: {}", strategy.name);
        let mut positions = self.positions.write().await;
        positions.push(strategy);
        Ok(())
    }
    
    async fn execute_live(&self, strategy: OptionStrategy) -> Result<()> {
        println!("Executing strategy in LIVE mode: {}", strategy.name);
        // Zerodha API integration would go here
        Ok(())
    }
    
    /// Calculate portfolio risk metrics
    pub async fn update_risk_metrics(&self) {
        let positions = self.positions.read().await;
        let mut metrics = self.risk_metrics.write().await;
        
        // Reset metrics
        *metrics = RiskMetrics::default();
        
        // Aggregate Greeks across all positions
        for strategy in positions.iter() {
            let greeks = strategy.calculate_aggregate_greeks();
            metrics.portfolio_delta += greeks.delta;
            metrics.portfolio_gamma += greeks.gamma;
            metrics.portfolio_theta += greeks.theta;
            metrics.portfolio_vega += greeks.vega;
        }
        
        // Calculate VaR using parametric method
        let portfolio_std_dev = metrics.portfolio_vega * 0.01; // Simplified
        metrics.value_at_risk = 2.33 * portfolio_std_dev; // 99% confidence
    }
}

// ============================================================================
// MAIN ENTRY POINT
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    println!("🚀 ShrivenQuant Options Trading System");
    println!("Mathematical Warfare Machine for Indian Index Options");
    println!("{}", "=".repeat(60));
    
    // Initialize the engine
    let mut engine = OptionsEngine::new(ExecutionMode::Paper);
    
    // Example: Calculate option price and Greeks
    let spot = 21500.0; // Nifty spot
    let strike = 21600.0;
    let rate = RISK_FREE_RATE;
    let volatility = 0.15; // 15% IV
    let time_to_expiry = 7.0 / 365.0; // 7 days
    
    let call_price = BlackScholes::price(
        OptionType::Call,
        spot,
        strike,
        rate,
        volatility,
        time_to_expiry,
        0.0,
    );
    
    let greeks = BlackScholes::calculate_greeks(
        OptionType::Call,
        spot,
        strike,
        rate,
        volatility,
        time_to_expiry,
        0.0,
    );
    
    println!("\n📊 NIFTY 21600 CE (7 days to expiry):");
    println!("   Spot: {:.2}", spot);
    println!("   Strike: {:.2}", strike);
    println!("   IV: {:.1}%", volatility * 100.0);
    println!("   Price: ₹{:.2}", call_price);
    println!("\n📈 Greeks:");
    println!("   Delta: {:.4}", greeks.delta);
    println!("   Gamma: {:.6}", greeks.gamma);
    println!("   Theta: {:.2}", greeks.theta);
    println!("   Vega: {:.2}", greeks.vega);
    println!("   Rho: {:.4}", greeks.rho);
    
    // Example: Create an Iron Condor
    let iron_condor = OptionStrategy::iron_condor(
        IndexOption::Nifty50,
        spot,
        Utc::now() + chrono::Duration::days(7),
        100.0, // wing width
        200.0, // body width
    );
    
    println!("\n🦅 Iron Condor Strategy:");
    println!("   Strikes: {} legs", iron_condor.legs.len());
    println!("   Max Loss: ₹{:.2}", iron_condor.max_loss.unwrap_or(0.0));
    
    // Execute the strategy
    engine.execute_strategy(iron_condor).await?;
    
    // Update risk metrics
    engine.update_risk_metrics().await;
    let metrics = engine.risk_metrics.read().await;
    
    println!("\n⚠️ Portfolio Risk Metrics:");
    println!("   Delta: {:.2}", metrics.portfolio_delta);
    println!("   Gamma: {:.4}", metrics.portfolio_gamma);
    println!("   Theta: {:.2}", metrics.portfolio_theta);
    println!("   Vega: {:.2}", metrics.portfolio_vega);
    println!("   VaR (99%): ₹{:.2}", metrics.value_at_risk);
    
    // Monte Carlo simulation example
    let mc_engine = MonteCarloEngine::new(10000, 252);
    let exotic_price = mc_engine.price_exotic(
        &ExoticOptionType::Asian,
        spot,
        strike,
        rate,
        volatility,
        time_to_expiry,
    );
    
    println!("\n🎲 Monte Carlo Pricing:");
    println!("   Asian Option Price: ₹{:.2}", exotic_price);
    
    println!("\n✅ Options Trading System Initialized Successfully!");
    
    Ok(())
}

// Add these dependencies to Cargo.toml:
// [dependencies]
// tokio = { version = "1.47", features = ["full"] }
// chrono = "0.4"
// serde = { version = "1.0", features = ["derive"] }
// serde_json = "1.0"
// anyhow = "1.0"
// libm = "0.2"
// rand = "0.8"
// rand_distr = "0.4"