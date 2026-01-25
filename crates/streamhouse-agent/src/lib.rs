//! StreamHouse Agent - Multi-Agent Coordination
//!
//! This crate implements the multi-agent architecture for StreamHouse Phase 4.
//!
//! ## Architecture
//!
//! StreamHouse agents are **stateless** - they can be killed and restarted anytime without
//! data loss. Coordination happens through the metadata store using lease-based leadership.
//!
//! ## Components
//!
//! - **Agent**: Main agent struct with lifecycle management
//! - **LeaseManager**: Partition leadership using time-based leases
//! - **HeartbeatTask**: Background task for agent liveness
//!
//! ## Example Usage
//!
//! ```rust,no_run
//! use streamhouse_agent::Agent;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let agent = Agent::builder()
//!     .agent_id("agent-us-east-1a-001")
//!     .address("10.0.1.5:9090")
//!     .availability_zone("us-east-1a")
//!     .agent_group("prod")
//!     .build()
//!     .await?;
//!
//! // Start agent (registers + begins heartbeat)
//! agent.start().await?;
//!
//! // ... serve traffic ...
//!
//! // Graceful shutdown (flushes data + deregisters)
//! agent.stop().await?;
//! # Ok(())
//! # }
//! ```

pub mod agent;
pub mod error;
pub mod heartbeat;
pub mod lease_manager;

pub use agent::{Agent, AgentBuilder, AgentConfig};
pub use error::{AgentError, Result};
pub use heartbeat::HeartbeatTask;
pub use lease_manager::{validate_epoch, LeaseManager};
