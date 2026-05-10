// End-to-end inference benchmark

use aegis_proto::InferenceRequest;
use aegis_runtime::{AEGISRuntime, RuntimeConfig};
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_e2e_inference(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("e2e_inference_small", |b| {
        b.to_async(&rt).iter(|| async {
            let config = RuntimeConfig::default();
            let runtime = AEGISRuntime::new(config).await.unwrap();

            let request = black_box(InferenceRequest {
                request_id: "bench-req-1".to_string(),
                prompt: "The quick brown fox".to_string(),
                max_tokens: 10,
                temperature: 0.7,
                top_p: 0.9,
                stop_tokens: vec![],
                seed: 42,
                enable_speculation: false,
                draft_length: 0,
                auth_token: "bearer-token".to_string(),
                metadata: Default::default(),
            });

            let _ = runtime.execute(request).await;
        })
    });

    c.bench_function("e2e_inference_with_speculation", |b| {
        b.to_async(&rt).iter(|| async {
            let config = RuntimeConfig::default();
            let runtime = AEGISRuntime::new(config).await.unwrap();

            let request = black_box(InferenceRequest {
                request_id: "bench-req-spec".to_string(),
                prompt: "The quick brown fox".to_string(),
                max_tokens: 10,
                temperature: 0.7,
                top_p: 0.9,
                stop_tokens: vec![],
                seed: 42,
                enable_speculation: true,
                draft_length: 4,
                auth_token: "bearer-token".to_string(),
                metadata: Default::default(),
            });

            let _ = runtime.execute(request).await;
        })
    });
}

criterion_group!(benches, benchmark_e2e_inference);
criterion_main!(benches);
