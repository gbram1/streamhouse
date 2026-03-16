#![allow(dead_code)]
//! Authentication manager for StreamHouse CLI
//!
//! Manages credential storage with per-organization API keys,
//! org switching, and multi-server support.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Context for a single organization the user has access to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgContext {
    pub org_id: String,
    pub name: String,
    pub slug: String,
    pub api_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
}

/// Stored credentials for a StreamHouse server.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Credentials {
    /// Server URL this credential set is for
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,

    /// Direct API key for self-hosted mode (no org scoping needed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Organizations the user has access to, keyed by slug
    #[serde(default)]
    pub organizations: HashMap<String, OrgContext>,

    /// Currently active organization slug
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_org: Option<String>,

    // --- Legacy fields (kept for backward compat on load) ---
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub instances: HashMap<String, LegacyAuthConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_instance: Option<String>,
}

/// Legacy auth config (kept for backward compat deserialization).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyAuthConfig {
    pub server_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_expiry: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_id: Option<String>,
}

impl Credentials {
    /// Get the active organization's context.
    pub fn active_org_context(&self) -> Option<&OrgContext> {
        self.active_org
            .as_ref()
            .and_then(|slug| self.organizations.get(slug))
    }

    /// Get the active API key.
    /// Returns direct api_key if set (self-hosted), otherwise falls back to active org's key.
    pub fn active_api_key(&self) -> Option<&str> {
        self.api_key
            .as_deref()
            .or_else(|| self.active_org_context().map(|ctx| ctx.api_key.as_str()))
    }
}

/// Manages authentication credentials for the StreamHouse CLI.
pub struct AuthManager {
    credentials: Credentials,
    credentials_path: PathBuf,
}

