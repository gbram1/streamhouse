//! Consumer group coordinator
//!
//! Manages consumer group state, rebalancing, and partition assignments.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use tokio::sync::RwLock;
use tracing::{debug, info};

use streamhouse_metadata::MetadataStore;

use crate::error::{ErrorCode, KafkaResult};

/// Session timeout bounds
const MIN_SESSION_TIMEOUT_MS: i32 = 6000;
const MAX_SESSION_TIMEOUT_MS: i32 = 300000;
const DEFAULT_SESSION_TIMEOUT_MS: i32 = 30000;

/// Rebalance timeout
const REBALANCE_TIMEOUT_MS: i32 = 60000;

/// Consumer group state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupState {
    /// Group has no members
    Empty,
    /// Group is preparing for rebalance, waiting for members to join
    PreparingRebalance,
    /// Group is waiting for leader to provide assignments
    CompletingRebalance,
    /// Group is stable with valid assignments
    Stable,
    /// Group is being deleted
    Dead,
}

impl GroupState {
    pub fn as_str(&self) -> &'static str {
        match self {
            GroupState::Empty => "Empty",
            GroupState::PreparingRebalance => "PreparingRebalance",
            GroupState::CompletingRebalance => "CompletingRebalance",
            GroupState::Stable => "Stable",
            GroupState::Dead => "Dead",
        }
    }
}

/// Consumer group member
#[derive(Debug, Clone)]
pub struct GroupMember {
    pub member_id: String,
    pub client_id: String,
    pub client_host: String,
    pub session_timeout_ms: i32,
    pub rebalance_timeout_ms: i32,
    pub protocol_type: String,
    pub protocols: Vec<MemberProtocol>,
    pub assignment: Vec<u8>,
    pub last_heartbeat: Instant,
}

/// Member protocol metadata
#[derive(Debug, Clone)]
pub struct MemberProtocol {
    pub name: String,
    pub metadata: Vec<u8>,
}

/// Consumer group
#[derive(Debug)]
pub struct ConsumerGroup {
    pub group_id: String,
    pub state: GroupState,
    pub generation_id: i32,
    pub protocol_type: Option<String>,
    pub protocol_name: Option<String>,
    pub leader_id: Option<String>,
    pub members: HashMap<String, GroupMember>,
    pub pending_members: HashMap<String, GroupMember>,
    state_timestamp: Instant,
}

impl ConsumerGroup {
    pub fn new(group_id: String) -> Self {
        Self {
            group_id,
            state: GroupState::Empty,
            generation_id: 0,
            protocol_type: None,
            protocol_name: None,
            leader_id: None,
            members: HashMap::new(),
            pending_members: HashMap::new(),
            state_timestamp: Instant::now(),
        }
    }

    /// Transition to a new state
    fn transition_to(&mut self, new_state: GroupState) {
        debug!(
            "Group {} state transition: {:?} -> {:?}",
            self.group_id, self.state, new_state
        );
        self.state = new_state;
        self.state_timestamp = Instant::now();
    }

    /// Check if all members have joined
    fn all_members_joined(&self) -> bool {
        self.pending_members.is_empty()
    }

    /// Get all member IDs
    pub fn member_ids(&self) -> Vec<String> {
        self.members.keys().cloned().collect()
    }

    /// Select a protocol that all members support
    fn select_protocol(&self) -> Option<String> {
        if self.members.is_empty() {
            return None;
        }

        // Find protocols supported by all members
        let mut protocol_votes: HashMap<String, usize> = HashMap::new();

        for member in self.members.values() {
            for protocol in &member.protocols {
                *protocol_votes.entry(protocol.name.clone()).or_insert(0) += 1;
            }
        }

        let member_count = self.members.len();
        protocol_votes
            .into_iter()
            .filter(|(_, count)| *count == member_count)
            .map(|(name, _)| name)
            .next()
    }
}

/// Group coordinator manages all consumer groups
pub struct GroupCoordinator {
    groups: DashMap<String, RwLock<ConsumerGroup>>,
    metadata: Arc<dyn MetadataStore>,
}

