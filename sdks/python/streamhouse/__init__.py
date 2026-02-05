"""StreamHouse Python Client SDK

A Python client for interacting with StreamHouse streaming data platform.

Example usage:
    from streamhouse import StreamHouseClient

    # Sync usage
    client = StreamHouseClient("http://localhost:8080")
    topics = client.list_topics()

    # Async usage
    async with StreamHouseClient("http://localhost:8080") as client:
        topics = await client.list_topics_async()
"""

from .client import StreamHouseClient
from .models import (
    Topic,
    Partition,
    ProduceResult,
    ConsumedRecord,
    ConsumeResult,
    ConsumerGroup,
    ConsumerGroupDetail,
    ConsumerOffset,
    Agent,
    SqlResult,
    ColumnInfo,
)
from .exceptions import (
    StreamHouseError,
    ConnectionError,
    NotFoundError,
    ValidationError,
    TimeoutError,
    ConflictError,
)

__version__ = "0.1.0"
__all__ = [
    "StreamHouseClient",
    "Topic",
    "Partition",
    "ProduceResult",
    "ConsumedRecord",
    "ConsumeResult",
    "ConsumerGroup",
    "ConsumerGroupDetail",
    "ConsumerOffset",
    "Agent",
    "SqlResult",
    "ColumnInfo",
    "StreamHouseError",
    "ConnectionError",
    "NotFoundError",
    "ValidationError",
    "TimeoutError",
    "ConflictError",
]
