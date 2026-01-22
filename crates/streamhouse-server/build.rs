//! Build Script for StreamHouse gRPC Server
//!
//! This build script compiles Protocol Buffer definitions and generates
//! Rust code for the gRPC server implementation.
//!
//! ## What it does:
//! 1. Compiles `proto/streamhouse.proto` into Rust code
//! 2. Generates server-side gRPC stubs (client disabled)
//! 3. Creates file descriptor set for gRPC reflection support
//!
//! ## Reflection Support:
//! The file descriptor set (`streamhouse_descriptor.bin`) enables gRPC
//! reflection, allowing tools like `grpcurl` to discover and call services
//! without needing the proto files. This is essential for testing and
//! debugging.
//!
//! ## Generated Files:
//! - `streamhouse.rs`: Generated Rust code for protocol buffer messages
//! - `streamhouse_descriptor.bin`: File descriptor set for reflection
//!
//! The generated code is automatically included in the library via
//! `include!` macros in the source files.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .file_descriptor_set_path("proto/streamhouse_descriptor.bin")
        .compile(&["proto/streamhouse.proto"], &["proto"])?;
    Ok(())
}
