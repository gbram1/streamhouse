#![allow(dead_code)]
//! Authentication manager for StreamHouse CLI
//!
//! Manages OAuth2 device flow authentication, credential storage,
//! and multi-instance switching.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Buffer time in seconds before token expiry to trigger refresh
const EXPIRY_BUFFER_SECS: i64 = 300;

/// Configuration for a single StreamHouse server instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
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

impl AuthConfig {
    /// Create a new AuthConfig with the given server URL and no tokens.
    pub fn new(server_url: &str) -> Self {
        Self {
            server_url: server_url.to_string(),
            access_token: None,
            refresh_token: None,
            token_expiry: None,
            org_id: None,
        }
    }

    /// Returns true if the config has a non-expired access token.
    pub fn has_valid_token(&self) -> bool {
        match (&self.access_token, self.token_expiry) {
            (Some(_token), Some(expiry)) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                now < expiry
            }
            (Some(_token), None) => true, // Token with no expiry is considered valid
            _ => false,
        }
    }

    /// Returns true if the token is valid but will expire within the buffer period.
    pub fn needs_refresh(&self) -> bool {
        match (&self.access_token, &self.refresh_token, self.token_expiry) {
            (Some(_), Some(_), Some(expiry)) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                now >= expiry - EXPIRY_BUFFER_SECS && now < expiry
            }
            _ => false,
        }
    }
}

/// Stored credentials for all known StreamHouse instances.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Credentials {
    pub instances: HashMap<String, AuthConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_instance: Option<String>,
}

impl Credentials {
    /// Get a reference to the active instance's config.
    pub fn active_config(&self) -> Option<&AuthConfig> {
        self.active_instance
            .as_ref()
            .and_then(|name| self.instances.get(name))
    }

    /// Get a mutable reference to the active instance's config.
    pub fn active_config_mut(&mut self) -> Option<&mut AuthConfig> {
        self.active_instance
            .as_ref()
            .and_then(|name| self.instances.get_mut(name))
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
    /// Useful for testing.
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
            std::fs::create_dir_all(parent)
                .context("Failed to create credentials directory")?;
        }

        let json = serde_json::to_string_pretty(&self.credentials)
            .context("Failed to serialize credentials")?;

        std::fs::write(&self.credentials_path, &json)
            .context("Failed to write credentials file")?;

