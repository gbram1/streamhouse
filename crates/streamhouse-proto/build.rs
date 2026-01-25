// Build script to compile protobuf files into Rust code.
//
// This runs at compile time and generates Rust structs and trait implementations
// from the .proto files in the proto/ directory.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile producer.proto
    tonic_build::compile_protos("proto/producer.proto")?;

    Ok(())
}
