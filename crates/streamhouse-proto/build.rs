// Build script to compile protobuf files into Rust code.
//
// This runs at compile time and generates Rust structs and trait implementations
// from the .proto files in the proto/ directory.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile producer.proto (legacy - used by some internal components)
    tonic_build::compile_protos("proto/producer.proto")?;

    // Compile streamhouse.proto (main gRPC API)
    // This is the service exposed by the unified server on port 50051
    tonic_build::compile_protos("proto/streamhouse.proto")?;

    Ok(())
}
