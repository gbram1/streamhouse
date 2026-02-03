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
            _ => ErrorCode::UnknownServerError,
        }
    }
}

impl From<KafkaError> for ErrorCode {
    fn from(err: KafkaError) -> Self {
        ErrorCode::from(&err)
    }
}
