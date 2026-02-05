"""Tests for StreamHouse Python client."""

import json
import pytest
from unittest.mock import Mock, patch, MagicMock
from streamhouse import StreamHouseClient
from streamhouse.models import Topic, ProduceResult, ConsumeResult, SqlResult
from streamhouse.exceptions import (
    NotFoundError,
    ValidationError,
    ConflictError,
    TimeoutError,
    ServerError,
)


class TestClientInitialization:
    """Tests for client initialization."""

    def test_basic_init(self):
        """Test basic client initialization."""
        client = StreamHouseClient("http://localhost:8080")
        assert client.base_url == "http://localhost:8080"
        assert client.api_url == "http://localhost:8080/api/v1"
        assert client.api_key is None

    def test_init_with_trailing_slash(self):
        """Test that trailing slash is stripped."""
        client = StreamHouseClient("http://localhost:8080/")
        assert client.base_url == "http://localhost:8080"

    def test_init_with_api_key(self):
        """Test initialization with API key."""
        client = StreamHouseClient("http://localhost:8080", api_key="test-key")
        assert client.api_key == "test-key"

    def test_init_with_timeout(self):
        """Test initialization with custom timeout."""
        client = StreamHouseClient("http://localhost:8080", timeout=60.0)
        assert client.timeout == 60.0


class TestTopicOperations:
    """Tests for topic operations."""

    @patch("streamhouse.client.requests")
    def test_list_topics(self, mock_requests):
        """Test listing topics."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = [
            {"name": "topic1", "partitions": 3, "replication_factor": 1, "created_at": "2024-01-01"},
            {"name": "topic2", "partitions": 6, "replication_factor": 2, "created_at": "2024-01-02"},
        ]
        mock_response.text = json.dumps(mock_response.json.return_value)
        mock_requests.request.return_value = mock_response

        client = StreamHouseClient("http://localhost:8080")
        topics = client.list_topics()

        assert len(topics) == 2
        assert topics[0].name == "topic1"
        assert topics[0].partitions == 3
        assert topics[1].name == "topic2"
        assert topics[1].partitions == 6

    @patch("streamhouse.client.requests")
    def test_create_topic(self, mock_requests):
        """Test creating a topic."""
        mock_response = Mock()
        mock_response.status_code = 201
        mock_response.json.return_value = {
            "name": "my-topic",
            "partitions": 3,
            "replication_factor": 1,
            "created_at": "2024-01-01",
        }
        mock_response.text = json.dumps(mock_response.json.return_value)
        mock_requests.request.return_value = mock_response

        client = StreamHouseClient("http://localhost:8080")
        topic = client.create_topic("my-topic", 3)

        assert topic.name == "my-topic"
        assert topic.partitions == 3

        # Verify the request was made correctly
        mock_requests.request.assert_called_once()
        call_kwargs = mock_requests.request.call_args
        assert call_kwargs[1]["json"]["name"] == "my-topic"
        assert call_kwargs[1]["json"]["partitions"] == 3

    @patch("streamhouse.client.requests")
    def test_get_topic(self, mock_requests):
        """Test getting a topic."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = {
            "name": "my-topic",
            "partitions": 3,
            "replication_factor": 1,
            "created_at": "2024-01-01",
        }
        mock_response.text = json.dumps(mock_response.json.return_value)
        mock_requests.request.return_value = mock_response

        client = StreamHouseClient("http://localhost:8080")
        topic = client.get_topic("my-topic")

        assert topic.name == "my-topic"

    @patch("streamhouse.client.requests")
    def test_delete_topic(self, mock_requests):
        """Test deleting a topic."""
        mock_response = Mock()
        mock_response.status_code = 204
        mock_response.text = ""
        mock_requests.request.return_value = mock_response

        client = StreamHouseClient("http://localhost:8080")
        client.delete_topic("my-topic")

        mock_requests.request.assert_called_once()
        assert mock_requests.request.call_args[0][0] == "DELETE"


class TestProducerOperations:
    """Tests for producer operations."""

    @patch("streamhouse.client.requests")
    def test_produce(self, mock_requests):
        """Test producing a message."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = {"offset": 42, "partition": 0}
        mock_response.text = json.dumps(mock_response.json.return_value)
        mock_requests.request.return_value = mock_response

        client = StreamHouseClient("http://localhost:8080")
        result = client.produce("events", '{"event": "click"}')

        assert result.offset == 42
        assert result.partition == 0

    @patch("streamhouse.client.requests")
    def test_produce_with_key(self, mock_requests):
        """Test producing a message with key."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = {"offset": 42, "partition": 0}
        mock_response.text = json.dumps(mock_response.json.return_value)
        mock_requests.request.return_value = mock_response

        client = StreamHouseClient("http://localhost:8080")
        result = client.produce("events", '{"event": "click"}', key="user-123")

        call_kwargs = mock_requests.request.call_args
        assert call_kwargs[1]["json"]["key"] == "user-123"

    @patch("streamhouse.client.requests")
    def test_produce_batch(self, mock_requests):
        """Test batch produce."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = {
            "count": 2,
            "offsets": [
                {"partition": 0, "offset": 10},
                {"partition": 0, "offset": 11},
            ],
        }
        mock_response.text = json.dumps(mock_response.json.return_value)
        mock_requests.request.return_value = mock_response

        client = StreamHouseClient("http://localhost:8080")
        result = client.produce_batch("events", [
            '{"event": "a"}',
            {"key": "user-1", "value": '{"event": "b"}'},
        ])

        assert result.count == 2
        assert len(result.offsets) == 2


class TestConsumerOperations:
    """Tests for consumer operations."""

    @patch("streamhouse.client.requests")
    def test_consume(self, mock_requests):
        """Test consuming messages."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = {
            "records": [
                {"partition": 0, "offset": 0, "key": None, "value": '{"event": "a"}', "timestamp": 1000},
                {"partition": 0, "offset": 1, "key": "k1", "value": '{"event": "b"}', "timestamp": 1001},
            ],
            "next_offset": 2,
        }
        mock_response.text = json.dumps(mock_response.json.return_value)
        mock_requests.request.return_value = mock_response

        client = StreamHouseClient("http://localhost:8080")
        result = client.consume("events", partition=0)

        assert len(result.records) == 2
        assert result.next_offset == 2
        assert result.records[0].value == '{"event": "a"}'
        assert result.records[1].key == "k1"


