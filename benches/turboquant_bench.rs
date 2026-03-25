//! TurboQuant Criterion benchmarks for detailed performance analysis

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::Rng;
use sqlite_knowledge_graph::vector::{LinearScanIndex, TurboQuantConfig, TurboQuantIndex};

fn generate_random_vector(dimension: usize) -> Vec<f32> {
    let mut rng = rand::thread_rng();
    let mut vector: Vec<f32> = (0..dimension)
        .map(|_| rng.gen::<f32>() * 2.0 - 1.0)
        .collect();

    // Normalize
    let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in vector.iter_mut() {
            *v /= norm;
        }
    }

    vector
}

fn benchmark_indexing(c: &mut Criterion) {
    let mut group = c.benchmark_group("indexing");

    for dimension in [128, 384, 768] {
        for num_vectors in [100, 1000, 5000] {
            group.throughput(Throughput::Elements(num_vectors as u64));

            group.bench_with_input(
                BenchmarkId::new("turboquant", format!("dim={}_n={}", dimension, num_vectors)),
                &(dimension, num_vectors),
                |b, (dim, n)| {
                    let config = TurboQuantConfig {
                        dimension: *dim,
                        bit_width: 3,
                        seed: 42,
                    };
                    let vectors: Vec<(i64, Vec<f32>)> = (0..*n)
                        .map(|i| {
                            let vec = generate_random_vector(*dim);
                            (i as i64, vec)
                        })
                        .collect();

                    b.iter(|| {
                        let mut index = TurboQuantIndex::new(config.clone()).unwrap();
                        for (id, vec) in &vectors {
                            index.add_vector(*id, vec).unwrap();
                        }
                        black_box(index.len());
                    });
                },
            );

            group.bench_with_input(
                BenchmarkId::new("linear_scan", format!("dim={}_n={}", dimension, num_vectors)),
                &(dimension, num_vectors),
                |b, (dim, n)| {
                    let config = TurboQuantConfig {
                        dimension: *dim,
                        bit_width: 3,
                        seed: 42,
                    };
                    let vectors: Vec<(i64, Vec<f32>)> = (0..*n)
                        .map(|i| {
                            let vec = generate_random_vector(*dim);
                            (i as i64, vec)
                        })
                        .collect();

                    b.iter(|| {
                        let mut index = LinearScanIndex::new(config.clone()).unwrap();
                        for (id, vec) in &vectors {
                            index.add_vector(*id, vec).unwrap();
                        }
                        black_box(index.len());
                    });
                },
            );
        }
    }

    group.finish();
}

fn benchmark_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("search");

    for dimension in [128, 384, 768] {
        for num_vectors in [100, 1000, 5000] {
            // Prepare indices
            let config = TurboQuantConfig {
                dimension,
                bit_width: 3,
                seed: 42,
            };
            let vectors: Vec<(i64, Vec<f32>)> = (0..num_vectors)
                .map(|i| {
                    let vec = generate_random_vector(dimension);
                    (i as i64, vec)
                })
                .collect();
            let query = generate_random_vector(dimension);

            let mut turbo_index = TurboQuantIndex::new(config.clone()).unwrap();
            for (id, vec) in &vectors {
                turbo_index.add_vector(*id, vec).unwrap();
            }

            let mut linear_index = LinearScanIndex::new(config.clone()).unwrap();
            for (id, vec) in &vectors {
                linear_index.add_vector(*id, vec).unwrap();
            }

            group.bench_with_input(
                BenchmarkId::new("turboquant", format!("dim={}_n={}", dimension, num_vectors)),
                &(turbo_index, query.clone()),
                |b, (index, query)| {
                    b.iter(|| {
                        let results = index.search(query, 10).unwrap();
                        black_box(results);
                    });
                },
            );

            group.bench_with_input(
                BenchmarkId::new("linear_scan", format!("dim={}_n={}", dimension, num_vectors)),
                &(linear_index, query.clone()),
                |b, (index, query)| {
                    b.iter(|| {
                        let results = index.search(query, 10).unwrap();
                        black_box(results);
                    });
                },
            );
        }
    }

    group.finish();
}

fn benchmark_bit_widths(c: &mut Criterion) {
    let mut group = c.benchmark_group("bit_widths");

    let dimension = 384;
    let num_vectors = 1000;

    for bit_width in [1, 2, 3, 4] {
        let config = TurboQuantConfig {
            dimension,
            bit_width,
            seed: 42,
        };

        let vectors: Vec<(i64, Vec<f32>)> = (0..num_vectors)
            .map(|i| {
                let vec = generate_random_vector(dimension);
                (i as i64, vec)
            })
            .collect();
        let query = generate_random_vector(dimension);

        let mut turbo_index = TurboQuantIndex::new(config.clone()).unwrap();
        for (id, vec) in &vectors {
            turbo_index.add_vector(*id, vec).unwrap();
        }

        group.bench_with_input(
            BenchmarkId::new("turboquant_indexing", format!("bits={}", bit_width)),
            &vectors,
            |b, vectors| {
                b.iter(|| {
                    let mut index = TurboQuantIndex::new(config.clone()).unwrap();
                    for (id, vec) in vectors {
                        index.add_vector(*id, vec).unwrap();
                    }
                    black_box(index.len());
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("turboquant_search", format!("bits={}", bit_width)),
            &turbo_index,
            |b, index| {
                b.iter(|| {
                    let results = index.search(&query, 10).unwrap();
                    black_box(results);
                });
            },
        );
    }

    group.finish();
}

fn benchmark_batch_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_operations");

    let dimension = 384;
    let num_vectors = 1000;

    let config = TurboQuantConfig {
        dimension,
        bit_width: 3,
        seed: 42,
    };

    let vectors: Vec<(i64, Vec<f32>)> = (0..num_vectors)
        .map(|i| {
            let vec = generate_random_vector(dimension);
            (i as i64, vec)
        })
        .collect();

    group.bench_function("add_vector_individual", |b| {
        b.iter(|| {
            let mut index = TurboQuantIndex::new(config.clone()).unwrap();
            for (id, vec) in &vectors {
                index.add_vector(*id, vec).unwrap();
            }
            black_box(index.len());
        });
    });

    group.bench_function("add_vector_batch", |b| {
        b.iter(|| {
            let mut index = TurboQuantIndex::new(config.clone()).unwrap();
            index.add_vectors_batch(&vectors).unwrap();
            black_box(index.len());
        });
    });

    let mut turbo_index = TurboQuantIndex::new(config.clone()).unwrap();
    for (id, vec) in &vectors {
        turbo_index.add_vector(*id, vec).unwrap();
    }

    let queries: Vec<Vec<f32>> = (0..10).map(|_| generate_random_vector(dimension)).collect();

    group.bench_function("search_individual", |b| {
        b.iter(|| {
            for query in &queries {
                let _ = turbo_index.search(query, 10).unwrap();
            }
            black_box(());
        });
    });

    group.bench_function("search_batch", |b| {
        b.iter(|| {
            let _ = turbo_index.search_batch(&queries, 10).unwrap();
            black_box(());
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_indexing,
    benchmark_search,
    benchmark_bit_widths,
    benchmark_batch_operations
);
criterion_main!(benches);
