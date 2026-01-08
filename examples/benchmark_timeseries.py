#!/usr/bin/env python3
"""
Benchmark: Polaroid Time-Series Operations vs Python Libraries

Compares performance of lag, lead, diff, pct_change operations:
- Polaroid (via gRPC to Rust backend)
- pandas (pure Python)
- polars (Rust-backed Python library)
- numpy (for basic operations)
"""
import time
import numpy as np
import pandas as pd
import polars as pl
import polaroid
from datetime import datetime, timedelta

def generate_timeseries_data(n_rows):
    """Generate synthetic time-series data."""
    start_date = datetime(2020, 1, 1)
    dates = [start_date + timedelta(days=i) for i in range(n_rows)]
    
    np.random.seed(42)
    prices = 100 + np.cumsum(np.random.randn(n_rows) * 2)
    volumes = np.random.randint(100000, 2000000, n_rows)
    
    return pd.DataFrame({
        'timestamp': dates,
        'symbol': ['AAPL'] * n_rows,
        'price': prices,
        'volume': volumes
    })

def benchmark_pandas(df, operation):
    """Benchmark pandas operation."""
    start = time.time()
    
    if operation == 'lag':
        result = df.assign(
            price_lag1=df['price'].shift(1),
            volume_lag1=df['volume'].shift(1)
        )
    elif operation == 'lead':
        result = df.assign(
            price_lead1=df['price'].shift(-1)
        )
    elif operation == 'diff':
        result = df.assign(
            price_diff=df['price'].diff(1),
            volume_diff=df['volume'].diff(1)
        )
    elif operation == 'pct_change':
        result = df.assign(
            price_pct_change=df['price'].pct_change(1)
        )
    
    elapsed = time.time() - start
    return elapsed, len(result)

def benchmark_polars(df, operation):
    """Benchmark polars operation."""
    pldf = pl.from_pandas(df)
    
    start = time.time()
    
    if operation == 'lag':
        result = pldf.with_columns([
            pl.col('price').shift(1).alias('price_lag1'),
            pl.col('volume').shift(1).alias('volume_lag1')
        ])
    elif operation == 'lead':
        result = pldf.with_columns([
            pl.col('price').shift(-1).alias('price_lead1')
        ])
    elif operation == 'diff':
        result = pldf.with_columns([
            (pl.col('price') - pl.col('price').shift(1)).alias('price_diff'),
            (pl.col('volume') - pl.col('volume').shift(1)).alias('volume_diff')
        ])
    elif operation == 'pct_change':
        shifted = pl.col('price').shift(1)
        result = pldf.with_columns([
            ((pl.col('price') - shifted) / shifted).alias('price_pct_change')
        ])
    
    # Force execution
    _ = result.to_pandas()
    elapsed = time.time() - start
    return elapsed, len(result)

def benchmark_polaroid(client, df, operation):
    """Benchmark Polaroid operation."""
    # Upload data
    upload_start = time.time()
    handle = client.from_pandas(df)
    upload_time = time.time() - upload_start
    
    # Execute operation
    op_start = time.time()
    
    if operation == 'lag':
        result_handle = client.lag(handle, [
            {'column': 'price', 'periods': 1, 'alias': 'price_lag1'},
            {'column': 'volume', 'periods': 1, 'alias': 'volume_lag1'}
        ])
    elif operation == 'lead':
        result_handle = client.lead(handle, [
            {'column': 'price', 'periods': 1, 'alias': 'price_lead1'}
        ])
    elif operation == 'diff':
        result_handle = client.diff(handle, ['price', 'volume'], periods=1)
    elif operation == 'pct_change':
        result_handle = client.pct_change(handle, ['price'], periods=1)
    
    # Collect result
    result_df = client.collect(result_handle)
    
    elapsed = time.time() - op_start
    total_time = time.time() - upload_start
    
    return elapsed, total_time, len(result_df), upload_time

