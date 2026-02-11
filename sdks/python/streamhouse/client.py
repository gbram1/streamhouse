"""StreamHouse Python Client.

Provides both synchronous and asynchronous APIs for interacting with StreamHouse.
"""

import json
from typing import Any, Dict, List, Optional, Union
from urllib.parse import urljoin

try:
    import aiohttp
    HAS_AIOHTTP = True
except ImportError:
    HAS_AIOHTTP = False

try:
    import requests
    HAS_REQUESTS = True
except ImportError:
    HAS_REQUESTS = False

from .models import (
    Topic,
    Partition,
    ProduceResult,
    BatchProduceResult,
    BatchRecord,
    ConsumedRecord,
    ConsumeResult,
    ConsumerGroup,
    ConsumerGroupDetail,
    Agent,
    SqlResult,
    MetricsSnapshot,
)
from .exceptions import (
    StreamHouseError,
    ConnectionError,
    NotFoundError,
    ValidationError,
    TimeoutError,
    ConflictError,
    AuthenticationError,
    ServerError,
)


class StreamHouseClient:
    """Client for interacting with StreamHouse streaming data platform.

    Supports both synchronous (using requests) and asynchronous (using aiohttp)
    operations.

    Example:
        # Synchronous usage
        client = StreamHouseClient("http://localhost:8080")
        topics = client.list_topics()

        # Asynchronous usage
        async with StreamHouseClient("http://localhost:8080") as client:
            topics = await client.list_topics_async()

        # With API key authentication
        client = StreamHouseClient(
            "http://localhost:8080",
            api_key="sk_live_xxx"
        )
    """

    def __init__(
        self,
        base_url: str,
        api_key: Optional[str] = None,
        timeout: float = 30.0,
    ):
        """Initialize the StreamHouse client.

        Args:
            base_url: Base URL of the StreamHouse server (e.g., "http://localhost:8080")
            api_key: Optional API key for authentication
            timeout: Request timeout in seconds (default: 30)
        """
        self.base_url = base_url.rstrip("/")
        self.api_url = f"{self.base_url}/api/v1"
        self.api_key = api_key
        self.timeout = timeout
        self._session: Optional[aiohttp.ClientSession] = None

    def _get_headers(self) -> Dict[str, str]:
        """Get request headers including authentication."""
        headers = {"Content-Type": "application/json"}
        if self.api_key:
            headers["Authorization"] = f"Bearer {self.api_key}"
        return headers

    def _handle_response_error(self, status_code: int, body: str) -> None:
        """Handle HTTP error responses."""
        try:
            error_data = json.loads(body)
            message = error_data.get("error", body)
        except json.JSONDecodeError:
            message = body

        if status_code == 400:
            raise ValidationError(message, status_code)
        elif status_code == 401:
            raise AuthenticationError(message, status_code)
        elif status_code == 404:
            raise NotFoundError(message, status_code)
        elif status_code == 408:
            raise TimeoutError(message, status_code)
        elif status_code == 409:
            raise ConflictError(message, status_code)
        elif status_code >= 500:
            raise ServerError(message, status_code)
        else:
            raise StreamHouseError(message, status_code)

    # =========================================================================
    # Async context manager
    # =========================================================================

    async def __aenter__(self) -> "StreamHouseClient":
        """Enter async context."""
        if not HAS_AIOHTTP:
            raise ImportError("aiohttp is required for async support. Install with: pip install aiohttp")
        self._session = aiohttp.ClientSession(
            headers=self._get_headers(),
            timeout=aiohttp.ClientTimeout(total=self.timeout),
        )
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb) -> None:
        """Exit async context."""
        if self._session:
            await self._session.close()
            self._session = None

    async def _async_request(
        self,
        method: str,
        path: str,
        json_data: Optional[Dict[str, Any]] = None,
        params: Optional[Dict[str, Any]] = None,
    ) -> Any:
        """Make an async HTTP request."""
        if not self._session:
            raise RuntimeError("Client not initialized. Use 'async with' context manager.")

        url = f"{self.api_url}{path}"
        try:
            async with self._session.request(
                method, url, json=json_data, params=params
            ) as response:
                body = await response.text()
                if response.status >= 400:
                    self._handle_response_error(response.status, body)
                if response.status == 204:
                    return None
                return json.loads(body) if body else None
        except aiohttp.ClientError as e:
            raise ConnectionError(f"Connection failed: {e}")

    # =========================================================================
    # Sync methods using requests
    # =========================================================================

    def _sync_request(
        self,
        method: str,
        path: str,
        json_data: Optional[Dict[str, Any]] = None,
        params: Optional[Dict[str, Any]] = None,
    ) -> Any:
        """Make a synchronous HTTP request."""
        if not HAS_REQUESTS:
            raise ImportError("requests is required for sync support. Install with: pip install requests")

        url = f"{self.api_url}{path}"
        try:
            response = requests.request(
                method,
                url,
                headers=self._get_headers(),
                json=json_data,
                params=params,
                timeout=self.timeout,
            )
        except requests.exceptions.RequestException as e:
            raise ConnectionError(f"Connection failed: {e}")
        if response.status_code >= 400:
            self._handle_response_error(response.status_code, response.text)
        if response.status_code == 204:
            return None
        return response.json() if response.text else None

    # =========================================================================
    # Topic Operations
    # =========================================================================

    def list_topics(self) -> List[Topic]:
        """List all topics (sync)."""
        data = self._sync_request("GET", "/topics")
        return [Topic.from_dict(t) for t in data]

    async def list_topics_async(self) -> List[Topic]:
        """List all topics (async)."""
        data = await self._async_request("GET", "/topics")
        return [Topic.from_dict(t) for t in data]

    def create_topic(
        self,
        name: str,
        partitions: int,
        replication_factor: int = 1,
    ) -> Topic:
        """Create a new topic (sync)."""
        data = self._sync_request(
            "POST",
            "/topics",
            json_data={
                "name": name,
                "partitions": partitions,
                "replication_factor": replication_factor,
            },
        )
        return Topic.from_dict(data)

    async def create_topic_async(
        self,
        name: str,
        partitions: int,
        replication_factor: int = 1,
    ) -> Topic:
        """Create a new topic (async)."""
        data = await self._async_request(
            "POST",
            "/topics",
            json_data={
                "name": name,
                "partitions": partitions,
                "replication_factor": replication_factor,
            },
        )
        return Topic.from_dict(data)

    def get_topic(self, name: str) -> Topic:
        """Get topic details (sync)."""
        data = self._sync_request("GET", f"/topics/{name}")
        return Topic.from_dict(data)

    async def get_topic_async(self, name: str) -> Topic:
        """Get topic details (async)."""
        data = await self._async_request("GET", f"/topics/{name}")
        return Topic.from_dict(data)

    def delete_topic(self, name: str) -> None:
        """Delete a topic (sync)."""
        self._sync_request("DELETE", f"/topics/{name}")

    async def delete_topic_async(self, name: str) -> None:
        """Delete a topic (async)."""
        await self._async_request("DELETE", f"/topics/{name}")

    def list_partitions(self, topic: str) -> List[Partition]:
        """List partitions for a topic (sync)."""
        data = self._sync_request("GET", f"/topics/{topic}/partitions")
        return [Partition.from_dict(p) for p in data]

    async def list_partitions_async(self, topic: str) -> List[Partition]:
        """List partitions for a topic (async)."""
        data = await self._async_request("GET", f"/topics/{topic}/partitions")
        return [Partition.from_dict(p) for p in data]

    # =========================================================================
    # Producer Operations
    # =========================================================================

    def produce(
        self,
        topic: str,
        value: str,
        key: Optional[str] = None,
        partition: Optional[int] = None,
    ) -> ProduceResult:
        """Produce a single message (sync)."""
        payload: Dict[str, Any] = {"topic": topic, "value": value}
        if key is not None:
            payload["key"] = key
        if partition is not None:
            payload["partition"] = partition

        data = self._sync_request("POST", "/produce", json_data=payload)
        return ProduceResult.from_dict(data)

    async def produce_async(
        self,
        topic: str,
        value: str,
        key: Optional[str] = None,
        partition: Optional[int] = None,
    ) -> ProduceResult:
        """Produce a single message (async)."""
        payload: Dict[str, Any] = {"topic": topic, "value": value}
        if key is not None:
            payload["key"] = key
        if partition is not None:
            payload["partition"] = partition

        data = await self._async_request("POST", "/produce", json_data=payload)
        return ProduceResult.from_dict(data)

    def produce_batch(
        self,
        topic: str,
        records: List[Union[str, Dict[str, Any], BatchRecord]],
    ) -> BatchProduceResult:
        """Produce a batch of messages (sync).

        Args:
            topic: Target topic name
            records: List of records. Each can be:
                - A string (used as value)
                - A dict with 'value', optional 'key', optional 'partition'
                - A BatchRecord object
        """
        formatted_records = []
        for r in records:
            if isinstance(r, str):
                formatted_records.append({"value": r})
            elif isinstance(r, BatchRecord):
                formatted_records.append(r.to_dict())
            else:
                formatted_records.append(r)

        data = self._sync_request(
            "POST",
            "/produce/batch",
            json_data={"topic": topic, "records": formatted_records},
        )
        return BatchProduceResult.from_dict(data)

    async def produce_batch_async(
        self,
        topic: str,
        records: List[Union[str, Dict[str, Any], BatchRecord]],
    ) -> BatchProduceResult:
        """Produce a batch of messages (async)."""
        formatted_records = []
        for r in records:
            if isinstance(r, str):
                formatted_records.append({"value": r})
            elif isinstance(r, BatchRecord):
                formatted_records.append(r.to_dict())
            else:
                formatted_records.append(r)

        data = await self._async_request(
            "POST",
            "/produce/batch",
            json_data={"topic": topic, "records": formatted_records},
        )
        return BatchProduceResult.from_dict(data)

    # =========================================================================
    # Consumer Operations
    # =========================================================================

    def consume(
        self,
        topic: str,
        partition: int,
        offset: int = 0,
        max_records: int = 100,
    ) -> ConsumeResult:
        """Consume messages from a partition (sync)."""
        data = self._sync_request(
            "GET",
            "/consume",
            params={
                "topic": topic,
                "partition": partition,
                "offset": offset,
                "maxRecords": max_records,
            },
        )
        return ConsumeResult.from_dict(data)

    async def consume_async(
        self,
        topic: str,
        partition: int,
        offset: int = 0,
        max_records: int = 100,
    ) -> ConsumeResult:
        """Consume messages from a partition (async)."""
        data = await self._async_request(
            "GET",
            "/consume",
            params={
                "topic": topic,
                "partition": partition,
                "offset": offset,
                "maxRecords": max_records,
            },
        )
        return ConsumeResult.from_dict(data)

    # =========================================================================
    # Consumer Group Operations
    # =========================================================================

    def list_consumer_groups(self) -> List[ConsumerGroup]:
        """List all consumer groups (sync)."""
        data = self._sync_request("GET", "/consumer-groups")
        return [ConsumerGroup.from_dict(g) for g in data]

    async def list_consumer_groups_async(self) -> List[ConsumerGroup]:
        """List all consumer groups (async)."""
        data = await self._async_request("GET", "/consumer-groups")
        return [ConsumerGroup.from_dict(g) for g in data]

    def get_consumer_group(self, group_id: str) -> ConsumerGroupDetail:
        """Get consumer group details (sync)."""
        data = self._sync_request("GET", f"/consumer-groups/{group_id}")
        return ConsumerGroupDetail.from_dict(data)

    async def get_consumer_group_async(self, group_id: str) -> ConsumerGroupDetail:
        """Get consumer group details (async)."""
        data = await self._async_request("GET", f"/consumer-groups/{group_id}")
        return ConsumerGroupDetail.from_dict(data)

    def commit_offset(
        self,
        group_id: str,
        topic: str,
        partition: int,
        offset: int,
    ) -> bool:
        """Commit a consumer offset (sync)."""
        data = self._sync_request(
            "POST",
            "/consumer-groups/commit",
            json_data={
                "group_id": group_id,
                "topic": topic,
                "partition": partition,
                "offset": offset,
            },
        )
        return data.get("success", False)

    async def commit_offset_async(
        self,
        group_id: str,
        topic: str,
        partition: int,
        offset: int,
    ) -> bool:
        """Commit a consumer offset (async)."""
        data = await self._async_request(
            "POST",
            "/consumer-groups/commit",
            json_data={
                "group_id": group_id,
                "topic": topic,
                "partition": partition,
                "offset": offset,
            },
        )
        return data.get("success", False)

    def delete_consumer_group(self, group_id: str) -> bool:
        """Delete a consumer group (sync)."""
        data = self._sync_request("DELETE", f"/consumer-groups/{group_id}")
        return data.get("success", False) if data else True

    async def delete_consumer_group_async(self, group_id: str) -> bool:
        """Delete a consumer group (async)."""
        data = await self._async_request("DELETE", f"/consumer-groups/{group_id}")
        return data.get("success", False) if data else True

    # =========================================================================
    # SQL Operations
    # =========================================================================

    def query(
        self,
        sql: str,
        timeout_ms: Optional[int] = None,
    ) -> SqlResult:
        """Execute a SQL query (sync)."""
        payload: Dict[str, Any] = {"query": sql}
        if timeout_ms is not None:
            payload["timeout_ms"] = timeout_ms

        data = self._sync_request("POST", "/sql", json_data=payload)
        return SqlResult.from_dict(data)

    async def query_async(
        self,
        sql: str,
        timeout_ms: Optional[int] = None,
    ) -> SqlResult:
        """Execute a SQL query (async)."""
        payload: Dict[str, Any] = {"query": sql}
        if timeout_ms is not None:
            payload["timeout_ms"] = timeout_ms

        data = await self._async_request("POST", "/sql", json_data=payload)
        return SqlResult.from_dict(data)

    # =========================================================================
    # Agent/Cluster Operations
    # =========================================================================

    def list_agents(self) -> List[Agent]:
        """List all agents (sync)."""
        data = self._sync_request("GET", "/agents")
        return [Agent.from_dict(a) for a in data]

    async def list_agents_async(self) -> List[Agent]:
        """List all agents (async)."""
        data = await self._async_request("GET", "/agents")
        return [Agent.from_dict(a) for a in data]

    def get_metrics(self) -> MetricsSnapshot:
        """Get cluster metrics (sync)."""
        data = self._sync_request("GET", "/metrics")
        return MetricsSnapshot.from_dict(data)

    async def get_metrics_async(self) -> MetricsSnapshot:
        """Get cluster metrics (async)."""
        data = await self._async_request("GET", "/metrics")
        return MetricsSnapshot.from_dict(data)

    # =========================================================================
    # Health Check
    # =========================================================================

    def health_check(self) -> bool:
        """Check if the server is healthy (sync)."""
        try:
            # Health endpoint is at /health, not /api/v1/health
            if not HAS_REQUESTS:
                raise ImportError("requests is required")
            response = requests.get(
                f"{self.base_url}/health",
                timeout=self.timeout,
            )
            return response.status_code == 200
        except Exception:
            return False

    async def health_check_async(self) -> bool:
        """Check if the server is healthy (async)."""
        try:
            if not self._session:
                raise RuntimeError("Client not initialized")
            async with self._session.get(f"{self.base_url}/health") as response:
                return response.status == 200
        except Exception:
            return False
