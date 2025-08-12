//! Demonstration of advanced feature extraction with LOB v2

use common::{L2Update, Px, Qty, Side, Symbol, Ts};
use lob::{CrossResolution, MarketRegime, OrderBookV2, features_v2};
use rand::prelude::*;

fn main() {
    println!("🎯 Advanced Feature Extraction Demo");
    println!("====================================\n");

    // Create LOB v2 with ROI optimization
    let symbol = Symbol(1);
    let mut book = OrderBookV2::new_with_roi(
        symbol, 0.01,  // tick size
        1.0,   // lot size
        100.0, // ROI center
        10.0,  // ROI width
    );
    book.set_cross_resolution(CrossResolution::AutoResolve);

    // Create feature calculators
    let mut hft_calc = features_v2::create_hft_calculator(symbol);
    let mut mm_calc = features_v2::create_mm_calculator(symbol);

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
        println!("  Price Trend Signal: {:.4}", final_features.price_trend);
        println!(
            "  Mean Reversion Signal: {:.4}",
            final_features.mean_reversion_signal
        );
        println!("  Momentum: {:.4}", final_features.momentum);
        println!(
            "  Adverse Selection Risk: {:.2}%",
            final_features.adverse_selection * 100.0
        );

        // Trading recommendations
        println!("\n💡 Trading Recommendations:");

        if final_features.price_trend.abs() > 0.5 {
            if final_features.price_trend > 0.0 {
                println!("  ✅ BULLISH: Strong positive price trend");
            } else {
                println!("  ❌ BEARISH: Strong negative price trend");
            }
        }

        if final_features.mean_reversion_signal.abs() > 0.3 {
            println!("  🔄 MEAN REVERSION opportunity detected");
        }

        if final_features.adverse_selection > 0.7 {
            println!("  ⚠️ HIGH TOXICITY: Avoid aggressive market making");
        } else if final_features.liquidity_score > 0.8 {
            println!("  💰 GOOD LIQUIDITY: Favorable for market making");
        }

        if final_features.volatility_forecast > 0.02 {
            println!("  📊 HIGH VOLATILITY expected - widen spreads");
        }
    }

    // Compare with MM calculator
    if let Some(mm_features) = mm_calc.calculate(&book) {
        println!("\n📊 Market Maker Signals:");
        println!("  Liquidity Score: {:.2}", mm_features.liquidity_score);
        println!("  Stability Index: {:.2}", mm_features.stability_index);
        println!(
            "  Effective Spread: {:.2} bps",
            mm_features.effective_spread
        );
        println!("  Price Impact: {:.2} bps", mm_features.price_impact);

        // MM recommendations
        if mm_features.liquidity_score > 0.7 && mm_features.stability_index > 0.6 {
            println!("\n  ✅ FAVORABLE conditions for market making");
            let suggested_spread = mm_features.effective_spread * 1.2;
            println!("  Suggested spread: {:.2} bps", suggested_spread);
        } else {
            println!("\n  ⚠️ CHALLENGING conditions for market making");
        }
    }

    println!("\n✅ Demo completed successfully!");
}

fn print_features(features: &lob::features_v2::FeatureFrameV2, strategy: &str) {
    println!("\n  {} Features:", strategy);
    println!(
        "    Spread: {} ticks ({:.2} bps)",
        features.spread_ticks, features.weighted_spread
    );
    println!("    Imbalance: {:.4}", features.imbalance);
    println!("    Flow Toxicity: {:.4}", features.flow_toxicity);
    println!("    Book Pressure: {:.4}", features.book_pressure);
    println!("    Regime: {:?}", features.regime);
    println!("    Liquidity Score: {:.2}", features.liquidity_score);
}
