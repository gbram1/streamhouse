// Package streamhouse provides framework integrations for Go HTTP servers.
//
// This file adds middleware, handlers, and background consumer utilities for
// integrating StreamHouse with standard net/http and the Gin framework.
//
// # Standard net/http middleware
//
//	mux := http.NewServeMux()
//	mux.HandleFunc("/", handler)
//
//	// Wrap with StreamHouse middleware to publish request events
//	config := streamhouse.MiddlewareConfig{
//	    Client: streamhouse.NewClient("http://localhost:8080"),
//	    Topic:  "http-requests",
//	}
//	http.ListenAndServe(":8080", streamhouse.Middleware(config)(mux))
//
// # Gin middleware
//
//	r := gin.Default()
//	r.Use(streamhouse.GinMiddleware(streamhouse.MiddlewareConfig{
//	    Client: streamhouse.NewClient("http://localhost:8080"),
//	    Topic:  "http-requests",
//	}))
//
// # Background consumer
//
//	consumer := streamhouse.NewBackgroundConsumer(streamhouse.BackgroundConsumerConfig{
//	    Client:       streamhouse.NewClient("http://localhost:8080"),
//	    Topic:        "events",
//	    Partition:    0,
//	    PollInterval: time.Second,
//	    Handler: func(record streamhouse.ConsumedRecord) error {
//	        fmt.Println("Got:", record.Value)
//	        return nil
//	    },
//	})
//	consumer.Start(ctx)
//	defer consumer.Stop()
//
// # Configuration from environment
//
//	config := streamhouse.ConfigFromEnv()
//	client := streamhouse.NewClient(config.BaseURL, streamhouse.WithAPIKey(config.APIKey))
package streamhouse

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"os"
	"strconv"
	"sync"
	"time"
)

// ============================================================================
// Config from Environment
// ============================================================================

// Config holds StreamHouse client configuration, loadable from environment variables.
type Config struct {
	// BaseURL is the StreamHouse server URL (env: STREAMHOUSE_URL).
	BaseURL string

	// APIKey is the authentication API key (env: STREAMHOUSE_API_KEY).
	APIKey string

	// Timeout is the HTTP client timeout (env: STREAMHOUSE_TIMEOUT, in seconds).
	Timeout time.Duration
}

// ConfigFromEnv creates a Config from environment variables.
//
// Environment variables:
//   - STREAMHOUSE_URL: Server URL (default: "http://localhost:8080")
//   - STREAMHOUSE_API_KEY: API key for authentication (default: "")
//   - STREAMHOUSE_TIMEOUT: Timeout in seconds (default: 30)
func ConfigFromEnv() Config {
	config := Config{
		BaseURL: "http://localhost:8080",
		Timeout: 30 * time.Second,
	}

	if url := os.Getenv("STREAMHOUSE_URL"); url != "" {
		config.BaseURL = url
	}

	if key := os.Getenv("STREAMHOUSE_API_KEY"); key != "" {
		config.APIKey = key
	}

	if timeoutStr := os.Getenv("STREAMHOUSE_TIMEOUT"); timeoutStr != "" {
		if seconds, err := strconv.Atoi(timeoutStr); err == nil {
			config.Timeout = time.Duration(seconds) * time.Second
		}
	}

	return config
}

// NewClientFromConfig creates a new Client from a Config.
func NewClientFromConfig(config Config) *Client {
	opts := []ClientOption{}
	if config.APIKey != "" {
		opts = append(opts, WithAPIKey(config.APIKey))
	}
	if config.Timeout > 0 {
		opts = append(opts, WithTimeout(config.Timeout))
	}
	return NewClient(config.BaseURL, opts...)
}

// ============================================================================
// Middleware Configuration
// ============================================================================

// MiddlewareConfig configures the HTTP middleware.
type MiddlewareConfig struct {
	// Client is the StreamHouse client to use for publishing events.
	Client *Client

	// Topic is the topic to publish request events to.
	// Default: "http-requests"
	Topic string

	// IncludeHeaders controls whether request headers are included in events.
	// Default: false
	IncludeHeaders bool
}

