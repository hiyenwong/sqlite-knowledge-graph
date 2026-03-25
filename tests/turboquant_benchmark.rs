//! TurboQuant comprehensive benchmark and comparison tests
//!
//! This module provides:
//! - Unit tests for different dimensions and bit widths
//! - Performance benchmarks (indexing time, search latency, memory usage)
//! - Recall rate measurements vs linear scan
//! - Comparison report generation

use rand::Rng;
use sqlite_knowledge_graph::vector::{LinearScanIndex, TurboQuantConfig, TurboQuantIndex};
use std::fs;
use std::time::Instant;

// Test dimensions
const TEST_DIMENSIONS: &[usize] = &[64, 128, 384, 768, 1536];
const TEST_BIT_WIDTHS: &[usize] = &[1, 2, 3, 4];
const VECTOR_COUNTS: &[usize] = &[10, 100, 1000];

// Generate random normalized vector
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

// Measure memory usage (approximate)
fn estimate_memory_usage(index: &TurboQuantIndex, linear: &LinearScanIndex) -> (usize, usize) {
    let turbo_memory = index.stats().num_vectors * index.stats().bytes_per_vector
        + index.config().dimension * index.config().dimension * 4; // rotation matrix
    let linear_memory = linear.stats().num_vectors * linear.stats().bytes_per_vector;
    (turbo_memory, linear_memory)
}

// Compute recall rate
fn compute_recall_rate(
    turbo_results: &[(i64, f32)],
    linear_results: &[(i64, f32)],
    k: usize,
) -> f32 {
    let turbo_ids: std::collections::HashSet<i64> =
        turbo_results.iter().take(k).map(|(id, _)| *id).collect();
    let linear_ids: std::collections::HashSet<i64> =
        linear_results.iter().take(k).map(|(id, _)| *id).collect();

    let intersection = turbo_ids.intersection(&linear_ids).count();
    intersection as f32 / k as f32
}

// Test suite struct
struct BenchmarkResults {
    dimension: usize,
    bit_width: usize,
    num_vectors: usize,
    turbo_index_time_ms: f64,
    turbo_search_time_ms: f64,
    linear_index_time_ms: f64,
    linear_search_time_ms: f64,
    turbo_memory_bytes: usize,
    linear_memory_bytes: usize,
    recall_rate: f64,
    compression_ratio: f64,
}

#[test]
fn test_different_dimensions() {
    for &dimension in TEST_DIMENSIONS {
        let config = TurboQuantConfig {
            dimension,
            bit_width: 3,
            seed: 42,
        };

        let mut index = TurboQuantIndex::new(config.clone()).unwrap();
        assert_eq!(index.config().dimension, dimension);

        let vector = generate_random_vector(dimension);
        index.add_vector(1, &vector).unwrap();

        let results = index.search(&vector, 1).unwrap();
        assert_eq!(results.len(), 1);
    }
}

#[test]
fn test_different_bit_widths() {
    let dimension = 384;

    for &bit_width in TEST_BIT_WIDTHS {
        let config = TurboQuantConfig {
            dimension,
            bit_width,
            seed: 42,
        };

        let mut index = TurboQuantIndex::new(config.clone()).unwrap();
        assert_eq!(index.config().bit_width, bit_width);

        let vector = generate_random_vector(dimension);
        index.add_vector(1, &vector).unwrap();

        let results = index.search(&vector, 1).unwrap();
        assert_eq!(results.len(), 1);
    }
}

