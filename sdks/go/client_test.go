package streamhouse

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

// Helper to create a test server
func newTestServer(t *testing.T, handler http.HandlerFunc) (*httptest.Server, *Client) {
	server := httptest.NewServer(handler)
	client := NewClient(server.URL)
	return server, client
}

func TestNewClient(t *testing.T) {
	client := NewClient("http://localhost:8080")
	if client.baseURL != "http://localhost:8080" {
		t.Errorf("expected baseURL 'http://localhost:8080', got '%s'", client.baseURL)
	}
	if client.apiURL != "http://localhost:8080/api/v1" {
		t.Errorf("expected apiURL 'http://localhost:8080/api/v1', got '%s'", client.apiURL)
	}
}

func TestNewClientWithTrailingSlash(t *testing.T) {
	client := NewClient("http://localhost:8080/")
	if client.baseURL != "http://localhost:8080" {
		t.Errorf("expected baseURL without trailing slash, got '%s'", client.baseURL)
	}
}

func TestNewClientWithOptions(t *testing.T) {
	client := NewClient("http://localhost:8080",
		WithAPIKey("test-key"),
		WithTimeout(60*time.Second),
	)
	if client.apiKey != "test-key" {
		t.Errorf("expected apiKey 'test-key', got '%s'", client.apiKey)
	}
	if client.httpClient.Timeout != 60*time.Second {
		t.Errorf("expected timeout 60s, got '%v'", client.httpClient.Timeout)
	}
}

func TestListTopics(t *testing.T) {
	server, client := newTestServer(t, func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/v1/topics" {
			t.Errorf("expected path '/api/v1/topics', got '%s'", r.URL.Path)
		}
		if r.Method != http.MethodGet {
			t.Errorf("expected method GET, got '%s'", r.Method)
		}

		topics := []Topic{
			{Name: "topic1", Partitions: 3, ReplicationFactor: 1},
			{Name: "topic2", Partitions: 6, ReplicationFactor: 2},
		}
		json.NewEncoder(w).Encode(topics)
	})
	defer server.Close()

	ctx := context.Background()
	topics, err := client.ListTopics(ctx)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(topics) != 2 {
		t.Errorf("expected 2 topics, got %d", len(topics))
	}
	if topics[0].Name != "topic1" {
		t.Errorf("expected topic name 'topic1', got '%s'", topics[0].Name)
	}
}

func TestCreateTopic(t *testing.T) {
	server, client := newTestServer(t, func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/v1/topics" {
			t.Errorf("expected path '/api/v1/topics', got '%s'", r.URL.Path)
		}
		if r.Method != http.MethodPost {
			t.Errorf("expected method POST, got '%s'", r.Method)
		}

		var req createTopicRequest
		json.NewDecoder(r.Body).Decode(&req)

		if req.Name != "my-topic" {
			t.Errorf("expected name 'my-topic', got '%s'", req.Name)
		}
		if req.Partitions != 3 {
			t.Errorf("expected partitions 3, got %d", req.Partitions)
		}

		topic := Topic{
			Name:              req.Name,
			Partitions:        req.Partitions,
			ReplicationFactor: req.ReplicationFactor,
			CreatedAt:         "2024-01-01T00:00:00Z",
		}
		w.WriteHeader(http.StatusCreated)
		json.NewEncoder(w).Encode(topic)
	})
	defer server.Close()

	ctx := context.Background()
	topic, err := client.CreateTopic(ctx, "my-topic", 3)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if topic.Name != "my-topic" {
		t.Errorf("expected topic name 'my-topic', got '%s'", topic.Name)
	}
	if topic.Partitions != 3 {
		t.Errorf("expected 3 partitions, got %d", topic.Partitions)
	}
}

func TestCreateTopicWithReplicationFactor(t *testing.T) {
	server, client := newTestServer(t, func(w http.ResponseWriter, r *http.Request) {
		var req createTopicRequest
		json.NewDecoder(r.Body).Decode(&req)

		if req.ReplicationFactor != 2 {
			t.Errorf("expected replication factor 2, got %d", req.ReplicationFactor)
		}

		topic := Topic{
			Name:              req.Name,
			Partitions:        req.Partitions,
			ReplicationFactor: req.ReplicationFactor,
		}
		json.NewEncoder(w).Encode(topic)
	})
	defer server.Close()

	ctx := context.Background()
	topic, err := client.CreateTopic(ctx, "my-topic", 3, WithReplicationFactor(2))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if topic.ReplicationFactor != 2 {
		t.Errorf("expected replication factor 2, got %d", topic.ReplicationFactor)
	}
}

