use criterion::{black_box, criterion_group, criterion_main, Criterion};
use polars::prelude::*;
use polars_streaming_adaptive::{AdaptiveStreamingReader, ParallelStreamReader};
use std::path::PathBuf;
use tempfile::TempDir;

fn create_test_parquet(path: &PathBuf, rows: usize) {
    let timestamp = Column::new("timestamp".into(), (0..rows as i64).collect::<Vec<_>>());
    let symbol = Column::new("symbol".into(), vec!["AAPL"; rows]);
    let open = Column::new("open".into(), (0..rows).map(|i| 100.0 + (i as f64)).collect::<Vec<_>>());
    let high = Column::new("high".into(), (0..rows).map(|i| 105.0 + (i as f64)).collect::<Vec<_>>());
    let low = Column::new("low".into(), (0..rows).map(|i| 95.0 + (i as f64)).collect::<Vec<_>>());
    let close = Column::new("close".into(), (0..rows).map(|i| 100.0 + (i as f64)).collect::<Vec<_>>());
    let volume = Column::new("volume".into(), (0..rows).map(|i| 1_000_000 + i as i64).collect::<Vec<_>>());
    
    let df = DataFrame::new(vec![timestamp, symbol, open, high, low, close, volume])
        .unwrap();

    ParquetWriter::new(std::fs::File::create(path).unwrap())
        .finish(&mut df.clone())
        .unwrap();
}

fn bench_adaptive_reader(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.parquet");
    create_test_parquet(&file_path, 100_000);

    c.bench_function("adaptive_reader_100k", |b| {
        b.iter(|| {
            let reader = AdaptiveStreamingReader::new(black_box(&file_path)).unwrap();
            let df = reader.collect().unwrap();
            black_box(df);
        });
    });
}

fn bench_parallel_reader(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    
    // Create 5 files
    let mut paths = Vec::new();
    for i in 0..5 {
        let path = temp_dir.path().join(format!("file_{}.parquet", i));
        create_test_parquet(&path, 50_000);
        paths.push(path);
    }

    c.bench_function("parallel_reader_5_files", |b| {
        b.iter(|| {
            let reader = ParallelStreamReader::new(black_box(paths.clone()));
            let df = reader.collect_concatenated().unwrap();
            black_box(df);
        });
    });
}

fn bench_comparison(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.parquet");
    create_test_parquet(&file_path, 100_000);

    let mut group = c.benchmark_group("comparison");

    // Standard Polars
    group.bench_function("standard_polars", |b| {
        b.iter(|| {
            let df = ParquetReader::new(std::fs::File::open(black_box(&file_path)).unwrap())
                .finish()
                .unwrap();
            black_box(df);
        });
    });

    // Adaptive streaming
    group.bench_function("adaptive_streaming", |b| {
        b.iter(|| {
            let reader = AdaptiveStreamingReader::new(black_box(&file_path)).unwrap();
            let df = reader.collect().unwrap();
            black_box(df);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_adaptive_reader, bench_parallel_reader, bench_comparison);
criterion_main!(benches);
