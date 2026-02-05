// Package streamhouse provides a Go client for the StreamHouse streaming data platform.
//
// Example usage:
//
//	client := streamhouse.NewClient("http://localhost:8080")
//
//	// Create a topic
//	topic, err := client.CreateTopic(ctx, "events", 3)
//
//	// Produce a message
//	result, err := client.Produce(ctx, "events", `{"event": "click"}`)
//
//	// Consume messages
//	result, err := client.Consume(ctx, "events", 0, streamhouse.ConsumeOptions{})
package streamhouse

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"
)

// Client is a StreamHouse API client.
type Client struct {
	baseURL    string
	apiURL     string
	apiKey     string
	httpClient *http.Client
}

// ClientOption is a function that configures a Client.
type ClientOption func(*Client)

// WithAPIKey sets the API key for authentication.
func WithAPIKey(key string) ClientOption {
	return func(c *Client) {
		c.apiKey = key
	}
}

// WithHTTPClient sets a custom HTTP client.
func WithHTTPClient(client *http.Client) ClientOption {
	return func(c *Client) {
		c.httpClient = client
	}
}

// WithTimeout sets the HTTP client timeout.
func WithTimeout(timeout time.Duration) ClientOption {
	return func(c *Client) {
		c.httpClient.Timeout = timeout
	}
}

// NewClient creates a new StreamHouse client.
func NewClient(baseURL string, opts ...ClientOption) *Client {
	baseURL = strings.TrimSuffix(baseURL, "/")
	c := &Client{
		baseURL: baseURL,
		apiURL:  baseURL + "/api/v1",
		httpClient: &http.Client{
			Timeout: 30 * time.Second,
		},
	}

	for _, opt := range opts {
		opt(c)
	}

	return c
}

// request makes an HTTP request and decodes the JSON response.
func (c *Client) request(ctx context.Context, method, path string, body interface{}, result interface{}) error {
	var bodyReader io.Reader
	if body != nil {
		jsonBody, err := json.Marshal(body)
		if err != nil {
			return fmt.Errorf("failed to marshal request body: %w", err)
		}
		bodyReader = bytes.NewReader(jsonBody)
	}

	req, err := http.NewRequestWithContext(ctx, method, c.apiURL+path, bodyReader)
	if err != nil {
		return fmt.Errorf("failed to create request: %w", err)
	}

	req.Header.Set("Content-Type", "application/json")
	if c.apiKey != "" {
		req.Header.Set("Authorization", "Bearer "+c.apiKey)
	}

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return &ConnectionError{Err: err}
	}
	defer resp.Body.Close()

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return fmt.Errorf("failed to read response body: %w", err)
	}

	if resp.StatusCode >= 400 {
		return c.handleError(resp.StatusCode, respBody)
	}

	if result != nil && len(respBody) > 0 {
		if err := json.Unmarshal(respBody, result); err != nil {
			return fmt.Errorf("failed to unmarshal response: %w", err)
		}
	}

	return nil
}

// handleError converts HTTP errors to typed errors.
func (c *Client) handleError(statusCode int, body []byte) error {
	var errResp struct {
		Error string `json:"error"`
	}
	msg := string(body)
	if err := json.Unmarshal(body, &errResp); err == nil && errResp.Error != "" {
		msg = errResp.Error
	}

	switch statusCode {
	case 400:
		return &ValidationError{Message: msg}
	case 401:
		return &AuthenticationError{Message: msg}
	case 404:
		return &NotFoundError{Message: msg}
	case 408:
		return &TimeoutError{Message: msg}
	case 409:
		return &ConflictError{Message: msg}
	default:
		if statusCode >= 500 {
			return &ServerError{Message: msg, StatusCode: statusCode}
		}
		return &Error{Message: msg, StatusCode: statusCode}
	}
}

// ===========================================================================
// Topic Operations
// ===========================================================================

