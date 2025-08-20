//! Build script for compiling protobuf definitions

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::compile_protos("../../proto/ml_inference.proto")?;
    Ok(())
}