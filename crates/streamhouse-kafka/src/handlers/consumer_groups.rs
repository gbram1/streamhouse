//! Consumer group API handlers
//!
//! - FindCoordinator (API 10)
//! - JoinGroup (API 11)
//! - Heartbeat (API 12)
//! - LeaveGroup (API 13)
//! - SyncGroup (API 14)
//! - DescribeGroups (API 15)
//! - ListGroups (API 16)

use bytes::{Buf, BufMut, BytesMut};
use tracing::debug;

use crate::codec::{
    encode_compact_nullable_bytes, encode_compact_nullable_string, encode_compact_string,
    encode_empty_tagged_fields, encode_nullable_bytes, encode_nullable_string, encode_string,
    encode_unsigned_varint, parse_array, parse_compact_array, parse_compact_nullable_bytes,
    parse_compact_nullable_string, parse_compact_string, parse_nullable_bytes,
    parse_nullable_string, parse_string, skip_tagged_fields, RequestHeader,
};
use crate::coordinator::{MemberProtocol, SyncGroupAssignment};
use crate::error::{ErrorCode, KafkaResult};
use crate::server::KafkaServerState;

/// Handle FindCoordinator request (API 10)
pub async fn handle_find_coordinator(
    state: &KafkaServerState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    // Parse request
    let (key, key_type) = if header.api_version >= 4 {
        // Compact protocol (v4+)
        let key = parse_compact_string(body)?;
        let key_type = body.get_i8();
        skip_tagged_fields(body)?;
        (key, key_type)
    } else if header.api_version >= 3 {
        // Compact protocol (v3)
        let key = parse_compact_string(body)?;
        let key_type = body.get_i8();
        skip_tagged_fields(body)?;
        (key, key_type)
    } else {
        // Legacy protocol
        let key = parse_string(body)?;
        let key_type = if header.api_version >= 1 {
            body.get_i8()
        } else {
            0 // GROUP coordinator
        };
        (key, key_type)
    };

    debug!("FindCoordinator: key={}, type={}", key, key_type);

    // We're always the coordinator (single node)
    let node_id = state.config.node_id;
    let host = &state.config.advertised_host;
    let port = state.config.advertised_port;

    // Build response
    let mut response = BytesMut::new();

    if header.api_version >= 3 {
        // Compact protocol
        response.put_i32(0); // throttle time

        if header.api_version >= 4 {
            // v4+ has an array of coordinators
            encode_unsigned_varint(&mut response, 2); // 1 coordinator + 1

            encode_compact_string(&mut response, &key);
            response.put_i32(node_id);
            encode_compact_string(&mut response, host);
            response.put_i32(port);
            response.put_i16(ErrorCode::None.as_i16());
            encode_compact_nullable_string(&mut response, None); // error_message
            encode_empty_tagged_fields(&mut response);

            encode_empty_tagged_fields(&mut response);
        } else {
            // v3
            response.put_i16(ErrorCode::None.as_i16());
            encode_compact_nullable_string(&mut response, None); // error_message
            response.put_i32(node_id);
            encode_compact_string(&mut response, host);
            response.put_i32(port);
            encode_empty_tagged_fields(&mut response);
        }
    } else {
        // Legacy protocol
        if header.api_version >= 1 {
            response.put_i32(0); // throttle time
        }

        response.put_i16(ErrorCode::None.as_i16());

        if header.api_version >= 1 {
            encode_nullable_string(&mut response, None); // error_message
        }

        response.put_i32(node_id);
        encode_string(&mut response, host);
        response.put_i32(port);
    }

    Ok(response)
}

