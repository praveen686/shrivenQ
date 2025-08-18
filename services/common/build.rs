use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .file_descriptor_set_path(out_dir.join("shrivenquant_descriptor.bin"))
        .compile_protos(
            &[
                "../../proto/auth.proto",
                "../../proto/market_data.proto",
                "../../proto/risk.proto",
                "../../proto/execution.proto",
                "../../proto/trading.proto",
                "../../proto/backtesting.proto",
            ],
            &["../../proto"],
        )?;

    Ok(())
}