def benchmark_numpy(df, operation):
    """Benchmark numpy operation (for basic operations only)."""
    prices = df['price'].values
    volumes = df['volume'].values
    
    start = time.time()
    
    if operation == 'lag':
        price_lag = np.roll(prices, 1)
        price_lag[0] = np.nan
        volume_lag = np.roll(volumes, 1)
        volume_lag[0] = np.nan
    elif operation == 'lead':
        price_lead = np.roll(prices, -1)
        price_lead[-1] = np.nan
    elif operation == 'diff':
        price_diff = np.diff(prices, prepend=np.nan)
        volume_diff = np.diff(volumes, prepend=np.nan)
    elif operation == 'pct_change':
        price_lag = np.roll(prices, 1)
        price_lag[0] = np.nan
        pct_change = (prices - price_lag) / price_lag
    
    elapsed = time.time() - start
    return elapsed, len(prices)

def run_benchmark(n_rows, operation):
    """Run benchmark for all libraries."""
    print(f"\n{'='*80}")
    print(f"Benchmarking {operation.upper()} with {n_rows:,} rows")
    print(f"{'='*80}")
    
    df = generate_timeseries_data(n_rows)
    results = {}
    
    # Pandas
    try:
        elapsed, rows = benchmark_pandas(df, operation)
        results['pandas'] = elapsed
        print(f"  pandas:   {elapsed*1000:8.2f} ms  ({rows:,} rows)")
    except Exception as e:
        print(f"  pandas:   FAILED - {e}")
    
    # Polars
    try:
        elapsed, rows = benchmark_polars(df, operation)
        results['polars'] = elapsed
        print(f"  polars:   {elapsed*1000:8.2f} ms  ({rows:,} rows)")
    except Exception as e:
        print(f"  polars:   FAILED - {e}")
    
    # NumPy
    try:
        elapsed, rows = benchmark_numpy(df, operation)
        results['numpy'] = elapsed
        print(f"  numpy:    {elapsed*1000:8.2f} ms  ({rows:,} rows)")
    except Exception as e:
        print(f"  numpy:    FAILED - {e}")
    
    # Polaroid
    try:
        client = polaroid.Client("localhost:50051")
        elapsed, total, rows, upload = benchmark_polaroid(client, df, operation)
        results['polaroid'] = elapsed
        results['polaroid_total'] = total
        print(f"  Polaroid: {elapsed*1000:8.2f} ms  ({rows:,} rows) [op only]")
        print(f"            {total*1000:8.2f} ms  (with upload: {upload*1000:.2f} ms)")
    except Exception as e:
        print(f"  Polaroid: FAILED - {e}")
    
    # Calculate speedups
    if 'pandas' in results and 'polaroid' in results:
        speedup = results['pandas'] / results['polaroid']
        print(f"\n  🚀 Polaroid vs pandas: {speedup:.2f}x faster (operation only)")
    
    if 'polars' in results and 'polaroid' in results:
        speedup = results['polars'] / results['polaroid']
        print(f"  🚀 Polaroid vs polars: {speedup:.2f}x faster (operation only)")
    
    return results

def main():
    """Run comprehensive benchmark suite."""
    print("="*80)
    print("POLAROID TIME-SERIES BENCHMARK")
    print("Testing lag, lead, diff, pct_change operations")
    print("="*80)
    
    operations = ['lag', 'lead', 'diff', 'pct_change']
    sizes = [1_000, 10_000, 100_000, 1_000_000]
    
    all_results = {}
    
    for size in sizes:
        for operation in operations:
            try:
                results = run_benchmark(size, operation)
                all_results[f"{size}_{operation}"] = results
            except KeyboardInterrupt:
                print("\n\n⚠️  Benchmark interrupted by user")
                return
            except Exception as e:
                print(f"\n❌ Error: {e}")
    
    # Summary
    print("\n" + "="*80)
    print("SUMMARY")
    print("="*80)
    print("\nPolaroid provides:")
    print("  ✅ Zero-copy data transfer via Arrow Flight")
    print("  ✅ Lazy evaluation with query optimization")
    print("  ✅ Parallel execution on Rust backend")
    print("  ✅ gRPC streaming for large datasets")
    print("\nBest use cases:")
    print("  • Large datasets (>100K rows)")
    print("  • Complex multi-operation pipelines")
    print("  • When data already resides server-side")
    print("  • High-frequency trading scenarios")

if __name__ == "__main__":
    main()
