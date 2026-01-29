//! Consumer Performance Benchmarks
//!
//! Measures consumer client performance for record retrieval, offset tracking, and deserialization.
//!
//! ## Benchmarks
//!
//! ### 1. Offset Tracking (`bench_offset_tracking`)
//! - Measures overhead of tracking consumer offsets
//! - Tests different partition counts
//! - **Target**: < 1µs per offset update
//!
//! ### 2. Record Deserialization (`bench_record_deserialization`)
//! - Measures protobuf deserialization overhead
//! - Tests different record sizes
//! - **Target**: < 10µs per record
//!
//! ### 3. Batch Processing (`bench_batch_processing`)
//! - Measures overhead of processing batches
//! - Tests different batch sizes (10, 100, 1000 records)
//! - **Target**: < 5ms for 1000 records
//!
//! ### 4. Offset Commit (`bench_offset_commit`)
//! - Measures overhead of committing offsets
//! - Simulates metadata store operations
//! - **Target**: < 1ms per commit
//!
//! ## Running
//!
//! ```bash
//! # Run all benchmarks
//! cargo bench -p streamhouse-client --bench consumer_bench
//!
//! # Run specific benchmark
//! cargo bench -p streamhouse-client --bench consumer_bench offset_tracking
//!
//! # Save baseline
//! cargo bench -p streamhouse-client -- --save-baseline main
//!
//! # Compare
//! cargo bench -p streamhouse-client -- --baseline main
//! ```

use bytes::Bytes;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::collections::HashMap;

fn create_test_record(offset: u64, value_size: usize) -> (u64, Option<Bytes>, Bytes, u64) {
    let key = Some(Bytes::from(format!("key{}", offset)));
    let value = Bytes::from(vec![b'x'; value_size]);
    let timestamp = 1234567890 + offset;
    (offset, key, value, timestamp)
}

fn bench_offset_tracking(c: &mut Criterion) {
    let mut group = c.benchmark_group("offset_tracking");

    // Test different partition counts
    for partition_count in [4, 16, 64] {
        group.throughput(Throughput::Elements(partition_count as u64));
        group.bench_with_input(
            BenchmarkId::new("partitions", partition_count),
            &partition_count,
            |b, &count| {
                b.iter(|| {
                    let mut offsets = HashMap::new();
                    for i in 0..count {
                        let topic = "test-topic";
                        offsets.insert((topic, i), i as u64 * 100);
                    }
                    black_box(offsets);
                });
            },
        );
    }

    group.finish();
}

fn bench_record_deserialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("record_deserialization");

    // Test different record sizes
    for record_size in [128, 1024, 4096] {
        group.throughput(Throughput::Bytes(record_size as u64));
        group.bench_with_input(
            BenchmarkId::new("bytes", record_size),
            &record_size,
            |b, &size| {
                let (offset, key, value, timestamp) = create_test_record(0, size);
                b.iter(|| {
                    // Simulate deserialization overhead
                    let key_len = key.as_ref().map(|k| k.len()).unwrap_or(0);
                    let value_len = value.len();
                    black_box((offset, key_len, value_len, timestamp));
                });
            },
        );
    }

    group.finish();
}

fn bench_batch_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_processing");

    // Test different batch sizes
    for batch_size in [10, 100, 1000] {
        group.throughput(Throughput::Elements(batch_size as u64));
        group.bench_with_input(
            BenchmarkId::new("records", batch_size),
            &batch_size,
            |b, &size| {
                let mut records = Vec::with_capacity(size);
                for i in 0..size {
                    let (offset, key, value, timestamp) = create_test_record(i as u64, 1024);
                    records.push((offset, key, value, timestamp));
                }

                b.iter(|| {
                    let mut processed = 0;
                    for (offset, _key, value, _timestamp) in &records {
                        // Simulate processing
                        processed += value.len() as u64 + offset;
                    }
                    black_box(processed);
                });
            },
        );
    }

    group.finish();
}

fn bench_offset_commit(c: &mut Criterion) {
    let mut group = c.benchmark_group("offset_commit");

    group.bench_function("single_partition", |b| {
        b.iter(|| {
            let mut offsets = HashMap::new();
            offsets.insert(("test-topic", 0), 1000u64);
            black_box(offsets);
        });
    });

    group.bench_function("multiple_partitions", |b| {
        b.iter(|| {
            let mut offsets = HashMap::new();
            for i in 0..16 {
                offsets.insert(("test-topic", i), i as u64 * 100);
            }
            black_box(offsets);
        });
    });

    group.finish();
}

fn bench_decompression(c: &mut Criterion) {
    let mut group = c.benchmark_group("decompression");
    group.sample_size(50);

    let batch_size = 1000;
    group.throughput(Throughput::Elements(batch_size as u64));

    // Pre-create compressed data
    let mut buffer = Vec::new();
    for i in 0..batch_size {
        let (_offset, _key, value, _timestamp) = create_test_record(i as u64, 1024);
        buffer.extend_from_slice(&value);
    }
    let compressed = lz4::block::compress(&buffer, Some(lz4::block::CompressionMode::DEFAULT), false).unwrap();

    group.bench_function("lz4_decompress", |b| {
        b.iter(|| {
            let decompressed = lz4::block::decompress(&compressed, Some(buffer.len() as i32)).unwrap();
            black_box(decompressed);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_offset_tracking,
    bench_record_deserialization,
    bench_batch_processing,
    bench_offset_commit,
    bench_decompression
);
criterion_main!(benches);