// ListTopics returns all topics.
func (c *Client) ListTopics(ctx context.Context) ([]Topic, error) {
	var topics []Topic
	err := c.request(ctx, http.MethodGet, "/topics", nil, &topics)
	return topics, err
}

// CreateTopic creates a new topic.
func (c *Client) CreateTopic(ctx context.Context, name string, partitions int, opts ...CreateTopicOption) (*Topic, error) {
	req := createTopicRequest{
		Name:              name,
		Partitions:        partitions,
		ReplicationFactor: 1,
	}
	for _, opt := range opts {
		opt(&req)
	}

	var topic Topic
	err := c.request(ctx, http.MethodPost, "/topics", req, &topic)
	return &topic, err
}

// CreateTopicOption configures topic creation.
type CreateTopicOption func(*createTopicRequest)

// WithReplicationFactor sets the replication factor.
func WithReplicationFactor(rf int) CreateTopicOption {
	return func(r *createTopicRequest) {
		r.ReplicationFactor = rf
	}
}

type createTopicRequest struct {
	Name              string `json:"name"`
	Partitions        int    `json:"partitions"`
	ReplicationFactor int    `json:"replication_factor"`
}

// GetTopic returns topic details.
func (c *Client) GetTopic(ctx context.Context, name string) (*Topic, error) {
	var topic Topic
	err := c.request(ctx, http.MethodGet, "/topics/"+url.PathEscape(name), nil, &topic)
	return &topic, err
}

// DeleteTopic deletes a topic.
func (c *Client) DeleteTopic(ctx context.Context, name string) error {
	return c.request(ctx, http.MethodDelete, "/topics/"+url.PathEscape(name), nil, nil)
}

// ListPartitions returns partitions for a topic.
func (c *Client) ListPartitions(ctx context.Context, topic string) ([]Partition, error) {
	var partitions []Partition
	err := c.request(ctx, http.MethodGet, "/topics/"+url.PathEscape(topic)+"/partitions", nil, &partitions)
	return partitions, err
}

// ===========================================================================
// Producer Operations
// ===========================================================================

// Produce produces a single message.
func (c *Client) Produce(ctx context.Context, topic, value string, opts ...ProduceOption) (*ProduceResult, error) {
	req := produceRequest{
		Topic: topic,
		Value: value,
	}
	for _, opt := range opts {
		opt(&req)
	}

	var result ProduceResult
	err := c.request(ctx, http.MethodPost, "/produce", req, &result)
	return &result, err
}

// ProduceOption configures message production.
type ProduceOption func(*produceRequest)

// WithKey sets the message key.
func WithKey(key string) ProduceOption {
	return func(r *produceRequest) {
		r.Key = &key
	}
}

// WithPartition sets the target partition.
func WithPartition(partition int) ProduceOption {
	return func(r *produceRequest) {
		r.Partition = &partition
	}
}

type produceRequest struct {
	Topic     string  `json:"topic"`
	Value     string  `json:"value"`
	Key       *string `json:"key,omitempty"`
	Partition *int    `json:"partition,omitempty"`
}

// ProduceBatch produces multiple messages.
func (c *Client) ProduceBatch(ctx context.Context, topic string, records []BatchRecord) (*BatchProduceResult, error) {
	req := batchProduceRequest{
		Topic:   topic,
		Records: records,
	}

	var result BatchProduceResult
	err := c.request(ctx, http.MethodPost, "/produce/batch", req, &result)
	return &result, err
}

type batchProduceRequest struct {
	Topic   string        `json:"topic"`
	Records []BatchRecord `json:"records"`
}

// ===========================================================================
// Consumer Operations
// ===========================================================================

// ConsumeOptions configures message consumption.
type ConsumeOptions struct {
	Offset     int64
	MaxRecords int
}

