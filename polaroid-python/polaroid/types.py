"""Type definitions for Polaroid."""

from typing import Union, List, Dict, Any, Optional
from typing_extensions import TypeAlias
import pyarrow as pa


# Type aliases
PathLike: TypeAlias = Union[str, "Path"]
ColumnSelector: TypeAlias = Union[str, List[str]]
SchemaLike: TypeAlias = Union[pa.Schema, Dict[str, pa.DataType]]

# Re-export for convenience
__all__ = [
    "PathLike",
    "ColumnSelector", 
    "SchemaLike",
]
