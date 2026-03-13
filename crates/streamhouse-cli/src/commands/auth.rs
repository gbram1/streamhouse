//! Auth CLI commands
//!
//! Provides subcommands for authentication management:
//! login, logout, whoami, status, switch, instances, and api-key management.

use anyhow::{Context, Result};
use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::auth::AuthManager;
use crate::rest_client::RestClient;

#[derive(Debug, Subcommand)]
pub enum AuthCommands {
    /// Login to a StreamHouse server
    Login {
        /// Server URL to authenticate against
        #[arg(long)]
        server: Option<String>,
        /// Instance name (derived from URL if not provided)
        #[arg(long)]
        name: Option<String>,
        /// API key for authentication
        #[arg(long)]
        api_key: Option<String>,
    },
    /// Logout from the current instance
    Logout,
    /// Show the current authenticated user
    Whoami,
    /// Show authentication status
    Status,
    /// Switch to a different instance
    Switch {
        /// Instance name to switch to
        name: String,
    },
    /// List all known instances
    Instances,
    /// API key management
    ApiKey {
        #[command(subcommand)]
        command: ApiKeyCommands,
    },
}

#[derive(Debug, Subcommand)]
pub enum ApiKeyCommands {
    /// Create a new API key
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
    /// List API keys
    List,
    /// Get API key details
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HealthResponse {
    status: String,
    #[serde(default)]
    version: Option<String>,
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
    #[serde(default)]
    last_used_at: Option<String>,
}

/// Handle auth commands
pub async fn handle_auth_command(
    command: AuthCommands,
    api_url: &str,
    api_key: Option<&str>,
) -> Result<()> {
    match command {
        AuthCommands::Login {
            server,
            name,
            api_key: login_key,
        } => {
            let mut manager = AuthManager::new()?;
            let server_url = server.as_deref().unwrap_or(api_url);
            manager.login(server_url, name.as_deref())?;

            if let Some(key) = login_key {
                // Store the API key as the access token
                let instance_name = name
                    .unwrap_or_else(|| AuthManager::instance_name_from_url(server_url));
                if let Some(config) = manager
                    .credentials_for_login_mut()
                    .instances
                    .get_mut(&instance_name)
                {
                    config.access_token = Some(key);
                    manager.save()?;
                }
            }

            println!("Logged in to {}", server_url);
        }
        AuthCommands::Logout => {
            let mut manager = AuthManager::new()?;
            let instance = manager
                .credentials_for_login()
                .active_instance
                .clone();
            manager.logout()?;
            if let Some(name) = instance {
                println!("Logged out from '{}'", name);
            } else {
                println!("No active instance to log out from");
            }
        }
        AuthCommands::Whoami => {
            let client = RestClient::with_api_key(api_url, api_key.map(String::from));
            let health: HealthResponse = client
                .get("/health")
                .await
                .context("Failed to reach server")?;
            println!("Server status: {}", health.status);
            if let Some(version) = health.version {
                println!("Server version: {}", version);
            }
            if let Some(key) = api_key {
                println!("Authenticated with API key: {}...", &key[..key.len().min(8)]);
            } else {
                println!("No API key configured");
            }
        }
        AuthCommands::Status => {
            let manager = AuthManager::new()?;
            let creds = manager.credentials_for_login();
            if let Some(ref active) = creds.active_instance {
                println!("Active instance: {}", active);
                if let Some(config) = creds.instances.get(active) {
                    println!("  Server: {}", config.server_url);
                    println!(
                        "  Token:  {}",
                        if config.has_valid_token() {
                            "valid"
                        } else if config.access_token.is_some() {
                            "expired"
                        } else {
                            "none"
                        }
                    );
                    if let Some(ref org) = config.org_id {
                        println!("  Org:    {}", org);
                    }
                }
            } else {
                println!("No active instance. Run `streamctl auth login` to authenticate.");
            }
        }
        AuthCommands::Switch { name } => {
            let mut manager = AuthManager::new()?;
            manager.switch_instance(&name)?;
            println!("Switched to instance '{}'", name);
        }
        AuthCommands::Instances => {
            let manager = AuthManager::new()?;
            let creds = manager.credentials_for_login();
            let instances = manager.list_instances();
            if instances.is_empty() {
                println!("No instances configured. Run `streamctl auth login` to add one.");
            } else {
                println!("Instances:");
                for name in &instances {
                    let marker = if creds.active_instance.as_deref() == Some(name) {
                        " (active)"
                    } else {
                        ""
                    };
                    if let Some(config) = creds.instances.get(name) {
                        println!("  {} - {}{}", name, config.server_url, marker);
                    }
                }
            }
        }
        AuthCommands::ApiKey { command } => {
            handle_api_key_command(command, api_url, api_key).await?;
        }
    }

    Ok(())
}

async fn handle_api_key_command(
    command: ApiKeyCommands,
    api_url: &str,
    api_key: Option<&str>,
) -> Result<()> {
    let client = RestClient::with_api_key(api_url, api_key.map(String::from));

    match command {
        ApiKeyCommands::Create {
            name,
            permissions,
            scopes,
            expires_in_ms,
        } => {
            let req = CreateApiKeyRequest {
                name: name.clone(),
                permissions: permissions.map(|p| p.split(',').map(|s| s.trim().to_string()).collect()),
                scopes: scopes.map(|s| s.split(',').map(|s| s.trim().to_string()).collect()),
                expires_in_ms,
            };
            let resp: ApiKeyCreatedResponse = client
                .post("/api/v1/api-keys", &req)
                .await
                .context("Failed to create API key")?;

            println!("API key created:");
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
        ApiKeyCommands::List => {
            let keys: Vec<ApiKeyResponse> = client
                .get("/api/v1/api-keys")
                .await
                .context("Failed to list API keys")?;

            if keys.is_empty() {
                println!("No API keys found");
            } else {
                println!("API Keys ({}):", keys.len());
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
        ApiKeyCommands::Get { id } => {
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
            if let Some(last_used) = key.last_used_at {
                println!("  Last used:   {}", last_used);
            }
        }
        ApiKeyCommands::Revoke { id } => {
            client
                .delete(&format!("/api/v1/api-keys/{}", id))
                .await
                .context("Failed to revoke API key")?;
            println!("API key '{}' revoked", id);
        }
    }

    Ok(())
}
