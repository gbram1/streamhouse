//! Schema Registry command handlers
//!
//! Provides commands for managing schemas:
//! - list: List all subjects
//! - register: Register a new schema
//! - get: Get schema by subject/version or ID
//! - check: Check schema compatibility
//! - evolve: Evolve schema (register after compatibility check)
//! - delete: Delete schema version or subject
//! - config: Get/set compatibility configuration

use crate::rest_client::SchemaRegistryClient;
use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum SchemaCommands {
    /// List all subjects
    List,

    /// Register a new schema
    Register {
        /// Subject name (e.g., "orders-value")
        subject: String,

        /// Path to schema file
        schema_file: String,

        /// Schema type (AVRO, PROTOBUF, JSON)
        #[arg(short, long, default_value = "AVRO")]
        schema_type: String,
    },

    /// Get schema by subject/version or ID
    Get {
        /// Subject name
        #[arg(required_unless_present = "id")]
        subject: Option<String>,

        /// Version number or "latest"
        #[arg(short, long, default_value = "latest")]
        version: String,

        /// Schema ID
        #[arg(long, conflicts_with_all = &["subject", "version"])]
        id: Option<i32>,
    },

    /// Check schema compatibility
    Check {
        /// Subject name
        subject: String,

        /// Path to new schema file
        schema_file: String,
    },

    /// Evolve schema (check compatibility then register)
    Evolve {
        /// Subject name
        subject: String,

        /// Path to new schema file
        schema_file: String,

        /// Schema type (AVRO, PROTOBUF, JSON)
        #[arg(short, long, default_value = "AVRO")]
        schema_type: String,
    },

    /// Delete schema version or subject
    Delete {
        /// Subject name
        subject: String,

        /// Version to delete (if not specified, deletes entire subject)
        #[arg(short, long)]
        version: Option<String>,
    },

    /// Configuration management
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Get compatibility configuration
    Get {
        /// Subject name (if not specified, gets global config)
        subject: Option<String>,
    },

    /// Set compatibility configuration
    Set {
        /// Subject name
        subject: String,

        /// Compatibility mode
        #[arg(long)]
        compatibility: String,
    },
}

/// Handle schema commands
pub async fn handle_schema_command(command: SchemaCommands, registry_url: &str) -> Result<()> {
    let client = SchemaRegistryClient::new(registry_url);

    match command {
        SchemaCommands::List => handle_list(&client).await,
        SchemaCommands::Register {
            subject,
            schema_file,
            schema_type,
        } => handle_register(&client, &subject, &schema_file, &schema_type).await,
        SchemaCommands::Get {
            subject,
            version,
            id,
        } => handle_get(&client, subject, &version, id).await,
        SchemaCommands::Check {
            subject,
            schema_file,
        } => handle_check(&client, &subject, &schema_file).await,
        SchemaCommands::Evolve {
            subject,
            schema_file,
            schema_type,
        } => handle_evolve(&client, &subject, &schema_file, &schema_type).await,
        SchemaCommands::Delete { subject, version } => {
            handle_delete(&client, &subject, version).await
        }
        SchemaCommands::Config { command } => handle_config_command(&client, command).await,
    }
}

/// List all subjects
async fn handle_list(client: &SchemaRegistryClient) -> Result<()> {
    let subjects = client.list_subjects().await?;

    if subjects.is_empty() {
        println!("No subjects found");
    } else {
        println!("Subjects ({}):", subjects.len());
        for subject in subjects {
            println!("  {}", subject);
        }
    }

    Ok(())
}

/// Register a new schema
async fn handle_register(
    client: &SchemaRegistryClient,
    subject: &str,
    schema_file: &str,
    schema_type: &str,
) -> Result<()> {
    let schema = std::fs::read_to_string(schema_file)
        .context(format!("Failed to read schema file: {}", schema_file))?;

    let response = client
        .register_schema(subject, &schema, Some(schema_type))
        .await?;

    println!("✅ Schema registered:");
    println!("  Subject: {}", subject);
    println!("  Schema ID: {}", response.id);
    println!("  Type: {}", schema_type);

    Ok(())
}

