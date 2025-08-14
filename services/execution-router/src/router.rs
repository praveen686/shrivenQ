//! Smart order routing logic

use crate::{OrderRequest, VenueStrategy};
use common::Symbol;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// Venue characteristics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VenueInfo {
    /// Venue name
    pub name: String,
    /// Maker fee (basis points)
    pub maker_fee_bps: i32,
    /// Taker fee (basis points)
    pub taker_fee_bps: i32,
    /// Average latency (microseconds)
    pub avg_latency_us: u64,
    /// Available liquidity
    pub liquidity_score: i32,
    /// Supported order types
    pub supported_order_types: Vec<String>,
    /// Is active
    pub is_active: bool,
}

/// Routing decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    /// Primary venue
    pub primary_venue: String,
    /// Backup venues
    pub backup_venues: Vec<String>,
    /// Split allocations (venue -> percentage)
    pub split_allocation: FxHashMap<String, i32>,
    /// Routing reason
    pub reason: String,
}

/// Smart order router
pub struct SmartOrderRouter {
    /// Available venues
    venues: FxHashMap<String, VenueInfo>,
    /// Symbol to venue mapping
    symbol_venues: FxHashMap<Symbol, Vec<String>>,
    /// Default venue
    default_venue: String,
}

impl SmartOrderRouter {
    /// Create new router
    pub fn new(default_venue: String) -> Self {
        Self {
            venues: FxHashMap::default(),
            symbol_venues: FxHashMap::default(),
            default_venue,
        }
    }

    /// Add venue
    pub fn add_venue(&mut self, venue: VenueInfo) {
        self.venues.insert(venue.name.clone(), venue);
    }

    /// Add symbol-specific venue mapping
    pub fn add_symbol_venue_mapping(&mut self, symbol: Symbol, venues: Vec<String>) {
        self.symbol_venues.insert(symbol, venues);
    }

    /// Get available venues for a specific symbol
    pub fn get_symbol_venues(&self, symbol: Symbol) -> Vec<String> {
        self.symbol_venues.get(&symbol).cloned().unwrap_or_else(|| {
            // Return all active venues if no specific mapping exists
            self.venues
                .iter()
                .filter(|(_, info)| info.is_active)
                .map(|(name, _)| name.clone())
                .collect()
        })
    }

    /// Route order
    pub fn route_order(&self, request: &OrderRequest, strategy: VenueStrategy) -> RoutingDecision {
        match strategy {
            VenueStrategy::Primary => {
                // Use symbol-specific primary venue if available
                let available_venues = self.get_symbol_venues(request.symbol);
                let primary_venue = available_venues
                    .first()
                    .cloned()
                    .unwrap_or_else(|| self.default_venue.clone());

                RoutingDecision {
                    primary_venue,
                    backup_venues: vec![],
                    split_allocation: FxHashMap::default(),
                    reason: "Primary venue routing".to_string(),
                }
            }
            VenueStrategy::CostOptimal => self.route_cost_optimal(request),
            VenueStrategy::Liquidity => self.route_liquidity_based(request),
            VenueStrategy::Smart => self.route_smart(request),
            VenueStrategy::Split => self.route_split(request),
        }
    }

    /// Cost-optimal routing
    fn route_cost_optimal(&self, _request: &OrderRequest) -> RoutingDecision {
        // Find venue with lowest fees
        let mut best_venue = self.default_venue.clone();
        let mut lowest_fee = i32::MAX;

        for (name, info) in &self.venues {
            if info.is_active && info.taker_fee_bps < lowest_fee {
                lowest_fee = info.taker_fee_bps;
                best_venue = name.clone();
            }
        }

        RoutingDecision {
            primary_venue: best_venue,
            backup_venues: vec![],
            split_allocation: FxHashMap::default(),
            reason: format!("Lowest fee: {} bps", lowest_fee),
        }
    }

    /// Liquidity-based routing
    fn route_liquidity_based(&self, _request: &OrderRequest) -> RoutingDecision {
        // Find venue with best liquidity
        let mut best_venue = self.default_venue.clone();
        let mut best_liquidity = 0;

        for (name, info) in &self.venues {
            if info.is_active && info.liquidity_score > best_liquidity {
                best_liquidity = info.liquidity_score;
                best_venue = name.clone();
            }
        }

        RoutingDecision {
            primary_venue: best_venue,
            backup_venues: vec![],
            split_allocation: FxHashMap::default(),
            reason: format!("Best liquidity score: {}", best_liquidity),
        }
    }

    /// Smart routing combining multiple factors
    fn route_smart(&self, _request: &OrderRequest) -> RoutingDecision {
        // Score each venue based on multiple factors
        let mut scores: FxHashMap<String, i32> = FxHashMap::default();

        for (name, info) in &self.venues {
            if !info.is_active {
                continue;
            }

            let mut score = 0;

            // Fee score (lower is better)
            score += (1000 - info.taker_fee_bps) / 10;

            // Liquidity score
            score += info.liquidity_score;

            // Latency score (lower is better)
            if info.avg_latency_us < 1000 {
                score += 50;
            } else if info.avg_latency_us < 5000 {
                score += 30;
            } else if info.avg_latency_us < 10000 {
                score += 10;
            }

            scores.insert(name.clone(), score);
        }

        // Select best venue
        let best_venue = scores
            .iter()
            .max_by_key(|&(_, score)| score)
            .map(|(name, _)| name.clone())
            .unwrap_or(self.default_venue.clone());

        RoutingDecision {
            primary_venue: best_venue,
            backup_venues: vec![],
            split_allocation: FxHashMap::default(),
            reason: "Smart routing based on fees, liquidity, and latency".to_string(),
        }
    }

    /// Split order across venues
    fn route_split(&self, _request: &OrderRequest) -> RoutingDecision {
        let mut allocations = FxHashMap::default();
        let active_venues: Vec<_> = self
            .venues
            .iter()
            .filter(|(_, info)| info.is_active)
            .collect();

        if active_venues.is_empty() {
            allocations.insert(self.default_venue.clone(), 10000); // 100%
        } else {
            // Equal split for simplicity
            // SAFETY: Clamped to i32::MAX ensures safe cast
            let venue_count = active_venues.len().min(i32::MAX as usize) as i32;
            let split_pct = 10000 / venue_count;
            for (name, _) in active_venues {
                allocations.insert(name.clone(), split_pct);
            }
        }

        RoutingDecision {
            primary_venue: self.default_venue.clone(),
            backup_venues: vec![],
            split_allocation: allocations,
            reason: "Split order across venues".to_string(),
        }
    }
}
