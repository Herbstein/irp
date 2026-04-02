fn main() {
    let mut builder = tonic_prost_build::configure();

    #[cfg(feature = "server")]
    {
        builder = builder.build_server(true);
    }

    #[cfg(feature = "client")]
    {
        builder = builder.build_client(true);
    }

    builder.compile_protos(&["irp.proto"], &["proto/"]).unwrap();
}
