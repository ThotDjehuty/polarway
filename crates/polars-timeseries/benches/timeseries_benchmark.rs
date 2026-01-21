//! Benchmarks for polars-timeseries

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use polars::prelude::*;
use polars_timeseries::{vwap, twap};

fn create_test_data(rows: usize) -> DataFrame {
    DataFrame::new(vec![
        Series::new("timestamp".into(), (0..rows as i64).collect::<Vec<_>>()).into(),
        Series::new(
            "close".into(),
            (0..rows).map(|i| 100.0 + (i as f64 % 10.0)).collect::<Vec<_>>(),
        )
        .into(),
        Series::new(
            "volume".into(),
            (0..rows).map(|i| 1000i64 + (i as i64 % 500)).collect::<Vec<_>>(),
        )
        .into(),
    ])
    .unwrap()
}

fn bench_vwap(c: &mut Criterion) {
    let df_small = create_test_data(1000);
    let df_large = create_test_data(100_000);

    c.bench_function("vwap_1k_rows", |b| {
        b.iter(|| {
            vwap(
                black_box(&df_small),
                black_box("timestamp"),
                black_box("close"),
                black_box("volume"),
            )
        })
    });

    c.bench_function("vwap_100k_rows", |b| {
        b.iter(|| {
            vwap(
                black_box(&df_large),
                black_box("timestamp"),
                black_box("close"),
                black_box("volume"),
            )
        })
    });
}

fn bench_twap(c: &mut Criterion) {
    let df_small = create_test_data(1000);
    let df_large = create_test_data(100_000);

    c.bench_function("twap_1k_rows", |b| {
        b.iter(|| {
            twap(
                black_box(&df_small),
                black_box("close"),
                black_box(10),
            )
        })
    });

    c.bench_function("twap_100k_rows", |b| {
        b.iter(|| {
            twap(
                black_box(&df_large),
                black_box("close"),
                black_box(10),
            )
        })
    });
}

criterion_group!(benches, bench_vwap, bench_twap);
criterion_main!(benches);