impl GroupCoordinator {
    pub fn new(metadata: Arc<dyn MetadataStore>) -> Self {
        Self {
            groups: DashMap::new(),
            metadata,
        }
    }

    /// Get or create a consumer group
    fn get_or_create_group(&self, group_id: &str) -> dashmap::mapref::one::RefMut<'_, String, RwLock<ConsumerGroup>> {
        self.groups
            .entry(group_id.to_string())
            .or_insert_with(|| RwLock::new(ConsumerGroup::new(group_id.to_string())))
    }

    /// Handle JoinGroup request
    pub async fn join_group(
        &self,
        group_id: &str,
        member_id: Option<&str>,
        client_id: &str,
        client_host: &str,
        session_timeout_ms: i32,
        rebalance_timeout_ms: i32,
        protocol_type: &str,
        protocols: Vec<MemberProtocol>,
    ) -> KafkaResult<JoinGroupResult> {
        // Validate session timeout
        let session_timeout_ms = session_timeout_ms.clamp(MIN_SESSION_TIMEOUT_MS, MAX_SESSION_TIMEOUT_MS);

        // Get or create group
        let group_entry = self.get_or_create_group(group_id);
        let mut group = group_entry.write().await;

        // Generate or validate member ID
        let member_id = match member_id {
            Some(id) if !id.is_empty() => id.to_string(),
            _ => {
                // Generate new member ID
                let new_id = format!("{}-{}", client_id, uuid::Uuid::new_v4());

                // For the first join with no member ID, return MEMBER_ID_REQUIRED
                // The client will retry with the provided member ID
                return Ok(JoinGroupResult {
                    error_code: ErrorCode::MemberIdRequired,
                    generation_id: -1,
                    protocol_type: None,
                    protocol_name: None,
                    leader: String::new(),
                    member_id: new_id,
                    members: vec![],
                });
            }
        };

        // Check protocol type compatibility
        if let Some(ref existing_type) = group.protocol_type {
            if existing_type != protocol_type {
                return Ok(JoinGroupResult {
                    error_code: ErrorCode::InconsistentGroupProtocol,
                    generation_id: -1,
                    protocol_type: None,
                    protocol_name: None,
                    leader: String::new(),
                    member_id,
                    members: vec![],
                });
            }
        }

        // Create member
        let member = GroupMember {
            member_id: member_id.clone(),
            client_id: client_id.to_string(),
            client_host: client_host.to_string(),
            session_timeout_ms,
            rebalance_timeout_ms,
            protocol_type: protocol_type.to_string(),
            protocols,
            assignment: vec![],
            last_heartbeat: Instant::now(),
        };

        // Handle based on current state
        match group.state {
            GroupState::Empty | GroupState::Dead => {
                // First member, start fresh
                group.members.clear();
                group.members.insert(member_id.clone(), member);
                group.protocol_type = Some(protocol_type.to_string());
                group.leader_id = Some(member_id.clone());
                group.generation_id += 1;

                // Select protocol
                group.protocol_name = group.select_protocol();

                group.transition_to(GroupState::CompletingRebalance);
            }
            GroupState::PreparingRebalance => {
                // Add member to pending
                group.pending_members.insert(member_id.clone(), member);
            }
            GroupState::CompletingRebalance | GroupState::Stable => {
                // Trigger rebalance
                group.pending_members.insert(member_id.clone(), member);

                // Move all current members to pending
                let current_members: Vec<_> = group.members.drain().collect();
                for (id, m) in current_members {
                    group.pending_members.insert(id, m);
                }

                group.generation_id += 1;
                group.transition_to(GroupState::PreparingRebalance);
            }
        }

        // Check if we should complete the join
        if group.state == GroupState::PreparingRebalance && !group.pending_members.is_empty() {
            // Move pending to members
            let pending: Vec<_> = group.pending_members.drain().collect();
            for (id, m) in pending {
                group.members.insert(id, m);
            }

            // Select leader if needed
            if group.leader_id.is_none() || !group.members.contains_key(group.leader_id.as_ref().unwrap()) {
                group.leader_id = group.members.keys().next().cloned();
            }

            // Select protocol
            group.protocol_name = group.select_protocol();

            group.transition_to(GroupState::CompletingRebalance);
        }

        // Build response
        let is_leader = group.leader_id.as_ref() == Some(&member_id);
        let members = if is_leader {
            group.members.values().map(|m| JoinGroupMember {
                member_id: m.member_id.clone(),
                metadata: m.protocols.first().map(|p| p.metadata.clone()).unwrap_or_default(),
            }).collect()
        } else {
            vec![]
        };

        Ok(JoinGroupResult {
            error_code: ErrorCode::None,
            generation_id: group.generation_id,
            protocol_type: group.protocol_type.clone(),
            protocol_name: group.protocol_name.clone(),
            leader: group.leader_id.clone().unwrap_or_default(),
            member_id,
            members,
        })
    }

    /// Handle SyncGroup request
    pub async fn sync_group(
        &self,
        group_id: &str,
        generation_id: i32,
        member_id: &str,
        assignments: Vec<SyncGroupAssignment>,
    ) -> KafkaResult<SyncGroupResult> {
        let Some(group_entry) = self.groups.get(group_id) else {
            return Ok(SyncGroupResult {
                error_code: ErrorCode::GroupIdNotFound,
                assignment: vec![],
            });
        };

        let mut group = group_entry.write().await;

        // Validate member
        if !group.members.contains_key(member_id) {
            return Ok(SyncGroupResult {
                error_code: ErrorCode::UnknownMemberId,
                assignment: vec![],
            });
        }

        // Validate generation
        if generation_id != group.generation_id {
            return Ok(SyncGroupResult {
                error_code: ErrorCode::IllegalGeneration,
                assignment: vec![],
            });
        }

        // Handle based on state
        match group.state {
            GroupState::Empty | GroupState::Dead => {
                return Ok(SyncGroupResult {
                    error_code: ErrorCode::UnknownMemberId,
                    assignment: vec![],
                });
            }
            GroupState::PreparingRebalance => {
                return Ok(SyncGroupResult {
                    error_code: ErrorCode::RebalanceInProgress,
                    assignment: vec![],
                });
            }
            GroupState::CompletingRebalance => {
                // Leader provides assignments
                if group.leader_id.as_ref() == Some(&member_id.to_string()) {
                    for assignment in assignments {
                        if let Some(member) = group.members.get_mut(&assignment.member_id) {
                            member.assignment = assignment.assignment;
                        }
                    }
                    group.transition_to(GroupState::Stable);
                }
            }
            GroupState::Stable => {
                // Already stable, just return assignment
            }
        }

        // Get member's assignment
        let assignment = group
            .members
            .get(member_id)
            .map(|m| m.assignment.clone())
            .unwrap_or_default();

        Ok(SyncGroupResult {
            error_code: ErrorCode::None,
            assignment,
        })
    }

    /// Handle Heartbeat request
    pub async fn heartbeat(
        &self,
        group_id: &str,
        generation_id: i32,
        member_id: &str,
    ) -> KafkaResult<ErrorCode> {
        let Some(group_entry) = self.groups.get(group_id) else {
            return Ok(ErrorCode::GroupIdNotFound);
        };

        let mut group = group_entry.write().await;

        // Validate member exists
        if !group.members.contains_key(member_id) {
            return Ok(ErrorCode::UnknownMemberId);
        }

        // Validate generation
        if generation_id != group.generation_id {
            return Ok(ErrorCode::IllegalGeneration);
        }

        // Update heartbeat
        if let Some(member) = group.members.get_mut(member_id) {
            member.last_heartbeat = Instant::now();
        }

        // Check state
        match group.state {
            GroupState::PreparingRebalance | GroupState::CompletingRebalance => {
                Ok(ErrorCode::RebalanceInProgress)
            }
            GroupState::Stable => Ok(ErrorCode::None),
            GroupState::Empty | GroupState::Dead => Ok(ErrorCode::UnknownMemberId),
        }
    }

    /// Handle LeaveGroup request
    pub async fn leave_group(
        &self,
        group_id: &str,
        member_id: &str,
    ) -> KafkaResult<ErrorCode> {
        let Some(group_entry) = self.groups.get(group_id) else {
            return Ok(ErrorCode::GroupIdNotFound);
        };

        let mut group = group_entry.write().await;

        // Remove member
        if group.members.remove(member_id).is_none() {
            return Ok(ErrorCode::UnknownMemberId);
        }

        info!("Member {} left group {}", member_id, group_id);

        // If group is now empty
        if group.members.is_empty() {
            group.transition_to(GroupState::Empty);
            group.leader_id = None;
            group.protocol_type = None;
            group.protocol_name = None;
        } else {
            // Trigger rebalance
            group.generation_id += 1;

            // Update leader if needed
            if group.leader_id.as_ref() == Some(&member_id.to_string()) {
                group.leader_id = group.members.keys().next().cloned();
            }

            group.transition_to(GroupState::PreparingRebalance);
        }

        Ok(ErrorCode::None)
    }

    /// Get group state for DescribeGroups
    pub async fn describe_group(&self, group_id: &str) -> Option<DescribeGroupResult> {
        let group_entry = self.groups.get(group_id)?;
        let group = group_entry.read().await;

        let members = group.members.values().map(|m| DescribeGroupMember {
            member_id: m.member_id.clone(),
            client_id: m.client_id.clone(),
            client_host: m.client_host.clone(),
            member_metadata: m.protocols.first().map(|p| p.metadata.clone()).unwrap_or_default(),
            member_assignment: m.assignment.clone(),
        }).collect();

        Some(DescribeGroupResult {
            error_code: ErrorCode::None,
            group_id: group.group_id.clone(),
            state: group.state.as_str().to_string(),
            protocol_type: group.protocol_type.clone().unwrap_or_default(),
            protocol: group.protocol_name.clone().unwrap_or_default(),
            members,
        })
    }

    /// List all groups
    pub async fn list_groups(&self) -> Vec<ListGroupResult> {
        let mut results = vec![];

        for entry in self.groups.iter() {
            let group = entry.read().await;
            results.push(ListGroupResult {
                group_id: group.group_id.clone(),
                protocol_type: group.protocol_type.clone().unwrap_or_default(),
                group_state: group.state.as_str().to_string(),
            });
        }

        results
    }
}