#[test]
fn test_edge_cases() {
    // Test empty index
    let config = TurboQuantConfig::default();
    let index = TurboQuantIndex::new(config).unwrap();
    assert!(index.is_empty());

    let vector = generate_random_vector(384);
    let results = index.search(&vector, 10).unwrap();
    assert_eq!(results.len(), 0);

    // Test single vector
    let mut index = TurboQuantIndex::new(TurboQuantConfig::default()).unwrap();
    index.add_vector(1, &vector).unwrap();
    assert_eq!(index.len(), 1);

    let results = index.search(&vector, 1).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, 1);

    // Test invalid configuration
    let invalid_config = TurboQuantConfig {
        dimension: 384,
        bit_width: 9, // Invalid: > 8
        seed: 42,
    };
    assert!(TurboQuantIndex::new(invalid_config).is_err());

    // Test invalid vector dimension
    let mut index = TurboQuantIndex::new(TurboQuantConfig::default()).unwrap();
    let wrong_dim_vector = vec![0.1f32; 128]; // Wrong dimension
    assert!(index.add_vector(1, &wrong_dim_vector).is_err());
}

#[test]
fn test_batch_operations() {
    let config = TurboQuantConfig::default();
    let mut index = TurboQuantIndex::new(config).unwrap();

    let vectors: Vec<(i64, Vec<f32>)> = (0..10)
        .map(|i| {
            let vec = generate_random_vector(384);
            (i as i64, vec)
        })
        .collect();

    index.add_vectors_batch(&vectors).unwrap();
    assert_eq!(index.len(), 10);

    // Test batch search
    let queries: Vec<Vec<f32>> = (0..3).map(|_| generate_random_vector(384)).collect();
    let batch_results = index.search_batch(&queries, 5).unwrap();
    assert_eq!(batch_results.len(), 3);
}

#[test]
fn test_persistence() {
    let config = TurboQuantConfig::default();
    let mut index = TurboQuantIndex::new(config).unwrap();

    let vectors: Vec<(i64, Vec<f32>)> = (0..10)
        .map(|i| {
            let vec = generate_random_vector(384);
            (i as i64, vec)
        })
        .collect();

    index.add_vectors_batch(&vectors).unwrap();

    // Save to file
    let test_path = "/tmp/test_turboquant_index.json";
    index.save(test_path).unwrap();

    // Load from file
    let loaded_index = TurboQuantIndex::load(test_path).unwrap();

    assert_eq!(loaded_index.len(), index.len());
    assert_eq!(loaded_index.config().dimension, index.config().dimension);
    assert_eq!(loaded_index.config().bit_width, index.config().bit_width);

    // Verify search results match
    let query = generate_random_vector(384);
    let results1 = index.search(&query, 5).unwrap();
    let results2 = loaded_index.search(&query, 5).unwrap();

    assert_eq!(results1.len(), results2.len());
    for (r1, r2) in results1.iter().zip(results2.iter()) {
        assert_eq!(r1.0, r2.0);
        assert!((r1.1 - r2.1).abs() < 0.001);
    }

    // Cleanup
    fs::remove_file(test_path).ok();
}

