//! Auth CLI commands
//!
//! Provides subcommands for authentication management:
//! login (browser flow or API key), logout, whoami, status.

use anyhow::{Context, Result};
use base64::Engine as _;
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

use crate::auth::{AuthManager, OrgContext};
use crate::rest_client::RestClient;

const DEFAULT_SERVER: &str = "https://streamhouse.app";

#[derive(Debug, Subcommand)]
pub enum AuthCommands {
    /// Login to StreamHouse (opens browser for authentication)
    Login {
        /// Server URL (defaults to managed service)
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

/// Credentials received from the browser callback.
#[derive(Debug, Deserialize)]
struct BrowserCredentials {
    server_url: String,
    #[serde(default)]
    user: Option<BrowserUser>,
    organizations: Vec<BrowserOrg>,
}

#[derive(Debug, Deserialize)]
struct BrowserUser {
    #[serde(default)]
    id: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct BrowserOrg {
    org_id: String,
    name: String,
    slug: String,
    api_key: String,
    #[serde(default)]
    plan: Option<String>,
}

/// Handle auth commands.
pub async fn handle_auth_command(
    command: AuthCommands,
    _api_url: &str,
    _api_key: Option<&str>,
) -> Result<()> {
    match command {
        AuthCommands::Login { server } => {
            // Always use browser login
            let server_url = server.as_deref().unwrap_or(DEFAULT_SERVER);
            handle_browser_login(server_url).await?;
        }

        AuthCommands::Logout => {
            let mut manager = AuthManager::new()?;
            manager.logout()?;
            println!("Logged out. All stored credentials cleared.");
        }

        AuthCommands::Whoami => {
            let manager = AuthManager::new()?;
            let server = manager.server_url().unwrap_or("(unknown)");

            if let Some(org) = manager.active_org() {
                println!("Server: {}", server);
                println!("Organization: {} ({})", org.name, org.slug);
                if let Some(ref plan) = org.plan {
                    println!("Plan: {}", plan);
                }
                let prefix_len = org.api_key.len().min(16);
                println!("API key: {}...", &org.api_key[..prefix_len]);
            } else if manager.active_api_key().is_some() {
                println!("Server: {}", server);
                println!("Authenticated (no organization scope)");
                let key = manager.active_api_key().unwrap();
                let prefix_len = key.len().min(16);
                println!("API key: {}...", &key[..prefix_len]);
            } else {
                println!("Not logged in. Run `streamctl auth login` to authenticate.");
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
                if creds.api_key.is_some() {
                    println!("Authenticated with direct API key (no org scoping).");
                } else {
                    println!("No organizations stored.");
                }
            } else {
                println!("Organizations ({}):", creds.organizations.len());
                for (slug, org) in &creds.organizations {
                    let active = if creds.active_org.as_deref() == Some(slug.as_str()) {
                        " * "
                    } else {
                        "   "
                    };
                    println!(
                        "  {}{}  [{}]",
                        active,
                        org.name,
                        org.plan.as_deref().unwrap_or("—")
                    );
                }
                if let Some(org) = creds.active_org_context() {
                    println!();
                    println!("Active: {}", org.name);
                }
            }
        }
    }

    Ok(())
}

/// Self-hosted login: validate key, discover orgs, store credentials.
async fn handle_api_key_login(api_key: String, server_url: &str) -> Result<()> {
    let client = RestClient::with_api_key(server_url, Some(api_key.clone()));

    // Verify the key works
    let _: serde_json::Value = client
        .get("/health")
        .await
        .context("Failed to connect to server. Check your --server URL and API key.")?;

    // Discover orgs
    let orgs: Vec<OrganizationResponse> = client
        .get("/api/v1/organizations")
        .await
        .unwrap_or_default();

    let mut manager = AuthManager::new()?;
    manager.logout()?;
    manager.set_server_url(server_url);

    if orgs.is_empty() {
        // No orgs — store key directly without creating a fake org
        manager.set_api_key(&api_key);
        manager.save()?;
        println!("Logged in to {}", server_url);
        println!("  API key stored (no organizations found)");
    } else {
        // For each org, provision a CLI-specific API key
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
                    println!("    {} ({}) — using login key ({})", org.name, org.slug, e);
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

        manager.save()?;
    }

    Ok(())
}

/// Browser login: open browser, receive credentials via local callback server.
async fn handle_browser_login(server_url: &str) -> Result<()> {
    // Start local callback server on random port
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("Failed to start local callback server")?;
    let port = listener.local_addr()?.port();

    // Generate random state for CSRF protection
    let state = format!("{:016x}", rand::random::<u64>());

    let auth_url = format!(
        "{}/cli/authorize?callback_port={}&state={}",
        server_url.trim_end_matches('/'),
        port,
        urlencoding::encode(&state)
    );

    println!("Opening browser for authentication...");
    println!();
    println!("If your browser doesn't open, visit:");
    println!("  {}", auth_url);
    println!();
    println!("Waiting for authorization...");

    // Open browser
    if let Err(e) = open::that(&auth_url) {
        eprintln!("Could not open browser: {}. Please visit the URL above.", e);
    }

    // Accept one connection
    let (stream, _) = listener
        .accept()
        .await
        .context("Failed to accept callback connection")?;

    // Wait for the stream to be readable
    stream.readable().await?;

    // Read the HTTP request
    let mut buf = vec![0u8; 65536];
    let n = stream.try_read(&mut buf)?;
    let request = String::from_utf8_lossy(&buf[..n]);

    // Parse the GET request line to extract query params
    let request_line = request.lines().next().unwrap_or("");
    let path = request_line.split_whitespace().nth(1).unwrap_or("");

    // Parse query params from the path
    let query_string = path.split('?').nth(1).unwrap_or("");
    let params: std::collections::HashMap<String, String> = query_string
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?;
            let value = parts.next().unwrap_or("");
            Some((
                urlencoding::decode(key).unwrap_or_default().into_owned(),
                urlencoding::decode(value).unwrap_or_default().into_owned(),
            ))
        })
        .collect();

    // Send response HTML before processing
    let response_html = r#"<!DOCTYPE html>
<html>
<head><title>StreamHouse CLI</title></head>
<body style="display:flex;justify-content:center;align-items:center;min-height:100vh;font-family:system-ui;background:#f8f9fa">
<div style="text-align:center;max-width:400px">
<h1>CLI Authorized!</h1>
<p style="color:#666">You can close this tab and return to your terminal.</p>
</div>
</body>
</html>"#;

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response_html.len(),
        response_html
    );