func TestGetTopic(t *testing.T) {
	server, client := newTestServer(t, func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/v1/topics/my-topic" {
			t.Errorf("expected path '/api/v1/topics/my-topic', got '%s'", r.URL.Path)
		}

		topic := Topic{Name: "my-topic", Partitions: 3}
		json.NewEncoder(w).Encode(topic)
	})
	defer server.Close()

	ctx := context.Background()
	topic, err := client.GetTopic(ctx, "my-topic")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if topic.Name != "my-topic" {
		t.Errorf("expected topic name 'my-topic', got '%s'", topic.Name)
	}
}

func TestDeleteTopic(t *testing.T) {
	server, client := newTestServer(t, func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/v1/topics/my-topic" {
			t.Errorf("expected path '/api/v1/topics/my-topic', got '%s'", r.URL.Path)
		}
		if r.Method != http.MethodDelete {
			t.Errorf("expected method DELETE, got '%s'", r.Method)
		}
		w.WriteHeader(http.StatusNoContent)
	})
	defer server.Close()

	ctx := context.Background()
	err := client.DeleteTopic(ctx, "my-topic")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestProduce(t *testing.T) {
	server, client := newTestServer(t, func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/v1/produce" {
			t.Errorf("expected path '/api/v1/produce', got '%s'", r.URL.Path)
		}

		var req produceRequest
		json.NewDecoder(r.Body).Decode(&req)

		if req.Topic != "events" {
			t.Errorf("expected topic 'events', got '%s'", req.Topic)
		}
		if req.Value != `{"event":"click"}` {
			t.Errorf("unexpected value: %s", req.Value)
		}

		result := ProduceResult{Offset: 42, Partition: 0}
		json.NewEncoder(w).Encode(result)
	})
	defer server.Close()

	ctx := context.Background()
	result, err := client.Produce(ctx, "events", `{"event":"click"}`)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Offset != 42 {
		t.Errorf("expected offset 42, got %d", result.Offset)
	}
}

func TestProduceWithKey(t *testing.T) {
	server, client := newTestServer(t, func(w http.ResponseWriter, r *http.Request) {
		var req produceRequest
		json.NewDecoder(r.Body).Decode(&req)

		if req.Key == nil || *req.Key != "user-123" {
			t.Errorf("expected key 'user-123', got %v", req.Key)
		}

		result := ProduceResult{Offset: 42, Partition: 0}
		json.NewEncoder(w).Encode(result)
	})
	defer server.Close()

	ctx := context.Background()
	_, err := client.Produce(ctx, "events", `{"event":"click"}`, WithKey("user-123"))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestProduceBatch(t *testing.T) {
	server, client := newTestServer(t, func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/v1/produce/batch" {
			t.Errorf("expected path '/api/v1/produce/batch', got '%s'", r.URL.Path)
		}

		var req batchProduceRequest
		json.NewDecoder(r.Body).Decode(&req)

		if len(req.Records) != 2 {
			t.Errorf("expected 2 records, got %d", len(req.Records))
		}

		result := BatchProduceResult{
			Count: 2,
			Offsets: []BatchRecordResult{
				{Partition: 0, Offset: 10},
				{Partition: 0, Offset: 11},
			},
		}
		json.NewEncoder(w).Encode(result)
	})
	defer server.Close()

	ctx := context.Background()
	result, err := client.ProduceBatch(ctx, "events", []BatchRecord{
		NewBatchRecord(`{"event":"a"}`),
		NewBatchRecordWithKey(`{"event":"b"}`, "key1"),
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Count != 2 {
		t.Errorf("expected count 2, got %d", result.Count)
	}
}

func TestConsume(t *testing.T) {
	server, client := newTestServer(t, func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/v1/consume" {
			t.Errorf("expected path '/api/v1/consume', got '%s'", r.URL.Path)
		}

		query := r.URL.Query()
		if query.Get("topic") != "events" {
			t.Errorf("expected topic 'events', got '%s'", query.Get("topic"))
		}
		if query.Get("partition") != "0" {
			t.Errorf("expected partition '0', got '%s'", query.Get("partition"))
		}

		result := ConsumeResult{
			Records: []ConsumedRecord{
				{Partition: 0, Offset: 0, Value: `{"event":"a"}`},
				{Partition: 0, Offset: 1, Value: `{"event":"b"}`},
			},
			NextOffset: 2,
		}
		json.NewEncoder(w).Encode(result)
	})
	defer server.Close()

	ctx := context.Background()
	result, err := client.Consume(ctx, "events", 0, ConsumeOptions{})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(result.Records) != 2 {
		t.Errorf("expected 2 records, got %d", len(result.Records))
	}
	if result.NextOffset != 2 {
		t.Errorf("expected next offset 2, got %d", result.NextOffset)
	}
}