        // Set file permissions to owner-only read/write
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&self.credentials_path, permissions)
                .context("Failed to set credentials file permissions")?;
        }

        Ok(())
    }

    /// Load credentials from the configured path.
    fn load_from_path(path: &PathBuf) -> Result<Credentials> {
        let data = std::fs::read_to_string(path)
            .context("Failed to read credentials file")?;
        let credentials: Credentials =
            serde_json::from_str(&data).context("Failed to parse credentials file")?;
        Ok(credentials)
    }

    /// Reload credentials from disk.
    pub fn load(&mut self) -> Result<()> {
        if self.credentials_path.exists() {
            self.credentials = Self::load_from_path(&self.credentials_path)?;
        }
        Ok(())
    }

    /// Login to a StreamHouse server instance, storing it as the active instance.
    ///
    /// If `instance_name` is None, a name is derived from the server URL.
    pub fn login(&mut self, server_url: &str, instance_name: Option<&str>) -> Result<()> {
        let name = match instance_name {
            Some(n) => n.to_string(),
            None => Self::instance_name_from_url(server_url),
        };

        let config = AuthConfig::new(server_url);
        self.credentials.instances.insert(name.clone(), config);
        self.credentials.active_instance = Some(name);
        self.save()?;
        Ok(())
    }

    /// Logout by removing the active instance.
    pub fn logout(&mut self) -> Result<()> {
        if let Some(name) = self.credentials.active_instance.take() {
            self.credentials.instances.remove(&name);
            self.save()?;
        }
        Ok(())
    }

    /// Switch to a different named instance.
    pub fn switch_instance(&mut self, name: &str) -> Result<()> {
        if !self.credentials.instances.contains_key(name) {
            anyhow::bail!("Instance '{}' not found. Available: {:?}", name, self.list_instances());
        }
        self.credentials.active_instance = Some(name.to_string());
        self.save()?;
        Ok(())
    }

    /// List all known instance names.
    pub fn list_instances(&self) -> Vec<String> {
        self.credentials.instances.keys().cloned().collect()
    }

    /// Get a valid access token for the active instance, refreshing if needed.
    pub fn get_token(&mut self) -> Result<Option<String>> {
        let needs_refresh = self
            .credentials
            .active_config()
            .map(|c| c.needs_refresh())
            .unwrap_or(false);

        if needs_refresh {
            self.refresh_token()?;
        }

        Ok(self
            .credentials
            .active_config()
            .and_then(|c| c.access_token.clone()))
    }

    /// Exchange the refresh token for a new access token.
    ///
    /// In a full implementation, this would make an HTTP request to the token endpoint.
    /// For now, this is a placeholder that returns an error if no refresh token is available.
    pub fn refresh_token(&mut self) -> Result<()> {
        let has_refresh = self
            .credentials
            .active_config()
            .and_then(|c| c.refresh_token.as_ref())
            .is_some();

        if !has_refresh {
            anyhow::bail!("No refresh token available. Please login again.");
        }

        // TODO: Implement actual token refresh via HTTP request to the auth server.
        // For now, this is a no-op placeholder. The actual implementation would:
        // 1. POST to the token endpoint with grant_type=refresh_token
        // 2. Update the access_token, refresh_token, and token_expiry
        // 3. Save credentials to disk

        Ok(())
    }

    /// Derive a short instance name from a server URL.
    ///
    /// Examples:
    /// - "https://api.streamhouse.dev" -> "streamhouse"
    /// - "https://api.streamhouse.io" -> "streamhouse"
    /// - "http://localhost:8080" -> "localhost"
    /// - "https://my-company.example.com" -> "my-company-example-com"
    pub fn instance_name_from_url(url: &str) -> String {
        // Strip scheme
        let without_scheme = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .unwrap_or(url);

        // Extract hostname (remove port and path)
        let hostname = without_scheme
            .split(':')
            .next()
            .unwrap_or(without_scheme)
            .split('/')
            .next()
            .unwrap_or(without_scheme);

        // Remove "api." prefix
        let hostname = hostname.strip_prefix("api.").unwrap_or(hostname);

        // Handle streamhouse domains specially
        if hostname == "streamhouse.dev" || hostname == "streamhouse.io" {
            return "streamhouse".to_string();
        }

        if let Some(prefix) = hostname.strip_suffix(".streamhouse.dev") {
            return prefix.replace('.', "-");
        }

        if let Some(prefix) = hostname.strip_suffix(".streamhouse.io") {
            return prefix.replace('.', "-");
        }

        // Handle localhost
        if hostname == "localhost" {
            return "localhost".to_string();
        }

        // Default: replace dots with dashes
        hostname.replace('.', "-")
    }

    /// Access credentials (for testing).
    #[cfg(test)]
    pub fn credentials(&self) -> &Credentials {
        &self.credentials
    }

    /// Access credentials mutably (for testing).
    #[cfg(test)]
    pub fn credentials_mut(&mut self) -> &mut Credentials {
        &mut self.credentials
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Helper to create a temporary credentials path for testing.
    fn temp_credentials_path() -> PathBuf {
        let dir = tempfile::tempdir().unwrap();
        // Leak the tempdir so it doesn't get cleaned up during the test
        let path = dir.path().join("auth.json");
        std::mem::forget(dir);
        path
    }

    // --- AuthConfig tests ---

    #[test]
    fn test_auth_config_new() {
        let config = AuthConfig::new("https://api.streamhouse.dev");
        assert_eq!(config.server_url, "https://api.streamhouse.dev");
        assert!(config.access_token.is_none());
        assert!(config.refresh_token.is_none());
        assert!(config.token_expiry.is_none());
        assert!(config.org_id.is_none());
    }

    #[test]
    fn test_has_valid_token_no_token() {
        let config = AuthConfig::new("https://api.streamhouse.dev");
        assert!(!config.has_valid_token());
    }

    #[test]
    fn test_has_valid_token_with_valid_token() {
        let future = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + 3600;
        let config = AuthConfig {
            server_url: "https://api.streamhouse.dev".to_string(),
            access_token: Some("test-token".to_string()),
            refresh_token: None,
            token_expiry: Some(future),
            org_id: None,
        };
        assert!(config.has_valid_token());
    }

    #[test]
    fn test_has_valid_token_expired() {
        let past = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            - 3600;
        let config = AuthConfig {
            server_url: "https://api.streamhouse.dev".to_string(),
            access_token: Some("test-token".to_string()),
            refresh_token: None,
            token_expiry: Some(past),
            org_id: None,
        };
        assert!(!config.has_valid_token());
    }

    #[test]
    fn test_has_valid_token_no_expiry() {
        let config = AuthConfig {
            server_url: "https://api.streamhouse.dev".to_string(),
            access_token: Some("test-token".to_string()),
            refresh_token: None,
            token_expiry: None,
            org_id: None,
        };
        assert!(config.has_valid_token());
    }

    #[test]
    fn test_needs_refresh_no_refresh_token() {
        let config = AuthConfig {
            server_url: "https://api.streamhouse.dev".to_string(),
            access_token: Some("test-token".to_string()),
            refresh_token: None,
            token_expiry: Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64
                    + 100,
            ),
            org_id: None,
        };
        assert!(!config.needs_refresh());
    }

    #[test]
    fn test_needs_refresh_within_buffer() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let config = AuthConfig {
            server_url: "https://api.streamhouse.dev".to_string(),
            access_token: Some("test-token".to_string()),
            refresh_token: Some("refresh-token".to_string()),
            token_expiry: Some(now + 100), // Within the 300s buffer
            org_id: None,
        };
        assert!(config.needs_refresh());
    }

    #[test]
    fn test_needs_refresh_not_needed() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let config = AuthConfig {
            server_url: "https://api.streamhouse.dev".to_string(),
            access_token: Some("test-token".to_string()),
            refresh_token: Some("refresh-token".to_string()),
            token_expiry: Some(now + 3600), // Well beyond buffer
            org_id: None,
        };
        assert!(!config.needs_refresh());
    }

    // --- Credentials tests ---

    #[test]
    fn test_credentials_default() {
        let creds = Credentials::default();
        assert!(creds.instances.is_empty());
        assert!(creds.active_instance.is_none());
    }

    #[test]
    fn test_credentials_active_config() {
        let mut creds = Credentials::default();
        creds.instances.insert(
            "test".to_string(),
            AuthConfig::new("https://api.streamhouse.dev"),
        );
        creds.active_instance = Some("test".to_string());

        let config = creds.active_config().unwrap();
        assert_eq!(config.server_url, "https://api.streamhouse.dev");
    }

    #[test]
    fn test_credentials_active_config_none() {
        let creds = Credentials::default();
        assert!(creds.active_config().is_none());
    }

    #[test]
    fn test_credentials_serialization_roundtrip() {
        let mut creds = Credentials::default();
        creds.instances.insert(
            "prod".to_string(),
            AuthConfig {
                server_url: "https://api.streamhouse.dev".to_string(),
                access_token: Some("tok123".to_string()),
                refresh_token: Some("ref456".to_string()),
                token_expiry: Some(1700000000),
                org_id: Some("org-1".to_string()),
            },
        );
        creds.active_instance = Some("prod".to_string());

        let json = serde_json::to_string(&creds).unwrap();
        let deserialized: Credentials = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.active_instance, Some("prod".to_string()));
        let config = deserialized.instances.get("prod").unwrap();
        assert_eq!(config.server_url, "https://api.streamhouse.dev");
        assert_eq!(config.access_token, Some("tok123".to_string()));
        assert_eq!(config.refresh_token, Some("ref456".to_string()));
        assert_eq!(config.token_expiry, Some(1700000000));
        assert_eq!(config.org_id, Some("org-1".to_string()));
    }

    #[test]
    fn test_credentials_skip_serializing_none() {
        let config = AuthConfig::new("https://api.streamhouse.dev");
        let json = serde_json::to_string(&config).unwrap();
        assert!(!json.contains("access_token"));
        assert!(!json.contains("refresh_token"));
        assert!(!json.contains("token_expiry"));
        assert!(!json.contains("org_id"));
    }

    // --- AuthManager tests ---

    #[test]
    fn test_auth_manager_with_path_nonexistent() {
        let path = temp_credentials_path();
        let manager = AuthManager::with_path(path).unwrap();
        assert!(manager.credentials().instances.is_empty());
        assert!(manager.credentials().active_instance.is_none());
    }

    #[test]
    fn test_auth_manager_save_and_load() {
        let path = temp_credentials_path();
        {
            let mut manager = AuthManager::with_path(path.clone()).unwrap();
            manager.login("https://api.streamhouse.dev", Some("prod")).unwrap();
        }

        // Reload from disk
        let manager = AuthManager::with_path(path).unwrap();
        assert_eq!(manager.credentials().active_instance, Some("prod".to_string()));
        assert!(manager.credentials().instances.contains_key("prod"));
    }

    #[test]
    fn test_auth_manager_file_permissions() {
        let path = temp_credentials_path();
        let mut manager = AuthManager::with_path(path.clone()).unwrap();
        manager.login("https://api.streamhouse.dev", Some("test")).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&path).unwrap();
            let mode = metadata.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "Credentials file should have 0600 permissions");
        }
    }

    #[test]
    fn test_auth_manager_login() {
        let path = temp_credentials_path();
        let mut manager = AuthManager::with_path(path).unwrap();
        manager.login("https://api.streamhouse.dev", Some("prod")).unwrap();

        assert_eq!(manager.credentials().active_instance, Some("prod".to_string()));
        let config = manager.credentials().instances.get("prod").unwrap();
        assert_eq!(config.server_url, "https://api.streamhouse.dev");
    }

    #[test]
    fn test_auth_manager_login_auto_name() {
        let path = temp_credentials_path();
        let mut manager = AuthManager::with_path(path).unwrap();
        manager.login("https://api.streamhouse.dev", None).unwrap();

        assert_eq!(
            manager.credentials().active_instance,
            Some("streamhouse".to_string())
        );
    }

    #[test]
    fn test_auth_manager_logout() {
        let path = temp_credentials_path();
        let mut manager = AuthManager::with_path(path).unwrap();
        manager.login("https://api.streamhouse.dev", Some("prod")).unwrap();
        manager.logout().unwrap();

        assert!(manager.credentials().active_instance.is_none());
        assert!(!manager.credentials().instances.contains_key("prod"));
    }

    #[test]
    fn test_auth_manager_switch_instance() {
        let path = temp_credentials_path();
        let mut manager = AuthManager::with_path(path).unwrap();
        manager.login("https://api.streamhouse.dev", Some("prod")).unwrap();
        manager.login("http://localhost:9090", Some("local")).unwrap();

        assert_eq!(
            manager.credentials().active_instance,
            Some("local".to_string())
        );

        manager.switch_instance("prod").unwrap();
        assert_eq!(
            manager.credentials().active_instance,
            Some("prod".to_string())
        );
    }

    #[test]
    fn test_auth_manager_switch_nonexistent() {
        let path = temp_credentials_path();
        let mut manager = AuthManager::with_path(path).unwrap();
        let result = manager.switch_instance("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_auth_manager_list_instances() {
        let path = temp_credentials_path();
        let mut manager = AuthManager::with_path(path).unwrap();
        manager.login("https://api.streamhouse.dev", Some("prod")).unwrap();
        manager.login("http://localhost:9090", Some("local")).unwrap();

        let mut instances = manager.list_instances();
        instances.sort();
        assert_eq!(instances, vec!["local", "prod"]);
    }

    #[test]
    fn test_auth_manager_get_token_no_active() {
        let path = temp_credentials_path();
        let mut manager = AuthManager::with_path(path).unwrap();
        let token = manager.get_token().unwrap();
        assert!(token.is_none());
    }

    #[test]
    fn test_auth_manager_get_token_with_token() {
        let path = temp_credentials_path();
        let mut manager = AuthManager::with_path(path).unwrap();
        manager.login("https://api.streamhouse.dev", Some("prod")).unwrap();

        // Manually set a token
        let config = manager.credentials_mut().instances.get_mut("prod").unwrap();
        config.access_token = Some("my-token".to_string());

        let token = manager.get_token().unwrap();
        assert_eq!(token, Some("my-token".to_string()));
    }

    // --- instance_name_from_url tests ---

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

    #[test]
    fn test_instance_name_bare_streamhouse_dev() {
        assert_eq!(
            AuthManager::instance_name_from_url("https://streamhouse.dev"),
            "streamhouse"
        );
    }

    #[test]
    fn test_instance_name_bare_streamhouse_io() {
        assert_eq!(
            AuthManager::instance_name_from_url("https://streamhouse.io"),
            "streamhouse"
        );
    }
}