// Consume consumes messages from a partition.
func (c *Client) Consume(ctx context.Context, topic string, partition int, opts ConsumeOptions) (*ConsumeResult, error) {
	params := url.Values{}
	params.Set("topic", topic)
	params.Set("partition", strconv.Itoa(partition))
	if opts.Offset > 0 {
		params.Set("offset", strconv.FormatInt(opts.Offset, 10))
	}
	if opts.MaxRecords > 0 {
		params.Set("maxRecords", strconv.Itoa(opts.MaxRecords))
	}

	var result ConsumeResult
	err := c.request(ctx, http.MethodGet, "/consume?"+params.Encode(), nil, &result)
	return &result, err
}

// ===========================================================================
// Consumer Group Operations
// ===========================================================================

// ListConsumerGroups returns all consumer groups.
func (c *Client) ListConsumerGroups(ctx context.Context) ([]ConsumerGroup, error) {
	var groups []ConsumerGroup
	err := c.request(ctx, http.MethodGet, "/consumer-groups", nil, &groups)
	return groups, err
}

// GetConsumerGroup returns consumer group details.
func (c *Client) GetConsumerGroup(ctx context.Context, groupID string) (*ConsumerGroupDetail, error) {
	var group ConsumerGroupDetail
	err := c.request(ctx, http.MethodGet, "/consumer-groups/"+url.PathEscape(groupID), nil, &group)
	return &group, err
}

// CommitOffset commits a consumer offset.
func (c *Client) CommitOffset(ctx context.Context, groupID, topic string, partition int, offset int64) error {
	req := commitOffsetRequest{
		GroupID:   groupID,
		Topic:     topic,
		Partition: partition,
		Offset:    offset,
	}

	var result struct {
		Success bool `json:"success"`
	}
	err := c.request(ctx, http.MethodPost, "/consumer-groups/commit", req, &result)
	return err
}

type commitOffsetRequest struct {
	GroupID   string `json:"group_id"`
	Topic     string `json:"topic"`
	Partition int    `json:"partition"`
	Offset    int64  `json:"offset"`
}

// DeleteConsumerGroup deletes a consumer group.
func (c *Client) DeleteConsumerGroup(ctx context.Context, groupID string) error {
	return c.request(ctx, http.MethodDelete, "/consumer-groups/"+url.PathEscape(groupID), nil, nil)
}

// ===========================================================================
// SQL Operations
// ===========================================================================

// QueryOptions configures SQL query execution.
type QueryOptions struct {
	TimeoutMs int64
}

// Query executes a SQL query.
func (c *Client) Query(ctx context.Context, sql string, opts QueryOptions) (*SQLResult, error) {
	req := sqlQueryRequest{
		Query: sql,
	}
	if opts.TimeoutMs > 0 {
		req.TimeoutMs = &opts.TimeoutMs
	}

	var result SQLResult
	err := c.request(ctx, http.MethodPost, "/sql", req, &result)
	return &result, err
}

type sqlQueryRequest struct {
	Query     string `json:"query"`
	TimeoutMs *int64 `json:"timeout_ms,omitempty"`
}

// ===========================================================================
// Agent/Cluster Operations
// ===========================================================================

// ListAgents returns all agents.
func (c *Client) ListAgents(ctx context.Context) ([]Agent, error) {
	var agents []Agent
	err := c.request(ctx, http.MethodGet, "/agents", nil, &agents)
	return agents, err
}

// GetMetrics returns cluster metrics.
func (c *Client) GetMetrics(ctx context.Context) (*MetricsSnapshot, error) {
	var metrics MetricsSnapshot
	err := c.request(ctx, http.MethodGet, "/metrics", nil, &metrics)
	return &metrics, err
}

// ===========================================================================
// Health Check
// ===========================================================================

// HealthCheck checks if the server is healthy.
func (c *Client) HealthCheck(ctx context.Context) bool {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, c.baseURL+"/health", nil)
	if err != nil {
		return false
	}

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return false
	}
	defer resp.Body.Close()

	return resp.StatusCode == http.StatusOK
}
