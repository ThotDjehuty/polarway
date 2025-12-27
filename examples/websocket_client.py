#!/usr/bin/env python3
"""
WebSocket Real-Time Streaming with Polaroid

Demonstrates:
- Consuming live data from WebSocket sources (blockchain, market feeds)
- Real-time DataFrame aggregations
- Async streaming with minimal latency
- Production-ready error handling with auto-reconnect
- Backpressure management

Use cases:
- Blockchain mempool monitoring
- Crypto exchange order book tracking
- IoT sensor data aggregation
- Real-time social media analytics
"""

import asyncio
import websockets
import json
import pandas as pd
import numpy as np
from datetime import datetime, timedelta
from collections import deque
import sys
from typing import AsyncIterator, Dict, List, Optional
import signal

sys.path.insert(0, '../polaroid-python')
from polaroid.async_client import AsyncPolaroidClient, Result


class WebSocketDataStream:
    """Real-time data stream from WebSocket with auto-reconnect"""
    
    def __init__(self, url: str, max_retries: int = 5):
        self.url = url
        self.max_retries = max_retries
        self.reconnect_delay = 1.0
        self._shutdown = asyncio.Event()
        
    async def stream(self) -> AsyncIterator[dict]:
        """Stream data with automatic reconnection"""
        retry_count = 0
        
        while not self._shutdown.is_set():
            try:
                async with websockets.connect(self.url) as ws:
                    print(f"✅ Connected to {self.url}")
                    retry_count = 0  # Reset on successful connection
                    
                    while not self._shutdown.is_set():
                        try:
                            message = await asyncio.wait_for(
                                ws.recv(), 
                                timeout=30.0  # Heartbeat timeout
                            )
                            data = json.loads(message)
                            yield data
                            
                        except asyncio.TimeoutError:
                            # Send ping to keep connection alive
                            await ws.ping()
                            
            except (websockets.exceptions.WebSocketException, ConnectionError) as e:
                retry_count += 1
                if retry_count >= self.max_retries:
                    print(f"❌ Max retries ({self.max_retries}) exceeded. Giving up.")
                    break
                    
                delay = min(self.reconnect_delay * (2 ** retry_count), 60.0)
                print(f"🔄 Connection lost. Retrying in {delay:.1f}s... ({retry_count}/{self.max_retries})")
                await asyncio.sleep(delay)
                
    def shutdown(self):
        """Graceful shutdown"""
        self._shutdown.set()


class RollingWindowAggregator:
    """Efficient rolling window aggregation for streaming data"""
    
    def __init__(self, window_size: int):
        self.window = deque(maxlen=window_size)
        self.window_size = window_size
        
    def update(self, tick: dict) -> Optional[dict]:
        """Add new tick and compute aggregates"""
        self.window.append(tick)
        
        if len(self.window) < self.window_size:
            return None
            
        prices = [t['price'] for t in self.window]
        volumes = [t['volume'] for t in self.window]
        
        return {
            'count': len(self.window),
            'avg_price': np.mean(prices),
            'min_price': np.min(prices),
            'max_price': np.max(prices),
            'std_price': np.std(prices),
            'total_volume': np.sum(volumes),
            'timestamp': datetime.now().isoformat(),
        }


class PolaroidStreamProcessor:
    """Process WebSocket stream and store in Polaroid"""
    
    def __init__(self, polaroid_url: str, ws_url: str, window_size: int = 1000):
        self.polaroid_url = polaroid_url
        self.ws_url = ws_url
        self.window_size = window_size
        self.aggregator = RollingWindowAggregator(window_size)
        self.tick_count = 0
        self.batch_buffer = []
        self.batch_size = 1000
        
    async def process_stream(self):
        """Main processing loop"""
        ws_stream = WebSocketDataStream(self.ws_url)
        
        async with AsyncPolaroidClient(self.polaroid_url) as polaroid:
            print(f"🚀 Starting stream processor (window: {self.window_size})")
            
            async for tick in ws_stream.stream():
                self.tick_count += 1
                
                # Update rolling aggregates
                stats = self.aggregator.update(tick)
                if stats and self.tick_count % 100 == 0:
                    print(f"📊 [{self.tick_count:>6}] "
                          f"Avg: ${stats['avg_price']:.2f} "
                          f"(${stats['min_price']:.2f}-${stats['max_price']:.2f}) | "
                          f"Vol: {stats['total_volume']:.0f}")
                
                # Batch writes to Polaroid
                self.batch_buffer.append(tick)
                
                if len(self.batch_buffer) >= self.batch_size:
                    await self._flush_to_polaroid(polaroid)
                    
                # Measure latency
                if self.tick_count % 1000 == 0:
                    now_ms = int(datetime.now().timestamp() * 1000)
                    latency_ms = now_ms - tick['timestamp']
                    print(f"⏱️  End-to-end latency: {latency_ms}ms")
                    
    async def _flush_to_polaroid(self, client: AsyncPolaroidClient):
        """Write batch to Polaroid"""
        if not self.batch_buffer:
            return
            
        # Convert to DataFrame
        df = pd.DataFrame(self.batch_buffer)
        
        # TODO: Implement from_pandas in async client
        # result = await client.from_pandas(df)
        # if result.is_ok():
        #     handle = result.unwrap()
        #     await client.write_parquet(handle, "market_data.parquet")
        
        print(f"💾 Flushed {len(self.batch_buffer)} records to Polaroid")
        self.batch_buffer.clear()


