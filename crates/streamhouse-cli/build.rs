//! Build Script for StreamHouse CLI
//!
//! This build script compiles Protocol Buffer definitions and generates
//! Rust code for the gRPC client implementation.
//!
//! ## What it does:
//! 1. Locates the proto file in the server crate
//! 2. Generates gRPC client stubs (server disabled)
//! 3. Creates Rust types for all protobuf messages
//!
//! ## Generated Code:
//! The generated code is included in the library via `tonic::include_proto!`
//! in `src/main.rs`. This provides:
//! - `StreamHouseClient` - gRPC client for calling the server
//! - All request/response message types
//! - Serialization/deserialization for protobuf
//!
//! ## Why Client-Only:
//! The CLI only needs to make requests (client), not serve requests (server).
//! Setting `build_server(false)` reduces compile time and binary size.
//!
//! ## Proto Location:
//! The proto file is located in `../streamhouse-server/proto/streamhouse.proto`
//! to ensure the CLI and server use the exact same API definition.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile(
            &["../streamhouse-server/proto/streamhouse.proto"],
            &["../streamhouse-server/proto"],
        )?;
    Ok(())
}
