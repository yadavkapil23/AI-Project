fn main() {
    // Plain prost-only protos (no gRPC service definitions)
    prost_build::Config::new()
        .compile_protos(
            &[
                "src/proto/inference.proto",
                "src/proto/audit.proto",
            ],
            &["src/proto"],
        )
        .expect("prost protobuf compilation failed");

    // tonic gRPC services live in scheduling.proto
    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .compile(&["src/proto/scheduling.proto"], &["src/proto"])
        .expect("tonic scheduling.proto compilation failed");
}
