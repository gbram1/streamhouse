"""Exceptions for StreamHouse Python client."""

from typing import Optional


class StreamHouseError(Exception):
    """Base exception for StreamHouse client errors."""

    def __init__(self, message: str, status_code: Optional[int] = None):
        self.message = message
        self.status_code = status_code
        super().__init__(message)


class ConnectionError(StreamHouseError):
    """Raised when connection to StreamHouse server fails."""
    pass


class NotFoundError(StreamHouseError):
    """Raised when a requested resource is not found (404)."""
    pass


class ValidationError(StreamHouseError):
    """Raised when request validation fails (400)."""
    pass


class TimeoutError(StreamHouseError):
    """Raised when a request times out (408)."""
    pass


class ConflictError(StreamHouseError):
    """Raised when a resource already exists (409)."""
    pass


class AuthenticationError(StreamHouseError):
    """Raised when authentication fails (401)."""
    pass


class AuthorizationError(StreamHouseError):
    """Raised when authorization fails (403)."""
    pass


class ServerError(StreamHouseError):
    """Raised when the server returns an error (5xx)."""
    pass