/// Handle JoinGroup request (API 11)
pub async fn handle_join_group(
    state: &KafkaServerState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    // Parse request
    let (group_id, session_timeout_ms, rebalance_timeout_ms, member_id, protocol_type, protocols) =
        if header.api_version >= 6 {
            // Compact protocol
            let group_id = parse_compact_string(body)?;
            let session_timeout_ms = body.get_i32();
            let rebalance_timeout_ms = body.get_i32();
            let member_id = parse_compact_string(body)?;
            let _group_instance_id = parse_compact_nullable_string(body)?;
            let protocol_type = parse_compact_string(body)?;

            let protocols = parse_compact_array(body, |b| {
                let name = parse_compact_string(b)?;
                let metadata = parse_compact_nullable_bytes(b)?.unwrap_or_default();
                skip_tagged_fields(b)?;
                Ok(MemberProtocol { name, metadata })
            })?;

            let _reason = if header.api_version >= 8 {
                parse_compact_nullable_string(body)?
            } else {
                None
            };

            skip_tagged_fields(body)?;

            (
                group_id,
                session_timeout_ms,
                rebalance_timeout_ms,
                member_id,
                protocol_type,
                protocols,
            )
        } else {
            // Legacy protocol
            let group_id = parse_string(body)?;
            let session_timeout_ms = body.get_i32();
            let rebalance_timeout_ms = if header.api_version >= 1 {
                body.get_i32()
            } else {
                session_timeout_ms
            };
            let member_id = parse_string(body)?;

            let _group_instance_id = if header.api_version >= 5 {
                parse_nullable_string(body)?
            } else {
                None
            };

            let protocol_type = parse_string(body)?;

            let protocols = parse_array(body, |b| {
                let name = parse_string(b)?;
                let metadata = parse_nullable_bytes(b)?.unwrap_or_default();
                Ok(MemberProtocol { name, metadata })
            })?;

            (
                group_id,
                session_timeout_ms,
                rebalance_timeout_ms,
                member_id,
                protocol_type,
                protocols,
            )
        };

    debug!(
        "JoinGroup: group={}, member={}, protocol_type={}",
        group_id, member_id, protocol_type
    );

    // Process join request
    let member_id_opt = if member_id.is_empty() {
        None
    } else {
        Some(member_id.as_str())
    };

    let result = state
        .group_coordinator
        .join_group(
            &group_id,
            member_id_opt,
            header.client_id.as_deref().unwrap_or("unknown"),
            "unknown", // client_host
            session_timeout_ms,
            rebalance_timeout_ms,
            &protocol_type,
            protocols,
        )
        .await?;

    // Build response
    let mut response = BytesMut::new();

    if header.api_version >= 6 {
        // Compact protocol
        response.put_i32(0); // throttle time
        response.put_i16(result.error_code.as_i16());
        response.put_i32(result.generation_id);
        encode_compact_nullable_string(&mut response, result.protocol_type.as_deref());
        encode_compact_nullable_string(&mut response, result.protocol_name.as_deref());
        encode_compact_string(&mut response, &result.leader);
        if header.api_version >= 7 {
            response.put_u8(0); // skip_assignment
        }
        encode_compact_string(&mut response, &result.member_id);

        // Members array
        encode_unsigned_varint(&mut response, (result.members.len() + 1) as u64);
        for member in &result.members {
            encode_compact_string(&mut response, &member.member_id);
            encode_compact_nullable_string(&mut response, None); // group_instance_id
            encode_compact_nullable_bytes(&mut response, Some(&member.metadata));
            encode_empty_tagged_fields(&mut response);
        }

        encode_empty_tagged_fields(&mut response);
    } else {
        // Legacy protocol
        if header.api_version >= 2 {
            response.put_i32(0); // throttle time
        }

        response.put_i16(result.error_code.as_i16());
        response.put_i32(result.generation_id);
        encode_string(&mut response, result.protocol_name.as_deref().unwrap_or(""));
        encode_string(&mut response, &result.leader);
        encode_string(&mut response, &result.member_id);

        // Members array
        response.put_i32(result.members.len() as i32);
        for member in &result.members {
            encode_string(&mut response, &member.member_id);
            if header.api_version >= 5 {
                encode_nullable_string(&mut response, None); // group_instance_id
            }
            encode_nullable_bytes(&mut response, Some(&member.metadata));
        }
    }

    Ok(response)
}

