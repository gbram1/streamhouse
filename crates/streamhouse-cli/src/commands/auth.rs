//! Auth CLI commands
//!
//! Provides subcommands for authentication management:
//! login (discovers orgs via API key), logout, whoami, status.

use anyhow::{Context, Result};
use clap::Subcommand;
use serde::Deserialize;

use crate::auth::{AuthManager, OrgContext};
use crate::rest_client::RestClient;

#[derive(Debug, Subcommand)]
pub enum AuthCommands {
    /// Login to a StreamHouse server with an API key
    Login {
        /// API key (e.g. sk_live_xxx)
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

// --- API response types for org discovery ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ApiKeyInfoResponse {
    id: String,
    organization_id: String,
    name: String,
    #[serde(default)]
    permissions: Vec<String>,
    #[serde(default)]
    scopes: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct OrganizationResponse {
    id: String,
    name: String,
    slug: String,
    #[serde(default)]
    plan: Option<String>,
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

            // Step 1: Verify the key works by hitting health
            let _: serde_json::Value = client
                .get("/health")
                .await
                .context("Failed to connect to server. Check your --server URL and API key.")?;

            // Step 2: Try to discover which org this key belongs to.
            // The admin API can list orgs; with a scoped key we may get just ours.
            let orgs: Vec<OrganizationResponse> = client
                .get("/api/v1/organizations")
                .await
                .unwrap_or_default();

            let mut manager = AuthManager::new()?;
            manager.set_server_url(server_url);

            if orgs.is_empty() {
                // No org discovery available — store as a "default" org context
                manager.add_org(OrgContext {
                    org_id: "default".to_string(),
                    name: "Default".to_string(),
                    slug: "default".to_string(),
                    api_key: api_key.clone(),
                    plan: None,
                })?;
                println!("Logged in to {}", server_url);
                println!("  API key stored (no org discovery available)");
            } else {
                // Store each discovered org with this key
                for org in &orgs {
                    manager.add_org(OrgContext {
                        org_id: org.id.clone(),
                        name: org.name.clone(),
                        slug: org.slug.clone(),
                        api_key: api_key.clone(),
                        plan: org.plan.clone(),
                    })?;
                }

                println!("Logged in to {}", server_url);
                if orgs.len() == 1 {
                    println!("  Organization: {} ({})", orgs[0].name, orgs[0].slug);
                } else {
                    println!("  {} organizations found:", orgs.len());
                    for org in &orgs {
                        let active = if manager.active_org_slug() == Some(org.slug.as_str()) {
                            " (active)"
                        } else {
                            ""
                        };
                        println!("    {} ({}){}", org.name, org.slug, active);
                    }
                    println!();
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
                println!("Active organization: {} ({})", org.name, org.slug);
                if let Some(ref plan) = org.plan {
                    println!("Plan: {}", plan);
                }
                println!(
                    "API key: {}...{}",
                    &org.api_key[..org.api_key.len().min(12)],
                    &org.api_key[org.api_key.len().saturating_sub(4)..]
                );
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
