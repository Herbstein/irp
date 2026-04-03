fn main() {
    tonic_prost_build::configure()
        .build_server(cfg!(feature = "server"))
        .build_client(cfg!(feature = "client"))
        .compile_protos(&["irp.proto"], &["proto/"])
        .unwrap();
}