/// Get schema by subject/version or ID
async fn handle_get(
    client: &SchemaRegistryClient,
    subject: Option<String>,
    version: &str,
    id: Option<i32>,
) -> Result<()> {
    if let Some(schema_id) = id {
        // Get by ID
        let schema = client.get_schema_by_id(schema_id).await?;
        print_schema(&schema);
    } else if let Some(subj) = subject {
        // Get by subject + version
        let schema = client.get_schema_by_version(&subj, version).await?;
        print_schema(&schema);
    } else {
        anyhow::bail!("Must specify either --id or subject");
    }

    Ok(())
}

/// Check schema compatibility
async fn handle_check(
    client: &SchemaRegistryClient,
    subject: &str,
    schema_file: &str,
) -> Result<()> {
    let new_schema = std::fs::read_to_string(schema_file)
        .context(format!("Failed to read schema file: {}", schema_file))?;

    // Get current schema
    let current = client.get_schema_by_version(subject, "latest").await?;

    // Get compatibility config
    let config = client.get_subject_config(subject).await.ok();
    let compatibility_mode = config
        .as_ref()
        .map(|c| c.compatibility_level.as_str())
        .unwrap_or("BACKWARD");

    // For this version, we'll do a simple check:
    // Just verify the new schema is valid by trying to parse it
    // TODO: Implement proper Avro/Protobuf/JSON schema compatibility checking
    println!("Checking compatibility for subject: {}", subject);
    println!("Current version: {}", current.version);
    println!("Compatibility mode: {}", compatibility_mode);
    println!();

    // Simple validation: check if schema is non-empty and valid JSON
    if new_schema.trim().is_empty() {
        anyhow::bail!("Schema file is empty");
    }

    // Try to parse as JSON
    if let Err(e) = serde_json::from_str::<serde_json::Value>(&new_schema) {
        anyhow::bail!("Invalid JSON schema: {}", e);
    }

    println!("✓ Schema is valid");
    println!("⚠️  Full compatibility check not yet implemented");
    println!("   (Evolution will still fail if incompatible)");

    Ok(())
}

/// Evolve schema (check then register)
async fn handle_evolve(
    client: &SchemaRegistryClient,
    subject: &str,
    schema_file: &str,
    schema_type: &str,
) -> Result<()> {
    // First check compatibility
    println!("Checking compatibility...");
    handle_check(client, subject, schema_file).await?;

    println!();
    println!("Registering new schema...");

    // Then register
    handle_register(client, subject, schema_file, schema_type).await
}

/// Delete schema version or subject
async fn handle_delete(
    client: &SchemaRegistryClient,
    subject: &str,
    version: Option<String>,
) -> Result<()> {
    if let Some(ver) = version {
        // Delete specific version
        let deleted_version = client.delete_schema_version(subject, &ver).await?;

        println!("✅ Schema version deleted:");
        println!("  Subject: {}", subject);
        println!("  Version: {}", deleted_version);
    } else {
        // Delete entire subject
        let deleted_versions = client.delete_subject(subject).await?;

        println!("✅ Subject deleted:");
        println!("  Subject: {}", subject);
        println!("  Versions deleted: {:?}", deleted_versions);
    }

    Ok(())
}

/// Handle config subcommands
async fn handle_config_command(
    client: &SchemaRegistryClient,
    command: ConfigCommands,
) -> Result<()> {
    match command {
        ConfigCommands::Get { subject } => {
            if let Some(subj) = subject {
                // Get subject-specific config
                let config = client.get_subject_config(&subj).await?;
                println!("Subject: {}", subj);
                println!("Compatibility: {}", config.compatibility_level);
            } else {
                // Get global config
                let config = client.get_global_config().await?;
                println!("Global compatibility: {}", config.compatibility_level);
            }
        }
        ConfigCommands::Set {
            subject,
            compatibility,
        } => {
            let config = client.set_subject_config(&subject, &compatibility).await?;

            println!("✅ Compatibility updated:");
            println!("  Subject: {}", subject);
            println!("  Compatibility: {}", config.compatibility_level);
        }
    }

    Ok(())
}

/// Print schema details
fn print_schema(schema: &crate::rest_client::SchemaResponse) {
    println!("Schema Details:");
    println!("  Subject: {}", schema.subject);
    println!("  Version: {}", schema.version);
    println!("  ID: {}", schema.id);
    println!("  Type: {}", schema.schema_type);
    println!();
    println!("Schema:");

    // Try to pretty-print JSON schema
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&schema.schema) {
        if let Ok(pretty) = serde_json::to_string_pretty(&json) {
            println!("{}", pretty);
            return;
        }
    }

    // Fallback to raw schema
    println!("{}", schema.schema);
}
