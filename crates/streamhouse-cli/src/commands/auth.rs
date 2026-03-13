//! Auth CLI commands
//!
//! Provides subcommands for authentication management:
//! login (discovers orgs, provisions per-org API keys), logout, whoami, status.

use anyhow::{Context, Result};
use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::auth::{AuthManager, OrgContext};
use crate::rest_client::RestClient;

#[derive(Debug, Subcommand)]
pub enum AuthCommands {
    /// Login to a StreamHouse server with an API key
    Login {
        /// API key (e.g. sk_live_xxx or admin key)
        #[arg(long)]
        api_key: String,
        /// Server URL (defaults to --api-url or STREAMHOUSE_API_URL)
        #[arg(long)]
        server: Option<String>,
    },
    /// Logout and clear stored credentials
    Logout,
    /// Show the current authenticated user and active org
    Whoami,
    /// Show authentication status and stored orgs
    Status,
}

// --- API types ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct OrganizationResponse {
    id: String,
    name: String,
    slug: String,
    #[serde(default)]
    plan: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct CreateApiKeyRequest {
    name: String,
    permissions: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ApiKeyCreatedResponse {
    id: String,
    name: String,
    key: String,
    #[serde(default)]
    permissions: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ApiKeyListEntry {
    id: String,
    name: String,
    #[serde(default)]
    permissions: Vec<String>,
}

/// Handle auth commands.
pub async fn handle_auth_command(
    command: AuthCommands,
    api_url: &str,
    _api_key: Option<&str>,
) -> Result<()> {
    match command {
        AuthCommands::Login { api_key, server } => {
            let server_url = server.as_deref().unwrap_or(api_url);
            let client = RestClient::with_api_key(server_url, Some(api_key.clone()));

            // Step 1: Verify the key works
            let _: serde_json::Value = client
                .get("/health")
                .await
                .context("Failed to connect to server. Check your --server URL and API key.")?;

            // Step 2: Discover orgs
            let orgs: Vec<OrganizationResponse> = client
                .get("/api/v1/organizations")
                .await
                .unwrap_or_default();

            let mut manager = AuthManager::new()?;
            manager.logout()?;
            manager.set_server_url(server_url);

            if orgs.is_empty() {
                // No orgs — store the key directly
                manager.add_org(OrgContext {
                    org_id: "default".to_string(),
                    name: "Default".to_string(),
                    slug: "default".to_string(),
                    api_key: api_key.clone(),
                    plan: None,
                })?;
                println!("Logged in to {}", server_url);
                println!("  API key stored (no organizations found)");
            } else {
                // Step 3: For each org, provision a CLI-specific API key
                println!("Logged in to {}", server_url);
                println!("  Provisioning per-org API keys...");

                for org in &orgs {
                    let org_key = provision_org_key(&client, &org.id, &org.slug).await;

                    match org_key {
                        Ok(key) => {
                            manager.add_org(OrgContext {
                                org_id: org.id.clone(),
                                name: org.name.clone(),
                                slug: org.slug.clone(),
                                api_key: key,
                                plan: org.plan.clone(),
                            })?;
                            println!("    {} ({}) — key provisioned", org.name, org.slug);
                        }
                        Err(e) => {
                            // Fallback: use the admin key for this org
                            manager.add_org(OrgContext {
                                org_id: org.id.clone(),
                                name: org.name.clone(),
                                slug: org.slug.clone(),
                                api_key: api_key.clone(),
                                plan: org.plan.clone(),
                            })?;
                            println!(
                                "    {} ({}) — using login key ({})",
                                org.name, org.slug, e
                            );
                        }
                    }
                }

                println!();
                if orgs.len() == 1 {
                    println!("  Active: {} ({})", orgs[0].name, orgs[0].slug);
                } else {
                    println!("  Active: {} ({})", orgs[0].name, orgs[0].slug);
                    println!("  Use `streamctl org switch <slug>` to change active org.");
                }
            }

            manager.save()?;
        }

        AuthCommands::Logout => {
            let mut manager = AuthManager::new()?;
            manager.logout()?;
            println!("Logged out. All stored credentials cleared.");
        }

        AuthCommands::Whoami => {
            let manager = AuthManager::new()?;
            if let Some(org) = manager.active_org() {
                let server = manager.server_url().unwrap_or("(unknown)");
                println!("Server: {}", server);
                println!("Organization: {} ({})", org.name, org.slug);
                if let Some(ref plan) = org.plan {
                    println!("Plan: {}", plan);
                }
                // Show key prefix only
                let prefix_len = org.api_key.len().min(16);
                println!("API key: {}...", &org.api_key[..prefix_len]);
            } else {
                println!("Not logged in. Run `streamctl auth login --api-key <key>` to authenticate.");
            }
        }

        AuthCommands::Status => {
            let manager = AuthManager::new()?;
            let creds = manager.credentials();

            if let Some(ref url) = creds.server_url {
                println!("Server: {}", url);
            } else {
                println!("Not logged in.");
                return Ok(());
            }

            if creds.organizations.is_empty() {
                println!("No organizations stored.");
            } else {
                println!("Organizations ({}):", creds.organizations.len());
                for (slug, org) in &creds.organizations {
                    let active = if creds.active_org.as_deref() == Some(slug.as_str()) {
                        " * "
                    } else {
                        "   "
                    };
                    println!(
                        "  {}{}  {} [{}]",
                        active,
                        slug,
                        org.name,
                        org.plan.as_deref().unwrap_or("—")
                    );
                }
                if let Some(ref active) = creds.active_org {
                    println!();
                    println!("Active: {}", active);
                }
            }
        }
    }

    Ok(())
}

/// Provision a per-org API key for the CLI.
/// First checks if a "streamctl-cli" key already exists, reuses if so.
/// Otherwise creates a new one.
async fn provision_org_key(
    client: &RestClient,
    org_id: &str,
    org_slug: &str,
) -> Result<String> {
    let key_name = format!("streamctl-cli-{}", org_slug);

    // Check if we already have a CLI key for this org
    let existing_keys: Vec<ApiKeyListEntry> = client
        .get(&format!("/api/v1/organizations/{}/api-keys", org_id))
        .await
        .unwrap_or_default();

    // If a CLI key already exists, we can't retrieve the secret again.
    // We need to create a new one (or the user needs to revoke the old one).
    // For simplicity, always create a fresh key with a unique name.
    let has_existing = existing_keys.iter().any(|k| k.name == key_name);
    let actual_name = if has_existing {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        format!("streamctl-cli-{}-{}", org_slug, ts)
    } else {
        key_name
    };

    let req = CreateApiKeyRequest {
        name: actual_name,
        permissions: vec!["read".to_string(), "write".to_string()],
    };

    let resp: ApiKeyCreatedResponse = client
        .post(
            &format!("/api/v1/organizations/{}/api-keys", org_id),
            &req,
        )
        .await
        .context("Failed to create API key for org")?;

    Ok(resp.key)
}
