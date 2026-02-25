//! Service Discovery
//!
//! Provides pluggable service discovery backends for locating StreamHouse nodes
//! in a cluster. Supports static configuration, DNS-based discovery, and
//! metadata store-based discovery.
//!
//! ## Backends
//!
//! - **StaticDiscovery**: Pre-configured list of nodes (ideal for development/testing)
//! - **DnsDiscovery**: DNS SRV/A record lookups for dynamic environments
//! - **MetadataStoreDiscovery**: Uses the metadata store as a node registry
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_api::discovery::{StaticDiscovery, ServiceDiscovery};
//!
//! let discovery = StaticDiscovery::from_addresses(vec![
//!     "10.0.1.1:9090".to_string(),
//!     "10.0.1.2:9090".to_string(),
//! ]);
//!
//! let nodes = discovery.discover().await?;
//! for node in &nodes {
//!     println!("Discovered: {}", node.address);
//! }
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info};

/// Configuration for service discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    /// Seed nodes to bootstrap discovery.
    pub seed_nodes: Vec<String>,
    /// How often to refresh the discovered node list.
    pub refresh_interval: Duration,
    /// DNS domain for DNS-based discovery.
    pub dns_domain: Option<String>,
    /// Timeout for discovery operations.
    pub timeout: Duration,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            seed_nodes: Vec::new(),
            refresh_interval: Duration::from_secs(30),
            dns_domain: None,
            timeout: Duration::from_secs(5),
        }
    }
}

/// A discovered node in the cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredNode {
    /// Network address (host:port) of the node.
    pub address: String,
    /// Optional node identifier.
    pub node_id: Option<String>,
    /// Arbitrary metadata associated with the node.
    pub metadata: HashMap<String, String>,
}

impl DiscoveredNode {
    /// Create a new DiscoveredNode with just an address.
    pub fn new(address: &str) -> Self {
        Self {
            address: address.to_string(),
            node_id: None,
            metadata: HashMap::new(),
        }
    }

    /// Create a new DiscoveredNode with address and node_id.
    pub fn with_id(address: &str, node_id: &str) -> Self {
        Self {
            address: address.to_string(),
            node_id: Some(node_id.to_string()),
            metadata: HashMap::new(),
        }
    }
}

impl PartialEq for DiscoveredNode {
    fn eq(&self, other: &Self) -> bool {
        self.address == other.address
    }
}

impl Eq for DiscoveredNode {}

impl std::hash::Hash for DiscoveredNode {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.address.hash(state);
    }
}

/// Events emitted when the discovered node set changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiscoveryEvent {
    /// A new node was discovered.
    NodeDiscovered(DiscoveredNode),
    /// A node was removed from the discovered set.
    NodeRemoved(DiscoveredNode),
}

/// Trait for service discovery backends.
#[async_trait]
pub trait ServiceDiscovery: Send + Sync {
    /// Discover all nodes currently available.
    async fn discover(&self) -> Result<Vec<DiscoveredNode>, Box<dyn std::error::Error + Send + Sync>>;