impl AuthManager {
    /// Create a new AuthManager that loads from the default credentials path
    /// (~/.streamhouse/auth.json).
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        let credentials_path = home.join(".streamhouse").join("auth.json");
        Self::with_path(credentials_path)
    }

    /// Create a new AuthManager with a custom credentials path.
    pub fn with_path(credentials_path: PathBuf) -> Result<Self> {
        let credentials = if credentials_path.exists() {
            Self::load_from_path(&credentials_path)?
        } else {
            Credentials::default()
        };

        Ok(Self {
            credentials,
            credentials_path,
        })
    }

    /// Save credentials to disk as JSON with restricted file permissions (0o600).
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.credentials_path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create credentials directory")?;
        }

        let json = serde_json::to_string_pretty(&self.credentials)
            .context("Failed to serialize credentials")?;

        std::fs::write(&self.credentials_path, &json)
            .context("Failed to write credentials file")?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&self.credentials_path, permissions)
                .context("Failed to set credentials file permissions")?;
        }

        Ok(())
    }

    fn load_from_path(path: &PathBuf) -> Result<Credentials> {
        let data = std::fs::read_to_string(path).context("Failed to read credentials file")?;
        let credentials: Credentials =
            serde_json::from_str(&data).context("Failed to parse credentials file")?;
        Ok(credentials)
    }

    /// Set the server URL.
    pub fn set_server_url(&mut self, url: &str) {
        self.credentials.server_url = Some(url.to_string());
    }

    /// Get the server URL.
    pub fn server_url(&self) -> Option<&str> {
        self.credentials.server_url.as_deref()
    }

    /// Add an organization context. If it's the only org, make it active.
    pub fn add_org(&mut self, ctx: OrgContext) -> Result<()> {
        let slug = ctx.slug.clone();
        self.credentials.organizations.insert(slug.clone(), ctx);

        // Auto-activate if it's the first or only org
        if self.credentials.active_org.is_none() || self.credentials.organizations.len() == 1 {
            self.credentials.active_org = Some(slug);
        }

        self.save()
    }

    /// Switch to a different organization by slug.
    pub fn switch_org(&mut self, slug: &str) -> Result<()> {
        if !self.credentials.organizations.contains_key(slug) {
            let available: Vec<&str> = self
                .credentials
                .organizations
                .keys()
                .map(|s| s.as_str())
                .collect();
            anyhow::bail!(
                "Organization '{}' not found. Available: {}",
                slug,
                if available.is_empty() {
                    "(none — run `streamctl auth login` first)".to_string()
                } else {
                    available.join(", ")
                }
            );
        }
        self.credentials.active_org = Some(slug.to_string());
        self.save()
    }

    /// List all organization slugs.
    pub fn list_orgs(&self) -> Vec<&OrgContext> {
        self.credentials.organizations.values().collect()
    }

    /// Get the active org context.
    pub fn active_org(&self) -> Option<&OrgContext> {
        self.credentials.active_org_context()
    }

    /// Get the active API key (for injecting into REST requests).
    pub fn active_api_key(&self) -> Option<&str> {
        self.credentials.active_api_key()
    }

    /// Get the active org slug.
    pub fn active_org_slug(&self) -> Option<&str> {
        self.credentials.active_org.as_deref()
    }

    /// Remove an org by slug.
    pub fn remove_org(&mut self, slug: &str) -> Result<()> {
        self.credentials.organizations.remove(slug);
        if self.credentials.active_org.as_deref() == Some(slug) {
            // Switch to another org if available
            self.credentials.active_org = self.credentials.organizations.keys().next().cloned();
        }
        self.save()
    }

    /// Set a direct API key (for self-hosted, no org scoping).
    pub fn set_api_key(&mut self, key: &str) {
        self.credentials.api_key = Some(key.to_string());
    }

    /// Logout: clear all orgs and server URL.
    pub fn logout(&mut self) -> Result<()> {
        self.credentials.organizations.clear();
        self.credentials.active_org = None;
        self.credentials.server_url = None;
        self.credentials.api_key = None;
        // Also clear legacy fields
        self.credentials.instances.clear();
        self.credentials.active_instance = None;
        self.save()
    }

    /// Access credentials (read-only).
    pub fn credentials(&self) -> &Credentials {
        &self.credentials
    }

    /// Derive a short instance name from a server URL.
    pub fn instance_name_from_url(url: &str) -> String {
        let without_scheme = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .unwrap_or(url);

        let hostname = without_scheme
            .split(':')
            .next()
            .unwrap_or(without_scheme)
            .split('/')
            .next()
            .unwrap_or(without_scheme);

        let hostname = hostname.strip_prefix("api.").unwrap_or(hostname);

        if hostname == "streamhouse.dev" || hostname == "streamhouse.io" {
            return "streamhouse".to_string();
        }
        if let Some(prefix) = hostname.strip_suffix(".streamhouse.dev") {
            return prefix.replace('.', "-");
        }
        if let Some(prefix) = hostname.strip_suffix(".streamhouse.io") {
            return prefix.replace('.', "-");
        }
        if hostname == "localhost" {
            return "localhost".to_string();
        }
        hostname.replace('.', "-")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_credentials_path() -> PathBuf {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        std::mem::forget(dir);
        path
    }

    #[test]
    fn test_new_manager_empty() {
        let path = temp_credentials_path();
        let manager = AuthManager::with_path(path).unwrap();
        assert!(manager.credentials().organizations.is_empty());
        assert!(manager.active_org().is_none());
        assert!(manager.active_api_key().is_none());
    }

    #[test]
    fn test_add_org_auto_activates() {
        let path = temp_credentials_path();
        let mut manager = AuthManager::with_path(path).unwrap();
        manager.set_server_url("http://localhost:8080");
        manager
            .add_org(OrgContext {
                org_id: "org-1".into(),
                name: "Test Org".into(),
                slug: "test-org".into(),
                api_key: "sk_live_abc".into(),
                plan: Some("free".into()),
            })
            .unwrap();

        assert_eq!(manager.active_org_slug(), Some("test-org"));
        assert_eq!(manager.active_api_key(), Some("sk_live_abc"));
    }

    #[test]
    fn test_add_multiple_orgs_and_switch() {
        let path = temp_credentials_path();
        let mut manager = AuthManager::with_path(path).unwrap();
        manager
            .add_org(OrgContext {
                org_id: "org-1".into(),
                name: "Org One".into(),
                slug: "org-one".into(),
                api_key: "sk_live_111".into(),
                plan: None,
            })
            .unwrap();
        manager
            .add_org(OrgContext {
                org_id: "org-2".into(),
                name: "Org Two".into(),
                slug: "org-two".into(),
                api_key: "sk_live_222".into(),
                plan: None,
            })
            .unwrap();

        // First org stays active
        assert_eq!(manager.active_org_slug(), Some("org-one"));

        // Switch
        manager.switch_org("org-two").unwrap();
        assert_eq!(manager.active_org_slug(), Some("org-two"));
        assert_eq!(manager.active_api_key(), Some("sk_live_222"));
    }

    #[test]
    fn test_switch_nonexistent_org() {
        let path = temp_credentials_path();
        let mut manager = AuthManager::with_path(path).unwrap();
        let result = manager.switch_org("nope");
        assert!(result.is_err());
    }

    #[test]
    fn test_logout_clears_everything() {
        let path = temp_credentials_path();
        let mut manager = AuthManager::with_path(path).unwrap();
        manager.set_server_url("http://localhost:8080");
        manager
            .add_org(OrgContext {
                org_id: "org-1".into(),
                name: "Org".into(),
                slug: "org".into(),
                api_key: "sk_live_key".into(),
                plan: None,
            })
            .unwrap();

        manager.logout().unwrap();
        assert!(manager.credentials().organizations.is_empty());
        assert!(manager.active_org().is_none());
        assert!(manager.server_url().is_none());
    }

    #[test]
    fn test_save_and_reload() {
        let path = temp_credentials_path();
        {
            let mut manager = AuthManager::with_path(path.clone()).unwrap();
            manager.set_server_url("http://localhost:8080");
            manager
                .add_org(OrgContext {
                    org_id: "org-1".into(),
                    name: "Persisted Org".into(),
                    slug: "persisted".into(),
                    api_key: "sk_live_persist".into(),
                    plan: Some("pro".into()),
                })
                .unwrap();
        }

        let manager = AuthManager::with_path(path).unwrap();
        assert_eq!(manager.active_org_slug(), Some("persisted"));
        assert_eq!(manager.active_api_key(), Some("sk_live_persist"));
        let org = manager.active_org().unwrap();
        assert_eq!(org.name, "Persisted Org");
        assert_eq!(org.plan, Some("pro".to_string()));
    }

    #[test]
    fn test_file_permissions() {
        let path = temp_credentials_path();
        let mut manager = AuthManager::with_path(path.clone()).unwrap();
        manager
            .add_org(OrgContext {
                org_id: "o".into(),
                name: "O".into(),
                slug: "o".into(),
                api_key: "k".into(),
                plan: None,
            })
            .unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&path).unwrap();
            let mode = metadata.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn test_remove_org_switches_active() {
        let path = temp_credentials_path();
        let mut manager = AuthManager::with_path(path).unwrap();
        manager
            .add_org(OrgContext {
                org_id: "1".into(),
                name: "A".into(),
                slug: "a".into(),
                api_key: "k1".into(),
                plan: None,
            })
            .unwrap();
        manager
            .add_org(OrgContext {
                org_id: "2".into(),
                name: "B".into(),
                slug: "b".into(),
                api_key: "k2".into(),
                plan: None,
            })
            .unwrap();

        manager.remove_org("a").unwrap();
        assert_eq!(manager.active_org_slug(), Some("b"));
        assert_eq!(manager.credentials().organizations.len(), 1);
    }

    // instance_name_from_url tests (kept from original)

    #[test]
    fn test_instance_name_streamhouse_dev() {
        assert_eq!(
            AuthManager::instance_name_from_url("https://api.streamhouse.dev"),
            "streamhouse"
        );
    }

    #[test]
    fn test_instance_name_streamhouse_io() {
        assert_eq!(
            AuthManager::instance_name_from_url("https://api.streamhouse.io"),
            "streamhouse"
        );
    }

    #[test]
    fn test_instance_name_localhost() {
        assert_eq!(
            AuthManager::instance_name_from_url("http://localhost:8080"),
            "localhost"
        );
    }

    #[test]
    fn test_instance_name_custom_domain() {
        assert_eq!(
            AuthManager::instance_name_from_url("https://my-company.example.com"),
            "my-company-example-com"
        );
    }
}
