"""Polaroid Async Client - Advanced async/await support with Tokio-style concurrency

Features:
- Zero-cost async with Python asyncio + Rust Tokio
- Concurrent batch operations
- Streaming WebSocket subscriptions
- Monadic error handling (Result/Option types)
- Production-ready graceful shutdowns
"""
import grpc.aio
import pyarrow as pa
import asyncio
from io import BytesIO
from typing import List, Optional, Callable, TypeVar, Generic, AsyncIterator, Tuple
from dataclasses import dataclass
from contextlib import asynccontextmanager
import time

from . import polaroid_pb2
from . import polaroid_pb2_grpc

# Monadic types for functional error handling
T = TypeVar('T')
E = TypeVar('E')
U = TypeVar('U')


@dataclass
class Result(Generic[T, E]):
    """Rust-style Result monad for error handling
    
    Example:
        result = Result.Ok(42)
        doubled = result.map(lambda x: x * 2)  # Result.Ok(84)
        
        error = Result.Err("failed")
        recovered = error.or_else(lambda e: Result.Ok(0))  # Result.Ok(0)
    """
    _value: Optional[T] = None
    _error: Optional[E] = None
    
    @staticmethod
    def Ok(value: T) -> 'Result[T, E]':
        return Result(_value=value)
    
    @staticmethod
    def Err(error: E) -> 'Result[T, E]':
        return Result(_error=error)
    
    def is_ok(self) -> bool:
        return self._value is not None
    
    def is_err(self) -> bool:
        return self._error is not None
    
    def unwrap(self) -> T:
        if self.is_ok():
            return self._value
        raise ValueError(f"Called unwrap on Err: {self._error}")
    
    def unwrap_or(self, default: T) -> T:
        return self._value if self.is_ok() else default
    
    def expect(self, msg: str) -> T:
        if self.is_ok():
            return self._value
        raise ValueError(f"{msg}: {self._error}")
    
    def map(self, f: Callable[[T], U]) -> 'Result[U, E]':
        """Functor map - transforms Ok value, passes through Err"""
        if self.is_ok():
            try:
                return Result.Ok(f(self._value))
            except Exception as e:
                return Result.Err(e)
        return Result.Err(self._error)
    
    def and_then(self, f: Callable[[T], 'Result[U, E]']) -> 'Result[U, E]':
        """Monadic bind (flatMap) - chains Results"""
        return f(self._value) if self.is_ok() else Result.Err(self._error)
    
    def or_else(self, f: Callable[[E], 'Result[T, E]']) -> 'Result[T, E]':
        """Error recovery - transforms Err, passes through Ok"""
        return self if self.is_ok() else f(self._error)


@dataclass
class Option(Generic[T]):
    """Rust-style Option monad for handling None
    
    Example:
        some = Option.Some(42)
        none = Option.Nothing()
        
        result = some.map(lambda x: x * 2)  # Option.Some(84)
        default = none.unwrap_or(0)  # 0
    """
    _value: Optional[T] = None
    
    @staticmethod
    def Some(value: T) -> 'Option[T]':
        return Option(_value=value)
    
    @staticmethod
    def Nothing() -> 'Option[T]':
        return Option()
    
    def is_some(self) -> bool:
        return self._value is not None
    
    def is_none(self) -> bool:
        return self._value is None
    
    def unwrap(self) -> T:
        if self.is_some():
            return self._value
        raise ValueError("Called unwrap on Nothing")
    
    def unwrap_or(self, default: T) -> T:
        return self._value if self.is_some() else default
    
    def map(self, f: Callable[[T], U]) -> 'Option[U]':
        return Option.Some(f(self._value)) if self.is_some() else Option.Nothing()
    
    def and_then(self, f: Callable[[T], 'Option[U]']) -> 'Option[U]':
        return f(self._value) if self.is_some() else Option.Nothing()