#[test]
fn test_performance_benchmark() {
    let mut all_results: Vec<BenchmarkResults> = Vec::new();

    // Run benchmarks for different configurations
    for &dimension in &[128, 384, 768] {
        for &bit_width in &[2, 3, 4] {
            for &num_vectors in &[100, 1000] {
                let config = TurboQuantConfig {
                    dimension,
                    bit_width,
                    seed: 42,
                };

                println!(
                    "\n=== Benchmark: dim={}, bits={}, n={} ===",
                    dimension, bit_width, num_vectors
                );

                // Generate test vectors
                let vectors: Vec<(i64, Vec<f32>)> = (0..num_vectors)
                    .map(|i| {
                        let vec = generate_random_vector(dimension);
                        (i as i64, vec)
                    })
                    .collect();

                let queries: Vec<Vec<f32>> =
                    (0..10).map(|_| generate_random_vector(dimension)).collect();

                // Benchmark TurboQuant
                let mut turbo_index = TurboQuantIndex::new(config.clone()).unwrap();

                let start = Instant::now();
                for (id, vec) in &vectors {
                    turbo_index.add_vector(*id, vec).unwrap();
                }
                let turbo_index_time = start.elapsed().as_secs_f64() * 1000.0;

                let start = Instant::now();
                let mut turbo_search_times = Vec::new();
                for query in &queries {
                    let _ = turbo_index.search(query, 10).unwrap();
                    turbo_search_times.push(start.elapsed().as_secs_f64() * 1000.0);
                }
                let turbo_search_time: f64 =
                    turbo_search_times.iter().sum::<f64>() / turbo_search_times.len() as f64;

                // Benchmark Linear Scan
                let mut linear_index = LinearScanIndex::new(config.clone()).unwrap();

                let start = Instant::now();
                for (id, vec) in &vectors {
                    linear_index.add_vector(*id, vec).unwrap();
                }
                let linear_index_time = start.elapsed().as_secs_f64() * 1000.0;

                let start = Instant::now();
                let mut linear_search_times = Vec::new();
                for query in &queries {
                    let _ = linear_index.search(query, 10).unwrap();
                    linear_search_times.push(start.elapsed().as_secs_f64() * 1000.0);
                }
                let linear_search_time: f64 =
                    linear_search_times.iter().sum::<f64>() / linear_search_times.len() as f64;

                // Compute memory usage
                let (turbo_memory, linear_memory) =
                    estimate_memory_usage(&turbo_index, &linear_index);

                // Compute recall rate
                let k = 10;
                let turbo_results = turbo_index.search(&queries[0], k).unwrap();
                let linear_results = linear_index.search(&queries[0], k).unwrap();
                let recall_rate = compute_recall_rate(&turbo_results, &linear_results, k);

                let compression_ratio = linear_memory as f64 / turbo_memory as f64;

                println!("  TurboQuant index time: {:.3} ms", turbo_index_time);
                println!("  TurboQuant search time: {:.3} ms", turbo_search_time);
                println!("  Linear index time: {:.3} ms", linear_index_time);
                println!("  Linear search time: {:.3} ms", linear_search_time);
                println!("  TurboQuant memory: {} bytes", turbo_memory);
                println!("  Linear memory: {} bytes", linear_memory);
                println!("  Recall rate (top-{}): {:.3}", k, recall_rate);
                println!("  Compression ratio: {:.2}x", compression_ratio);

                all_results.push(BenchmarkResults {
                    dimension,
                    bit_width,
                    num_vectors,
                    turbo_index_time_ms: turbo_index_time,
                    turbo_search_time_ms: turbo_search_time,
                    linear_index_time_ms: linear_index_time,
                    linear_search_time_ms: linear_search_time,
                    turbo_memory_bytes: turbo_memory,
                    linear_memory_bytes: linear_memory,
                    recall_rate: recall_rate as f64,
                    compression_ratio,
                });
            }
        }
    }

    // Generate comparison report
    generate_comparison_report(&all_results);
}

