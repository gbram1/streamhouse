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
        /// Organization slug to switch to
        slug: String,
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
#[serde(rename_all = "camelCase")]
struct CreateOrganizationRequest {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    slug: Option<String>,
    plan: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateOrganizationRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrganizationResponse {
    id: String,
    name: String,
    slug: String,
    plan: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaResponse {
    org_id: String,
    plan: String,
    topics_limit: i64,
    topics_used: i64,
    storage_limit_bytes: i64,
    storage_used_bytes: i64,
    throughput_limit_bytes_per_sec: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageResponse {
    org_id: String,
    period: String,
    records_produced: i64,
    records_consumed: i64,
    storage_bytes: i64,
    api_calls: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
struct ApiKeyCreatedResponse {
    id: String,
    name: String,
    key: String,
    #[serde(default)]
    permissions: Vec<String>,
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(default)]
    expires_at: Option<String>,
    created_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiKeyResponse {
    id: String,
    name: String,
    #[serde(default)]
    permissions: Vec<String>,
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(default)]
    expires_at: Option<String>,
    created_at: String,
}

/// Handle organization commands
pub async fn handle_org_command(
    command: OrgCommands,
    api_url: &str,
    api_key: Option<&str>,
) -> Result<()> {
    let client = RestClient::with_api_key(api_url, api_key.map(String::from));

    match command {
        OrgCommands::Switch { slug } => {
            let mut manager = AuthManager::new()?;
            manager.switch_org(&slug)?;
            let org = manager.active_org().unwrap();
            println!("Switched to organization: {} ({})", org.name, org.slug);
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
                println!(
                    "{:<36} {:<20} {:<15} {:<10}",
                    "ID", "Name", "Slug", "Plan"
                );
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

            println!("Quota for organization {}:", quota.org_id);
            println!("  Plan:       {}", quota.plan);
            println!(
                "  Topics:     {}/{}",
                quota.topics_used, quota.topics_limit
            );
            println!(
                "  Storage:    {} / {} bytes",
                quota.storage_used_bytes, quota.storage_limit_bytes
            );
            println!(
                "  Throughput: {} bytes/sec limit",
                quota.throughput_limit_bytes_per_sec
            );
        }
        OrgCommands::Usage { org } => {
            let usage: UsageResponse = client
                .get(&format!("/api/v1/organizations/{}/usage", org))
                .await
                .context("Failed to get usage")?;

            println!("Usage for organization {}:", usage.org_id);
            println!("  Period:           {}", usage.period);
            println!("  Records produced: {}", usage.records_produced);
            println!("  Records consumed: {}", usage.records_consumed);
            println!("  Storage:          {} bytes", usage.storage_bytes);
            println!("  API calls:        {}", usage.api_calls);
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
                .post(
                    &format!("/api/v1/organizations/{}/api-keys", org),
                    &req,
                )
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
            if let Some(expires) = resp.expires_at {
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
                    println!(
                        "{:<36} {:<20} {:<20} {:<20}",
                        key.id,
                        key.name,
                        key.created_at,
                        key.expires_at.as_deref().unwrap_or("never"),
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
            println!("  Created:     {}", key.created_at);
            if let Some(expires) = key.expires_at {
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
    println!("  Created: {}", org.created_at);
    println!("  Updated: {}", org.updated_at);
}
