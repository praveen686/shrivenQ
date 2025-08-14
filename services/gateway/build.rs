fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile protobuf definitions for gRPC client integration
    tonic_build::configure()
        .build_server(false) // We only need clients
        .build_client(true)
        .compile_protos(
            &[
                "../../proto/auth.proto",
                "../../proto/execution.proto",
                "../../proto/market_data.proto",
                "../../proto/risk.proto",
            ],
            &["../../proto"],
        )?;

    println!("cargo:rerun-if-changed=../../proto");
    Ok(())
}
