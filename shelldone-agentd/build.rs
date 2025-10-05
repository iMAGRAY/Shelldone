fn main() {
    let proto_path = "proto/mcp.proto";
    println!("cargo:rerun-if-changed={proto_path}");
    println!("cargo:rerun-if-changed=proto");

    let protoc_path =
        protoc_bin_vendored::protoc_bin_path().expect("failed to locate vendored protoc binary");
    std::env::set_var("PROTOC", protoc_path);

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&[proto_path], &["proto"])
        .expect("failed to compile MCP proto");
}
