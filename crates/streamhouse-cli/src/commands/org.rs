//! Organization and API key management commands
//!
//! These commands use the REST API to manage organizations and their API keys.

use anyhow::{Context, Result};
use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::auth::AuthManager;
use crate::rest_client::RestClient;

#[derive(Subcommand)]
pub enum OrgCommands {
    /// Switch active organization (changes which API key is used)
    Switch {
        /// Organization name or slug (interactive picker if omitted)
        #[arg(num_args = 1.., value_delimiter = ' ')]
        name: Option<Vec<String>>,
    },
    /// Create a new organization
    Create {
        /// Organization name
        name: String,
        /// Organization slug
        #[arg(long)]
        slug: Option<String>,
        /// Plan (free, pro, enterprise)
        #[arg(long, default_value = "free")]
        plan: String,
    },
    /// List all organizations
    List,
    /// Get organization details
    Get {
        /// Organization ID or slug
        org: String,
    },
    /// Update an organization
    Update {
        /// Organization ID or slug
        org: String,
        /// New name
        #[arg(long)]
        name: Option<String>,
        /// New plan
        #[arg(long)]
        plan: Option<String>,
    },
    /// Delete an organization
    Delete {
        /// Organization ID or slug
        org: String,
    },
    /// Get organization quota
    Quota {
        /// Organization ID or slug
        org: String,
    },
    /// Get organization usage
    Usage {
        /// Organization ID or slug
        org: String,
    },
    /// Manage organization API keys
    ApiKey {
        /// Organization ID or slug
        #[arg(long)]
        org: String,
        #[command(subcommand)]
        command: OrgApiKeyCommands,
    },
}

#[derive(Subcommand)]
pub enum OrgApiKeyCommands {
    /// Create an API key for the organization
    Create {
        /// Key name
        name: String,
        /// Permissions (comma-separated)
        #[arg(long)]
        permissions: Option<String>,
        /// Scopes (comma-separated)
        #[arg(long)]
        scopes: Option<String>,
        /// Expiration in milliseconds
        #[arg(long)]
        expires_in_ms: Option<u64>,
    },
    /// List organization API keys
    List,
    /// Get an API key
    Get {
        /// API key ID
        id: String,
    },
    /// Revoke an API key
    Revoke {
        /// API key ID
        id: String,
    },
}

