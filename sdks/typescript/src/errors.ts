/**
 * Error types for StreamHouse client.
 */

/** Base error class for StreamHouse client errors */
export class StreamHouseError extends Error {
  public readonly statusCode?: number;

  constructor(message: string, statusCode?: number) {
    super(message);
    this.name = 'StreamHouseError';
    this.statusCode = statusCode;
    Object.setPrototypeOf(this, StreamHouseError.prototype);
  }
}

/** Raised when connection to StreamHouse server fails */
export class ConnectionError extends StreamHouseError {
  constructor(message: string) {
    super(message);
    this.name = 'ConnectionError';
    Object.setPrototypeOf(this, ConnectionError.prototype);
  }
}

/** Raised when a requested resource is not found (404) */
export class NotFoundError extends StreamHouseError {
  constructor(message: string) {
    super(message, 404);
    this.name = 'NotFoundError';
    Object.setPrototypeOf(this, NotFoundError.prototype);
  }
}

/** Raised when request validation fails (400) */
export class ValidationError extends StreamHouseError {
  constructor(message: string) {
    super(message, 400);
    this.name = 'ValidationError';
    Object.setPrototypeOf(this, ValidationError.prototype);
  }
}

/** Raised when a request times out (408) */
export class TimeoutError extends StreamHouseError {
  constructor(message: string) {
    super(message, 408);
    this.name = 'TimeoutError';
    Object.setPrototypeOf(this, TimeoutError.prototype);
  }
}

/** Raised when a resource already exists (409) */
export class ConflictError extends StreamHouseError {
  constructor(message: string) {
    super(message, 409);
    this.name = 'ConflictError';
    Object.setPrototypeOf(this, ConflictError.prototype);
  }
}

/** Raised when authentication fails (401) */
export class AuthenticationError extends StreamHouseError {
  constructor(message: string) {
    super(message, 401);
    this.name = 'AuthenticationError';
    Object.setPrototypeOf(this, AuthenticationError.prototype);
  }
}

/** Raised when authorization fails (403) */
export class AuthorizationError extends StreamHouseError {
  constructor(message: string) {
    super(message, 403);
    this.name = 'AuthorizationError';
    Object.setPrototypeOf(this, AuthorizationError.prototype);
  }
}

/** Raised when the server returns an error (5xx) */
export class ServerError extends StreamHouseError {
  constructor(message: string, statusCode: number = 500) {
    super(message, statusCode);
    this.name = 'ServerError';
    Object.setPrototypeOf(this, ServerError.prototype);
  }
}
