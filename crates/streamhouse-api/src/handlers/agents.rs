//! Agent monitoring endpoints
//!
//! These endpoints expose agent information (IDs, addresses, availability zones).
//! When auth is enabled, they require a valid API key like any other endpoint.
//! No special admin privileges needed — useful for BYOC and managed deployments
//! where users need visibility into their own agents.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use crate::{models::*, AppState};

#[utoipa::path(
    get,
    path = "/api/v1/agents",
    responses(
        (status = 200, description = "List all agents", body = Vec<Agent>),
    ),
    tag = "agents"
)]
pub async fn list_agents(
    State(state): State<AppState>,
) -> Result<Json<Vec<Agent>>, StatusCode> {
    let agents = state
        .metadata
        .list_agents(None, None) // No filters
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get all leases (non-fatal — agents can exist without leases)
    let lease_counts = match state.metadata.list_partition_leases(None, None).await {
        Ok(all_leases) => {
            let mut counts = std::collections::HashMap::new();
            for lease in all_leases {
                *counts.entry(lease.leader_agent_id.clone()).or_insert(0u32) += 1;
            }
            counts
        }
        Err(_) => std::collections::HashMap::new(),
    };

    // Build response with lease counts
    let response = agents
        .into_iter()
        .map(|agent| Agent {
            active_leases: *lease_counts.get(&agent.agent_id).unwrap_or(&0),
            agent_id: agent.agent_id,
            address: agent.address,
            availability_zone: agent.availability_zone,
            agent_group: agent.agent_group,
            last_heartbeat: agent.last_heartbeat,
            started_at: agent.started_at,
        })
        .collect();

    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/api/v1/agents/{id}",
    params(
        ("id" = String, Path, description = "Agent ID")
    ),
    responses(
        (status = 200, description = "Agent details", body = Agent),
        (status = 404, description = "Agent not found")
    ),
    tag = "agents"
)]
pub async fn get_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Agent>, StatusCode> {
    let agent = state
        .metadata
        .get_agent(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Count leases for this agent
    let leases = state
        .metadata
        .list_partition_leases(None, Some(&agent.agent_id))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(Agent {
        agent_id: agent.agent_id,
        address: agent.address,
        availability_zone: agent.availability_zone,
        agent_group: agent.agent_group,
        last_heartbeat: agent.last_heartbeat,
        started_at: agent.started_at,
        active_leases: leases.len() as u32,
    }))
}
