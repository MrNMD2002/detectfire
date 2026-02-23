fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile protobuf definitions for gRPC
    // Use default OUT_DIR instead of hardcoded path for Docker compatibility
    tonic_build::configure()
        .build_server(true)
        .build_client(true) // Also build client for internal use
        .compile(&["proto/detector.proto"], &["proto"])?;

    println!("cargo:rerun-if-changed=proto/detector.proto");
    Ok(())
}
