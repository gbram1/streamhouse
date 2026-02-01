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
