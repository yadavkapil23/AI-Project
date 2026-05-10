fn main() {
    // Compile proto files
    prost_build::Config::new()
        .compile_protos(
            &["src/proto/inference.proto", "src/proto/audit.proto"],
            &["src/proto"],
        )
        .expect("protobuf compilation failed");
}