class TestSqlOperations:
    """Tests for SQL operations."""

    @patch("streamhouse.client.requests")
    def test_query(self, mock_requests):
        """Test executing a SQL query."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = {
            "columns": [
                {"name": "offset", "data_type": "bigint"},
                {"name": "value", "data_type": "string"},
            ],
            "rows": [[0, "test"]],
            "row_count": 1,
            "execution_time_ms": 50,
            "truncated": False,
        }
        mock_response.text = json.dumps(mock_response.json.return_value)
        mock_requests.request.return_value = mock_response

        client = StreamHouseClient("http://localhost:8080")
        result = client.query("SELECT * FROM events")

        assert result.row_count == 1
        assert len(result.columns) == 2
        assert result.columns[0].name == "offset"


class TestErrorHandling:
    """Tests for error handling."""

    @patch("streamhouse.client.requests")
    def test_not_found_error(self, mock_requests):
        """Test handling 404 errors."""
        mock_response = Mock()
        mock_response.status_code = 404
        mock_response.text = '{"error": "topic not found"}'
        mock_requests.request.return_value = mock_response

        client = StreamHouseClient("http://localhost:8080")
        with pytest.raises(NotFoundError) as exc_info:
            client.get_topic("nonexistent")

        assert exc_info.value.status_code == 404

    @patch("streamhouse.client.requests")
    def test_validation_error(self, mock_requests):
        """Test handling 400 errors."""
        mock_response = Mock()
        mock_response.status_code = 400
        mock_response.text = '{"error": "invalid partitions"}'
        mock_requests.request.return_value = mock_response

        client = StreamHouseClient("http://localhost:8080")
        with pytest.raises(ValidationError) as exc_info:
            client.create_topic("test", 0)

        assert exc_info.value.status_code == 400

    @patch("streamhouse.client.requests")
    def test_conflict_error(self, mock_requests):
        """Test handling 409 errors."""
        mock_response = Mock()
        mock_response.status_code = 409
        mock_response.text = '{"error": "topic already exists"}'
        mock_requests.request.return_value = mock_response

        client = StreamHouseClient("http://localhost:8080")
        with pytest.raises(ConflictError) as exc_info:
            client.create_topic("existing", 3)

        assert exc_info.value.status_code == 409

    @patch("streamhouse.client.requests")
    def test_server_error(self, mock_requests):
        """Test handling 5xx errors."""
        mock_response = Mock()
        mock_response.status_code = 500
        mock_response.text = '{"error": "internal server error"}'
        mock_requests.request.return_value = mock_response

        client = StreamHouseClient("http://localhost:8080")
        with pytest.raises(ServerError) as exc_info:
            client.list_topics()

        assert exc_info.value.status_code == 500


class TestAuthentication:
    """Tests for authentication."""

    @patch("streamhouse.client.requests")
    def test_api_key_header(self, mock_requests):
        """Test that API key is sent in header."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = []
        mock_response.text = "[]"
        mock_requests.request.return_value = mock_response

        client = StreamHouseClient("http://localhost:8080", api_key="test-api-key")
        client.list_topics()

        call_kwargs = mock_requests.request.call_args
        assert call_kwargs[1]["headers"]["Authorization"] == "Bearer test-api-key"


class TestHealthCheck:
    """Tests for health check."""

    @patch("streamhouse.client.requests")
    def test_health_check_success(self, mock_requests):
        """Test successful health check."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_requests.get.return_value = mock_response

        client = StreamHouseClient("http://localhost:8080")
        assert client.health_check() is True

    @patch("streamhouse.client.requests")
    def test_health_check_failure(self, mock_requests):
        """Test failed health check."""
        mock_response = Mock()
        mock_response.status_code = 503
        mock_requests.get.return_value = mock_response

        client = StreamHouseClient("http://localhost:8080")
        assert client.health_check() is False


class TestModels:
    """Tests for data models."""

    def test_topic_from_dict(self):
        """Test Topic.from_dict()."""
        data = {
            "name": "my-topic",
            "partitions": 3,
            "replication_factor": 1,
            "created_at": "2024-01-01",
            "message_count": 100,
            "size_bytes": 1024,
        }
        topic = Topic.from_dict(data)
        assert topic.name == "my-topic"
        assert topic.partitions == 3
        assert topic.message_count == 100

    def test_produce_result_from_dict(self):
        """Test ProduceResult.from_dict()."""
        data = {"offset": 42, "partition": 0}
        result = ProduceResult.from_dict(data)
        assert result.offset == 42
        assert result.partition == 0

    def test_sql_result_from_dict(self):
        """Test SqlResult.from_dict()."""
        data = {
            "columns": [{"name": "col1", "data_type": "string"}],
            "rows": [["value1"]],
            "row_count": 1,
            "execution_time_ms": 50,
            "truncated": False,
        }
        result = SqlResult.from_dict(data)
        assert result.row_count == 1
        assert result.columns[0].name == "col1"