/// Result of JoinGroup request
#[derive(Debug)]
pub struct JoinGroupResult {
    pub error_code: ErrorCode,
    pub generation_id: i32,
    pub protocol_type: Option<String>,
    pub protocol_name: Option<String>,
    pub leader: String,
    pub member_id: String,
    pub members: Vec<JoinGroupMember>,
}

/// Member info returned in JoinGroup response (for leader only)
#[derive(Debug)]
pub struct JoinGroupMember {
    pub member_id: String,
    pub metadata: Vec<u8>,
}

/// Assignment provided in SyncGroup request
#[derive(Debug)]
pub struct SyncGroupAssignment {
    pub member_id: String,
    pub assignment: Vec<u8>,
}

/// Result of SyncGroup request
#[derive(Debug)]
pub struct SyncGroupResult {
    pub error_code: ErrorCode,
    pub assignment: Vec<u8>,
}

/// Result for DescribeGroups
#[derive(Debug)]
pub struct DescribeGroupResult {
    pub error_code: ErrorCode,
    pub group_id: String,
    pub state: String,
    pub protocol_type: String,
    pub protocol: String,
    pub members: Vec<DescribeGroupMember>,
}

/// Member info for DescribeGroups
#[derive(Debug)]
pub struct DescribeGroupMember {
    pub member_id: String,
    pub client_id: String,
    pub client_host: String,
    pub member_metadata: Vec<u8>,
    pub member_assignment: Vec<u8>,
}

/// Result for ListGroups
#[derive(Debug)]
pub struct ListGroupResult {
    pub group_id: String,
    pub protocol_type: String,
    pub group_state: String,
}
