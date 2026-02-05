package streamhouse

import "fmt"

// Error is a base error type for StreamHouse client errors.
type Error struct {
	Message    string
	StatusCode int
}

func (e *Error) Error() string {
	if e.StatusCode > 0 {
		return fmt.Sprintf("streamhouse: %s (status %d)", e.Message, e.StatusCode)
	}
	return fmt.Sprintf("streamhouse: %s", e.Message)
}

// ConnectionError is returned when connection to StreamHouse server fails.
type ConnectionError struct {
	Err error
}

func (e *ConnectionError) Error() string {
	return fmt.Sprintf("streamhouse: connection failed: %v", e.Err)
}

func (e *ConnectionError) Unwrap() error {
	return e.Err
}

// NotFoundError is returned when a requested resource is not found (404).
type NotFoundError struct {
	Message string
}

func (e *NotFoundError) Error() string {
	return fmt.Sprintf("streamhouse: not found: %s", e.Message)
}

// ValidationError is returned when request validation fails (400).
type ValidationError struct {
	Message string
}

func (e *ValidationError) Error() string {
	return fmt.Sprintf("streamhouse: validation error: %s", e.Message)
}

// TimeoutError is returned when a request times out (408).
type TimeoutError struct {
	Message string
}

func (e *TimeoutError) Error() string {
	return fmt.Sprintf("streamhouse: timeout: %s", e.Message)
}

// ConflictError is returned when a resource already exists (409).
type ConflictError struct {
	Message string
}

func (e *ConflictError) Error() string {
	return fmt.Sprintf("streamhouse: conflict: %s", e.Message)
}

// AuthenticationError is returned when authentication fails (401).
type AuthenticationError struct {
	Message string
}

func (e *AuthenticationError) Error() string {
	return fmt.Sprintf("streamhouse: authentication failed: %s", e.Message)
}

// AuthorizationError is returned when authorization fails (403).
type AuthorizationError struct {
	Message string
}

func (e *AuthorizationError) Error() string {
	return fmt.Sprintf("streamhouse: authorization failed: %s", e.Message)
}

// ServerError is returned when the server returns an error (5xx).
type ServerError struct {
	Message    string
	StatusCode int
}

func (e *ServerError) Error() string {
	return fmt.Sprintf("streamhouse: server error (status %d): %s", e.StatusCode, e.Message)
}

// IsNotFound returns true if the error is a NotFoundError.
func IsNotFound(err error) bool {
	_, ok := err.(*NotFoundError)
	return ok
}

// IsValidation returns true if the error is a ValidationError.
func IsValidation(err error) bool {
	_, ok := err.(*ValidationError)
	return ok
}

// IsConflict returns true if the error is a ConflictError.
func IsConflict(err error) bool {
	_, ok := err.(*ConflictError)
	return ok
}

// IsTimeout returns true if the error is a TimeoutError.
func IsTimeout(err error) bool {
	_, ok := err.(*TimeoutError)
	return ok
}

// IsConnection returns true if the error is a ConnectionError.
func IsConnection(err error) bool {
	_, ok := err.(*ConnectionError)
	return ok
}

// IsServer returns true if the error is a ServerError.
func IsServer(err error) bool {
	_, ok := err.(*ServerError)
	return ok
}
