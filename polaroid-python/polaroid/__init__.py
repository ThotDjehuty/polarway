"""
Polaroid: FDAP-Optimized DataFrame Engine

A next-generation DataFrame library with gRPC interface,
streaming-first architecture, and native time-series support.
"""

from .client import connect, DataFrame
from .config import config

__version__ = "0.1.0"
__all__ = ["connect", "DataFrame", "config"]


# Convenience functions
def read_parquet(path: str, **kwargs) -> "DataFrame":
    """Read a Parquet file into a DataFrame.
    
    Args:
        path: Path to the Parquet file
        **kwargs: Additional arguments passed to the server
    
    Returns:
        DataFrame handle
    
    Example:
        >>> import polaroid as pd
        >>> pd.connect("localhost:50051")
        >>> df = pd.read_parquet("data.parquet")
        >>> print(df.shape())
        (1000, 5)
    """
    from .client import get_default_client
    client = get_default_client()
    return client.read_parquet(path, **kwargs)
