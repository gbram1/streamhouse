//! Project scaffolding command
//!
//! `stm init [directory]` creates a new StreamHouse project with a template config.

use anyhow::{Context, Result};
use std::path::Path;

const TEMPLATE_YAML: &str = r#"# StreamHouse declarative configuration
# Apply with: stm apply
# Destroy with: stm destroy

# Topics define your data streams
topics:
  - name: events
    partitions: 3
  # - name: orders
  #   partitions: 1

# Targets define where pipelines sink data
# targets:
#   - name: pg-warehouse
#     type: postgres
#     config:
#       connection_url: postgres://user:pass@localhost:5432/db
#       table_name: events

# Pipelines connect topics to targets with optional SQL transforms
# pipelines:
#   - name: events-to-pg
#     source: events
#     target: pg-warehouse
#     sql: |
#       SELECT * FROM events

# Connectors for external system integration
# connectors:
#   - name: s3-backup
#     type: sink
#     class: s3
#     topics: [events]
#     config:
#       bucket: my-bucket
#       region: us-east-1
"#;

const ENV_EXAMPLE: &str = r#"# StreamHouse CLI configuration
STREAMHOUSE_API_URL=http://localhost:8080
STREAMHOUSE_ADDR=http://localhost:50051
SCHEMA_REGISTRY_URL=http://localhost:8080/schemas
# STREAMHOUSE_API_KEY=your-api-key-here
"#;

pub fn handle_init(directory: Option<&str>) -> Result<()> {
    let dir = match directory {
        Some(d) => {
            let path = Path::new(d);
            if !path.exists() {
                std::fs::create_dir_all(path)
                    .context(format!("Failed to create directory '{}'", d))?;
            }
            path.to_path_buf()
        }
        None => std::env::current_dir().context("Failed to get current directory")?,
    };

    let yaml_path = dir.join("streamhouse.yaml");
    let env_path = dir.join(".env.example");

    if yaml_path.exists() {
        anyhow::bail!("streamhouse.yaml already exists in {}", dir.display());
    }

    std::fs::write(&yaml_path, TEMPLATE_YAML).context("Failed to write streamhouse.yaml")?;
    std::fs::write(&env_path, ENV_EXAMPLE).context("Failed to write .env.example")?;

    println!("Project initialized in {}", dir.display());
    println!("  Created: streamhouse.yaml");
    println!("  Created: .env.example");
    println!();
    println!("Edit streamhouse.yaml, then run:");
    println!("  stm apply");

    Ok(())
}
