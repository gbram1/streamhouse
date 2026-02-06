//! Kafka protocol error handling
//!
//! Maps between StreamHouse errors and Kafka protocol error codes.

use thiserror::Error;

/// Result type for Kafka operations
pub type KafkaResult<T> = Result<T, KafkaError>;

/// Kafka protocol errors
#[derive(Debug, Error)]
pub enum KafkaError {
    #[error("Unknown error: {0}")]
    Unknown(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Topic not found: {0}")]
    TopicNotFound(String),

    #[error("Partition not found: topic={0}, partition={1}")]
    PartitionNotFound(String, u32),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Unsupported API: key={0}, version={1}")]
    UnsupportedApi(i16, i16),

    #[error("Consumer group error: {0}")]
    ConsumerGroup(String),

    #[error("Not coordinator for group: {0}")]
    NotCoordinator(String),

    #[error("Rebalance in progress for group: {0}")]
    RebalanceInProgress(String),

    #[error("Member not found: group={0}, member={1}")]
    MemberNotFound(String, String),

    #[error("Invalid generation: expected={0}, got={1}")]
    InvalidGeneration(i32, i32),

    #[error("Offset out of range: {0}")]
    OffsetOutOfRange(u64),

    #[error("Metadata store error: {0}")]
    MetadataStore(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Compression error: {0}")]
    Compression(String),

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Authentication error: {0}")]
    AuthenticationError(String),

    #[error("Authorization error: {0}")]
    AuthorizationError(String),
}

/// Kafka protocol error codes
/// See: https://kafka.apache.org/protocol#protocol_error_codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i16)]
pub enum ErrorCode {
    None = 0,
    UnknownServerError = -1,
    OffsetOutOfRange = 1,
    CorruptMessage = 2,
    UnknownTopicOrPartition = 3,
    InvalidFetchSize = 4,
    LeaderNotAvailable = 5,
    NotLeaderOrFollower = 6,
    RequestTimedOut = 7,
    BrokerNotAvailable = 8,
    ReplicaNotAvailable = 9,
    MessageTooLarge = 10,
    StaleControllerEpoch = 11,
    OffsetMetadataTooLarge = 12,
    NetworkException = 13,
    CoordinatorLoadInProgress = 14,
    CoordinatorNotAvailable = 15,
    NotCoordinator = 16,
    InvalidTopicException = 17,
    RecordListTooLarge = 18,
    NotEnoughReplicas = 19,
    NotEnoughReplicasAfterAppend = 20,
    InvalidRequiredAcks = 21,
    IllegalGeneration = 22,
    InconsistentGroupProtocol = 23,
    InvalidGroupId = 24,
    UnknownMemberId = 25,
    InvalidSessionTimeout = 26,
    RebalanceInProgress = 27,
    InvalidCommitOffsetSize = 28,
    TopicAuthorizationFailed = 29,
    GroupAuthorizationFailed = 30,
    ClusterAuthorizationFailed = 31,
    InvalidTimestamp = 32,
    UnsupportedSaslMechanism = 33,
    IllegalSaslState = 34,
    UnsupportedVersion = 35,
    TopicAlreadyExists = 36,
    InvalidPartitions = 37,
    InvalidReplicationFactor = 38,
    InvalidReplicaAssignment = 39,
    InvalidConfig = 40,
    NotController = 41,
    InvalidRequest = 42,
    UnsupportedForMessageFormat = 43,
    PolicyViolation = 44,
    OutOfOrderSequenceNumber = 45,
    DuplicateSequenceNumber = 46,
    InvalidProducerEpoch = 47,
    InvalidTxnState = 48,
    InvalidProducerIdMapping = 49,
    InvalidTransactionTimeout = 50,
    ConcurrentTransactions = 51,
    TransactionCoordinatorFenced = 52,
    TransactionalIdAuthorizationFailed = 53,
    SecurityDisabled = 54,
    OperationNotAttempted = 55,
    KafkaStorageError = 56,
    LogDirNotFound = 57,
    SaslAuthenticationFailed = 58,
    UnknownProducerId = 59,
    ReassignmentInProgress = 60,
    DelegationTokenAuthDisabled = 61,
    DelegationTokenNotFound = 62,
    DelegationTokenOwnerMismatch = 63,
    DelegationTokenRequestNotAllowed = 64,
    DelegationTokenAuthorizationFailed = 65,
    DelegationTokenExpired = 66,
    InvalidPrincipalType = 67,
    NonEmptyGroup = 68,
    GroupIdNotFound = 69,
    FetchSessionIdNotFound = 70,
    InvalidFetchSessionEpoch = 71,
    ListenerNotFound = 72,
    TopicDeletionDisabled = 73,
    FencedLeaderEpoch = 74,
    UnknownLeaderEpoch = 75,
    UnsupportedCompressionType = 76,
    StaleBrokerEpoch = 77,
    OffsetNotAvailable = 78,
    MemberIdRequired = 79,
    PreferredLeaderNotAvailable = 80,
    GroupMaxSizeReached = 81,
    FencedInstanceId = 82,
    EligibleLeadersNotAvailable = 83,
    ElectionNotNeeded = 84,
    NoReassignmentInProgress = 85,
    GroupSubscribedToTopic = 86,
    InvalidRecord = 87,
    UnstableOffsetCommit = 88,
}

impl ErrorCode {
    pub fn as_i16(self) -> i16 {
        self as i16
    }
}

impl From<&KafkaError> for ErrorCode {
    fn from(err: &KafkaError) -> Self {
        match err {
            KafkaError::TopicNotFound(_) => ErrorCode::UnknownTopicOrPartition,
            KafkaError::PartitionNotFound(_, _) => ErrorCode::UnknownTopicOrPartition,
            KafkaError::InvalidRequest(_) => ErrorCode::InvalidRequest,
            KafkaError::UnsupportedApi(_, _) => ErrorCode::UnsupportedVersion,
            KafkaError::NotCoordinator(_) => ErrorCode::NotCoordinator,
            KafkaError::RebalanceInProgress(_) => ErrorCode::RebalanceInProgress,
            KafkaError::MemberNotFound(_, _) => ErrorCode::UnknownMemberId,
            KafkaError::InvalidGeneration(_, _) => ErrorCode::IllegalGeneration,
            KafkaError::OffsetOutOfRange(_) => ErrorCode::OffsetOutOfRange,
            KafkaError::Compression(_) => ErrorCode::UnsupportedCompressionType,
            KafkaError::AuthenticationError(_) => ErrorCode::SaslAuthenticationFailed,
            KafkaError::AuthorizationError(_) => ErrorCode::ClusterAuthorizationFailed,
            _ => ErrorCode::UnknownServerError,
        }
    }
}

impl From<KafkaError> for ErrorCode {
    fn from(err: KafkaError) -> Self {
        ErrorCode::from(&err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------
    // KafkaError Display trait tests
    // ---------------------------------------------------------------

    #[test]
    fn test_display_unknown() {
        let err = KafkaError::Unknown("something went wrong".to_string());
        assert_eq!(format!("{}", err), "Unknown error: something went wrong");
    }

    #[test]
    fn test_display_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionReset, "connection reset");
        let err = KafkaError::Io(io_err);
        assert_eq!(format!("{}", err), "IO error: connection reset");
    }

    #[test]
    fn test_display_protocol() {
        let err = KafkaError::Protocol("bad frame".to_string());
        assert_eq!(format!("{}", err), "Protocol error: bad frame");
    }

    #[test]
    fn test_display_topic_not_found() {
        let err = KafkaError::TopicNotFound("my-topic".to_string());
        assert_eq!(format!("{}", err), "Topic not found: my-topic");
    }

    #[test]
    fn test_display_partition_not_found() {
        let err = KafkaError::PartitionNotFound("my-topic".to_string(), 5);
        assert_eq!(format!("{}", err), "Partition not found: topic=my-topic, partition=5");
    }

    #[test]
    fn test_display_invalid_request() {
        let err = KafkaError::InvalidRequest("missing field".to_string());
        assert_eq!(format!("{}", err), "Invalid request: missing field");
    }

    #[test]
    fn test_display_unsupported_api() {
        let err = KafkaError::UnsupportedApi(99, 5);
        assert_eq!(format!("{}", err), "Unsupported API: key=99, version=5");
    }

    #[test]
    fn test_display_consumer_group() {
        let err = KafkaError::ConsumerGroup("group error".to_string());
        assert_eq!(format!("{}", err), "Consumer group error: group error");
    }

    #[test]
    fn test_display_not_coordinator() {
        let err = KafkaError::NotCoordinator("my-group".to_string());
        assert_eq!(format!("{}", err), "Not coordinator for group: my-group");
    }

    #[test]
    fn test_display_rebalance_in_progress() {
        let err = KafkaError::RebalanceInProgress("my-group".to_string());
        assert_eq!(format!("{}", err), "Rebalance in progress for group: my-group");
    }

    #[test]
    fn test_display_member_not_found() {
        let err = KafkaError::MemberNotFound("g1".to_string(), "m1".to_string());
        assert_eq!(format!("{}", err), "Member not found: group=g1, member=m1");
    }

    #[test]
    fn test_display_invalid_generation() {
        let err = KafkaError::InvalidGeneration(5, 3);
        assert_eq!(format!("{}", err), "Invalid generation: expected=5, got=3");
    }

    #[test]
    fn test_display_offset_out_of_range() {
        let err = KafkaError::OffsetOutOfRange(12345);
        assert_eq!(format!("{}", err), "Offset out of range: 12345");
    }

    #[test]
    fn test_display_metadata_store() {
        let err = KafkaError::MetadataStore("db connection failed".to_string());
        assert_eq!(format!("{}", err), "Metadata store error: db connection failed");
    }

    #[test]
    fn test_display_storage() {
        let err = KafkaError::Storage("disk full".to_string());
        assert_eq!(format!("{}", err), "Storage error: disk full");
    }

    #[test]
    fn test_display_compression() {
        let err = KafkaError::Compression("decompression failed".to_string());
        assert_eq!(format!("{}", err), "Compression error: decompression failed");
    }

    #[test]
    fn test_display_connection_closed() {
        let err = KafkaError::ConnectionClosed;
        assert_eq!(format!("{}", err), "Connection closed");
    }

    #[test]
    fn test_display_authentication_error() {
        let err = KafkaError::AuthenticationError("bad credentials".to_string());
        assert_eq!(format!("{}", err), "Authentication error: bad credentials");
    }

    #[test]
    fn test_display_authorization_error() {
        let err = KafkaError::AuthorizationError("access denied".to_string());
        assert_eq!(format!("{}", err), "Authorization error: access denied");
    }

    // ---------------------------------------------------------------
    // KafkaError -> ErrorCode conversion tests
    // ---------------------------------------------------------------

    #[test]
    fn test_error_code_from_topic_not_found() {
        let err = KafkaError::TopicNotFound("t".to_string());
        assert_eq!(ErrorCode::from(&err), ErrorCode::UnknownTopicOrPartition);
        assert_eq!(ErrorCode::from(KafkaError::TopicNotFound("t".to_string())), ErrorCode::UnknownTopicOrPartition);
    }

    #[test]
    fn test_error_code_from_partition_not_found() {
        let err = KafkaError::PartitionNotFound("t".to_string(), 0);
        assert_eq!(ErrorCode::from(&err), ErrorCode::UnknownTopicOrPartition);
    }

    #[test]
    fn test_error_code_from_invalid_request() {
        let err = KafkaError::InvalidRequest("x".to_string());
        assert_eq!(ErrorCode::from(&err), ErrorCode::InvalidRequest);
    }

    #[test]
    fn test_error_code_from_unsupported_api() {
        let err = KafkaError::UnsupportedApi(100, 0);
        assert_eq!(ErrorCode::from(&err), ErrorCode::UnsupportedVersion);
    }

    #[test]
    fn test_error_code_from_not_coordinator() {
        let err = KafkaError::NotCoordinator("g".to_string());
        assert_eq!(ErrorCode::from(&err), ErrorCode::NotCoordinator);
    }

    #[test]
    fn test_error_code_from_rebalance_in_progress() {
        let err = KafkaError::RebalanceInProgress("g".to_string());
        assert_eq!(ErrorCode::from(&err), ErrorCode::RebalanceInProgress);
    }

    #[test]
    fn test_error_code_from_member_not_found() {
        let err = KafkaError::MemberNotFound("g".to_string(), "m".to_string());
        assert_eq!(ErrorCode::from(&err), ErrorCode::UnknownMemberId);
    }

    #[test]
    fn test_error_code_from_invalid_generation() {
        let err = KafkaError::InvalidGeneration(1, 2);
        assert_eq!(ErrorCode::from(&err), ErrorCode::IllegalGeneration);
    }

    #[test]
    fn test_error_code_from_offset_out_of_range() {
        let err = KafkaError::OffsetOutOfRange(0);
        assert_eq!(ErrorCode::from(&err), ErrorCode::OffsetOutOfRange);
    }

    #[test]
    fn test_error_code_from_compression() {
        let err = KafkaError::Compression("err".to_string());
        assert_eq!(ErrorCode::from(&err), ErrorCode::UnsupportedCompressionType);
    }

    #[test]
    fn test_error_code_from_authentication_error() {
        let err = KafkaError::AuthenticationError("err".to_string());
        assert_eq!(ErrorCode::from(&err), ErrorCode::SaslAuthenticationFailed);
    }

    #[test]
    fn test_error_code_from_authorization_error() {
        let err = KafkaError::AuthorizationError("err".to_string());
        assert_eq!(ErrorCode::from(&err), ErrorCode::ClusterAuthorizationFailed);
    }

    #[test]
    fn test_error_code_from_unknown_defaults_to_unknown_server_error() {
        // All variants not explicitly mapped should fall through to UnknownServerError
        let fallthrough_errors: Vec<KafkaError> = vec![
            KafkaError::Unknown("x".to_string()),
            KafkaError::Io(std::io::Error::new(std::io::ErrorKind::Other, "x")),
            KafkaError::Protocol("x".to_string()),
            KafkaError::ConsumerGroup("x".to_string()),
            KafkaError::MetadataStore("x".to_string()),
            KafkaError::Storage("x".to_string()),
            KafkaError::ConnectionClosed,
        ];

        for err in &fallthrough_errors {
            assert_eq!(
                ErrorCode::from(err),
                ErrorCode::UnknownServerError,
                "Expected UnknownServerError for {:?}",
                err,
            );
        }
    }

    // ---------------------------------------------------------------
    // ErrorCode value / as_i16 tests
    // ---------------------------------------------------------------

    #[test]
    fn test_error_code_none_is_zero() {
        assert_eq!(ErrorCode::None.as_i16(), 0);
    }

    #[test]
    fn test_error_code_unknown_server_error_is_negative_one() {
        assert_eq!(ErrorCode::UnknownServerError.as_i16(), -1);
    }

    #[test]
    fn test_error_code_specific_values() {
        assert_eq!(ErrorCode::OffsetOutOfRange.as_i16(), 1);
        assert_eq!(ErrorCode::CorruptMessage.as_i16(), 2);
        assert_eq!(ErrorCode::UnknownTopicOrPartition.as_i16(), 3);
        assert_eq!(ErrorCode::NotCoordinator.as_i16(), 16);
        assert_eq!(ErrorCode::RebalanceInProgress.as_i16(), 27);
        assert_eq!(ErrorCode::UnknownMemberId.as_i16(), 25);
        assert_eq!(ErrorCode::IllegalGeneration.as_i16(), 22);
        assert_eq!(ErrorCode::UnsupportedVersion.as_i16(), 35);
        assert_eq!(ErrorCode::TopicAlreadyExists.as_i16(), 36);
        assert_eq!(ErrorCode::InvalidRequest.as_i16(), 42);
        assert_eq!(ErrorCode::UnsupportedCompressionType.as_i16(), 76);
        assert_eq!(ErrorCode::MemberIdRequired.as_i16(), 79);
        assert_eq!(ErrorCode::SaslAuthenticationFailed.as_i16(), 58);
        assert_eq!(ErrorCode::ClusterAuthorizationFailed.as_i16(), 31);
    }

    #[test]
    fn test_error_code_clone_copy() {
        let code = ErrorCode::None;
        let cloned = code.clone();
        let copied = code;
        assert_eq!(code, cloned);
        assert_eq!(code, copied);
    }

    #[test]
    fn test_error_code_equality() {
        assert_eq!(ErrorCode::None, ErrorCode::None);
        assert_ne!(ErrorCode::None, ErrorCode::UnknownServerError);
    }

    #[test]
    fn test_error_code_debug() {
        let debug = format!("{:?}", ErrorCode::TopicAlreadyExists);
        assert_eq!(debug, "TopicAlreadyExists");
    }

    // ---------------------------------------------------------------
    // From<std::io::Error> conversion test
    // ---------------------------------------------------------------

    #[test]
    fn test_kafka_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe broken");
        let kafka_err: KafkaError = io_err.into();
        match kafka_err {
            KafkaError::Io(e) => {
                assert_eq!(e.kind(), std::io::ErrorKind::BrokenPipe);
                assert_eq!(format!("{}", e), "pipe broken");
            }
            _ => panic!("Expected KafkaError::Io"),
        }
    }

    // ---------------------------------------------------------------
    // ErrorCode comprehensive value range test
    // ---------------------------------------------------------------

    #[test]
    fn test_error_codes_are_contiguous_where_expected() {
        // Error codes 1-13 should be contiguous
        let contiguous_codes = vec![
            ErrorCode::OffsetOutOfRange,       // 1
            ErrorCode::CorruptMessage,         // 2
            ErrorCode::UnknownTopicOrPartition,// 3
            ErrorCode::InvalidFetchSize,       // 4
            ErrorCode::LeaderNotAvailable,     // 5
            ErrorCode::NotLeaderOrFollower,    // 6
            ErrorCode::RequestTimedOut,        // 7
            ErrorCode::BrokerNotAvailable,     // 8
            ErrorCode::ReplicaNotAvailable,    // 9
            ErrorCode::MessageTooLarge,        // 10
            ErrorCode::StaleControllerEpoch,   // 11
            ErrorCode::OffsetMetadataTooLarge, // 12
            ErrorCode::NetworkException,       // 13
        ];

        for (i, code) in contiguous_codes.iter().enumerate() {
            assert_eq!(code.as_i16(), (i + 1) as i16);
        }
    }

    #[test]
    fn test_all_error_codes_have_unique_values() {
        let all_codes = vec![
            ErrorCode::None,
            ErrorCode::UnknownServerError,
            ErrorCode::OffsetOutOfRange,
            ErrorCode::CorruptMessage,
            ErrorCode::UnknownTopicOrPartition,
            ErrorCode::InvalidFetchSize,
            ErrorCode::LeaderNotAvailable,
            ErrorCode::NotLeaderOrFollower,
            ErrorCode::RequestTimedOut,
            ErrorCode::BrokerNotAvailable,
            ErrorCode::ReplicaNotAvailable,
            ErrorCode::MessageTooLarge,
            ErrorCode::StaleControllerEpoch,
            ErrorCode::OffsetMetadataTooLarge,
            ErrorCode::NetworkException,
            ErrorCode::CoordinatorLoadInProgress,
            ErrorCode::CoordinatorNotAvailable,
            ErrorCode::NotCoordinator,
            ErrorCode::InvalidTopicException,
            ErrorCode::RecordListTooLarge,
            ErrorCode::NotEnoughReplicas,
            ErrorCode::NotEnoughReplicasAfterAppend,
            ErrorCode::InvalidRequiredAcks,
            ErrorCode::IllegalGeneration,
            ErrorCode::InconsistentGroupProtocol,
            ErrorCode::InvalidGroupId,
            ErrorCode::UnknownMemberId,
            ErrorCode::InvalidSessionTimeout,
            ErrorCode::RebalanceInProgress,
            ErrorCode::InvalidCommitOffsetSize,
            ErrorCode::TopicAuthorizationFailed,
            ErrorCode::GroupAuthorizationFailed,
            ErrorCode::ClusterAuthorizationFailed,
            ErrorCode::InvalidTimestamp,
            ErrorCode::UnsupportedSaslMechanism,
            ErrorCode::IllegalSaslState,
            ErrorCode::UnsupportedVersion,
            ErrorCode::TopicAlreadyExists,
            ErrorCode::InvalidPartitions,
            ErrorCode::InvalidReplicationFactor,
            ErrorCode::InvalidReplicaAssignment,
            ErrorCode::InvalidConfig,
            ErrorCode::NotController,
            ErrorCode::InvalidRequest,
            ErrorCode::UnsupportedForMessageFormat,
            ErrorCode::PolicyViolation,
            ErrorCode::OutOfOrderSequenceNumber,
            ErrorCode::DuplicateSequenceNumber,
            ErrorCode::InvalidProducerEpoch,
            ErrorCode::InvalidTxnState,
            ErrorCode::InvalidProducerIdMapping,
            ErrorCode::InvalidTransactionTimeout,
            ErrorCode::ConcurrentTransactions,
            ErrorCode::TransactionCoordinatorFenced,
            ErrorCode::TransactionalIdAuthorizationFailed,
            ErrorCode::SecurityDisabled,
            ErrorCode::OperationNotAttempted,
            ErrorCode::KafkaStorageError,
            ErrorCode::LogDirNotFound,
            ErrorCode::SaslAuthenticationFailed,
            ErrorCode::UnknownProducerId,
            ErrorCode::ReassignmentInProgress,
            ErrorCode::DelegationTokenAuthDisabled,
            ErrorCode::DelegationTokenNotFound,
            ErrorCode::DelegationTokenOwnerMismatch,
            ErrorCode::DelegationTokenRequestNotAllowed,
            ErrorCode::DelegationTokenAuthorizationFailed,
            ErrorCode::DelegationTokenExpired,
            ErrorCode::InvalidPrincipalType,
            ErrorCode::NonEmptyGroup,
            ErrorCode::GroupIdNotFound,
            ErrorCode::FetchSessionIdNotFound,
            ErrorCode::InvalidFetchSessionEpoch,
            ErrorCode::ListenerNotFound,
            ErrorCode::TopicDeletionDisabled,
            ErrorCode::FencedLeaderEpoch,
            ErrorCode::UnknownLeaderEpoch,
            ErrorCode::UnsupportedCompressionType,
            ErrorCode::StaleBrokerEpoch,
            ErrorCode::OffsetNotAvailable,
            ErrorCode::MemberIdRequired,
            ErrorCode::PreferredLeaderNotAvailable,
            ErrorCode::GroupMaxSizeReached,
            ErrorCode::FencedInstanceId,
            ErrorCode::EligibleLeadersNotAvailable,
            ErrorCode::ElectionNotNeeded,
            ErrorCode::NoReassignmentInProgress,
            ErrorCode::GroupSubscribedToTopic,
            ErrorCode::InvalidRecord,
            ErrorCode::UnstableOffsetCommit,
        ];

        let mut seen = std::collections::HashSet::new();
        for code in &all_codes {
            let val = code.as_i16();
            assert!(
                seen.insert(val),
                "Duplicate error code value: {} ({:?})",
                val,
                code,
            );
        }
    }

    // ---------------------------------------------------------------
    // KafkaError is Send + Sync (important for async usage)
    // ---------------------------------------------------------------

    #[test]
    fn test_kafka_error_is_send_and_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<KafkaError>();
        assert_sync::<KafkaError>();
    }

    // ---------------------------------------------------------------
    // Display with empty strings
    // ---------------------------------------------------------------

    #[test]
    fn test_display_with_empty_messages() {
        assert_eq!(format!("{}", KafkaError::Unknown("".to_string())), "Unknown error: ");
        assert_eq!(format!("{}", KafkaError::Protocol("".to_string())), "Protocol error: ");
        assert_eq!(format!("{}", KafkaError::TopicNotFound("".to_string())), "Topic not found: ");
    }
}
