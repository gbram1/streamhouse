package streamhouse

import "encoding/json"

// Topic represents a StreamHouse topic.
type Topic struct {
	Name              string `json:"name"`
	Partitions        int    `json:"partitions"`
	ReplicationFactor int    `json:"replication_factor"`
	CreatedAt         string `json:"created_at"`
	MessageCount      int64  `json:"message_count"`
	SizeBytes         int64  `json:"size_bytes"`
}

// Partition represents a topic partition.
type Partition struct {
	Topic         string  `json:"topic"`
	PartitionID   int     `json:"partition_id"`
	LeaderAgentID *string `json:"leader_agent_id"`
	HighWatermark int64   `json:"high_watermark"`
	LowWatermark  int64   `json:"low_watermark"`
}

// ProduceResult is the result of producing a message.
type ProduceResult struct {
	Offset    int64 `json:"offset"`
	Partition int   `json:"partition"`
}

// BatchRecord represents a single record in a batch produce request.
type BatchRecord struct {
	Value     string  `json:"value"`
	Key       *string `json:"key,omitempty"`
	Partition *int    `json:"partition,omitempty"`
}

// NewBatchRecord creates a new batch record with just a value.
func NewBatchRecord(value string) BatchRecord {
	return BatchRecord{Value: value}
}

// NewBatchRecordWithKey creates a new batch record with a key.
func NewBatchRecordWithKey(value, key string) BatchRecord {
	return BatchRecord{Value: value, Key: &key}
}

// BatchRecordResult is the result of a single record in batch produce.
type BatchRecordResult struct {
	Partition int   `json:"partition"`
	Offset    int64 `json:"offset"`
}

// BatchProduceResult is the result of batch produce.
type BatchProduceResult struct {
	Count   int                 `json:"count"`
	Offsets []BatchRecordResult `json:"offsets"`
}

// ConsumedRecord is a consumed message record.
type ConsumedRecord struct {
	Partition int     `json:"partition"`
	Offset    int64   `json:"offset"`
	Key       *string `json:"key"`
	Value     string  `json:"value"`
	Timestamp int64   `json:"timestamp"`
}

// ConsumeResult is the result of consuming messages.
type ConsumeResult struct {
	Records    []ConsumedRecord `json:"records"`
	NextOffset int64            `json:"next_offset"`
}

// ConsumerOffset represents a consumer group offset for a partition.
type ConsumerOffset struct {
	Topic           string `json:"topic"`
	PartitionID     int    `json:"partition_id"`
	CommittedOffset int64  `json:"committed_offset"`
	HighWatermark   int64  `json:"high_watermark"`
	Lag             int64  `json:"lag"`
}

// ConsumerGroup represents a consumer group summary.
type ConsumerGroup struct {
	GroupID        string   `json:"group_id"`
	Topics         []string `json:"topics"`
	TotalLag       int64    `json:"total_lag"`
	PartitionCount int      `json:"partition_count"`
}

// ConsumerGroupDetail represents detailed consumer group information.
type ConsumerGroupDetail struct {
	GroupID string           `json:"group_id"`
	Offsets []ConsumerOffset `json:"offsets"`
}

// Agent represents a StreamHouse agent/server.
type Agent struct {
	AgentID          string `json:"agent_id"`
	Address          string `json:"address"`
	AvailabilityZone string `json:"availability_zone"`
	AgentGroup       string `json:"agent_group"`
	LastHeartbeat    int64  `json:"last_heartbeat"`
	StartedAt        int64  `json:"started_at"`
	ActiveLeases     int    `json:"active_leases"`
}

// ColumnInfo represents column metadata for SQL results.
type ColumnInfo struct {
	Name     string `json:"name"`
	DataType string `json:"data_type"`
}

// SQLResult is the result of a SQL query.
type SQLResult struct {
	Columns         []ColumnInfo      `json:"columns"`
	Rows            []json.RawMessage `json:"rows"`
	RowCount        int               `json:"row_count"`
	ExecutionTimeMs int64             `json:"execution_time_ms"`
	Truncated       bool              `json:"truncated"`
}

// GetRow returns a row as a slice of interface{}.
func (r *SQLResult) GetRow(index int) ([]interface{}, error) {
	if index < 0 || index >= len(r.Rows) {
		return nil, &Error{Message: "row index out of range"}
	}

	var row []interface{}
	if err := json.Unmarshal(r.Rows[index], &row); err != nil {
		return nil, err
	}
	return row, nil
}

// MetricsSnapshot represents cluster metrics.
type MetricsSnapshot struct {
	TopicsCount     int64 `json:"topics_count"`
	AgentsCount     int64 `json:"agents_count"`
	PartitionsCount int64 `json:"partitions_count"`
	TotalMessages   int64 `json:"total_messages"`
}
