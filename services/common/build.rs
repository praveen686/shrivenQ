//! Build script for compiling protobuf definitions

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::configure()
        .compile_protos(
            &[
                "../../proto/auth.proto",
                "../../proto/market_data.proto",
                "../../proto/risk.proto",
                "../../proto/execution.proto",
                "../../proto/trading.proto",
                "../../proto/backtesting.proto",
                "../../proto/secrets.proto",
            ],
            &["../../proto"],
        )?;

    Ok(())
}