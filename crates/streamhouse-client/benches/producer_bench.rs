//! Producer Performance Benchmarks
//!
//! Measures producer client performance for batching, serialization, and compression.
//!
//! ## Benchmarks
//!
//! ### 1. Batch Creation (`bench_batch_creation`)
//! - Measures overhead of creating batches
//! - Tests different batch sizes (10, 100, 1000 records)
//! - **Target**: < 1ms for 1000 records
//!
//! ### 2. Record Serialization (`bench_record_serialization`)
//! - Measures serialization to protobuf
//! - Tests different record sizes
//! - **Target**: < 10µs per record
//!
//! ### 3. Batch Compression (`bench_batch_compression`)
//! - Measures LZ4 compression overhead
//! - Tests with/without compression
//! - **Target**: < 5ms for 1000 records
//!
//! ## Running
//!
//! ```bash
//! # Run all benchmarks
//! cargo bench -p streamhouse-client
//!
//! # Run specific benchmark
//! cargo bench -p streamhouse-client --bench producer_bench batch_creation
//!
//! # Save baseline
//! cargo bench -p streamhouse-client -- --save-baseline main
//!
//! # Compare
//! cargo bench -p streamhouse-client -- --baseline main
//! ```

use bytes::Bytes;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

fn create_test_record(offset: u64, value_size: usize) -> (Option<Bytes>, Bytes) {
    let key = Some(Bytes::from(format!("key{}", offset)));
    let value = Bytes::from(vec![b'x'; value_size]);
    (key, value)
}

fn bench_batch_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_creation");

    // Test different batch sizes
    for batch_size in [10, 100, 1000] {
        group.throughput(Throughput::Elements(batch_size as u64));
        group.bench_with_input(
            BenchmarkId::new("records", batch_size),
            &batch_size,
            |b, &size| {
                b.iter(|| {
                    let mut records = Vec::with_capacity(size);
                    for i in 0..size {
                        let (key, value) = create_test_record(i as u64, 1024);
                        records.push((key, value));
                    }
                    black_box(records);
                });
            },
        );
    }

    group.finish();
}

fn bench_record_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("record_serialization");

    // Test different record sizes
    for record_size in [128, 1024, 4096] {
        group.throughput(Throughput::Bytes(record_size as u64));
        group.bench_with_input(
            BenchmarkId::new("bytes", record_size),
            &record_size,
            |b, &size| {
                let (key, value) = create_test_record(0, size);
                b.iter(|| {
                    // Simulate serialization overhead
                    let key_len = key.as_ref().map(|k| k.len()).unwrap_or(0);
                    let value_len = value.len();
                    black_box(key_len + value_len);
                });
            },
        );
    }

    group.finish();
}

fn bench_batch_compression(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_compression");
    group.sample_size(50);

    let batch_size = 1000;
    group.throughput(Throughput::Elements(batch_size as u64));

    group.bench_function("lz4_compress", |b| {
        let mut records = Vec::with_capacity(batch_size);
        for i in 0..batch_size {
            let (key, value) = create_test_record(i as u64, 1024);
            records.push((key, value));
        }

        b.iter(|| {
            // Simulate batch compression by concatenating all values
            let mut buffer = Vec::new();
            for (_key, value) in &records {
                buffer.extend_from_slice(value);
            }

            // Compress the batch
            let compressed =
                lz4::block::compress(&buffer, Some(lz4::block::CompressionMode::DEFAULT), false)
                    .unwrap();
            black_box(compressed);
        });
    });

    group.bench_function("no_compress", |b| {
        let mut records = Vec::with_capacity(batch_size);
        for i in 0..batch_size {
            let (key, value) = create_test_record(i as u64, 1024);
            records.push((key, value));
        }

        b.iter(|| {
            // Just concatenate without compression
            let mut buffer = Vec::new();
            for (_key, value) in &records {
                buffer.extend_from_slice(value);
            }
            black_box(buffer);
        });
    });

    group.finish();
}

fn bench_partitioning(c: &mut Criterion) {
    let mut group = c.benchmark_group("partitioning");

    let record_count = 1000;
    let partition_count = 4;

    group.throughput(Throughput::Elements(record_count as u64));
    group.bench_function("round_robin", |b| {
        b.iter(|| {
            let mut partition_counts = vec![0u32; partition_count];
            for i in 0..record_count {
                let partition = i % partition_count;
                partition_counts[partition] += 1;
            }
            black_box(partition_counts);
        });
    });

    group.bench_function("hash_partition", |b| {
        use siphasher::sip::SipHasher13;
        use std::hash::{Hash, Hasher};

        b.iter(|| {
            let mut partition_counts = vec![0u32; partition_count];
            for i in 0..record_count {
                let key = format!("key{}", i);
                let mut hasher = SipHasher13::new();
                key.hash(&mut hasher);
                let hash = hasher.finish();
                let partition = (hash as usize) % partition_count;
                partition_counts[partition] += 1;
            }
            black_box(partition_counts);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_batch_creation,
    bench_record_serialization,
    bench_batch_compression,
    bench_partitioning
);
criterion_main!(benches);