    stream.writable().await?;
    stream.try_write(response.as_bytes())?;

    // Validate state
    let received_state = params
        .get("state")
        .ok_or_else(|| anyhow::anyhow!("Missing state parameter in callback"))?;

    if received_state != &state {
        anyhow::bail!("State mismatch — possible CSRF attack. Please try again.");
    }

    // Decode credentials
    let credentials_b64 = params
        .get("credentials")
        .ok_or_else(|| anyhow::anyhow!("Missing credentials in callback"))?;

    let credentials_json = base64::engine::general_purpose::STANDARD
        .decode(credentials_b64)
        .context("Failed to decode credentials")?;

    let browser_creds: BrowserCredentials =
        serde_json::from_slice(&credentials_json).context("Failed to parse credentials")?;

    // Store credentials
    let mut manager = AuthManager::new()?;
    manager.logout()?;
    manager.set_server_url(&browser_creds.server_url);

    if browser_creds.organizations.is_empty() {
        println!("Warning: No organizations found in your account.");
        println!("Create an organization in the dashboard first.");
        manager.save()?;
        return Ok(());
    }

    for org in &browser_creds.organizations {
        manager.add_org(OrgContext {
            org_id: org.org_id.clone(),
            name: org.name.clone(),
            slug: org.slug.clone(),
            api_key: org.api_key.clone(),
            plan: org.plan.clone(),
        })?;
    }

    manager.save()?;

    println!();
    if let Some(user) = &browser_creds.user {
        println!("Authenticated as {} ({})", user.name, user.email);
    }
    println!("Server: {}", browser_creds.server_url);
    println!();
    println!("Organizations:");
    for (i, org) in browser_creds.organizations.iter().enumerate() {
        let marker = if i == 0 { " *" } else { "  " };
        println!(
            " {} {} ({}) [{}]",
            marker,
            org.name,
            org.slug,
            org.plan.as_deref().unwrap_or("—")
        );
    }

    if browser_creds.organizations.len() > 1 {
        println!();
        println!("Use `streamctl org switch <slug>` to change active org.");
    }

    Ok(())
}

/// Provision a per-org API key for the CLI.
/// First checks if a "streamctl-cli" key already exists, reuses if so.
/// Otherwise creates a new one.
async fn provision_org_key(client: &RestClient, org_id: &str, org_slug: &str) -> Result<String> {
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
        .post(&format!("/api/v1/organizations/{}/api-keys", org_id), &req)
        .await
        .context("Failed to create API key for org")?;

    Ok(resp.key)
}
