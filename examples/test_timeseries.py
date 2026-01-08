#!/usr/bin/env python3
"""
Test time-series operations for Polaroid Phase 4

Tests lag, lead, diff, and pct_change operations with financial data.
"""
import pandas as pd
import polaroid

def test_time_series_operations():
    """Test lag, lead, diff, and pct_change on stock price data."""
    
    # Connect to Polaroid server
    client = polaroid.Client("localhost:50051")
    print("✅ Connected to Polaroid server")
    
    # Create time-series data (stock prices)
    df = pd.DataFrame({
        'timestamp': pd.date_range('2024-01-01', periods=10, freq='D'),
        'symbol': ['AAPL'] * 10,
        'price': [150.0, 152.5, 151.0, 155.0, 153.5, 157.0, 159.5, 158.0, 162.0, 160.5],
        'volume': [1000000, 1100000, 950000, 1200000, 1050000, 1300000, 1250000, 1150000, 1400000, 1300000]
    })
    print(f"\n📊 Created DataFrame: {df.shape[0]} rows × {df.shape[1]} columns")
    print(df)
    
    # Upload to Polaroid
    handle = client.from_pandas(df)
    print(f"\n⬆️  Uploaded DataFrame with handle: {handle}")
    
    # Test 1: Lag (shift forward 1 period)
    print("\n" + "="*60)
    print("TEST 1: LAG (1 period)")
    print("="*60)
    try:
        lag_handle = client.lag(handle, [
            {'column': 'price', 'periods': 1, 'alias': 'price_lag1'},
            {'column': 'volume', 'periods': 1, 'alias': 'volume_lag1'}
        ])
        lag_df = client.collect(lag_handle)
        print("✅ LAG operation successful")
        print(lag_df[['timestamp', 'price', 'price_lag1', 'volume', 'volume_lag1']])
    except Exception as e:
        print(f"❌ LAG failed: {e}")
    
    # Test 2: Lead (shift backward 1 period)
    print("\n" + "="*60)
    print("TEST 2: LEAD (1 period)")
    print("="*60)
    try:
        lead_handle = client.lead(handle, [
            {'column': 'price', 'periods': 1, 'alias': 'price_lead1'}
        ])
        lead_df = client.collect(lead_handle)
        print("✅ LEAD operation successful")
        print(lead_df[['timestamp', 'price', 'price_lead1']])
    except Exception as e:
        print(f"❌ LEAD failed: {e}")
    
    # Test 3: Diff (price changes)
    print("\n" + "="*60)
    print("TEST 3: DIFF (price changes)")
    print("="*60)
    try:
        diff_handle = client.diff(handle, ['price', 'volume'], periods=1)
        diff_df = client.collect(diff_handle)
        print("✅ DIFF operation successful")
        print(diff_df[['timestamp', 'price', 'price_diff', 'volume', 'volume_diff']])
    except Exception as e:
        print(f"❌ DIFF failed: {e}")
    
    # Test 4: Pct Change (percentage returns)
    print("\n" + "="*60)
    print("TEST 4: PCT_CHANGE (percentage returns)")
    print("="*60)
    try:
        pct_handle = client.pct_change(handle, ['price'], periods=1)
        pct_df = client.collect(pct_handle)
        print("✅ PCT_CHANGE operation successful")
        print(pct_df[['timestamp', 'price', 'price_pct_change']])
        
        # Calculate returns manually to verify
        manual_returns = df['price'].pct_change()
        print("\n📈 Manual calculation (pandas):")
        print(manual_returns)
    except Exception as e:
        print(f"❌ PCT_CHANGE failed: {e}")
    
    print("\n" + "="*60)
    print("✅ All time-series tests completed!")
    print("="*60)

if __name__ == "__main__":
    test_time_series_operations()
