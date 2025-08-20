//! Build script for compiling protobuf definitions

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile protobuf definitions for gRPC client integration
    tonic_prost_build::configure()
        .file_descriptor_set_path("src/proto_descriptor.bin")
        .type_attribute(".", "#[allow(missing_docs)]")
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