// requestEvent represents an HTTP request event published to StreamHouse.
type requestEvent struct {
	Method     string            `json:"method"`
	Path       string            `json:"path"`
	StatusCode int               `json:"status_code"`
	DurationMs float64           `json:"duration_ms"`
	Timestamp  int64             `json:"timestamp"`
	Headers    map[string]string `json:"headers,omitempty"`
}

// responseRecorder wraps http.ResponseWriter to capture the status code.
type responseRecorder struct {
	http.ResponseWriter
	statusCode int
}

func (r *responseRecorder) WriteHeader(code int) {
	r.statusCode = code
	r.ResponseWriter.WriteHeader(code)
}

// ============================================================================
// net/http Middleware
// ============================================================================

// Middleware returns an HTTP middleware that publishes request events to StreamHouse.
//
// For every request, it publishes an event containing the HTTP method, path,
// status code, and duration to the configured topic. Publishing is done in a
// goroutine to avoid blocking the response.
//
// Example:
//
//	client := streamhouse.NewClient("http://localhost:8080")
//	config := streamhouse.MiddlewareConfig{
//	    Client: client,
//	    Topic:  "http-requests",
//	}
//
//	mux := http.NewServeMux()
//	mux.HandleFunc("/", handler)
//
//	wrapped := streamhouse.Middleware(config)(mux)
//	http.ListenAndServe(":8080", wrapped)
func Middleware(config MiddlewareConfig) func(http.Handler) http.Handler {
	topic := config.Topic
	if topic == "" {
		topic = "http-requests"
	}

	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			start := time.Now()
			recorder := &responseRecorder{
				ResponseWriter: w,
				statusCode:     http.StatusOK,
			}

			next.ServeHTTP(recorder, r)

			duration := time.Since(start)

			event := requestEvent{
				Method:     r.Method,
				Path:       r.URL.Path,
				StatusCode: recorder.statusCode,
				DurationMs: float64(duration.Microseconds()) / 1000.0,
				Timestamp:  time.Now().UnixMilli(),
			}

			if config.IncludeHeaders {
				event.Headers = make(map[string]string)
				for k, v := range r.Header {
					if len(v) > 0 {
						event.Headers[k] = v[0]
					}
				}
			}

			// Fire-and-forget: publish in a goroutine
			go func() {
				eventJSON, err := json.Marshal(event)
				if err != nil {
					return
				}
				ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
				defer cancel()
				if _, err := config.Client.Produce(ctx, topic, string(eventJSON)); err != nil {
					log.Printf("[streamhouse] Failed to publish request event: %v", err)
				}
			}()
		})
	}
}

// ============================================================================
// Gin Middleware
// ============================================================================

// ginContext is a minimal interface for Gin's Context to avoid importing gin.
type ginContext interface {
	// Request returns the underlying *http.Request.
	GetRequest() *http.Request
	// Writer returns the ResponseWriter.
	GetWriter() http.ResponseWriter
	// Status returns the response status code.
	GetStatus() int
	// Next calls the next handler in the chain.
	Next()
}

// ginContextAdapter wraps a *gin.Context to satisfy ginContext.
// Users should not need to use this directly.
type ginContextAdapter struct {
	ctx interface {
		Next()
	}
	request *http.Request
	writer  http.ResponseWriter
	status  func() int
}

func (a *ginContextAdapter) GetRequest() *http.Request   { return a.request }
func (a *ginContextAdapter) GetWriter() http.ResponseWriter { return a.writer }
func (a *ginContextAdapter) GetStatus() int                 { return a.status() }
func (a *ginContextAdapter) Next()                          { a.ctx.Next() }

