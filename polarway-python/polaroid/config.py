"""Configuration management for Polaroid client."""

from typing import Optional


class Config:
    """Global configuration for Polaroid client."""
    
    def __init__(self) -> None:
        self._default_server: Optional[str] = None
        self._timeout: int = 30
        self._pool_size: int = 10
        self._max_memory: str = "8GB"
        self._log_level: str = "INFO"
    
    def set(self, key: str, value: any) -> None:
        """Set a configuration value.
        
        Args:
            key: Configuration key
            value: Configuration value
        """
        attr = f"_{key}"
        if hasattr(self, attr):
            setattr(self, attr, value)
        else:
            raise ValueError(f"Unknown configuration key: {key}")
    
    def get(self, key: str) -> any:
        """Get a configuration value.
        
        Args:
            key: Configuration key
        
        Returns:
            Configuration value
        """
        attr = f"_{key}"
        if hasattr(self, attr):
            return getattr(self, attr)
        else:
            raise ValueError(f"Unknown configuration key: {key}")
    
    @property
    def default_server(self) -> Optional[str]:
        return self._default_server
    
    @default_server.setter
    def default_server(self, value: str) -> None:
        self._default_server = value
    
    @property
    def timeout(self) -> int:
        return self._timeout
    
    @timeout.setter
    def timeout(self, value: int) -> None:
        self._timeout = value
    
    @property
    def pool_size(self) -> int:
        return self._pool_size
    
    @pool_size.setter
    def pool_size(self, value: int) -> None:
        self._pool_size = value


# Global config instance
config = Config()