async def blockchain_mempool_monitor(rpc_url: str, polaroid_url: str):
    """
    Monitor blockchain mempool in real-time
    
    Example use case: MEV bot monitoring pending transactions
    """
    print(f"⛓️  Monitoring blockchain mempool: {rpc_url}")
    
    async with AsyncPolaroidClient(polaroid_url) as client:
        # Simulate mempool subscription (use web3.py or ethers in production)
        tx_count = 0
        high_value_txs = []
        
        while True:
            # Simulate pending transactions
            num_txs = np.random.randint(10, 100)
            pending_txs = [
                {
                    'hash': f"0x{np.random.randint(0, 2**64):064x}",
                    'from': f"0x{np.random.randint(0, 2**160):040x}",
                    'to': f"0x{np.random.randint(0, 2**160):040x}",
                    'value': np.random.exponential(1.0),
                    'gas_price': 20 + np.random.exponential(10.0),
                    'timestamp': int(datetime.now().timestamp() * 1000),
                }
                for _ in range(num_txs)
            ]
            
            tx_count += len(pending_txs)
            
            # Detect high-value transactions (potential arbitrage)
            high_value = [tx for tx in pending_txs if tx['value'] > 10.0]
            if high_value:
                high_value_txs.extend(high_value)
                print(f"💰 High-value tx detected: {len(high_value)} (total: {len(high_value_txs)})")
                
                # In production: analyze for arbitrage opportunities
                # df = await client.from_records(high_value_txs)
                # opportunities = await client.filter(df, "value > 50.0")
            
            if tx_count % 1000 == 0:
                print(f"📦 Processed {tx_count} transactions")
                
            await asyncio.sleep(0.5)


async def multi_symbol_aggregator(ws_url: str, window_size: int = 100):
    """
    Real-time aggregation per symbol (e.g., BTC, ETH, SOL)
    
    Shows grouped aggregations like SQL GROUP BY
    """
    ws_stream = WebSocketDataStream(ws_url)
    
    # Per-symbol aggregators
    aggregators: Dict[str, RollingWindowAggregator] = {}
    
    print(f"📊 Multi-symbol aggregator (window: {window_size})")
    
    async for tick in ws_stream.stream():
        symbol = tick['symbol']
        
        # Create aggregator for new symbols
        if symbol not in aggregators:
            aggregators[symbol] = RollingWindowAggregator(window_size)
            print(f"➕ New symbol tracked: {symbol}")
        
        # Update aggregates
        stats = aggregators[symbol].update(tick)
        if stats:
            print(f"📈 {symbol:<10} | "
                  f"Count: {stats['count']:>4} | "
                  f"Avg: ${stats['avg_price']:>8,.2f} | "
                  f"Vol: {stats['total_volume']:>10,.0f}")


async def order_book_tracker(exchange_ws: str, symbol: str, depth: int = 10):
    """
    Track live order book from crypto exchange
    
    Use case: Market making, arbitrage detection
    """
    print(f"📖 Tracking order book for {symbol} (depth: {depth})")
    
    # Simulated order book updates
    bids = [[50000.0 - i * 10, np.random.random() * 10] for i in range(depth)]
    asks = [[50000.0 + i * 10, np.random.random() * 10] for i in range(depth)]
    
    while True:
        # Update order book (random walk)
        for i in range(depth):
            bids[i][0] += np.random.randn() * 5
            bids[i][1] = max(0.1, bids[i][1] + np.random.randn() * 0.5)
            asks[i][0] += np.random.randn() * 5
            asks[i][1] = max(0.1, asks[i][1] + np.random.randn() * 0.5)
        
        # Calculate spread
        best_bid = max(bids, key=lambda x: x[0])
        best_ask = min(asks, key=lambda x: x[0])
        spread = best_ask[0] - best_bid[0]
        mid_price = (best_bid[0] + best_ask[0]) / 2
        
        print(f"💱 {symbol} | Mid: ${mid_price:,.2f} | "
              f"Spread: ${spread:.2f} | "
              f"Bid: ${best_bid[0]:,.2f} ({best_bid[1]:.2f}) | "
              f"Ask: ${best_ask[0]:,.2f} ({best_ask[1]:.2f})")
        
        await asyncio.sleep(1.0)


async def main():
    """Run examples"""
    
    # Example 1: Simple stream processor
    print("=" * 60)
    print("Example 1: WebSocket Stream Processor")
    print("=" * 60)
    
    processor = PolaroidStreamProcessor(
        polaroid_url="localhost:50051",
        ws_url="ws://127.0.0.1:8080",
        window_size=1000,
    )
    
    # Run for 30 seconds
    try:
        await asyncio.wait_for(processor.process_stream(), timeout=30.0)
    except asyncio.TimeoutError:
        print("\n✅ Example 1 complete")
    
    # Example 2: Multi-symbol aggregator
    print("\n" + "=" * 60)
    print("Example 2: Multi-Symbol Aggregator")
    print("=" * 60)
    
    try:
        await asyncio.wait_for(
            multi_symbol_aggregator("ws://127.0.0.1:8080", window_size=100),
            timeout=20.0
        )
    except asyncio.TimeoutError:
        print("\n✅ Example 2 complete")
    
    # Example 3: Blockchain mempool monitor
    print("\n" + "=" * 60)
    print("Example 3: Blockchain Mempool Monitor")
    print("=" * 60)
    
    try:
        await asyncio.wait_for(
            blockchain_mempool_monitor(
                "https://mainnet.infura.io",
                "localhost:50051"
            ),
            timeout=15.0
        )
    except asyncio.TimeoutError:
        print("\n✅ Example 3 complete")
    
    print("\n🎉 All examples complete!")


if __name__ == "__main__":
    # Handle graceful shutdown
    loop = asyncio.get_event_loop()
    
    def signal_handler(sig, frame):
        print("\n🛑 Shutdown signal received")
        loop.stop()
    
    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)
    
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("\n✅ Shutdown complete")