// Request/Response types

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct CreateOrganizationRequest {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    slug: Option<String>,
    plan: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct UpdateOrganizationRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct OrganizationResponse {
    id: String,
    name: String,
    slug: String,
    plan: String,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    created_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct QuotaResponse {
    organization_id: String,
    max_topics: i64,
    max_partitions_per_topic: i64,
    max_total_partitions: i64,
    max_storage_bytes: i64,
    max_retention_days: i64,
    max_produce_bytes_per_sec: i64,
    max_consume_bytes_per_sec: i64,
    max_requests_per_sec: i64,
    max_consumer_groups: i64,
    max_schemas: i64,
    max_connections: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct UsageResponse {
    organization_id: String,
    #[serde(default)]
    topics_count: i64,
    #[serde(default)]
    partitions_count: i64,
    #[serde(default)]
    storage_bytes: i64,
    #[serde(default)]
    produce_bytes_last_hour: i64,
    #[serde(default)]
    consume_bytes_last_hour: i64,
    #[serde(default)]
    requests_last_hour: i64,
    #[serde(default)]
    consumer_groups_count: i64,
    #[serde(default)]
    schemas_count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct CreateApiKeyRequest {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    permissions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scopes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_in_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ApiKeyCreatedResponse {
    id: String,
    #[serde(default)]
    organization_id: Option<String>,
    name: String,
    key: String,
    #[serde(default)]
    key_prefix: Option<String>,
    #[serde(default)]
    permissions: Vec<String>,
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(default)]
    expires_at: Option<serde_json::Value>,
    #[serde(default)]
    created_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ApiKeyResponse {
    id: String,
    name: String,
    #[serde(default)]
    permissions: Vec<String>,
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(default)]
    expires_at: Option<serde_json::Value>,
    #[serde(default)]
    created_at: Option<i64>,
}

/// Handle organization commands
pub async fn handle_org_command(
    command: OrgCommands,
    api_url: &str,
    api_key: Option<&str>,
    org_id: Option<&str>,
) -> Result<()> {
    let client = RestClient::with_org(api_url, api_key.map(String::from), org_id.map(String::from));

    match command {
        OrgCommands::Switch { name } => {
            let mut manager = AuthManager::new()?;
            let orgs: Vec<_> = manager.list_orgs().into_iter().cloned().collect();

            if orgs.is_empty() {
                anyhow::bail!("No organizations stored. Run `stm auth login` first.");
            }

            let slug = if let Some(name_parts) = name {
                // Name provided — match by slug or name
                let query = name_parts.join(" ");

                // Try exact slug match
                if orgs.iter().any(|o| o.slug == query) {
                    query
                } else {
                    // Try case-insensitive name match
                    let query_lower = query.to_lowercase();
                    orgs.iter()
                        .find(|o| o.name.to_lowercase() == query_lower)
                        .map(|o| o.slug.clone())
                        .ok_or_else(|| {
                            let available: Vec<&str> =
                                orgs.iter().map(|o| o.name.as_str()).collect();
                            anyhow::anyhow!(
                                "Organization '{}' not found. Available: {}",
                                query,
                                available.join(", ")
                            )
                        })?
                }
            } else {
                // Interactive picker
                let active_slug = manager.active_org_slug().map(|s| s.to_string());
                let items: Vec<String> = orgs
                    .iter()
                    .map(|o| {
                        let marker = if active_slug.as_deref() == Some(&o.slug) {
                            " (active)"
                        } else {
                            ""
                        };
                        format!("{}{}", o.name, marker)
                    })
                    .collect();

                let default = orgs
                    .iter()
                    .position(|o| active_slug.as_deref() == Some(&o.slug))
                    .unwrap_or(0);

                let selection = dialoguer::Select::new()
                    .with_prompt("Select organization")
                    .items(&items)
                    .default(default)
                    .interact()
                    .context("Selection cancelled")?;

                orgs[selection].slug.clone()
            };

            manager.switch_org(&slug)?;
            let org = manager.active_org().unwrap();
            println!("Switched to: {}", org.name);
            return Ok(());
        }
        OrgCommands::Create { name, slug, plan } => {
            let req = CreateOrganizationRequest {
                name: name.clone(),
                slug,
                plan,
            };
            let resp: OrganizationResponse = client
                .post("/api/v1/organizations", &req)
                .await
                .context("Failed to create organization")?;
            println!("Organization created:");
            print_org(&resp);
        }
        OrgCommands::List => {
            let orgs: Vec<OrganizationResponse> = client
                .get("/api/v1/organizations")
                .await
                .context("Failed to list organizations")?;

            if orgs.is_empty() {
                println!("No organizations found");
            } else {
                println!("Organizations ({}):", orgs.len());
                println!("{:<36} {:<20} {:<15} {:<10}", "ID", "Name", "Slug", "Plan");
                println!("{}", "-".repeat(81));
                for org in &orgs {
                    println!(
                        "{:<36} {:<20} {:<15} {:<10}",
                        org.id, org.name, org.slug, org.plan
                    );
                }
            }
        }
        OrgCommands::Get { org } => {
            let resp: OrganizationResponse = client
                .get(&format!("/api/v1/organizations/{}", org))
                .await
                .context("Organization not found")?;
            print_org(&resp);
        }
        OrgCommands::Update { org, name, plan } => {
            let req = UpdateOrganizationRequest { name, plan };
            let resp: OrganizationResponse = client
                .patch(&format!("/api/v1/organizations/{}", org), &req)
                .await
                .context("Failed to update organization")?;
            println!("Organization updated:");
            print_org(&resp);
        }
        OrgCommands::Delete { org } => {
            client
                .delete(&format!("/api/v1/organizations/{}", org))
                .await
                .context("Failed to delete organization")?;
            println!("Organization '{}' deleted", org);
        }
        OrgCommands::Quota { org } => {
            let quota: QuotaResponse = client
                .get(&format!("/api/v1/organizations/{}/quota", org))
                .await
                .context("Failed to get quota")?;

            println!("Quota for organization {}:", quota.organization_id);
            println!("  Max topics:            {}", quota.max_topics);
            println!(
                "  Max partitions/topic:  {}",
                quota.max_partitions_per_topic
            );
            println!("  Max total partitions:  {}", quota.max_total_partitions);
            println!("  Max storage:           {} bytes", quota.max_storage_bytes);
            println!("  Max retention:         {} days", quota.max_retention_days);
            println!(
                "  Max produce rate:      {} bytes/sec",
                quota.max_produce_bytes_per_sec
            );
            println!(
                "  Max consume rate:      {} bytes/sec",
                quota.max_consume_bytes_per_sec
            );
            println!("  Max requests/sec:      {}", quota.max_requests_per_sec);
            println!("  Max consumer groups:   {}", quota.max_consumer_groups);
            println!("  Max schemas:           {}", quota.max_schemas);
            println!("  Max connections:       {}", quota.max_connections);
        }
        OrgCommands::Usage { org } => {
            let usage: UsageResponse = client
                .get(&format!("/api/v1/organizations/{}/usage", org))
                .await
                .context("Failed to get usage")?;

            println!("Usage for organization {}:", usage.organization_id);
            println!("  Topics:             {}", usage.topics_count);
            println!("  Partitions:         {}", usage.partitions_count);
            println!("  Storage:            {} bytes", usage.storage_bytes);
            println!(
                "  Produce (last hr):  {} bytes",
                usage.produce_bytes_last_hour
            );
            println!(
                "  Consume (last hr):  {} bytes",
                usage.consume_bytes_last_hour
            );
            println!("  Requests (last hr): {}", usage.requests_last_hour);
            println!("  Consumer groups:    {}", usage.consumer_groups_count);
            println!("  Schemas:            {}", usage.schemas_count);
        }
        OrgCommands::ApiKey { org, command } => {
            handle_org_api_key_command(command, &org, &client).await?;
        }
    }

    Ok(())
}

async fn handle_org_api_key_command(
    command: OrgApiKeyCommands,
    org: &str,
    client: &RestClient,
) -> Result<()> {
    match command {
        OrgApiKeyCommands::Create {
            name,
            permissions,
            scopes,
            expires_in_ms,
        } => {
            let req = CreateApiKeyRequest {
                name: name.clone(),
                permissions: permissions
                    .map(|p| p.split(',').map(|s| s.trim().to_string()).collect()),
                scopes: scopes.map(|s| s.split(',').map(|s| s.trim().to_string()).collect()),
                expires_in_ms,
            };
            let resp: ApiKeyCreatedResponse = client
                .post(&format!("/api/v1/organizations/{}/api-keys", org), &req)
                .await
                .context("Failed to create API key")?;

            println!("API key created for organization {}:", org);
            println!("  ID:   {}", resp.id);
            println!("  Name: {}", resp.name);
            println!("  Key:  {}", resp.key);
            if !resp.permissions.is_empty() {
                println!("  Permissions: {}", resp.permissions.join(", "));
            }
            if !resp.scopes.is_empty() {
                println!("  Scopes: {}", resp.scopes.join(", "));
            }
            if let Some(ref expires) = resp.expires_at {
                println!("  Expires: {}", expires);
            }
            println!();
            println!("Save this key securely - it will not be shown again.");
        }
        OrgApiKeyCommands::List => {
            let keys: Vec<ApiKeyResponse> = client
                .get(&format!("/api/v1/organizations/{}/api-keys", org))
                .await
                .context("Failed to list API keys")?;

            if keys.is_empty() {
                println!("No API keys found for organization {}", org);
            } else {
                println!("API Keys for organization {} ({}):", org, keys.len());
                println!(
                    "{:<36} {:<20} {:<20} {:<20}",
                    "ID", "Name", "Created", "Expires"
                );
                println!("{}", "-".repeat(96));
                for key in keys {
                    let created = key
                        .created_at
                        .map(|ts| ts.to_string())
                        .unwrap_or_else(|| "—".to_string());
                    let expires = match &key.expires_at {
                        Some(v) => v.to_string(),
                        None => "never".to_string(),
                    };
                    println!(
                        "{:<36} {:<20} {:<20} {:<20}",
                        key.id, key.name, created, expires,
                    );
                }
            }
        }
        OrgApiKeyCommands::Get { id } => {
            let key: ApiKeyResponse = client
                .get(&format!("/api/v1/api-keys/{}", id))
                .await
                .context("Failed to get API key")?;

            println!("API Key:");
            println!("  ID:          {}", key.id);
            println!("  Name:        {}", key.name);
            if !key.permissions.is_empty() {
                println!("  Permissions: {}", key.permissions.join(", "));
            }
            if !key.scopes.is_empty() {
                println!("  Scopes:      {}", key.scopes.join(", "));
            }
            if let Some(created) = key.created_at {
                println!("  Created:     {}", created);
            }
            if let Some(ref expires) = key.expires_at {
                println!("  Expires:     {}", expires);
            }
        }
        OrgApiKeyCommands::Revoke { id } => {
            client
                .delete(&format!("/api/v1/api-keys/{}", id))
                .await
                .context("Failed to revoke API key")?;
            println!("API key '{}' revoked", id);
        }
    }

    Ok(())
}

fn print_org(org: &OrganizationResponse) {
    println!("  ID:      {}", org.id);
    println!("  Name:    {}", org.name);
    println!("  Slug:    {}", org.slug);
    println!("  Plan:    {}", org.plan);
    if let Some(ref status) = org.status {
        println!("  Status:  {}", status);
    }
    if let Some(created) = org.created_at {
        println!("  Created: {}", created);
    }
}
