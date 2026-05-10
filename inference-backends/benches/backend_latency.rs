// Backend latency benchmarks

use aegis_inference_backends::{BackendFactory, BackendConfig, GenerationParams};
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_backend_latency(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("mock_backend_10_tokens", |b| {
        b.to_async(&rt).iter(|| async {
            let config = BackendConfig::default();
            let backend = BackendFactory::create("mock", config).await.unwrap();

            let params = black_box(GenerationParams {
                prompt: "The quick brown fox jumps over the lazy dog".to_string(),
                max_tokens: 10,
                temperature: 0.7,
                top_p: 0.9,
                ..Default::default()
            });

            let _ = backend.generate(params).await;
        })
    });

    c.bench_function("mock_backend_100_tokens", |b| {
        b.to_async(&rt).iter(|| async {
            let config = BackendConfig::default();
            let backend = BackendFactory::create("mock", config).await.unwrap();

            let params = black_box(GenerationParams {
                prompt: "The quick brown fox jumps over the lazy dog".to_string(),
                max_tokens: 100,
                temperature: 0.7,
                top_p: 0.9,
                ..Default::default()
            });

            let _ = backend.generate(params).await;
        })
    });

    c.bench_function("backend_factory_creation", |b| {
        b.to_async(&rt).iter(|| async {
            let config = black_box(BackendConfig::default());
            let _ = BackendFactory::create("mock", config).await;
        })
    });
}

criterion_group!(benches, benchmark_backend_latency);
criterion_main!(benches);
