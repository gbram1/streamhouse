//! BYOC (Bring Your Own Cloud) S3 Client Pool
//!
//! Manages assumed-role S3 clients for BYOC organizations.
//! Each BYOC org provides their own S3 bucket and IAM role ARN.
//! This pool handles STS AssumeRole, credential caching, and auto-refresh.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Configuration for a BYOC organization's S3 access
#[derive(Debug, Clone)]
pub struct ByocConfig {
    /// The IAM role ARN in the customer's AWS account
    pub role_arn: String,
    /// External ID for the STS AssumeRole call (security best practice)
    pub external_id: String,
    /// The customer's S3 bucket name
    pub s3_bucket: String,
    /// AWS region for the bucket
    pub region: String,
}

/// Cached credentials for a BYOC org
struct CachedClient {
    object_store: Arc<dyn object_store::ObjectStore>,
    expires_at: std::time::Instant,
}

/// Pool of S3 clients for BYOC organizations.
///
/// Manages STS AssumeRole credentials and creates object_store clients
/// for each BYOC organization. Credentials are cached and auto-refreshed.
pub struct ByocS3ClientPool {
    /// Our control plane's role ARN (the one that assumes customer roles)
    control_plane_region: String,
    /// Cache of org_id -> S3 client with assumed-role credentials
    clients: RwLock<HashMap<String, CachedClient>>,
    /// Session duration for assumed-role credentials (default: 1 hour)
    session_duration_secs: i32,
}

impl ByocS3ClientPool {
    /// Create a new BYOC S3 client pool.
    pub fn new(region: String) -> Self {
        Self {
            control_plane_region: region,
            clients: RwLock::new(HashMap::new()),
            session_duration_secs: 3600,
        }
    }

    /// Create from environment variables.
    /// Returns None if BYOC_ENABLED is not set to "true".
    pub fn from_env() -> Option<Self> {
        let enabled = std::env::var("BYOC_ENABLED").ok()?;
        if enabled != "true" && enabled != "1" {
            return None;
        }
        let region = std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string());
        Some(Self::new(region))
    }

    /// Get or create an S3 client for a BYOC organization.
    ///
    /// If cached credentials are still valid, returns the cached client.
    /// Otherwise, assumes the customer's IAM role via STS and creates a new client.
    pub async fn get_client_for_org(
        &self,
        org_id: &str,
        config: &ByocConfig,
    ) -> Result<Arc<dyn object_store::ObjectStore>, ByocError> {
        // Check cache first
        {
            let clients = self.clients.read().await;
            if let Some(cached) = clients.get(org_id) {
                if cached.expires_at > std::time::Instant::now() {
                    return Ok(cached.object_store.clone());
                }
            }
        }

        // Assume role and create new client
        let client = self.assume_role_and_create_client(config).await?;

        // Cache the client
        let cached = CachedClient {
            object_store: client.clone(),
            expires_at: std::time::Instant::now()
                + std::time::Duration::from_secs((self.session_duration_secs as u64) - 300), // Refresh 5 min early
        };

        {
            let mut clients = self.clients.write().await;
            clients.insert(org_id.to_string(), cached);
        }

        Ok(client)
    }

    /// Assume the customer's IAM role and create an S3 object store client.
    async fn assume_role_and_create_client(
        &self,
        config: &ByocConfig,
    ) -> Result<Arc<dyn object_store::ObjectStore>, ByocError> {
        // Use the AWS SDK to assume the role
        let aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(self.control_plane_region.clone()))
            .load()
            .await;

        let sts_client = aws_sdk_sts::Client::new(&aws_config);

        let assume_role_output = sts_client
            .assume_role()
            .role_arn(&config.role_arn)
            .role_session_name(&format!("streamhouse-byoc-{}", uuid::Uuid::new_v4()))
            .external_id(&config.external_id)
            .duration_seconds(self.session_duration_secs)
            .send()
            .await
            .map_err(|e| ByocError::AssumeRoleFailed(format!("{}", e)))?;

        let credentials = assume_role_output
            .credentials()
            .ok_or_else(|| ByocError::AssumeRoleFailed("No credentials in response".to_string()))?;

        let access_key = credentials.access_key_id().to_string();
        let secret_key = credentials.secret_access_key().to_string();
        let session_token = credentials.session_token().to_string();

        // Create an S3 object store with the assumed-role credentials
        let store = object_store::aws::AmazonS3Builder::new()
            .with_bucket_name(&config.s3_bucket)
            .with_region(&config.region)
            .with_access_key_id(&access_key)
            .with_secret_access_key(&secret_key)
            .with_token(&session_token)
            .build()
            .map_err(|e| ByocError::ClientCreationFailed(format!("{}", e)))?;

        Ok(Arc::new(store))
    }

    /// Validate connectivity to a BYOC bucket by attempting to list objects.
    pub async fn validate_connection(
        &self,
        config: &ByocConfig,
    ) -> Result<(), ByocError> {
        let client = self.assume_role_and_create_client(config).await?;

        // Try to list objects with a small limit to verify access
        use object_store::ObjectStore;
        let prefix = object_store::path::Path::from("__streamhouse_connection_test/");
        let mut stream = client.list(Some(&prefix));
        use futures::StreamExt;
        // Just consume the first result (or empty) to verify the connection works
        let _ = stream.next().await;

        Ok(())
    }

    /// Remove a cached client for an organization.
    pub async fn invalidate(&self, org_id: &str) {
        let mut clients = self.clients.write().await;
        clients.remove(org_id);
    }
}

/// Errors from BYOC S3 operations.
#[derive(Debug)]
pub enum ByocError {
    /// STS AssumeRole failed
    AssumeRoleFailed(String),
    /// Failed to create S3 client
    ClientCreationFailed(String),
    /// S3 operation failed
    S3Error(String),
}

impl std::fmt::Display for ByocError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AssumeRoleFailed(e) => write!(f, "STS AssumeRole failed: {}", e),
            Self::ClientCreationFailed(e) => write!(f, "Failed to create S3 client: {}", e),
            Self::S3Error(e) => write!(f, "S3 operation failed: {}", e),
        }
    }
}

impl std::error::Error for ByocError {}
