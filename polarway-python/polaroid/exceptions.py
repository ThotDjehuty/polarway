"""Polaroid exception classes."""


class PolaroidError(Exception):
    """Base exception for Polaroid errors."""
    pass


class ConnectionError(PolaroidError):
    """Error connecting to Polaroid server."""
    pass


class ServerError(PolaroidError):
    """Error from Polaroid server."""
    pass


class HandleError(PolaroidError):
    """Error related to DataFrame handles."""
    pass


class HandleNotFoundError(HandleError):
    """Handle not found on server."""
    pass


class HandleExpiredError(HandleError):
    """Handle has expired (TTL exceeded)."""
    pass


class FileNotFoundError(PolaroidError):
    """File not found."""
    pass


class SchemaError(PolaroidError):
    """Schema-related error."""
    pass


class TypeMismatchError(SchemaError):
    """Type mismatch in operation."""
    pass


class OperationError(PolaroidError):
    """Error performing DataFrame operation."""
    pass


class TimeoutError(PolaroidError):
    """Operation timed out."""
    pass