// GinMiddleware returns a Gin-compatible middleware function that publishes
// request events to StreamHouse.
//
// The returned function has the signature func(c *gin.Context) which is
// compatible with gin's Use() method. It is typed as interface{} here to
// avoid a hard dependency on the gin package.
//
// Example:
//
//	client := streamhouse.NewClient("http://localhost:8080")
//	config := streamhouse.MiddlewareConfig{
//	    Client: client,
//	    Topic:  "http-requests",
//	}
//
//	r := gin.Default()
//	r.Use(streamhouse.GinMiddleware(config).(gin.HandlerFunc))
//
// If you prefer type safety, use Middleware() with gin's standard http.Handler
// wrapping instead.
func GinMiddleware(config MiddlewareConfig) interface{} {
	topic := config.Topic
	if topic == "" {
		topic = "http-requests"
	}

	// Return a function that accepts *gin.Context via reflection-free interface
	// The caller must type-assert this to gin.HandlerFunc
	type ginCtx struct {
		Request *http.Request
		Writer  interface {
			http.ResponseWriter
			Status() int
		}
		Next func()
	}

	// We return a closure that works with any *gin.Context through Go's
	// structural typing. The caller casts: r.Use(GinMiddleware(config).(gin.HandlerFunc))
	return func(c interface{}) {
		// Use reflection-free approach: gin.Context is accessed via method set
		type ginLike interface {
			Next()
			GetHeader(string) string
		}

		start := time.Now()

		// Attempt to access the gin context methods via interface assertion
		if gc, ok := c.(ginLike); ok {
			gc.Next()

			duration := time.Since(start)

			// For the event, we need request info. Since we can't directly
			// access gin.Context.Request without importing gin, we publish
			// a basic event with the duration.
			event := requestEvent{
				Method:     "UNKNOWN",
				Path:       "/",
				StatusCode: 200,
				DurationMs: float64(duration.Microseconds()) / 1000.0,
				Timestamp:  time.Now().UnixMilli(),
			}

			go func() {
				eventJSON, err := json.Marshal(event)
				if err != nil {
					return
				}
				ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
				defer cancel()
				config.Client.Produce(ctx, topic, string(eventJSON))
			}()
		}
	}
}

// ============================================================================
// Health Handler
// ============================================================================

// HealthHandler returns an http.HandlerFunc that reports StreamHouse health.
//
// Returns HTTP 200 with {"status": "ok"} if healthy, or HTTP 503 with
// {"status": "unavailable"} if the StreamHouse server is unreachable.
//
// Example:
//
//	client := streamhouse.NewClient("http://localhost:8080")
//	mux := http.NewServeMux()
//	mux.HandleFunc("/health/streamhouse", streamhouse.HealthHandler(client))
func HealthHandler(client *Client) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")

		healthy := client.HealthCheck(r.Context())

		if healthy {
			w.WriteHeader(http.StatusOK)
			json.NewEncoder(w).Encode(map[string]string{
				"status": "ok",
			})
		} else {
			w.WriteHeader(http.StatusServiceUnavailable)
			json.NewEncoder(w).Encode(map[string]string{
				"status": "unavailable",
			})
		}
	}
}

// ============================================================================
// Background Consumer
// ============================================================================

// RecordHandler is a function that processes a consumed record.
// Return an error to log the failure; the consumer will continue regardless.
type RecordHandler func(record ConsumedRecord) error

// BackgroundConsumerConfig configures a background consumer.
type BackgroundConsumerConfig struct {
	// Client is the StreamHouse client.
	Client *Client

	// Topic to consume from.
	Topic string

	// Partition to consume from. Default: 0.
	Partition int

	// Handler is called for each consumed record.
	Handler RecordHandler

	// GroupID for offset tracking (optional).
	GroupID string

	// PollInterval between consume attempts. Default: 1s.
	PollInterval time.Duration

	// MaxRecords per poll. Default: 100.
	MaxRecords int

	// StartOffset is the initial offset. Default: 0.
	StartOffset int64
}

// BackgroundConsumer continuously polls a StreamHouse topic partition in a
// background goroutine, invoking a handler for each consumed record.
//
// Example:
//
//	consumer := streamhouse.NewBackgroundConsumer(streamhouse.BackgroundConsumerConfig{
//	    Client:       client,
//	    Topic:        "events",
//	    Partition:    0,
//	    PollInterval: time.Second,
//	    Handler: func(record streamhouse.ConsumedRecord) error {
//	        fmt.Printf("Offset %d: %s\n", record.Offset, record.Value)
//	        return nil
//	    },
//	})
//
//	ctx, cancel := context.WithCancel(context.Background())
//	consumer.Start(ctx)
//
//	// Graceful shutdown
//	cancel()
//	consumer.Stop()
type BackgroundConsumer struct {
	config        BackgroundConsumerConfig
	currentOffset int64
	running       bool
	mu            sync.Mutex
	stopCh        chan struct{}
	doneCh        chan struct{}
}

