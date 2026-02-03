//! Kafka protocol types and constants

/// Kafka API keys
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i16)]
pub enum ApiKey {
    Produce = 0,
    Fetch = 1,
    ListOffsets = 2,
    Metadata = 3,
    OffsetCommit = 8,
    OffsetFetch = 9,
    FindCoordinator = 10,
    JoinGroup = 11,
    Heartbeat = 12,
    LeaveGroup = 13,
    SyncGroup = 14,
    DescribeGroups = 15,
    ListGroups = 16,
    ApiVersions = 18,
    CreateTopics = 19,
    DeleteTopics = 20,
}

impl ApiKey {
    pub fn from_i16(key: i16) -> Option<Self> {
        match key {
            0 => Some(ApiKey::Produce),
            1 => Some(ApiKey::Fetch),
            2 => Some(ApiKey::ListOffsets),
            3 => Some(ApiKey::Metadata),
            8 => Some(ApiKey::OffsetCommit),
            9 => Some(ApiKey::OffsetFetch),
            10 => Some(ApiKey::FindCoordinator),
            11 => Some(ApiKey::JoinGroup),
            12 => Some(ApiKey::Heartbeat),
            13 => Some(ApiKey::LeaveGroup),
            14 => Some(ApiKey::SyncGroup),
            15 => Some(ApiKey::DescribeGroups),
            16 => Some(ApiKey::ListGroups),
            18 => Some(ApiKey::ApiVersions),
            19 => Some(ApiKey::CreateTopics),
            20 => Some(ApiKey::DeleteTopics),
            _ => None,
        }
    }

    pub fn as_i16(self) -> i16 {
        self as i16
    }
}

/// Supported API versions for each API key
pub struct ApiVersionRange {
    pub api_key: i16,
    pub min_version: i16,
    pub max_version: i16,
}

/// Returns the supported API versions for StreamHouse
pub fn supported_api_versions() -> Vec<ApiVersionRange> {
    vec![
        ApiVersionRange {
            api_key: ApiKey::Produce as i16,
            min_version: 0,
            max_version: 9,
        },
        ApiVersionRange {
            api_key: ApiKey::Fetch as i16,
            min_version: 0,
            max_version: 12,
        },
        ApiVersionRange {
            api_key: ApiKey::ListOffsets as i16,
            min_version: 0,
            max_version: 7,
        },
        ApiVersionRange {
            api_key: ApiKey::Metadata as i16,
            min_version: 0,
            max_version: 12,
        },
        ApiVersionRange {
            api_key: ApiKey::OffsetCommit as i16,
            min_version: 0,
            max_version: 8,
        },
        ApiVersionRange {
            api_key: ApiKey::OffsetFetch as i16,
            min_version: 0,
            max_version: 8,
        },
        ApiVersionRange {
            api_key: ApiKey::FindCoordinator as i16,
            min_version: 0,
            max_version: 4,
        },
        ApiVersionRange {
            api_key: ApiKey::JoinGroup as i16,
            min_version: 0,
            max_version: 9,
        },
        ApiVersionRange {
            api_key: ApiKey::Heartbeat as i16,
            min_version: 0,
            max_version: 4,
        },
        ApiVersionRange {
            api_key: ApiKey::LeaveGroup as i16,
            min_version: 0,
            max_version: 5,
        },
        ApiVersionRange {
            api_key: ApiKey::SyncGroup as i16,
            min_version: 0,
            max_version: 5,
        },
        ApiVersionRange {
            api_key: ApiKey::DescribeGroups as i16,
            min_version: 0,
            max_version: 5,
        },
        ApiVersionRange {
            api_key: ApiKey::ListGroups as i16,
            min_version: 0,
            max_version: 4,
        },
        ApiVersionRange {
            api_key: ApiKey::ApiVersions as i16,
            min_version: 0,
            max_version: 3,
        },
        ApiVersionRange {
            api_key: ApiKey::CreateTopics as i16,
            min_version: 0,
            max_version: 7,
        },
        ApiVersionRange {
            api_key: ApiKey::DeleteTopics as i16,
            min_version: 0,
            max_version: 6,
        },
    ]
}

/// Coordinator types for FindCoordinator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum CoordinatorType {
    Group = 0,
    Transaction = 1,
}

/// Isolation levels for Fetch requests
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum IsolationLevel {
    ReadUncommitted = 0,
    ReadCommitted = 1,
}

/// Compression types for records
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum CompressionType {
    None = 0,
    Gzip = 1,
    Snappy = 2,
    Lz4 = 3,
    Zstd = 4,
}

impl CompressionType {
    pub fn from_attributes(attributes: i16) -> Self {
        match attributes & 0x07 {
            0 => CompressionType::None,
            1 => CompressionType::Gzip,
            2 => CompressionType::Snappy,
            3 => CompressionType::Lz4,
            4 => CompressionType::Zstd,
            _ => CompressionType::None,
        }
    }
}

/// Timestamp types for records
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum TimestampType {
    CreateTime = 0,
    LogAppendTime = 1,
}

impl TimestampType {
    pub fn from_attributes(attributes: i16) -> Self {
        if attributes & 0x08 != 0 {
            TimestampType::LogAppendTime
        } else {
            TimestampType::CreateTime
        }
    }
}

/// Broker node information
#[derive(Debug, Clone)]
pub struct BrokerInfo {
    pub node_id: i32,
    pub host: String,
    pub port: i32,
    pub rack: Option<String>,
}

impl Default for BrokerInfo {
    fn default() -> Self {
        Self {
            node_id: 0,
            host: "localhost".to_string(),
            port: 9092,
            rack: None,
        }
    }
}

/// Topic partition information
#[derive(Debug, Clone)]
pub struct TopicPartitionInfo {
    pub topic: String,
    pub partition: i32,
}

/// Consumer group member information
#[derive(Debug, Clone)]
pub struct GroupMemberInfo {
    pub member_id: String,
    pub client_id: String,
    pub client_host: String,
    pub member_metadata: Vec<u8>,
    pub member_assignment: Vec<u8>,
}

/// Consumer group protocol
pub const CONSUMER_PROTOCOL_TYPE: &str = "consumer";

/// Range assignor protocol name
pub const RANGE_ASSIGNOR: &str = "range";

/// Round-robin assignor protocol name
pub const ROUNDROBIN_ASSIGNOR: &str = "roundrobin";

/// Cooperative sticky assignor protocol name
pub const COOPERATIVE_STICKY_ASSIGNOR: &str = "cooperative-sticky";
