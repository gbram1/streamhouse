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
    InitProducerId = 22,
    AddPartitionsToTxn = 24,
    AddOffsetsToTxn = 25,
    EndTxn = 26,
    TxnOffsetCommit = 28,
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
            22 => Some(ApiKey::InitProducerId),
            24 => Some(ApiKey::AddPartitionsToTxn),
            25 => Some(ApiKey::AddOffsetsToTxn),
            26 => Some(ApiKey::EndTxn),
            28 => Some(ApiKey::TxnOffsetCommit),
            _ => None,
        }
    }

    pub fn as_i16(self) -> i16 {
        self as i16
    }
}

/// Returns true if the given (api_key, api_version) uses flexible protocol
/// (compact strings + tagged fields in request/response headers).
/// Per Kafka protocol spec, each API has a "flexible versions" threshold.
pub fn is_flexible_version(api_key: i16, api_version: i16) -> bool {
    let flex_start = match api_key {
        0 => 9,   // Produce
        1 => 12,  // Fetch
        2 => 6,   // ListOffsets
        3 => 9,   // Metadata
        8 => 8,   // OffsetCommit
        9 => 6,   // OffsetFetch
        10 => 3,  // FindCoordinator
        11 => 6,  // JoinGroup
        12 => 4,  // Heartbeat
        13 => 4,  // LeaveGroup
        14 => 4,  // SyncGroup
        15 => 5,  // DescribeGroups
        16 => 3,  // ListGroups
        18 => 3,  // ApiVersions
        19 => 5,  // CreateTopics
        20 => 4,  // DeleteTopics
        22 => 2,  // InitProducerId
        24 => 3,  // AddPartitionsToTxn
        25 => 3,  // AddOffsetsToTxn
        26 => 3,  // EndTxn
        28 => 3,  // TxnOffsetCommit
        _ => return false,
    };
    api_version >= flex_start
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
        ApiVersionRange {
            api_key: ApiKey::InitProducerId as i16,
            min_version: 0,
            max_version: 3,
        },
        ApiVersionRange {
            api_key: ApiKey::AddPartitionsToTxn as i16,
            min_version: 0,
            max_version: 3,
        },
        ApiVersionRange {
            api_key: ApiKey::AddOffsetsToTxn as i16,
            min_version: 0,
            max_version: 3,
        },
        ApiVersionRange {
            api_key: ApiKey::EndTxn as i16,
            min_version: 0,
            max_version: 3,
        },
        ApiVersionRange {
            api_key: ApiKey::TxnOffsetCommit as i16,
            min_version: 0,
            max_version: 3,
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

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------
    // ApiKey tests
    // ---------------------------------------------------------------

    #[test]
    fn test_api_key_from_i16_all_valid() {
        let cases: Vec<(i16, ApiKey)> = vec![
            (0, ApiKey::Produce),
            (1, ApiKey::Fetch),
            (2, ApiKey::ListOffsets),
            (3, ApiKey::Metadata),
            (8, ApiKey::OffsetCommit),
            (9, ApiKey::OffsetFetch),
            (10, ApiKey::FindCoordinator),
            (11, ApiKey::JoinGroup),
            (12, ApiKey::Heartbeat),
            (13, ApiKey::LeaveGroup),
            (14, ApiKey::SyncGroup),
            (15, ApiKey::DescribeGroups),
            (16, ApiKey::ListGroups),
            (18, ApiKey::ApiVersions),
            (19, ApiKey::CreateTopics),
            (20, ApiKey::DeleteTopics),
            (22, ApiKey::InitProducerId),
            (24, ApiKey::AddPartitionsToTxn),
            (25, ApiKey::AddOffsetsToTxn),
            (26, ApiKey::EndTxn),
            (28, ApiKey::TxnOffsetCommit),
        ];

        for (raw, expected) in cases {
            let parsed = ApiKey::from_i16(raw);
            assert_eq!(
                parsed,
                Some(expected),
                "from_i16({}) should return {:?}",
                raw,
                expected
            );
        }
    }

    #[test]
    fn test_api_key_from_i16_invalid_values() {
        // Gaps in the enum
        for invalid in [4, 5, 6, 7, 17, 21, 23, 27] {
            assert_eq!(
                ApiKey::from_i16(invalid),
                None,
                "from_i16({}) should be None",
                invalid
            );
        }
        // Negative values
        assert_eq!(ApiKey::from_i16(-1), None);
        assert_eq!(ApiKey::from_i16(i16::MIN), None);
        // Out of range positive
        assert_eq!(ApiKey::from_i16(29), None);
        assert_eq!(ApiKey::from_i16(100), None);
        assert_eq!(ApiKey::from_i16(i16::MAX), None);
    }

    #[test]
    fn test_api_key_as_i16_roundtrip() {
        let keys = vec![
            ApiKey::Produce,
            ApiKey::Fetch,
            ApiKey::ListOffsets,
            ApiKey::Metadata,
            ApiKey::OffsetCommit,
            ApiKey::OffsetFetch,
            ApiKey::FindCoordinator,
            ApiKey::JoinGroup,
            ApiKey::Heartbeat,
            ApiKey::LeaveGroup,
            ApiKey::SyncGroup,
            ApiKey::DescribeGroups,
            ApiKey::ListGroups,
            ApiKey::ApiVersions,
            ApiKey::CreateTopics,
            ApiKey::DeleteTopics,
            ApiKey::InitProducerId,
            ApiKey::AddPartitionsToTxn,
            ApiKey::AddOffsetsToTxn,
            ApiKey::EndTxn,
            ApiKey::TxnOffsetCommit,
        ];

        for key in keys {
            let raw = key.as_i16();
            let recovered = ApiKey::from_i16(raw).unwrap();
            assert_eq!(
                recovered, key,
                "roundtrip failed for {:?} (raw={})",
                key, raw
            );
        }
    }

    #[test]
    fn test_api_key_discriminant_values() {
        assert_eq!(ApiKey::Produce.as_i16(), 0);
        assert_eq!(ApiKey::Fetch.as_i16(), 1);
        assert_eq!(ApiKey::ListOffsets.as_i16(), 2);
        assert_eq!(ApiKey::Metadata.as_i16(), 3);
        assert_eq!(ApiKey::OffsetCommit.as_i16(), 8);
        assert_eq!(ApiKey::OffsetFetch.as_i16(), 9);
        assert_eq!(ApiKey::FindCoordinator.as_i16(), 10);
        assert_eq!(ApiKey::JoinGroup.as_i16(), 11);
        assert_eq!(ApiKey::Heartbeat.as_i16(), 12);
        assert_eq!(ApiKey::LeaveGroup.as_i16(), 13);
        assert_eq!(ApiKey::SyncGroup.as_i16(), 14);
        assert_eq!(ApiKey::DescribeGroups.as_i16(), 15);
        assert_eq!(ApiKey::ListGroups.as_i16(), 16);
        assert_eq!(ApiKey::ApiVersions.as_i16(), 18);
        assert_eq!(ApiKey::CreateTopics.as_i16(), 19);
        assert_eq!(ApiKey::DeleteTopics.as_i16(), 20);
        assert_eq!(ApiKey::InitProducerId.as_i16(), 22);
        assert_eq!(ApiKey::AddPartitionsToTxn.as_i16(), 24);
        assert_eq!(ApiKey::AddOffsetsToTxn.as_i16(), 25);
        assert_eq!(ApiKey::EndTxn.as_i16(), 26);
        assert_eq!(ApiKey::TxnOffsetCommit.as_i16(), 28);
    }

    #[test]
    fn test_api_key_clone_copy() {
        let key = ApiKey::Produce;
        let cloned = key.clone();
        let copied = key;
        assert_eq!(key, cloned);
        assert_eq!(key, copied);
    }

    #[test]
    fn test_api_key_debug_format() {
        let debug_str = format!("{:?}", ApiKey::Produce);
        assert_eq!(debug_str, "Produce");

        let debug_str = format!("{:?}", ApiKey::ApiVersions);
        assert_eq!(debug_str, "ApiVersions");
    }

    #[test]
    fn test_api_key_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(ApiKey::Produce);
        set.insert(ApiKey::Fetch);
        set.insert(ApiKey::Produce); // duplicate
        assert_eq!(set.len(), 2);
    }

    // ---------------------------------------------------------------
    // ApiVersionRange + supported_api_versions tests
    // ---------------------------------------------------------------

    #[test]
    fn test_supported_api_versions_count() {
        let versions = supported_api_versions();
        // Should have one entry per ApiKey variant (21 variants)
        assert_eq!(versions.len(), 21);
    }

    #[test]
    fn test_supported_api_versions_all_valid_api_keys() {
        let versions = supported_api_versions();
        for v in &versions {
            assert!(
                ApiKey::from_i16(v.api_key).is_some(),
                "api_key {} in supported_api_versions is not a valid ApiKey",
                v.api_key,
            );
        }
    }

    #[test]
    fn test_supported_api_versions_min_le_max() {
        let versions = supported_api_versions();
        for v in &versions {
            assert!(
                v.min_version <= v.max_version,
                "api_key {}: min_version ({}) > max_version ({})",
                v.api_key,
                v.min_version,
                v.max_version,
            );
        }
    }

    #[test]
    fn test_supported_api_versions_min_is_zero() {
        let versions = supported_api_versions();
        for v in &versions {
            assert_eq!(
                v.min_version, 0,
                "api_key {}: min_version should be 0 but is {}",
                v.api_key, v.min_version,
            );
        }
    }

    #[test]
    fn test_supported_api_versions_specific_ranges() {
        let versions = supported_api_versions();
        let map: std::collections::HashMap<i16, &ApiVersionRange> =
            versions.iter().map(|v| (v.api_key, v)).collect();

        assert_eq!(map[&0].max_version, 9); // Produce
        assert_eq!(map[&1].max_version, 12); // Fetch
        assert_eq!(map[&2].max_version, 7); // ListOffsets
        assert_eq!(map[&3].max_version, 12); // Metadata
        assert_eq!(map[&18].max_version, 3); // ApiVersions
        assert_eq!(map[&19].max_version, 7); // CreateTopics
        assert_eq!(map[&20].max_version, 6); // DeleteTopics
        assert_eq!(map[&22].max_version, 3); // InitProducerId
        assert_eq!(map[&24].max_version, 3); // AddPartitionsToTxn
        assert_eq!(map[&25].max_version, 3); // AddOffsetsToTxn
        assert_eq!(map[&26].max_version, 3); // EndTxn
        assert_eq!(map[&28].max_version, 3); // TxnOffsetCommit
    }

    #[test]
    fn test_supported_api_versions_no_duplicates() {
        let versions = supported_api_versions();
        let mut seen = std::collections::HashSet::new();
        for v in &versions {
            assert!(
                seen.insert(v.api_key),
                "duplicate api_key {} in supported_api_versions",
                v.api_key,
            );
        }
    }

    // ---------------------------------------------------------------
    // CoordinatorType tests
    // ---------------------------------------------------------------

    #[test]
    fn test_coordinator_type_discriminant() {
        assert_eq!(CoordinatorType::Group as i8, 0);
        assert_eq!(CoordinatorType::Transaction as i8, 1);
    }

    #[test]
    fn test_coordinator_type_equality() {
        assert_eq!(CoordinatorType::Group, CoordinatorType::Group);
        assert_ne!(CoordinatorType::Group, CoordinatorType::Transaction);
    }

    #[test]
    fn test_coordinator_type_clone_copy() {
        let ct = CoordinatorType::Group;
        let cloned = ct.clone();
        let copied = ct;
        assert_eq!(ct, cloned);
        assert_eq!(ct, copied);
    }

    // ---------------------------------------------------------------
    // IsolationLevel tests
    // ---------------------------------------------------------------

    #[test]
    fn test_isolation_level_discriminant() {
        assert_eq!(IsolationLevel::ReadUncommitted as i8, 0);
        assert_eq!(IsolationLevel::ReadCommitted as i8, 1);
    }

    #[test]
    fn test_isolation_level_equality() {
        assert_eq!(
            IsolationLevel::ReadUncommitted,
            IsolationLevel::ReadUncommitted
        );
        assert_ne!(
            IsolationLevel::ReadUncommitted,
            IsolationLevel::ReadCommitted
        );
    }

    // ---------------------------------------------------------------
    // CompressionType tests
    // ---------------------------------------------------------------

    #[test]
    fn test_compression_type_discriminant() {
        assert_eq!(CompressionType::None as i8, 0);
        assert_eq!(CompressionType::Gzip as i8, 1);
        assert_eq!(CompressionType::Snappy as i8, 2);
        assert_eq!(CompressionType::Lz4 as i8, 3);
        assert_eq!(CompressionType::Zstd as i8, 4);
    }

    #[test]
    fn test_compression_type_from_attributes_exact() {
        assert_eq!(CompressionType::from_attributes(0), CompressionType::None);
        assert_eq!(CompressionType::from_attributes(1), CompressionType::Gzip);
        assert_eq!(CompressionType::from_attributes(2), CompressionType::Snappy);
        assert_eq!(CompressionType::from_attributes(3), CompressionType::Lz4);
        assert_eq!(CompressionType::from_attributes(4), CompressionType::Zstd);
    }

    #[test]
    fn test_compression_type_from_attributes_masks_upper_bits() {
        // Only the lower 3 bits (0x07) matter for compression
        // e.g. attributes=0b00001001 => lower 3 bits = 1 => Gzip
        assert_eq!(
            CompressionType::from_attributes(0x09),
            CompressionType::Gzip
        );
        // attributes=0b11111100 => lower 3 bits = 4 => Zstd
        assert_eq!(
            CompressionType::from_attributes(0x0C),
            CompressionType::Zstd
        );
        // attributes=0xFF => lower 3 bits = 7 => out of range => None (default)
        assert_eq!(
            CompressionType::from_attributes(0xFF),
            CompressionType::None
        );
        // attributes=0b00010010 => lower 3 bits = 2 => Snappy
        assert_eq!(
            CompressionType::from_attributes(0x12),
            CompressionType::Snappy
        );
    }

    #[test]
    fn test_compression_type_from_attributes_unknown_falls_to_none() {
        // Lower 3 bits = 5, 6, 7 are not defined compression types
        assert_eq!(CompressionType::from_attributes(5), CompressionType::None);
        assert_eq!(CompressionType::from_attributes(6), CompressionType::None);
        assert_eq!(CompressionType::from_attributes(7), CompressionType::None);
    }

    #[test]
    fn test_compression_type_from_attributes_negative() {
        // In i16, -1 is 0xFFFF; lower 3 bits = 7 => None (default)
        assert_eq!(CompressionType::from_attributes(-1), CompressionType::None);
    }

    // ---------------------------------------------------------------
    // TimestampType tests
    // ---------------------------------------------------------------

    #[test]
    fn test_timestamp_type_discriminant() {
        assert_eq!(TimestampType::CreateTime as i8, 0);
        assert_eq!(TimestampType::LogAppendTime as i8, 1);
    }

    #[test]
    fn test_timestamp_type_from_attributes() {
        // Bit 3 (0x08) determines timestamp type
        assert_eq!(
            TimestampType::from_attributes(0x00),
            TimestampType::CreateTime
        );
        assert_eq!(
            TimestampType::from_attributes(0x08),
            TimestampType::LogAppendTime
        );
        assert_eq!(
            TimestampType::from_attributes(0x07),
            TimestampType::CreateTime
        ); // bits 0-2 set, bit 3 clear
        assert_eq!(
            TimestampType::from_attributes(0x0F),
            TimestampType::LogAppendTime
        ); // bit 3 set
    }

    #[test]
    fn test_timestamp_type_from_attributes_high_bits_irrelevant() {
        // Only bit 3 matters
        assert_eq!(
            TimestampType::from_attributes(0x70),
            TimestampType::CreateTime
        );
        assert_eq!(
            TimestampType::from_attributes(0x78),
            TimestampType::LogAppendTime
        );
    }

    #[test]
    fn test_timestamp_type_combined_with_compression() {
        // Attributes can encode both compression (bits 0-2) and timestamp (bit 3)
        // compression=Lz4(3) + timestamp=LogAppendTime(bit3) => 0b00001011 = 0x0B
        let attrs: i16 = 0x0B;
        assert_eq!(
            CompressionType::from_attributes(attrs),
            CompressionType::Lz4
        );
        assert_eq!(
            TimestampType::from_attributes(attrs),
            TimestampType::LogAppendTime
        );
    }

    // ---------------------------------------------------------------
    // BrokerInfo tests
    // ---------------------------------------------------------------

    #[test]
    fn test_broker_info_default() {
        let broker = BrokerInfo::default();
        assert_eq!(broker.node_id, 0);
        assert_eq!(broker.host, "localhost");
        assert_eq!(broker.port, 9092);
        assert!(broker.rack.is_none());
    }

    #[test]
    fn test_broker_info_custom() {
        let broker = BrokerInfo {
            node_id: 42,
            host: "kafka-broker-42.example.com".to_string(),
            port: 19092,
            rack: Some("us-east-1a".to_string()),
        };
        assert_eq!(broker.node_id, 42);
        assert_eq!(broker.host, "kafka-broker-42.example.com");
        assert_eq!(broker.port, 19092);
        assert_eq!(broker.rack.as_deref(), Some("us-east-1a"));
    }

    #[test]
    fn test_broker_info_clone() {
        let broker = BrokerInfo {
            node_id: 1,
            host: "host".to_string(),
            port: 9092,
            rack: Some("rack-a".to_string()),
        };
        let cloned = broker.clone();
        assert_eq!(cloned.node_id, broker.node_id);
        assert_eq!(cloned.host, broker.host);
        assert_eq!(cloned.port, broker.port);
        assert_eq!(cloned.rack, broker.rack);
    }

    // ---------------------------------------------------------------
    // TopicPartitionInfo tests
    // ---------------------------------------------------------------

    #[test]
    fn test_topic_partition_info_construction() {
        let tp = TopicPartitionInfo {
            topic: "my-topic".to_string(),
            partition: 3,
        };
        assert_eq!(tp.topic, "my-topic");
        assert_eq!(tp.partition, 3);
    }

    #[test]
    fn test_topic_partition_info_clone() {
        let tp = TopicPartitionInfo {
            topic: "t".to_string(),
            partition: 0,
        };
        let cloned = tp.clone();
        assert_eq!(cloned.topic, tp.topic);
        assert_eq!(cloned.partition, tp.partition);
    }

    #[test]
    fn test_topic_partition_info_empty_topic() {
        let tp = TopicPartitionInfo {
            topic: "".to_string(),
            partition: 0,
        };
        assert_eq!(tp.topic, "");
    }

    // ---------------------------------------------------------------
    // GroupMemberInfo tests
    // ---------------------------------------------------------------

    #[test]
    fn test_group_member_info_construction() {
        let member = GroupMemberInfo {
            member_id: "member-1".to_string(),
            client_id: "client-1".to_string(),
            client_host: "/127.0.0.1".to_string(),
            member_metadata: vec![0, 1, 2],
            member_assignment: vec![3, 4, 5],
        };
        assert_eq!(member.member_id, "member-1");
        assert_eq!(member.client_id, "client-1");
        assert_eq!(member.client_host, "/127.0.0.1");
        assert_eq!(member.member_metadata, vec![0, 1, 2]);
        assert_eq!(member.member_assignment, vec![3, 4, 5]);
    }

    #[test]
    fn test_group_member_info_empty_metadata() {
        let member = GroupMemberInfo {
            member_id: "m".to_string(),
            client_id: "c".to_string(),
            client_host: "h".to_string(),
            member_metadata: vec![],
            member_assignment: vec![],
        };
        assert!(member.member_metadata.is_empty());
        assert!(member.member_assignment.is_empty());
    }

    #[test]
    fn test_group_member_info_clone() {
        let member = GroupMemberInfo {
            member_id: "m".to_string(),
            client_id: "c".to_string(),
            client_host: "h".to_string(),
            member_metadata: vec![10, 20],
            member_assignment: vec![30],
        };
        let cloned = member.clone();
        assert_eq!(cloned.member_id, member.member_id);
        assert_eq!(cloned.member_metadata, member.member_metadata);
        assert_eq!(cloned.member_assignment, member.member_assignment);
    }

    // ---------------------------------------------------------------
    // Protocol constants tests
    // ---------------------------------------------------------------

    #[test]
    fn test_protocol_constants() {
        assert_eq!(CONSUMER_PROTOCOL_TYPE, "consumer");
        assert_eq!(RANGE_ASSIGNOR, "range");
        assert_eq!(ROUNDROBIN_ASSIGNOR, "roundrobin");
        assert_eq!(COOPERATIVE_STICKY_ASSIGNOR, "cooperative-sticky");
    }
}
