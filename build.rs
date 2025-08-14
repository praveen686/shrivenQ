fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile protobuf files
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir("src/generated")
        .compile(
            &[
                "proto/auth.proto",
                "proto/market_data.proto",
                "proto/risk.proto",
                "proto/execution.proto",
            ],
            &["proto"],
        )?;
    
    Ok(())
}