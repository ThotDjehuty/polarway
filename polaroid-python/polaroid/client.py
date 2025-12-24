"""Polaroid client implementation."""

import grpc
import pyarrow as pa
import pyarrow.ipc as ipc
from typing import Optional, List, Tuple
from contextlib import contextmanager

from .config import config
from . import polaroid_pb2
from . import polaroid_pb2_grpc


_default_client: Optional["PolaroidClient"] = None


def connect(server: str = "localhost:50051", **kwargs) -> "PolaroidClient":
    """Connect to a Polaroid gRPC server.
    
    Args:
        server: Server address (host:port)
        **kwargs: Additional gRPC channel options
    
    Returns:
        PolaroidClient instance
    
    Example:
        >>> import polaroid as pd
        >>> pd.connect("localhost:50051")
        >>> df = pd.read_parquet("data.parquet")
    """
    global _default_client
    client = PolaroidClient(server, **kwargs)
    _default_client = client
    config.default_server = server
    return client


def get_default_client() -> "PolaroidClient":
    """Get the default Polaroid client.
    
    Returns:
        PolaroidClient instance
    
    Raises:
        RuntimeError: If no connection has been established
    """
    if _default_client is None:
        raise RuntimeError(
            "No connection to Polaroid server. Call polaroid.connect() first."
        )
    return _default_client


class PolaroidClient:
    """Client for communicating with Polaroid gRPC server."""
    
    def __init__(self, server: str, **kwargs):
        """Initialize client.
        
        Args:
            server: Server address (host:port)
            **kwargs: Additional gRPC channel options
        """
        self.server = server
        self.channel = grpc.insecure_channel(server, **kwargs)
        self.stub = polaroid_pb2_grpc.DataFrameServiceStub(self.channel)
    
    def read_parquet(
        self,
        path: str,
        columns: Optional[List[str]] = None,
        predicate: Optional[str] = None,
        n_rows: Optional[int] = None,
    ) -> "DataFrame":
        """Read a Parquet file.
        
        Args:
            path: Path to Parquet file
            columns: Columns to read (None = all)
            predicate: Filter predicate (SQL-like)
            n_rows: Maximum number of rows to read
        
        Returns:
            DataFrame handle
        """
        request = polaroid_pb2.ReadParquetRequest(
            path=path,
            columns=columns or [],
            predicate=predicate,
            n_rows=n_rows,
        )
        
        response = self.stub.ReadParquet(request, timeout=config.timeout)
        
        if response.error:
            raise RuntimeError(f"Server error: {response.error}")
        
        return DataFrame(self, response.handle)
    
    def write_parquet(self, handle: str, path: str, **kwargs) -> None:
        """Write DataFrame to Parquet file.
        
        Args:
            handle: DataFrame handle
            path: Output path
            **kwargs: Additional write options
        """
        request = polaroid_pb2.WriteParquetRequest(
            handle=handle,
            path=path,
        )
        
        response = self.stub.WriteParquet(request, timeout=config.timeout)
        
        if not response.success or response.error:
            raise RuntimeError(f"Write failed: {response.error}")
    
    def get_schema(self, handle: str) -> Tuple[str, List]:
        """Get schema for a DataFrame.
        
        Args:
            handle: DataFrame handle
        
        Returns:
            Tuple of (schema_json, columns)
        """
        request = polaroid_pb2.GetSchemaRequest(handle=handle)
        response = self.stub.GetSchema(request, timeout=config.timeout)
        return response.schema_json, response.columns
    
    def get_shape(self, handle: str) -> Tuple[int, int]:
        """Get shape of DataFrame.
        
        Args:
            handle: DataFrame handle
        
        Returns:
            Tuple of (rows, columns)
        """
        request = polaroid_pb2.GetShapeRequest(handle=handle)
        response = self.stub.GetShape(request, timeout=config.timeout)
        return response.rows, response.columns
    
    def collect(self, handle: str) -> pa.Table:
        """Collect DataFrame as Arrow Table.
        
        Args:
            handle: DataFrame handle
        
        Returns:
            PyArrow Table
        """
        request = polaroid_pb2.CollectRequest(handle=handle)
        
        # Stream Arrow IPC batches
        batches = []
        for response in self.stub.Collect(request, timeout=config.timeout):
            if response.error:
                raise RuntimeError(f"Server error: {response.error}")
            
            # Decode Arrow IPC
            reader = ipc.open_stream(response.arrow_ipc)
            for batch in reader:
                batches.append(batch)
        
        if not batches:
            # Empty DataFrame
            return pa.table({})
        
        return pa.Table.from_batches(batches)
    
    def select(self, handle: str, columns: List[str]) -> str:
        """Select columns from DataFrame.
        
        Args:
            handle: DataFrame handle
            columns: Column names to select
        
        Returns:
            New DataFrame handle
        """
        request = polaroid_pb2.SelectRequest(
            handle=handle,
            columns=columns,
        )
        response = self.stub.Select(request, timeout=config.timeout)
        
        if response.error:
            raise RuntimeError(f"Server error: {response.error}")
        
        return response.handle
    
    def drop_handle(self, handle: str) -> None:
        """Drop a DataFrame handle.
        
        Args:
            handle: DataFrame handle
        """
        request = polaroid_pb2.DropHandleRequest(handle=handle)
        self.stub.DropHandle(request, timeout=config.timeout)
    
    def heartbeat(self, handles: List[str]) -> dict:
        """Send heartbeat for handles.
        
        Args:
            handles: List of handles to keep alive
        
        Returns:
            Dict of handle -> is_alive
        """
        request = polaroid_pb2.HeartbeatRequest(handles=handles)
        response = self.stub.Heartbeat(request, timeout=config.timeout)
        return response.alive
    
    def close(self) -> None:
        """Close the client connection."""
        self.channel.close()