/// Handle SyncGroup request (API 14)
pub async fn handle_sync_group(
    state: &KafkaServerState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    // Parse request
    let (group_id, generation_id, member_id, assignments) = if header.api_version >= 4 {
        // Compact protocol
        let group_id = parse_compact_string(body)?;
        let generation_id = body.get_i32();
        let member_id = parse_compact_string(body)?;
        let _group_instance_id = parse_compact_nullable_string(body)?;
        let _protocol_type = if header.api_version >= 5 {
            parse_compact_nullable_string(body)?
        } else {
            None
        };
        let _protocol_name = if header.api_version >= 5 {
            parse_compact_nullable_string(body)?
        } else {
            None
        };

        let assignments = parse_compact_array(body, |b| {
            let member_id = parse_compact_string(b)?;
            let assignment = parse_compact_nullable_bytes(b)?.unwrap_or_default();
            skip_tagged_fields(b)?;
            Ok(SyncGroupAssignment {
                member_id,
                assignment,
            })
        })?;

        skip_tagged_fields(body)?;

        (group_id, generation_id, member_id, assignments)
    } else {
        // Legacy protocol
        let group_id = parse_string(body)?;
        let generation_id = body.get_i32();
        let member_id = parse_string(body)?;

        let _group_instance_id = if header.api_version >= 3 {
            parse_nullable_string(body)?
        } else {
            None
        };

        let assignments = parse_array(body, |b| {
            let member_id = parse_string(b)?;
            let assignment = parse_nullable_bytes(b)?.unwrap_or_default();
            Ok(SyncGroupAssignment {
                member_id,
                assignment,
            })
        })?;

        (group_id, generation_id, member_id, assignments)
    };

    debug!(
        "SyncGroup: group={}, generation={}, member={}",
        group_id, generation_id, member_id
    );

    // Process sync request
    let result = state
        .group_coordinator
        .sync_group(&group_id, generation_id, &member_id, assignments)
        .await?;

    // Build response
    let mut response = BytesMut::new();

    if header.api_version >= 4 {
        // Compact protocol
        response.put_i32(0); // throttle time
        response.put_i16(result.error_code.as_i16());
        if header.api_version >= 5 {
            encode_compact_nullable_string(&mut response, None); // protocol_type
            encode_compact_nullable_string(&mut response, None); // protocol_name
        }
        encode_compact_nullable_bytes(&mut response, Some(&result.assignment));
        encode_empty_tagged_fields(&mut response);
    } else {
        // Legacy protocol
        if header.api_version >= 1 {
            response.put_i32(0); // throttle time
        }
        response.put_i16(result.error_code.as_i16());
        encode_nullable_bytes(&mut response, Some(&result.assignment));
    }

    Ok(response)
}

/// Handle Heartbeat request (API 12)
pub async fn handle_heartbeat(
    state: &KafkaServerState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    // Parse request
    let (group_id, generation_id, member_id) = if header.api_version >= 4 {
        // Compact protocol
        let group_id = parse_compact_string(body)?;
        let generation_id = body.get_i32();
        let member_id = parse_compact_string(body)?;
        let _group_instance_id = parse_compact_nullable_string(body)?;
        skip_tagged_fields(body)?;
        (group_id, generation_id, member_id)
    } else {
        // Legacy protocol
        let group_id = parse_string(body)?;
        let generation_id = body.get_i32();
        let member_id = parse_string(body)?;
        let _group_instance_id = if header.api_version >= 3 {
            parse_nullable_string(body)?
        } else {
            None
        };
        (group_id, generation_id, member_id)
    };

    debug!(
        "Heartbeat: group={}, generation={}, member={}",
        group_id, generation_id, member_id
    );

    // Process heartbeat
    let error_code = state
        .group_coordinator
        .heartbeat(&group_id, generation_id, &member_id)
        .await?;

    // Build response
    let mut response = BytesMut::new();

    if header.api_version >= 4 {
        // Compact protocol
        response.put_i32(0); // throttle time
        response.put_i16(error_code.as_i16());
        encode_empty_tagged_fields(&mut response);
    } else {
        // Legacy protocol
        if header.api_version >= 1 {
            response.put_i32(0); // throttle time
        }
        response.put_i16(error_code.as_i16());
    }

    Ok(response)
}

