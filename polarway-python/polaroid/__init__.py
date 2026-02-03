"""
Polaroid: FDAP-Optimized DataFrame Engine

A next-generation DataFrame library with gRPC interface,
streaming-first architecture, and native time-series support.
"""

from .client import connect, DataFrame
from .config import config

__version__ = "0.1.0"
__all__ = ["connect", "DataFrame", "config", "read_parquet", "read_rest_api"]


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


def read_rest_api(url: str, method: str = "GET", headers: dict = None, body: str = None) -> "DataFrame":
    """Read data from a REST API endpoint.
    
    Fetches JSON data from a REST API and converts it to a DataFrame.
    Perfect for loading data from financial APIs, weather services, etc.
    
    Args:
        url: REST API endpoint URL
        method: HTTP method (GET, POST, PUT). Default: GET
        headers: Optional HTTP headers (e.g., authentication, User-Agent)
        body: Optional request body for POST/PUT requests
    
    Returns:
        DataFrame handle pointing to server-side DataFrame
    
    Example:
        >>> import polaroid as pld
        >>> pld.connect("localhost:50051")
        >>> 
        >>> # Fetch stock data from Yahoo Finance
        >>> symbol = "AAPL"
        >>> url = f"https://query1.finance.yahoo.com/v8/finance/chart/{symbol}?interval=1d&range=30d"
        >>> headers = {"User-Agent": "Mozilla/5.0"}
        >>> df = pld.read_rest_api(url, headers=headers)
        >>> print(df.shape())
        >>> 
        >>> # Fetch cryptocurrency data
        >>> url = "https://api.coingecko.com/api/v3/coins/bitcoin/market_chart?vs_currency=usd&days=7"
        >>> df = pld.read_rest_api(url)
        >>> print(df.collect())
    """
    from .client import get_default_client
    client = get_default_client()
    return client.read_rest_api(url, method=method, headers=headers, body=body)
