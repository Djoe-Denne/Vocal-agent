fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);

    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_protos(&["../proto/tempo.proto"], &["../proto"])?;

    println!("cargo:rerun-if-changed=../proto/tempo.proto");
    Ok(())
}
