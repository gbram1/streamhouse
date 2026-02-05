"""Data models for StreamHouse Python client."""

from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional
from datetime import datetime


@dataclass
class Topic:
    """Represents a StreamHouse topic."""
    name: str
    partitions: int
    replication_factor: int
    created_at: str
    message_count: int = 0
    size_bytes: int = 0

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Topic":
        return cls(
            name=data["name"],
            partitions=data["partitions"],
            replication_factor=data.get("replication_factor", 1),
            created_at=data.get("created_at", ""),
            message_count=data.get("message_count", 0),
            size_bytes=data.get("size_bytes", 0),
        )


@dataclass
class Partition:
    """Represents a topic partition."""
    topic: str
    partition_id: int
    leader_agent_id: Optional[str]
    high_watermark: int
    low_watermark: int

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Partition":
        return cls(
            topic=data["topic"],
            partition_id=data["partition_id"],
            leader_agent_id=data.get("leader_agent_id"),
            high_watermark=data.get("high_watermark", 0),
            low_watermark=data.get("low_watermark", 0),
        )


@dataclass
class ProduceResult:
    """Result of producing a message."""
    offset: int
    partition: int

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "ProduceResult":
        return cls(
            offset=data["offset"],
            partition=data["partition"],
        )


@dataclass
class BatchProduceResult:
    """Result of producing a batch of messages."""
    count: int
    offsets: List["BatchRecordResult"]

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "BatchProduceResult":
        return cls(
            count=data["count"],
            offsets=[BatchRecordResult.from_dict(o) for o in data.get("offsets", [])],
        )


@dataclass
class BatchRecordResult:
    """Result of a single record in a batch produce."""
    partition: int
    offset: int

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "BatchRecordResult":
        return cls(
            partition=data["partition"],
            offset=data["offset"],
        )


@dataclass
class ConsumedRecord:
    """A consumed message record."""
    partition: int
    offset: int
    key: Optional[str]
    value: str
    timestamp: int

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "ConsumedRecord":
        return cls(
            partition=data["partition"],
            offset=data["offset"],
            key=data.get("key"),
            value=data["value"],
            timestamp=data.get("timestamp", 0),
        )


@dataclass
class ConsumeResult:
    """Result of consuming messages."""
    records: List[ConsumedRecord]
    next_offset: int

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "ConsumeResult":
        return cls(
            records=[ConsumedRecord.from_dict(r) for r in data.get("records", [])],
            next_offset=data.get("next_offset", 0),
        )


@dataclass
class ConsumerOffset:
    """Consumer group offset for a partition."""
    topic: str
    partition_id: int
    committed_offset: int
    high_watermark: int
    lag: int

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "ConsumerOffset":
        return cls(
            topic=data["topic"],
            partition_id=data["partition_id"],
            committed_offset=data.get("committed_offset", 0),
            high_watermark=data.get("high_watermark", 0),
            lag=data.get("lag", 0),
        )


@dataclass
class ConsumerGroup:
    """Summary of a consumer group."""
    group_id: str
    topics: List[str]
    total_lag: int
    partition_count: int

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "ConsumerGroup":
        return cls(
            group_id=data["group_id"],
            topics=data.get("topics", []),
            total_lag=data.get("total_lag", 0),
            partition_count=data.get("partition_count", 0),
        )


@dataclass
class ConsumerGroupDetail:
    """Detailed consumer group information."""
    group_id: str
    offsets: List[ConsumerOffset]

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "ConsumerGroupDetail":
        return cls(
            group_id=data["group_id"],
            offsets=[ConsumerOffset.from_dict(o) for o in data.get("offsets", [])],
        )


@dataclass
class Agent:
    """Represents a StreamHouse agent/server."""
    agent_id: str
    address: str
    availability_zone: str
    agent_group: str
    last_heartbeat: int
    started_at: int
    active_leases: int

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Agent":
        return cls(
            agent_id=data["agent_id"],
            address=data.get("address", ""),
            availability_zone=data.get("availability_zone", ""),
            agent_group=data.get("agent_group", ""),
            last_heartbeat=data.get("last_heartbeat", 0),
            started_at=data.get("started_at", 0),
            active_leases=data.get("active_leases", 0),
        )


@dataclass
class ColumnInfo:
    """Column metadata for SQL results."""
    name: str
    data_type: str

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "ColumnInfo":
        return cls(
            name=data["name"],
            data_type=data.get("data_type", "string"),
        )


@dataclass
class SqlResult:
    """Result of a SQL query."""
    columns: List[ColumnInfo]
    rows: List[List[Any]]
    row_count: int
    execution_time_ms: int
    truncated: bool

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "SqlResult":
        return cls(
            columns=[ColumnInfo.from_dict(c) for c in data.get("columns", [])],
            rows=data.get("rows", []),
            row_count=data.get("row_count", 0),
            execution_time_ms=data.get("execution_time_ms", 0),
            truncated=data.get("truncated", False),
        )


@dataclass
class MetricsSnapshot:
    """Cluster metrics snapshot."""
    topics_count: int
    agents_count: int
    partitions_count: int
    total_messages: int

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "MetricsSnapshot":
        return cls(
            topics_count=data.get("topics_count", 0),
            agents_count=data.get("agents_count", 0),
            partitions_count=data.get("partitions_count", 0),
            total_messages=data.get("total_messages", 0),
        )


@dataclass
class CreateTopicRequest:
    """Request to create a new topic."""
    name: str
    partitions: int
    replication_factor: int = 1

    def to_dict(self) -> Dict[str, Any]:
        return {
            "name": self.name,
            "partitions": self.partitions,
            "replication_factor": self.replication_factor,
        }


@dataclass
class ProduceRequest:
    """Request to produce a message."""
    topic: str
    value: str
    key: Optional[str] = None
    partition: Optional[int] = None

    def to_dict(self) -> Dict[str, Any]:
        data: Dict[str, Any] = {
            "topic": self.topic,
            "value": self.value,
        }
        if self.key is not None:
            data["key"] = self.key
        if self.partition is not None:
            data["partition"] = self.partition
        return data


@dataclass
class BatchRecord:
    """A single record in a batch produce request."""
    value: str
    key: Optional[str] = None
    partition: Optional[int] = None

    def to_dict(self) -> Dict[str, Any]:
        data: Dict[str, Any] = {"value": self.value}
        if self.key is not None:
            data["key"] = self.key
        if self.partition is not None:
            data["partition"] = self.partition
        return data


@dataclass
class CommitOffsetRequest:
    """Request to commit a consumer offset."""
    group_id: str
    topic: str
    partition: int
    offset: int

    def to_dict(self) -> Dict[str, Any]:
        return {
            "group_id": self.group_id,
            "topic": self.topic,
            "partition": self.partition,
            "offset": self.offset,
        }
