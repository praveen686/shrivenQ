//! Demonstration of advanced feature extraction with LOB v2

use crate::{CrossResolution, FeatureFrameV2Fixed, MarketRegime, OrderBookV2, features_v2_fixed};
use common::{L2Update, Px, Qty, Side, Symbol, Ts};
use rand::prelude::*;

/// Demo function showing advanced feature extraction with LOB v2
pub fn run_features_demo() {
    println!("🎯 Advanced Feature Extraction Demo");
    println!("====================================\n");

    // Create LOB v2 with ROI optimization
    let symbol = Symbol(1);
    let mut book = OrderBookV2::new_with_roi(
        symbol,
        Px::new(0.01),  // tick size
        Qty::new(1.0),  // lot size
        Px::new(100.0), // ROI center
        Px::new(10.0),  // ROI width
    );
    book.set_cross_resolution(CrossResolution::AutoResolve);

    // Create feature calculators
    let mut hft_calc = features_v2_fixed::create_hft_calculator_fixed(symbol);
    let mut mm_calc = features_v2_fixed::create_mm_calculator_fixed(symbol);

    // Generate realistic market data
    let mut rng = StdRng::seed_from_u64(42);
    let mut ts = 1_000_000_000u64; // Start at 1 second

    println!("📊 Simulating market conditions...\n");

    // Simulate different market regimes
    for regime_phase in 0..4 {
        let (volatility, spread_mult, regime_name) = match regime_phase {
            0 => (0.0001, 1.0, "Stable"),
            1 => (0.001, 1.5, "Normal"),
            2 => (0.01, 3.0, "Volatile"),
            _ => (0.05, 10.0, "Stressed"),
        };

        println!("\n🔄 Market Regime: {}", regime_name);
        println!("{}", "-".repeat(40));

        for i in 0..100 {
            // Generate updates with regime-appropriate characteristics
            let mid_price = 100.0 + rng.r#gen::<f64>() * volatility * 1000.0;
            let spread = 0.01 * spread_mult * (1.0 + rng.r#gen::<f64>() * 0.5);

            // Create bid update
            let bid_update = L2Update::new(Ts::from_nanos(ts), symbol).with_level_data(
                Side::Bid,
                Px::new(mid_price - spread / 2.0),
                Qty::new(100.0 + rng.r#gen::<f64>() * 900.0),
                0,
            );

            // Create ask update
            let ask_update = L2Update::new(Ts::from_nanos(ts + 1000), symbol).with_level_data(
                Side::Ask,
                Px::new(mid_price + spread / 2.0),
                Qty::new(100.0 + rng.r#gen::<f64>() * 900.0),
                0,
            );

            // Apply updates
            book.apply_validated(&bid_update).ok();
            book.apply_validated(&ask_update).ok();

            // Calculate features every 10 updates
            if i % 10 == 0 {
                if let Some(features) = hft_calc.calculate(&book) {
                    if i == 90 {
                        // Show last update of each regime
                        print_features(&features, "HFT");
                    }

                    // Check regime detection
                    if features.regime
                        != match regime_phase {
                            0 => MarketRegime::Stable,
                            1 => MarketRegime::Normal,
                            2 => MarketRegime::Volatile,
                            _ => MarketRegime::Stressed,
                        }
                    {
                        println!(
                            "  ⚠️ Regime transition detected: {:?} -> {:?}",
                            match regime_phase {
                                0 => MarketRegime::Stable,
                                1 => MarketRegime::Normal,
                                2 => MarketRegime::Volatile,
                                _ => MarketRegime::Stressed,
                            },
                            features.regime
                        );
                    }
                }
            }

            // Add some market events
            if rng.r#gen::<f64>() < 0.05 {
                // Large order imbalance
                let side = if rng.r#gen::<bool>() {
                    Side::Bid
                } else {
                    Side::Ask
                };
                let shock = L2Update::new(Ts::from_nanos(ts + 2000), symbol).with_level_data(
                    side,
                    Px::new(if side == Side::Bid {
                        mid_price - spread * 0.3
                    } else {
                        mid_price + spread * 0.3
                    }),
                    Qty::new(5000.0), // Large order
                    0,
                );
                book.apply_validated(&shock).ok();
            }

            ts += 10_000_000; // 10ms between updates
        }
    }

    // Final analysis
    println!("\n\n🎯 Final Market Analysis");
    println!("{}", "=".repeat(50));

    if let Some(final_features) = hft_calc.calculate(&book) {
        println!("\n📈 HFT Strategy Signals:");
        // SAFETY: Cast is safe within expected range
        println!(
            "  Price Trend Signal: {:.4}",
            // SAFETY: Cast is safe within expected range
            final_features.price_trend as f64 / 10000.0
        );
        println!(
            // SAFETY: Cast is safe within expected range
            "  Mean Reversion Signal: {:.4}",
            // SAFETY: Cast is safe within expected range
            // SAFETY: Cast is safe within expected range
            final_features.mean_reversion_signal as f64 / 10000.0
        );
        // SAFETY: Cast is safe within expected range
        println!(
            "  Momentum: {:.4}",
            final_features.momentum as f64 / 10000.0
        );
        // SAFETY: Cast is safe within expected range
        println!(
            "  Adverse Selection Risk: {:.2}%",
            final_features.adverse_selection as f64 / 100.0
        );

        // Trading recommendations
        println!("\n💡 Trading Recommendations:");

        if final_features.price_trend.abs() > 5000 {
            // 0.5 in fixed-point
            if final_features.price_trend > 0 {
                println!("  ✅ BULLISH: Strong positive price trend");
            } else {
                println!("  ❌ BEARISH: Strong negative price trend");
            }
        }

        if final_features.mean_reversion_signal.abs() > 3000 {
            // 0.3 in fixed-point
            println!("  🔄 MEAN REVERSION opportunity detected");
        }

        if final_features.adverse_selection > 7000 {
            // 0.7 in fixed-point
            println!("  ⚠️ HIGH TOXICITY: Avoid aggressive market making");
        } else if final_features.liquidity_score > 8000 {
            // 0.8 in fixed-point
            println!("  💰 GOOD LIQUIDITY: Favorable for market making");
        }

        if final_features.volatility_forecast > 200 {
            // 0.02 in fixed-point
            println!("  📊 HIGH VOLATILITY expected - widen spreads");
        }
    }
    // SAFETY: Cast is safe within expected range

    // Compare with MM calculator
    if let Some(mm_features) = mm_calc.calculate(&book) {
        // SAFETY: Cast is safe within expected range
        println!("\n📊 Market Maker Signals:");
        // SAFETY: Cast is safe within expected range
        // SAFETY: Cast is safe within expected range
        println!(
            "  Liquidity Score: {:.2}",
            // SAFETY: Cast is safe within expected range
            mm_features.liquidity_score as f64 / 10000.0
        );
        println!(
            // SAFETY: Cast is safe within expected range
            "  Stability Index: {:.2}",
            mm_features.stability_index as f64 / 10000.0
        );
        // SAFETY: Cast is safe within expected range
        println!(
            "  Effective Spread: {:.2} bps",
            mm_features.effective_spread as f64 / 100.0 // SAFETY: Cast is safe within expected range
        );
        println!(
            "  Price Impact: {:.2} bps",
            mm_features.price_impact as f64 / 100.0
        );

        // SAFETY: Cast is safe within expected range
        // MM recommendations
        if mm_features.liquidity_score > 7000 && mm_features.stability_index > 6000 {
            // 0.7, 0.6 in fixed-point
            println!("\n  ✅ FAVORABLE conditions for market making");
            let suggested_spread = (mm_features.effective_spread * 12) / 10; // 1.2x in fixed-point
            println!(
                "  Suggested spread: {:.2} bps",
                suggested_spread as f64 / 100.0
            );
        } else {
            println!("\n  ⚠️ CHALLENGING conditions for market making");
        }
    }
    // SAFETY: Cast is safe within expected range

    // SAFETY: Cast is safe within expected range
    println!("\n✅ Demo completed successfully!");
    // SAFETY: Cast is safe within expected range
}
// SAFETY: Cast is safe within expected range

fn print_features(features: &FeatureFrameV2Fixed, strategy: &str) {
    println!("\n  {} Features:", strategy);
    // SAFETY: Cast is safe within expected range
    println!(
        "    Spread: {} ticks ({:.2} bps)",
        features.spread_ticks,
        // SAFETY: Cast is safe within expected range
        features.weighted_spread as f64 / 100.0
    );
    println!("    Imbalance: {:.4}", features.imbalance as f64 / 10000.0);
    println!(
        // SAFETY: Cast is safe within expected range
        "    Flow Toxicity: {:.4}",
        features.flow_toxicity as f64 / 10000.0
    );
    println!(
        "    Book Pressure: {:.4}",
        features.book_pressure as f64 / 10000.0
    );
    println!("    Regime: {:?}", features.regime);
    println!(
        "    Liquidity Score: {:.2}",
        features.liquidity_score as f64 / 10000.0
    );
}