func TestQuery(t *testing.T) {
	server, client := newTestServer(t, func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/v1/sql" {
			t.Errorf("expected path '/api/v1/sql', got '%s'", r.URL.Path)
		}

		var req sqlQueryRequest
		json.NewDecoder(r.Body).Decode(&req)

		if req.Query != "SELECT * FROM events" {
			t.Errorf("unexpected query: %s", req.Query)
		}

		result := SQLResult{
			Columns: []ColumnInfo{
				{Name: "offset", DataType: "bigint"},
				{Name: "value", DataType: "string"},
			},
			Rows:            []json.RawMessage{[]byte(`[0, "test"]`)},
			RowCount:        1,
			ExecutionTimeMs: 50,
			Truncated:       false,
		}
		json.NewEncoder(w).Encode(result)
	})
	defer server.Close()

	ctx := context.Background()
	result, err := client.Query(ctx, "SELECT * FROM events", QueryOptions{})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.RowCount != 1 {
		t.Errorf("expected row count 1, got %d", result.RowCount)
	}
	if len(result.Columns) != 2 {
		t.Errorf("expected 2 columns, got %d", len(result.Columns))
	}
}

func TestHealthCheck(t *testing.T) {
	server, client := newTestServer(t, func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/health" {
			w.WriteHeader(http.StatusNotFound)
			return
		}
		w.WriteHeader(http.StatusOK)
		w.Write([]byte(`{"status":"ok"}`))
	})
	defer server.Close()

	ctx := context.Background()
	healthy := client.HealthCheck(ctx)
	if !healthy {
		t.Error("expected health check to pass")
	}
}

func TestHealthCheckFailed(t *testing.T) {
	server, client := newTestServer(t, func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusServiceUnavailable)
	})
	defer server.Close()

	ctx := context.Background()
	healthy := client.HealthCheck(ctx)
	if healthy {
		t.Error("expected health check to fail")
	}
}

// Error handling tests
func TestNotFoundError(t *testing.T) {
	server, client := newTestServer(t, func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusNotFound)
		w.Write([]byte(`{"error":"topic not found"}`))
	})
	defer server.Close()

	ctx := context.Background()
	_, err := client.GetTopic(ctx, "nonexistent")
	if err == nil {
		t.Fatal("expected error")
	}
	if !IsNotFound(err) {
		t.Errorf("expected NotFoundError, got %T", err)
	}
}

func TestValidationError(t *testing.T) {
	server, client := newTestServer(t, func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusBadRequest)
		w.Write([]byte(`{"error":"invalid partitions"}`))
	})
	defer server.Close()

	ctx := context.Background()
	_, err := client.CreateTopic(ctx, "test", 0)
	if err == nil {
		t.Fatal("expected error")
	}
	if !IsValidation(err) {
		t.Errorf("expected ValidationError, got %T", err)
	}
}

func TestConflictError(t *testing.T) {
	server, client := newTestServer(t, func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusConflict)
		w.Write([]byte(`{"error":"topic already exists"}`))
	})
	defer server.Close()

	ctx := context.Background()
	_, err := client.CreateTopic(ctx, "existing", 3)
	if err == nil {
		t.Fatal("expected error")
	}
	if !IsConflict(err) {
		t.Errorf("expected ConflictError, got %T", err)
	}
}

func TestServerError(t *testing.T) {
	server, client := newTestServer(t, func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
		w.Write([]byte(`{"error":"internal error"}`))
	})
	defer server.Close()

	ctx := context.Background()
	_, err := client.ListTopics(ctx)
	if err == nil {
		t.Fatal("expected error")
	}
	if !IsServer(err) {
		t.Errorf("expected ServerError, got %T", err)
	}
}

func TestAPIKeyHeader(t *testing.T) {
	server, client := newTestServer(t, func(w http.ResponseWriter, r *http.Request) {
		auth := r.Header.Get("Authorization")
		if auth != "Bearer test-api-key" {
			t.Errorf("expected Authorization header 'Bearer test-api-key', got '%s'", auth)
		}
		json.NewEncoder(w).Encode([]Topic{})
	})
	defer server.Close()

	client.apiKey = "test-api-key"
	ctx := context.Background()
	_, err := client.ListTopics(ctx)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestContextCancellation(t *testing.T) {
	server, client := newTestServer(t, func(w http.ResponseWriter, r *http.Request) {
		time.Sleep(100 * time.Millisecond)
		json.NewEncoder(w).Encode([]Topic{})
	})
	defer server.Close()

	ctx, cancel := context.WithCancel(context.Background())
	cancel() // Cancel immediately

	_, err := client.ListTopics(ctx)
	if err == nil {
		t.Fatal("expected error due to cancelled context")
	}
}