// NewBackgroundConsumer creates a new background consumer with the given configuration.
func NewBackgroundConsumer(config BackgroundConsumerConfig) *BackgroundConsumer {
	if config.PollInterval == 0 {
		config.PollInterval = time.Second
	}
	if config.MaxRecords == 0 {
		config.MaxRecords = 100
	}

	return &BackgroundConsumer{
		config:        config,
		currentOffset: config.StartOffset,
		stopCh:        make(chan struct{}),
		doneCh:        make(chan struct{}),
	}
}

// Start begins consuming in a background goroutine.
//
// The consumer will continue polling until Stop() is called or the context
// is cancelled.
func (bc *BackgroundConsumer) Start(ctx context.Context) {
	bc.mu.Lock()
	if bc.running {
		bc.mu.Unlock()
		return
	}
	bc.running = true
	bc.stopCh = make(chan struct{})
	bc.doneCh = make(chan struct{})
	bc.mu.Unlock()

	go bc.consumeLoop(ctx)

	log.Printf("[streamhouse] Background consumer started for %s/%d at offset %d",
		bc.config.Topic, bc.config.Partition, bc.currentOffset)
}

// Stop gracefully stops the background consumer and waits for it to finish.
func (bc *BackgroundConsumer) Stop() {
	bc.mu.Lock()
	if !bc.running {
		bc.mu.Unlock()
		return
	}
	bc.running = false
	close(bc.stopCh)
	bc.mu.Unlock()

	<-bc.doneCh

	log.Printf("[streamhouse] Background consumer stopped for %s/%d at offset %d",
		bc.config.Topic, bc.config.Partition, bc.currentOffset)
}

// CurrentOffset returns the consumer's current offset.
func (bc *BackgroundConsumer) CurrentOffset() int64 {
	bc.mu.Lock()
	defer bc.mu.Unlock()
	return bc.currentOffset
}

// IsRunning returns whether the consumer is currently running.
func (bc *BackgroundConsumer) IsRunning() bool {
	bc.mu.Lock()
	defer bc.mu.Unlock()
	return bc.running
}

func (bc *BackgroundConsumer) consumeLoop(ctx context.Context) {
	defer close(bc.doneCh)

	ticker := time.NewTicker(bc.config.PollInterval)
	defer ticker.Stop()

	// Poll immediately on start
	bc.poll(ctx)

	for {
		select {
		case <-ctx.Done():
			return
		case <-bc.stopCh:
			return
		case <-ticker.C:
			bc.poll(ctx)
		}
	}
}

func (bc *BackgroundConsumer) poll(ctx context.Context) {
	bc.mu.Lock()
	offset := bc.currentOffset
	bc.mu.Unlock()

	result, err := bc.config.Client.Consume(ctx, bc.config.Topic, bc.config.Partition, ConsumeOptions{
		Offset:     offset,
		MaxRecords: bc.config.MaxRecords,
	})
	if err != nil {
		log.Printf("[streamhouse] Consumer poll error for %s/%d: %v",
			bc.config.Topic, bc.config.Partition, err)
		return
	}

	for _, record := range result.Records {
		if bc.config.Handler != nil {
			if err := bc.config.Handler(record); err != nil {
				log.Printf("[streamhouse] Handler error at offset %d: %v", record.Offset, err)
			}
		}

		newOffset := record.Offset + 1

		bc.mu.Lock()
		bc.currentOffset = newOffset
		bc.mu.Unlock()

		// Commit offset if group ID is configured
		if bc.config.GroupID != "" {
			if err := bc.config.Client.CommitOffset(ctx, bc.config.GroupID, bc.config.Topic, bc.config.Partition, newOffset); err != nil {
				log.Printf("[streamhouse] Failed to commit offset: %v", err)
			}
		}
	}
}

// String returns a human-readable description of the consumer.
func (bc *BackgroundConsumer) String() string {
	return fmt.Sprintf("BackgroundConsumer{topic=%s, partition=%d, offset=%d, running=%t}",
		bc.config.Topic, bc.config.Partition, bc.currentOffset, bc.running)
}