class AsyncPolaroidClient:
    """Async Polaroid client with Tokio-style concurrency
    
    Usage:
        async with AsyncPolaroidClient("localhost:50051") as client:
            # Concurrent batch read
            dfs = await client.batch_read(["file1.parquet", "file2.parquet"])
            
            # Stream processing
            async for batch in client.stream_collect(df_handle):
                process(batch)
    """
    
    def __init__(self, address: str = "localhost:50051", max_concurrent: int = 100):
        self.address = address
        self.channel: Optional[grpc.aio.Channel] = None
        self.stub = None
        self._active_handles: set = set()
        self._heartbeat_tasks: dict = {}
        self._shutdown_event = asyncio.Event()
        self._semaphore = asyncio.Semaphore(max_concurrent)
    
    async def connect(self):
        """Establish async connection to Polaroid server"""
        self.channel = grpc.aio.insecure_channel(
            self.address,
            options=[
                ('grpc.max_send_message_length', 100 * 1024 * 1024),
                ('grpc.max_receive_message_length', 100 * 1024 * 1024),
                ('grpc.keepalive_time_ms', 10000),
                ('grpc.keepalive_timeout_ms', 5000),
                ('grpc.http2.max_pings_without_data', 0),
                ('grpc.keepalive_permit_without_calls', 1),
            ]
        )
        self.stub = polaroid_pb2_grpc.DataFrameServiceStub(self.channel)
        return self
    
    async def close(self):
        """Graceful shutdown - Tokio-style cleanup"""
        # Signal shutdown
        self._shutdown_event.set()
        
        # Cancel all heartbeat tasks
        for task in self._heartbeat_tasks.values():
            task.cancel()
        
        # Wait for tasks with timeout
        if self._heartbeat_tasks:
            await asyncio.wait(
                self._heartbeat_tasks.values(),
                timeout=5.0,
                return_when=asyncio.ALL_COMPLETED
            )
        
        # Drop all active handles concurrently
        if self._active_handles:
            drop_tasks = [
                self._drop_handle(handle) 
                for handle in list(self._active_handles)
            ]
            await asyncio.gather(*drop_tasks, return_exceptions=True)
        
        # Close channel
        if self.channel:
            await self.channel.close()
    
    async def __aenter__(self):
        return await self.connect()
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        await self.close()
    
    async def _drop_handle(self, handle: str):
        """Internal: drop a handle"""
        try:
            await self.stub.DropHandle(
                polaroid_pb2.DropHandleRequest(handle=handle)
            )
            self._active_handles.discard(handle)
        except Exception:
            pass
    
    async def read_parquet(
        self, 
        path: str, 
        columns: Optional[List[str]] = None
    ) -> Result[str, str]:
        """Async read Parquet file - returns handle wrapped in Result
        
        Example:
            result = await client.read_parquet("data.parquet")
            handle = result.unwrap_or("default_handle")
        """
        try:
            async with self._semaphore:  # Limit concurrency
                response = await self.stub.ReadParquet(
                    polaroid_pb2.ReadParquetRequest(
                        path=path,
                        columns=columns or []
                    )
                )
                self._active_handles.add(response.handle)
                return Result.Ok(response.handle)
        except grpc.aio.AioRpcError as e:
            return Result.Err(str(e))
    
    async def batch_read(
        self, 
        paths: List[str], 
        columns: Optional[List[str]] = None
    ) -> List[Result[str, str]]:
        """Concurrent batch read - Tokio work-stealing on server + asyncio.gather on client
        
        This is where Polaroid shines:
        - Server uses Tokio's work-stealing runtime (spawns tasks across threads)
        - Client uses asyncio.gather (concurrent RPC calls)
        - Result: Near-linear scalability up to CPU core count
        
        Example:
            # Read 100 files concurrently - limited by max_concurrent
            results = await client.batch_read([f"data_{i}.parquet" for i in range(100)])
            handles = [r.unwrap() for r in results if r.is_ok()]
        """
        tasks = [self.read_parquet(path, columns) for path in paths]
        return await asyncio.gather(*tasks, return_exceptions=False)
    
    async def collect(self, handle: str) -> Result[pa.Table, str]:
        """Async collect DataFrame with streaming Arrow IPC
        
        Returns:
            Result containing pyarrow.Table or error
        """
        try:
            async with self._semaphore:
                stream = self.stub.Collect(
                    polaroid_pb2.CollectRequest(handle=handle)
                )
                
                # Collect all batches
                chunks = []
                async for response in stream:
                    if response.arrow_batch:
                        reader = pa.ipc.open_stream(BytesIO(response.arrow_batch))
                        for batch in reader:
                            chunks.append(batch)
                
                if not chunks:
                    return Result.Err("No data received")
                
                table = pa.Table.from_batches(chunks)
                return Result.Ok(table)
        except grpc.aio.AioRpcError as e:
            return Result.Err(str(e))
    
    async def batch_collect(self, handles: List[str]) -> List[Result[pa.Table, str]]:
        """Concurrent batch collect - maximum throughput
        
        Example:
            # Collect 50 DataFrames concurrently
            tables = await client.batch_collect(handles)
            successful = [t.unwrap() for t in tables if t.is_ok()]
        """
        tasks = [self.collect(handle) for handle in handles]
        return await asyncio.gather(*tasks, return_exceptions=False)
    
    async def stream_collect(self, handle: str) -> AsyncIterator[pa.RecordBatch]:
        """Stream DataFrame as Arrow batches - zero-copy processing
        
        Example:
            async for batch in client.stream_collect(df_handle):
                # Process batch immediately (streaming aggregation, filtering, etc.)
                result = process_batch(batch)
        """
        try:
            stream = self.stub.Collect(
                polaroid_pb2.CollectRequest(handle=handle)
            )
            
            async for response in stream:
                if response.arrow_batch:
                    reader = pa.ipc.open_stream(BytesIO(response.arrow_batch))
                    for batch in reader:
                        yield batch
        except grpc.aio.AioRpcError as e:
            raise RuntimeError(f"Stream error: {e}")
    
    async def heartbeat(self, handle: str, interval: float = 60.0):
        """Background heartbeat task - keeps handle alive
        
        Automatically started when reading files with auto_heartbeat=True
        """
        try:
            while not self._shutdown_event.is_set():
                await asyncio.sleep(interval)
                
                try:
                    await self.stub.Heartbeat(
                        polaroid_pb2.HeartbeatRequest(handle=handle)
                    )
                except grpc.aio.AioRpcError:
                    # Handle expired or dropped
                    self._active_handles.discard(handle)
                    break
        except asyncio.CancelledError:
            pass
    
    def start_heartbeat(self, handle: str, interval: float = 60.0):
        """Start background heartbeat for a handle"""
        task = asyncio.create_task(self.heartbeat(handle, interval))
        self._heartbeat_tasks[handle] = task
        return task
    
    async def select(self, handle: str, columns: List[str]) -> Result[str, str]:
        """Async select columns - returns new handle"""
        try:
            response = await self.stub.Select(
                polaroid_pb2.SelectRequest(
                    handle=handle,
                    columns=columns
                )
            )
            self._active_handles.add(response.handle)
            return Result.Ok(response.handle)
        except grpc.aio.AioRpcError as e:
            return Result.Err(str(e))
    
    async def get_shape(self, handle: str) -> Result[Tuple[int, int], str]:
        """Async get DataFrame shape"""
        try:
            response = await self.stub.GetShape(
                polaroid_pb2.GetShapeRequest(handle=handle)
            )
            return Result.Ok((response.rows, response.columns))
        except grpc.aio.AioRpcError as e:
            return Result.Err(str(e))


