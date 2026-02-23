fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use vendored protoc so no system installation is required.
    // PROTOC env var takes priority if set (e.g. in CI with a custom protoc).
    if std::env::var("PROTOC").is_err() {
        let protoc = protoc_bin_vendored::protoc_bin_path()
            .expect("protoc-bin-vendored: could not find bundled protoc binary");
        std::env::set_var("PROTOC", protoc);
    }

    let proto_file = "../detector/proto/detector.proto";
    let proto_include = "../detector/proto";

    if !std::path::Path::new(proto_file).exists() {
        eprintln!("ERROR: Proto file not found at: {}", proto_file);
        eprintln!("Current directory: {:?}", std::env::current_dir().unwrap_or_default());
        return Err("Proto file not found".into());
    }

    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile(&[proto_file], &[proto_include])?;

    println!("cargo:rerun-if-changed={}", proto_file);
    println!("cargo:rerun-if-changed={}", proto_include);
    Ok(())
}
