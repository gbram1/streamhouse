//! Configuration management for streamctl

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Server address (REST API)
    pub rest_api_url: String,

    /// gRPC server address (legacy)
    pub grpc_url: Option<String>,

    /// Default output format
    pub output_format: OutputFormat,

    /// Enable colored output
    pub colored: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Table,
    Json,
    Yaml,
    Text,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rest_api_url: "http://localhost:8080".to_string(),
            grpc_url: Some("http://localhost:9090".to_string()),
            output_format: OutputFormat::Table,
            colored: true,
        }
    }
}

impl Config {
    /// Load config from file or create default
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;

        if path.exists() {
            let contents = std::fs::read_to_string(&path)?;
            let config: Config = toml::from_str(&contents)?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    /// Save config to file
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let contents = toml::to_string_pretty(self)?;
        std::fs::write(&path, contents)?;

        Ok(())
    }

    /// Get config file path (~/.streamhouse/config.toml)
    fn config_path() -> Result<PathBuf> {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let path = PathBuf::from(home).join(".streamhouse").join("config.toml");
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.rest_api_url, "http://localhost:8080");
        assert_eq!(config.grpc_url, Some("http://localhost:9090".to_string()));
        assert_eq!(config.output_format, OutputFormat::Table);
        assert!(config.colored);
    }

    #[test]
    fn test_output_format_equality() {
        assert_eq!(OutputFormat::Table, OutputFormat::Table);
        assert_eq!(OutputFormat::Json, OutputFormat::Json);
        assert_ne!(OutputFormat::Table, OutputFormat::Json);
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = Config {
            rest_api_url: "http://example.com:8080".to_string(),
            grpc_url: Some("http://example.com:9090".to_string()),
            output_format: OutputFormat::Json,
            colored: false,
        };

        let toml_str = toml::to_string_pretty(&config).unwrap();
        let deserialized: Config = toml::from_str(&toml_str).unwrap();

        assert_eq!(deserialized.rest_api_url, config.rest_api_url);
        assert_eq!(deserialized.grpc_url, config.grpc_url);
        assert_eq!(deserialized.output_format, config.output_format);
        assert_eq!(deserialized.colored, config.colored);
    }

    #[test]
    fn test_config_deserialize_all_formats() {
        let toml_str = r#"
            rest_api_url = "http://localhost:8080"
            output_format = "json"
            colored = true
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.output_format, OutputFormat::Json);

        let toml_str = r#"
            rest_api_url = "http://localhost:8080"
            output_format = "yaml"
            colored = true
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.output_format, OutputFormat::Yaml);

        let toml_str = r#"
            rest_api_url = "http://localhost:8080"
            output_format = "text"
            colored = true
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.output_format, OutputFormat::Text);
    }

    #[test]
    fn test_config_optional_grpc_url() {
        let toml_str = r#"
            rest_api_url = "http://localhost:8080"
            output_format = "table"
            colored = true
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.grpc_url.is_none());
    }

    #[test]
    fn test_config_save_and_load() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let config = Config {
            rest_api_url: "http://test:8080".to_string(),
            grpc_url: None,
            output_format: OutputFormat::Yaml,
            colored: false,
        };

        // Save manually
        let contents = toml::to_string_pretty(&config).unwrap();
        std::fs::write(&config_path, &contents).unwrap();

        // Read back
        let loaded_str = std::fs::read_to_string(&config_path).unwrap();
        let loaded: Config = toml::from_str(&loaded_str).unwrap();
        assert_eq!(loaded.rest_api_url, "http://test:8080");
        assert_eq!(loaded.output_format, OutputFormat::Yaml);
        assert!(!loaded.colored);
    }
}