/// Handle LeaveGroup request (API 13)
pub async fn handle_leave_group(
    state: &KafkaServerState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    // Parse request
    let (group_id, members) = if header.api_version >= 4 {
        // Compact protocol - v3+ supports multiple members
        let group_id = parse_compact_string(body)?;
        let members = parse_compact_array(body, |b| {
            let member_id = parse_compact_string(b)?;
            let _group_instance_id = parse_compact_nullable_string(b)?;
            let _reason = if header.api_version >= 5 {
                parse_compact_nullable_string(b)?
            } else {
                None
            };
            skip_tagged_fields(b)?;
            Ok(member_id)
        })?;
        skip_tagged_fields(body)?;
        (group_id, members)
    } else if header.api_version >= 3 {
        // v3 with multiple members
        let group_id = parse_string(body)?;
        let members = parse_array(body, |b| {
            let member_id = parse_string(b)?;
            let _group_instance_id = parse_nullable_string(b)?;
            Ok(member_id)
        })?;
        (group_id, members)
    } else {
        // Legacy protocol - single member
        let group_id = parse_string(body)?;
        let member_id = parse_string(body)?;
        (group_id, vec![member_id])
    };

    debug!("LeaveGroup: group={}, members={:?}", group_id, members);

    // Process leave requests
    let mut member_responses = Vec::new();
    for member_id in &members {
        let error_code = state
            .group_coordinator
            .leave_group(&group_id, member_id)
            .await?;
        member_responses.push((member_id.clone(), error_code));
    }

    // Build response
    let mut response = BytesMut::new();

    if header.api_version >= 4 {
        // Compact protocol
        response.put_i32(0); // throttle time
        response.put_i16(ErrorCode::None.as_i16()); // top-level error

        // Members array
        encode_unsigned_varint(&mut response, (member_responses.len() + 1) as u64);
        for (member_id, error_code) in &member_responses {
            encode_compact_string(&mut response, member_id);
            encode_compact_nullable_string(&mut response, None); // group_instance_id
            response.put_i16(error_code.as_i16());
            encode_empty_tagged_fields(&mut response);
        }

        encode_empty_tagged_fields(&mut response);
    } else if header.api_version >= 3 {
        // v3
        response.put_i32(0); // throttle time
        response.put_i16(ErrorCode::None.as_i16()); // top-level error

        // Members array
        response.put_i32(member_responses.len() as i32);
        for (member_id, error_code) in &member_responses {
            encode_string(&mut response, member_id);
            encode_nullable_string(&mut response, None); // group_instance_id
            response.put_i16(error_code.as_i16());
        }
    } else {
        // Legacy protocol
        if header.api_version >= 1 {
            response.put_i32(0); // throttle time
        }
        // Use first member's error
        let error_code = member_responses
            .first()
            .map(|(_, e)| *e)
            .unwrap_or(ErrorCode::None);
        response.put_i16(error_code.as_i16());
    }

    Ok(response)
}