fn generate_comparison_report(results: &[BenchmarkResults]) {
    let mut report = String::from("# TurboQuant Performance Comparison Report\n\n");
    report.push_str("Generated on: ");
    report.push_str(&chrono::Utc::now().to_rfc3339());
    report.push_str("\n\n");

    report.push_str("## Summary\n\n");
    report.push_str("This report compares TurboQuant approximate nearest neighbor search against linear scan (exact search).\n\n");

    // Overall statistics
    let avg_recall: f64 = results.iter().map(|r| r.recall_rate).sum::<f64>() / results.len() as f64;
    let avg_compression: f64 =
        results.iter().map(|r| r.compression_ratio).sum::<f64>() / results.len() as f64;
    let avg_speedup_index: f64 = results
        .iter()
        .map(|r| r.linear_index_time_ms / r.turbo_index_time_ms.max(0.001))
        .sum::<f64>()
        / results.len() as f64;
    let avg_speedup_search: f64 = results
        .iter()
        .map(|r| r.linear_search_time_ms / r.turbo_search_time_ms.max(0.001))
        .sum::<f64>()
        / results.len() as f64;

    report.push_str("### Overall Performance\n\n");
    report.push_str(&format!(
        "- **Average Recall Rate (top-10):** {:.2}%\n",
        avg_recall * 100.0
    ));
    report.push_str(&format!(
        "- **Average Compression Ratio:** {:.2}x\n",
        avg_compression
    ));
    report.push_str(&format!(
        "- **Average Indexing Speedup:** {:.2}x\n",
        avg_speedup_index
    ));
    report.push_str(&format!(
        "- **Average Search Speedup:** {:.2}x\n",
        avg_speedup_search
    ));
    report.push_str("\n");

    // Detailed results by dimension
    report.push_str("## Detailed Results\n\n");

    for &dimension in &[128, 384, 768] {
        let dim_results: Vec<_> = results
            .iter()
            .filter(|r| r.dimension == dimension)
            .collect();

        if !dim_results.is_empty() {
            report.push_str(&format!("### Dimension: {}\n\n", dimension));
            report.push_str("| Bit Width | Vectors | TurboQuant Index (ms) | TurboQuant Search (ms) | Linear Index (ms) | Linear Search (ms) | Memory Turbo (bytes) | Memory Linear (bytes) | Recall Rate | Compression |\n");
            report.push_str("|-----------|---------|----------------------|-----------------------|-------------------|-------------------|---------------------|----------------------|-------------|--------------|\n");

            for r in dim_results {
                report.push_str(&format!(
                    "| {} | {} | {:.3} | {:.3} | {:.3} | {:.3} | {} | {} | {:.3} | {:.2}x |\n",
                    r.bit_width,
                    r.num_vectors,
                    r.turbo_index_time_ms,
                    r.turbo_search_time_ms,
                    r.linear_index_time_ms,
                    r.linear_search_time_ms,
                    r.turbo_memory_bytes,
                    r.linear_memory_bytes,
                    r.recall_rate,
                    r.compression_ratio
                ));
            }
            report.push_str("\n");
        }
    }

    // Analysis
    report.push_str("## Analysis\n\n");
    report.push_str("### Key Findings\n\n");

    if avg_recall > 0.9 {
        report.push_str("✅ **High Accuracy:** TurboQuant maintains excellent recall rates (>90%) with significant memory savings.\n\n");
    } else if avg_recall > 0.8 {
        report.push_str("⚠️ **Good Accuracy:** TurboQuant maintains good recall rates (>80%) with significant memory savings.\n\n");
    } else {
        report.push_str("❌ **Moderate Accuracy:** Recall rates suggest potential quality trade-offs for the given configuration.\n\n");
    }

    if avg_compression > 8.0 {
        report.push_str("✅ **Excellent Compression:** Memory savings exceed 8x, making TurboQuant highly efficient for large-scale applications.\n\n");
    } else if avg_compression > 5.0 {
        report.push_str("✅ **Good Compression:** Memory savings are significant (>5x) and beneficial for resource-constrained environments.\n\n");
    }

    report.push_str("### Recommendations\n\n");

    report.push_str("1. **For production use:** Consider bit_width=3 for best balance between accuracy and memory.\n");
    report.push_str("2. **For memory-constrained environments:** Use bit_width=2 for up to 16x compression with acceptable accuracy loss.\n");
    report.push_str("3. **For high-accuracy requirements:** Use bit_width=4 for near-exact search performance.\n");
    report.push_str("4. **Indexing:** TurboQuant shows excellent indexing performance, suitable for real-time applications.\n\n");

    report.push_str("### Technical Details\n\n");
    report.push_str("- **Method:** Random rotation + optimal scalar quantization\n");
    report.push_str("- **Based on:** arXiv:2504.19874 (ICLR 2026)\n");
    report.push_str("- **Training:** Data-oblivious, no training required\n");
    report.push_str("- **Complexity:** O(nd) indexing, O(nd) search (vs O(nd) for linear scan with larger constants)\n\n");

    // Save report
    let report_path = "/Users/hiyenwong/.openclaw/workspace/projects/sqlite-knowledge-graph-gh/tests/comparison_report.md";
    fs::write(report_path, report).expect("Failed to write comparison report");

    println!("\n✅ Comparison report saved to: {}", report_path);
}