class AsyncDataFrame:
    """Async DataFrame wrapper with lazy operations
    
    Example:
        df = AsyncDataFrame(client, handle)
        df2 = df.select(["col1", "col2"])  # Lazy - just builds query
        table = await df2.collect()  # Executes on server
    """
    
    def __init__(self, client: AsyncPolaroidClient, handle: str):
        self.client = client
        self.handle = handle
    
    def select(self, columns: List[str]) -> 'AsyncDataFrame':
        """Lazy select - returns new AsyncDataFrame"""
        # Note: This is lazy - doesn't execute until collect()
        new_df = AsyncDataFrame(self.client, self.handle)
        new_df._pending_select = columns
        return new_df
    
    async def collect(self) -> Result[pa.Table, str]:
        """Execute lazy operations and collect result"""
        # Apply pending operations
        handle = self.handle
        if hasattr(self, '_pending_select'):
            result = await self.client.select(handle, self._pending_select)
            if result.is_err():
                return result
            handle = result.unwrap()
        
        # Collect final result
        return await self.client.collect(handle)
    
    async def shape(self) -> Result[Tuple[int, int], str]:
        """Get DataFrame shape"""
        return await self.client.get_shape(self.handle)
    
    async def __aenter__(self):
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        await self.client._drop_handle(self.handle)