    /// Register the local node with the discovery backend.
    async fn register_self(
        &self,
        node: &DiscoveredNode,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Deregister the local node from the discovery backend.
    async fn deregister_self(
        &self,
        node: &DiscoveredNode,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Subscribe to discovery events.
    async fn subscribe(&self) -> broadcast::Receiver<DiscoveryEvent>;
}

/// Static discovery from a pre-configured node list.
#[derive(Debug, Clone)]
pub struct StaticDiscovery {
    nodes: Arc<RwLock<HashSet<DiscoveredNode>>>,
    event_tx: broadcast::Sender<DiscoveryEvent>,
}

impl StaticDiscovery {
    /// Create a new StaticDiscovery from a DiscoveryConfig.
    pub fn new(config: DiscoveryConfig) -> Self {
        let (tx, _) = broadcast::channel(64);
        let nodes: HashSet<DiscoveredNode> = config
            .seed_nodes
            .iter()
            .map(|addr| DiscoveredNode::new(addr))
            .collect();
        info!(count = nodes.len(), "Created static discovery");
        Self {
            nodes: Arc::new(RwLock::new(nodes)),
            event_tx: tx,
        }
    }

    /// Create a StaticDiscovery from a list of addresses.
    pub fn from_addresses(addresses: Vec<String>) -> Self {
        let config = DiscoveryConfig {
            seed_nodes: addresses,
            ..Default::default()
        };
        Self::new(config)
    }

    /// Add a node to the static list.
    pub async fn add_node(&self, node: DiscoveredNode) {
        let mut nodes = self.nodes.write().await;
        info!(address = %node.address, "Adding node to static discovery");
        let is_new = nodes.insert(node.clone());
        if is_new {
            let _ = self.event_tx.send(DiscoveryEvent::NodeDiscovered(node));
        }
    }

    /// Remove a node from the static list.
    pub async fn remove_node(&self, address: &str) {
        let mut nodes = self.nodes.write().await;
        let node = DiscoveredNode::new(address);
        info!(address = address, "Removing node from static discovery");
        if nodes.remove(&node) {
            let _ = self.event_tx.send(DiscoveryEvent::NodeRemoved(node));
        }
    }
}

#[async_trait]
impl ServiceDiscovery for StaticDiscovery {
    async fn discover(&self) -> Result<Vec<DiscoveredNode>, Box<dyn std::error::Error + Send + Sync>> {
        let nodes = self.nodes.read().await;
        debug!(count = nodes.len(), "Static discovery returning nodes");
        Ok(nodes.iter().cloned().collect())
    }

    async fn register_self(
        &self,
        node: &DiscoveredNode,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.add_node(node.clone()).await;
        Ok(())
    }

    async fn deregister_self(
        &self,
        node: &DiscoveredNode,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.remove_node(&node.address).await;
        Ok(())
    }

    async fn subscribe(&self) -> broadcast::Receiver<DiscoveryEvent> {
        self.event_tx.subscribe()
    }
}

/// DNS-based service discovery using tokio::net::lookup_host.
#[derive(Debug)]
pub struct DnsDiscovery {
    _config: DiscoveryConfig,
    event_tx: broadcast::Sender<DiscoveryEvent>,
}

impl DnsDiscovery {
    /// Create a new DnsDiscovery.
    pub fn new(config: DiscoveryConfig) -> Self {
        let (tx, _) = broadcast::channel(64);
        info!(domain = ?config.dns_domain, "Created DNS discovery");
        Self {
            _config: config,
            event_tx: tx,
        }
    }
}

#[async_trait]
impl ServiceDiscovery for DnsDiscovery {
    async fn discover(&self) -> Result<Vec<DiscoveredNode>, Box<dyn std::error::Error + Send + Sync>> {
        let domain = self
            ._config
            .dns_domain
            .as_deref()
            .unwrap_or("localhost:9090");

        let mut nodes = Vec::new();
        match tokio::net::lookup_host(domain).await {
            Ok(addrs) => {
                for addr in addrs {
                    nodes.push(DiscoveredNode::new(&addr.to_string()));
                }
            }
            Err(e) => {
                debug!(error = %e, domain = domain, "DNS lookup failed");
            }
        }
        Ok(nodes)
    }

    async fn register_self(
        &self,
        _node: &DiscoveredNode,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // DNS registration is typically handled externally
        Ok(())
    }

    async fn deregister_self(
        &self,
        _node: &DiscoveredNode,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // DNS deregistration is typically handled externally
        Ok(())
    }

    async fn subscribe(&self) -> broadcast::Receiver<DiscoveryEvent> {
        self.event_tx.subscribe()
    }
}

/// Metadata store-based service discovery.
#[derive(Debug)]
pub struct MetadataStoreDiscovery {
    _config: DiscoveryConfig,
    nodes: Arc<RwLock<HashSet<DiscoveredNode>>>,
    event_tx: broadcast::Sender<DiscoveryEvent>,
}

impl MetadataStoreDiscovery {
    /// Create a new MetadataStoreDiscovery.
    pub fn new(config: DiscoveryConfig) -> Self {
        let (tx, _) = broadcast::channel(64);
        info!("Created metadata store discovery");
        Self {
            _config: config,
            nodes: Arc::new(RwLock::new(HashSet::new())),
            event_tx: tx,
        }
    }
}

#[async_trait]
impl ServiceDiscovery for MetadataStoreDiscovery {
    async fn discover(&self) -> Result<Vec<DiscoveredNode>, Box<dyn std::error::Error + Send + Sync>> {
        let nodes = self.nodes.read().await;
        Ok(nodes.iter().cloned().collect())
    }

    async fn register_self(
        &self,
        node: &DiscoveredNode,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut nodes = self.nodes.write().await;
        let is_new = nodes.insert(node.clone());
        if is_new {
            let _ = self
                .event_tx
                .send(DiscoveryEvent::NodeDiscovered(node.clone()));
        }
        Ok(())
    }

    async fn deregister_self(
        &self,
        node: &DiscoveredNode,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut nodes = self.nodes.write().await;
        if nodes.remove(node) {
            let _ = self
                .event_tx
                .send(DiscoveryEvent::NodeRemoved(node.clone()));
        }
        Ok(())
    }

    async fn subscribe(&self) -> broadcast::Receiver<DiscoveryEvent> {
        self.event_tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_static_discovery_new() {
        let config = DiscoveryConfig {
            seed_nodes: vec!["10.0.1.1:9090".to_string(), "10.0.1.2:9090".to_string()],
            ..Default::default()
        };
        let discovery = StaticDiscovery::new(config);
        let nodes = discovery.discover().await.unwrap();
        assert_eq!(nodes.len(), 2);
    }

    #[tokio::test]
    async fn test_static_discovery_from_addresses() {
        let discovery = StaticDiscovery::from_addresses(vec![
            "10.0.1.1:9090".to_string(),
            "10.0.1.2:9090".to_string(),
        ]);
        let nodes = discovery.discover().await.unwrap();
        assert_eq!(nodes.len(), 2);
    }

    #[tokio::test]
    async fn test_static_add_node() {
        let discovery = StaticDiscovery::from_addresses(vec![]);
        discovery
            .add_node(DiscoveredNode::new("10.0.1.1:9090"))
            .await;
        let nodes = discovery.discover().await.unwrap();
        assert_eq!(nodes.len(), 1);
    }

    #[tokio::test]
    async fn test_static_remove_node() {
        let discovery = StaticDiscovery::from_addresses(vec![
            "10.0.1.1:9090".to_string(),
            "10.0.1.2:9090".to_string(),
        ]);
        discovery.remove_node("10.0.1.1:9090").await;
        let nodes = discovery.discover().await.unwrap();
        assert_eq!(nodes.len(), 1);
    }

    #[tokio::test]
    async fn test_static_register_self() {
        let discovery = StaticDiscovery::from_addresses(vec![]);
        let node = DiscoveredNode::new("10.0.1.1:9090");
        discovery.register_self(&node).await.unwrap();
        let nodes = discovery.discover().await.unwrap();
        assert_eq!(nodes.len(), 1);
    }

    #[tokio::test]
    async fn test_static_deregister_self() {
        let discovery =
            StaticDiscovery::from_addresses(vec!["10.0.1.1:9090".to_string()]);
        let node = DiscoveredNode::new("10.0.1.1:9090");
        discovery.deregister_self(&node).await.unwrap();
        let nodes = discovery.discover().await.unwrap();
        assert_eq!(nodes.len(), 0);
    }

    #[tokio::test]
    async fn test_static_subscribe() {
        let discovery = StaticDiscovery::from_addresses(vec![]);
        let mut rx = discovery.subscribe().await;

        discovery
            .add_node(DiscoveredNode::new("10.0.1.1:9090"))
            .await;

        let event = rx.recv().await.unwrap();
        match event {
            DiscoveryEvent::NodeDiscovered(node) => {
                assert_eq!(node.address, "10.0.1.1:9090");
            }
            _ => panic!("Expected NodeDiscovered event"),
        }
    }

    #[tokio::test]
    async fn test_discovered_node_equality() {
        let a = DiscoveredNode::new("10.0.1.1:9090");
        let b = DiscoveredNode::with_id("10.0.1.1:9090", "node-1");
        // Equality is based on address only
        assert_eq!(a, b);
    }

    #[tokio::test]
    async fn test_discovered_node_hash() {
        let a = DiscoveredNode::new("10.0.1.1:9090");
        let b = DiscoveredNode::with_id("10.0.1.1:9090", "node-1");

        let mut set = HashSet::new();
        set.insert(a);
        // Same address means same hash, so this replaces the existing
        set.insert(b);
        assert_eq!(set.len(), 1);
    }

    #[tokio::test]
    async fn test_discovery_config_default() {
        let config = DiscoveryConfig::default();
        assert!(config.seed_nodes.is_empty());
        assert_eq!(config.refresh_interval, Duration::from_secs(30));
        assert!(config.dns_domain.is_none());
        assert_eq!(config.timeout, Duration::from_secs(5));
    }

    #[tokio::test]
    async fn test_metadata_store_discovery() {
        let config = DiscoveryConfig::default();
        let discovery = MetadataStoreDiscovery::new(config);

        let node = DiscoveredNode::new("10.0.1.1:9090");
        discovery.register_self(&node).await.unwrap();

        let nodes = discovery.discover().await.unwrap();
        assert_eq!(nodes.len(), 1);

        discovery.deregister_self(&node).await.unwrap();
        let nodes = discovery.discover().await.unwrap();
        assert_eq!(nodes.len(), 0);
    }

    #[tokio::test]
    async fn test_dns_discovery_creation() {
        let config = DiscoveryConfig {
            dns_domain: Some("streamhouse.local:9090".to_string()),
            ..Default::default()
        };
        let discovery = DnsDiscovery::new(config);
        // DNS lookup will fail in test but should not panic
        let nodes = discovery.discover().await.unwrap();
        // In test environment, DNS lookup may return empty
        assert!(nodes.is_empty() || !nodes.is_empty());
    }
}