class DataFrame:
    """Polaroid DataFrame - a handle to a server-side DataFrame."""
    
    def __init__(self, client: PolaroidClient, handle: str):
        """Initialize DataFrame.
        
        Args:
            client: PolaroidClient instance
            handle: Server-side handle
        """
        self._client = client
        self._handle = handle
    
    @property
    def handle(self) -> str:
        """Get the server-side handle."""
        return self._handle
    
    def shape(self) -> Tuple[int, int]:
        """Get DataFrame shape.
        
        Returns:
            Tuple of (rows, columns)
        """
        return self._client.get_shape(self._handle)
    
    def schema(self) -> Tuple[str, List]:
        """Get DataFrame schema.
        
        Returns:
            Tuple of (schema_json, columns)
        """
        return self._client.get_schema(self._handle)
    
    def select(self, columns: List[str]) -> "DataFrame":
        """Select columns.
        
        Args:
            columns: Column names to select
        
        Returns:
            New DataFrame
        """
        new_handle = self._client.select(self._handle, columns)
        return DataFrame(self._client, new_handle)
    
    def collect(self) -> pa.Table:
        """Collect DataFrame as Arrow Table.
        
        Returns:
            PyArrow Table
        """
        return self._client.collect(self._handle)
    
    def write_parquet(self, path: str, **kwargs) -> None:
        """Write to Parquet file.
        
        Args:
            path: Output path
            **kwargs: Additional write options
        """
        self._client.write_parquet(self._handle, path, **kwargs)
    
    def drop_handle(self) -> None:
        """Drop this DataFrame handle on the server."""
        self._client.drop_handle(self._handle)
    
    def is_alive(self) -> bool:
        """Check if handle is still alive on server.
        
        Returns:
            True if alive, False otherwise
        """
        result = self._client.heartbeat([self._handle])
        return result.get(self._handle, False)
    
    def heartbeat(self) -> None:
        """Send heartbeat to extend handle TTL."""
        self._client.heartbeat([self._handle])
    
    def __repr__(self) -> str:
        """String representation."""
        try:
            rows, cols = self.shape()
            return f"<DataFrame handle={self._handle[:8]}... shape=({rows}, {cols})>"
        except Exception:
            return f"<DataFrame handle={self._handle[:8]}...>"
    
    def __del__(self) -> None:
        """Cleanup when DataFrame is garbage collected."""
        try:
            self.drop_handle()
        except Exception:
            pass  # Server may have already cleaned up


@contextmanager
def connection(server: str, **kwargs):
    """Context manager for Polaroid connection.
    
    Args:
        server: Server address
        **kwargs: Additional options
    
    Yields:
        PolaroidClient instance
    
    Example:
        >>> with connection("localhost:50051") as client:
        ...     df = client.read_parquet("data.parquet")
        ...     result = df.collect()
    """
    client = connect(server, **kwargs)
    try:
        yield client
    finally:
        client.close()