/// Handle DescribeGroups request (API 15)
pub async fn handle_describe_groups(
    state: &KafkaServerState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    // Parse request
    let groups = if header.api_version >= 5 {
        // Compact protocol
        let groups = parse_compact_array(body, |b| {
            let group_id = parse_compact_string(b)?;
            skip_tagged_fields(b)?;
            Ok(group_id)
        })?;
        let _include_authorized_operations = body.get_i8();
        skip_tagged_fields(body)?;
        groups
    } else {
        // Legacy protocol
        parse_array(body, |b| parse_string(b))?
    };

    debug!("DescribeGroups: groups={:?}", groups);

    // Get group details
    let mut group_responses = Vec::new();
    for group_id in &groups {
        let result = state.group_coordinator.describe_group(group_id).await;
        group_responses.push((group_id.clone(), result));
    }

    // Build response
    let mut response = BytesMut::new();

    if header.api_version >= 5 {
        // Compact protocol
        response.put_i32(0); // throttle time

        encode_unsigned_varint(&mut response, (group_responses.len() + 1) as u64);

        for (group_id, result) in &group_responses {
            match result {
                Some(r) => {
                    response.put_i16(r.error_code.as_i16());
                    encode_compact_string(&mut response, &r.group_id);
                    encode_compact_string(&mut response, &r.state);
                    encode_compact_string(&mut response, &r.protocol_type);
                    encode_compact_string(&mut response, &r.protocol);

                    // Members
                    encode_unsigned_varint(&mut response, (r.members.len() + 1) as u64);
                    for member in &r.members {
                        encode_compact_string(&mut response, &member.member_id);
                        encode_compact_nullable_string(&mut response, None); // group_instance_id
                        encode_compact_string(&mut response, &member.client_id);
                        encode_compact_string(&mut response, &member.client_host);
                        encode_compact_nullable_bytes(&mut response, Some(&member.member_metadata));
                        encode_compact_nullable_bytes(
                            &mut response,
                            Some(&member.member_assignment),
                        );
                        encode_empty_tagged_fields(&mut response);
                    }

                    response.put_i32(-2147483648); // authorized_operations
                    encode_empty_tagged_fields(&mut response);
                }
                None => {
                    response.put_i16(ErrorCode::GroupIdNotFound.as_i16());
                    encode_compact_string(&mut response, group_id);
                    encode_compact_string(&mut response, "Dead");
                    encode_compact_string(&mut response, "");
                    encode_compact_string(&mut response, "");
                    encode_unsigned_varint(&mut response, 1); // empty members
                    response.put_i32(-2147483648); // authorized_operations
                    encode_empty_tagged_fields(&mut response);
                }
            }
        }

        encode_empty_tagged_fields(&mut response);
    } else {
        // Legacy protocol
        if header.api_version >= 1 {
            response.put_i32(0); // throttle time
        }

        response.put_i32(group_responses.len() as i32);

        for (group_id, result) in &group_responses {
            match result {
                Some(r) => {
                    response.put_i16(r.error_code.as_i16());
                    encode_string(&mut response, &r.group_id);
                    encode_string(&mut response, &r.state);
                    encode_string(&mut response, &r.protocol_type);
                    encode_string(&mut response, &r.protocol);

                    // Members
                    response.put_i32(r.members.len() as i32);
                    for member in &r.members {
                        encode_string(&mut response, &member.member_id);
                        if header.api_version >= 4 {
                            encode_nullable_string(&mut response, None); // group_instance_id
                        }
                        encode_string(&mut response, &member.client_id);
                        encode_string(&mut response, &member.client_host);
                        encode_nullable_bytes(&mut response, Some(&member.member_metadata));
                        encode_nullable_bytes(&mut response, Some(&member.member_assignment));
                    }

                    if header.api_version >= 3 {
                        response.put_i32(-2147483648); // authorized_operations
                    }
                }
                None => {
                    response.put_i16(ErrorCode::GroupIdNotFound.as_i16());
                    encode_string(&mut response, group_id);
                    encode_string(&mut response, "Dead");
                    encode_string(&mut response, "");
                    encode_string(&mut response, "");
                    response.put_i32(0); // empty members

                    if header.api_version >= 3 {
                        response.put_i32(-2147483648);
                    }
                }
            }
        }
    }

    Ok(response)
}

/// Handle ListGroups request (API 16)
pub async fn handle_list_groups(
    state: &KafkaServerState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    // Parse request
    if header.api_version >= 4 {
        // Compact protocol
        let _states_filter = parse_compact_array(body, |b| {
            let state = parse_compact_string(b)?;
            skip_tagged_fields(b)?;
            Ok(state)
        })?;
        skip_tagged_fields(body)?;
    } else if header.api_version >= 4 {
        // v4 with states filter
        let _states_filter = parse_array(body, |b| parse_string(b))?;
    }

    debug!("ListGroups");

    // Get all groups
    let groups = state.group_coordinator.list_groups().await;

    // Build response
    let mut response = BytesMut::new();

    if header.api_version >= 4 {
        // Compact protocol
        response.put_i32(0); // throttle time
        response.put_i16(ErrorCode::None.as_i16());

        encode_unsigned_varint(&mut response, (groups.len() + 1) as u64);
        for group in &groups {
            encode_compact_string(&mut response, &group.group_id);
            encode_compact_string(&mut response, &group.protocol_type);
            encode_compact_string(&mut response, &group.group_state);
            encode_empty_tagged_fields(&mut response);
        }

        encode_empty_tagged_fields(&mut response);
    } else {
        // Legacy protocol
        if header.api_version >= 1 {
            response.put_i32(0); // throttle time
        }
        response.put_i16(ErrorCode::None.as_i16());

        response.put_i32(groups.len() as i32);
        for group in &groups {
            encode_string(&mut response, &group.group_id);
            encode_string(&mut response, &group.protocol_type);
            if header.api_version >= 4 {
                encode_string(&mut response, &group.group_state);
            }
        }
    }

    Ok(response)
}
