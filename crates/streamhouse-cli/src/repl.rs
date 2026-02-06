//! Interactive REPL mode for streamctl
//!
//! Provides a rustyline-based interactive shell with:
//! - Command history (up/down arrows)
//! - Auto-completion (tab)
//! - Colorized output
//! - Multi-line input support

use anyhow::{Context, Result};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::path::PathBuf;
use tonic::transport::Channel;

use crate::commands;
use crate::pb::stream_house_client::StreamHouseClient;
use crate::{
    handle_consume, handle_offset_command, handle_produce, handle_topic_command, OffsetCommands,
    TopicCommands,
};

/// REPL context holding the gRPC client and editor
pub struct Repl {
    client: StreamHouseClient<Channel>,
    editor: DefaultEditor,
    schema_registry_url: String,
    api_url: String,
}

impl Repl {
    /// Create a new REPL instance
    pub fn new(
        client: StreamHouseClient<Channel>,
        schema_registry_url: String,
        api_url: String,
    ) -> Result<Self> {
        let mut editor = DefaultEditor::new()?;

        // Load history from file if it exists
        let history_path = Self::history_path()?;
        if history_path.exists() {
            let _ = editor.load_history(&history_path);
        }

        Ok(Self {
            client,
            editor,
            schema_registry_url,
            api_url,
        })
    }

    /// Run the interactive REPL loop
    pub async fn run(&mut self) -> Result<()> {
        println!("StreamHouse Interactive Shell");
        println!("Type 'help' for available commands, 'exit' or Ctrl+D to quit");
        println!();

        loop {
            // Read line with prompt
            match self.editor.readline("streamhouse> ") {
                Ok(line) => {
                    let line = line.trim();

                    // Skip empty lines
                    if line.is_empty() {
                        continue;
                    }

                    // Add to history
                    let _ = self.editor.add_history_entry(line);

                    // Handle exit commands
                    if line == "exit" || line == "quit" {
                        break;
                    }

                    // Handle help
                    if line == "help" {
                        Self::print_help();
                        continue;
                    }

                    // Parse and execute command
                    if let Err(e) = self.execute_command(line).await {
                        eprintln!("Error: {}", e);
                    }

                    println!();
                }
                Err(ReadlineError::Interrupted) => {
                    // Ctrl+C - continue
                    println!("^C");
                    continue;
                }
                Err(ReadlineError::Eof) => {
                    // Ctrl+D - exit
                    println!("exit");
                    break;
                }
                Err(err) => {
                    eprintln!("Error reading line: {}", err);
                    break;
                }
            }
        }

        // Save history on exit
        let history_path = Self::history_path()?;
        if let Some(parent) = history_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        self.editor.save_history(&history_path)?;

        println!("Goodbye!");
        Ok(())
    }

    /// Execute a command from the REPL
    async fn execute_command(&mut self, line: &str) -> Result<()> {
        // Split command into tokens (simple whitespace splitting for now)
        let tokens: Vec<&str> = line.split_whitespace().collect();

        if tokens.is_empty() {
            return Ok(());
        }

        match tokens[0] {
            "topic" => self.handle_topic(&tokens[1..]).await?,
            "schema" => self.handle_schema(&tokens[1..]).await?,
            "produce" => self.handle_produce(&tokens[1..]).await?,
            "consume" => self.handle_consume(&tokens[1..]).await?,
            "offset" => self.handle_offset(&tokens[1..]).await?,
            "sql" => self.handle_sql(&tokens[1..]).await?,
            "consumer" => self.handle_consumer(&tokens[1..]).await?,
            _ => {
                println!("Unknown command: {}", tokens[0]);
                println!("Type 'help' for available commands");
            }
        }

        Ok(())
    }

    /// Handle topic commands
    async fn handle_topic(&mut self, args: &[&str]) -> Result<()> {
        if args.is_empty() {
            println!("Usage: topic <list|create|get|delete> [args...]");
            return Ok(());
        }

        let command = match args[0] {
            "list" => TopicCommands::List,
            "create" => {
                if args.len() < 2 {
                    println!("Usage: topic create <name> [--partitions N]");
                    return Ok(());
                }
                let name = args[1].to_string();
                let partitions = Self::parse_flag_u32(args, "--partitions").unwrap_or(1);
                let retention_ms = Self::parse_flag_u64(args, "--retention");

                TopicCommands::Create {
                    name,
                    partitions,
                    retention_ms,
                }
            }
            "get" => {
                if args.len() < 2 {
                    println!("Usage: topic get <name>");
                    return Ok(());
                }
                TopicCommands::Get {
                    name: args[1].to_string(),
                }
            }
            "delete" => {
                if args.len() < 2 {
                    println!("Usage: topic delete <name>");
                    return Ok(());
                }
                TopicCommands::Delete {
                    name: args[1].to_string(),
                }
            }
            _ => {
                println!("Unknown topic command: {}", args[0]);
                return Ok(());
            }
        };

        handle_topic_command(&mut self.client, command).await
    }

    /// Handle produce command
    async fn handle_produce(&mut self, args: &[&str]) -> Result<()> {
        if args.is_empty() {
            println!("Usage: produce <topic> --partition N --value <value> [--key <key>]");
            return Ok(());
        }

        let topic = args[0].to_string();
        let partition =
            Self::parse_flag_u32(args, "--partition").context("Missing --partition flag")?;
        let value = Self::parse_flag_string(args, "--value").context("Missing --value flag")?;
        let key = Self::parse_flag_string(args, "--key");

        handle_produce(&mut self.client, topic, partition, key, value).await
    }

    /// Handle consume command
    async fn handle_consume(&mut self, args: &[&str]) -> Result<()> {
        if args.is_empty() {
            println!("Usage: consume <topic> --partition N [--offset O] [--limit L]");
            return Ok(());
        }

        let topic = args[0].to_string();
        let partition =
            Self::parse_flag_u32(args, "--partition").context("Missing --partition flag")?;
        let offset = Self::parse_flag_u64(args, "--offset").unwrap_or(0);
        let limit = Self::parse_flag_u32(args, "--limit");

        handle_consume(&mut self.client, topic, partition, offset, limit).await
    }

    /// Handle offset commands
    async fn handle_offset(&mut self, args: &[&str]) -> Result<()> {
        if args.is_empty() {
            println!("Usage: offset <commit|get> --group <group> --topic <topic> --partition N [--offset O]");
            return Ok(());
        }

        let command = match args[0] {
            "commit" => {
                let group =
                    Self::parse_flag_string(args, "--group").context("Missing --group flag")?;
                let topic =
                    Self::parse_flag_string(args, "--topic").context("Missing --topic flag")?;
                let partition = Self::parse_flag_u32(args, "--partition")
                    .context("Missing --partition flag")?;
                let offset =
                    Self::parse_flag_u64(args, "--offset").context("Missing --offset flag")?;

                OffsetCommands::Commit {
                    group,
                    topic,
                    partition,
                    offset,
                }
            }
            "get" => {
                let group =
                    Self::parse_flag_string(args, "--group").context("Missing --group flag")?;
                let topic =
                    Self::parse_flag_string(args, "--topic").context("Missing --topic flag")?;
                let partition = Self::parse_flag_u32(args, "--partition")
                    .context("Missing --partition flag")?;

                OffsetCommands::Get {
                    group,
                    topic,
                    partition,
                }
            }
            _ => {
                println!("Unknown offset command: {}", args[0]);
                return Ok(());
            }
        };

        handle_offset_command(&mut self.client, command).await
    }

    /// Parse a flag value as string
    fn parse_flag_string(args: &[&str], flag: &str) -> Option<String> {
        args.iter()
            .position(|&arg| arg == flag)
            .and_then(|pos| args.get(pos + 1))
            .map(|s| s.to_string())
    }

    /// Parse a flag value as u32
    fn parse_flag_u32(args: &[&str], flag: &str) -> Option<u32> {
        Self::parse_flag_string(args, flag).and_then(|s| s.parse().ok())
    }

    /// Parse a flag value as u64
    fn parse_flag_u64(args: &[&str], flag: &str) -> Option<u64> {
        Self::parse_flag_string(args, flag).and_then(|s| s.parse().ok())
    }

    /// Get history file path
    fn history_path() -> Result<PathBuf> {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        Ok(PathBuf::from(home).join(".streamhouse").join("history"))
    }

    /// Handle schema commands
    async fn handle_schema(&mut self, args: &[&str]) -> Result<()> {
        if args.is_empty() {
            println!("Usage: schema <list|register|get|check|evolve|delete|config> [args...]");
            return Ok(());
        }

        let command = match args[0] {
            "list" => commands::SchemaCommands::List,
            "register" => {
                if args.len() < 3 {
                    println!("Usage: schema register <subject> <schema-file> [--schema-type TYPE]");
                    return Ok(());
                }
                let schema_type = Self::parse_flag_string(args, "--schema-type")
                    .unwrap_or_else(|| "AVRO".to_string());

                commands::SchemaCommands::Register {
                    subject: args[1].to_string(),
                    schema_file: args[2].to_string(),
                    schema_type,
                }
            }
            "get" => {
                let id = Self::parse_flag_string(args, "--id").and_then(|s| s.parse().ok());
                let subject = if id.is_none() && args.len() > 1 {
                    Some(args[1].to_string())
                } else {
                    None
                };
                let version = Self::parse_flag_string(args, "--version")
                    .unwrap_or_else(|| "latest".to_string());

                commands::SchemaCommands::Get {
                    subject,
                    version,
                    id,
                }
            }
            "check" => {
                if args.len() < 3 {
                    println!("Usage: schema check <subject> <schema-file>");
                    return Ok(());
                }
                commands::SchemaCommands::Check {
                    subject: args[1].to_string(),
                    schema_file: args[2].to_string(),
                }
            }
            "evolve" => {
                if args.len() < 3 {
                    println!("Usage: schema evolve <subject> <schema-file> [--schema-type TYPE]");
                    return Ok(());
                }
                let schema_type = Self::parse_flag_string(args, "--schema-type")
                    .unwrap_or_else(|| "AVRO".to_string());

                commands::SchemaCommands::Evolve {
                    subject: args[1].to_string(),
                    schema_file: args[2].to_string(),
                    schema_type,
                }
            }
            "delete" => {
                if args.len() < 2 {
                    println!("Usage: schema delete <subject> [--version V]");
                    return Ok(());
                }
                let version = Self::parse_flag_string(args, "--version");

                commands::SchemaCommands::Delete {
                    subject: args[1].to_string(),
                    version,
                }
            }
            "config" => {
                if args.len() < 2 {
                    println!("Usage: schema config <get|set> [args...]");
                    return Ok(());
                }

                let config_command = match args[1] {
                    "get" => commands::schema::ConfigCommands::Get {
                        subject: args.get(2).map(|s| s.to_string()),
                    },
                    "set" => {
                        if args.len() < 3 {
                            println!("Usage: schema config set <subject> --compatibility MODE");
                            return Ok(());
                        }
                        let compatibility = Self::parse_flag_string(args, "--compatibility")
                            .context("Missing --compatibility flag")?;

                        commands::schema::ConfigCommands::Set {
                            subject: args[2].to_string(),
                            compatibility,
                        }
                    }
                    _ => {
                        println!("Unknown config command: {}", args[1]);
                        return Ok(());
                    }
                };

                commands::SchemaCommands::Config {
                    command: config_command,
                }
            }
            _ => {
                println!("Unknown schema command: {}", args[0]);
                return Ok(());
            }
        };

        commands::schema::handle_schema_command(command, &self.schema_registry_url).await
    }

    /// Handle SQL commands
    async fn handle_sql(&mut self, args: &[&str]) -> Result<()> {
        if args.is_empty() {
            println!("Usage: sql <query|shell> [args...]");
            println!("  sql query \"SELECT * FROM topic LIMIT 10\"");
            println!("  sql shell");
            return Ok(());
        }

        let command = match args[0] {
            "query" => {
                if args.len() < 2 {
                    println!(
                        "Usage: sql query \"<SQL query>\" [--timeout MS] [--format table|json|csv]"
                    );
                    return Ok(());
                }
                // Join remaining args as the query (handles quoted strings)
                let query = args[1..].join(" ");
                // Remove surrounding quotes if present
                let query = query.trim_matches('"').trim_matches('\'').to_string();
                let timeout = Self::parse_flag_u64(args, "--timeout").unwrap_or(30000);
                let format = Self::parse_flag_string(args, "--format")
                    .unwrap_or_else(|| "table".to_string());

                commands::SqlCommands::Query {
                    query,
                    timeout,
                    format,
                }
            }
            "shell" => commands::SqlCommands::Shell,
            // Allow direct SQL queries without "query" subcommand
            _ => {
                // Treat everything as a SQL query
                let query = args.join(" ");
                let query = query.trim_matches('"').trim_matches('\'').to_string();

                commands::SqlCommands::Query {
                    query,
                    timeout: 30000,
                    format: "table".to_string(),
                }
            }
        };

        commands::sql::handle_sql_command(command, &self.api_url).await
    }

    /// Handle consumer group commands
    async fn handle_consumer(&mut self, args: &[&str]) -> Result<()> {
        if args.is_empty() {
            println!("Usage: consumer <list|get|lag|reset|seek|delete> [args...]");
            return Ok(());
        }

        let command = match args[0] {
            "list" => commands::ConsumerCommands::List,
            "get" => {
                if args.len() < 2 {
                    println!("Usage: consumer get <group_id>");
                    return Ok(());
                }
                commands::ConsumerCommands::Get {
                    group_id: args[1].to_string(),
                }
            }
            "lag" => {
                if args.len() < 2 {
                    println!("Usage: consumer lag <group_id>");
                    return Ok(());
                }
                commands::ConsumerCommands::Lag {
                    group_id: args[1].to_string(),
                }
            }
            "reset" => {
                if args.len() < 2 {
                    println!("Usage: consumer reset <group_id> [--strategy earliest|latest|specific] [--topic T] [--partition P] [--offset O]");
                    return Ok(());
                }
                let strategy = Self::parse_flag_string(args, "--strategy")
                    .unwrap_or_else(|| "earliest".to_string());
                let topic = Self::parse_flag_string(args, "--topic");
                let partition = Self::parse_flag_u32(args, "--partition");
                let offset = Self::parse_flag_u64(args, "--offset");

                commands::ConsumerCommands::Reset {
                    group_id: args[1].to_string(),
                    strategy,
                    topic,
                    partition,
                    offset,
                }
            }
            "seek" => {
                if args.len() < 2 {
                    println!("Usage: consumer seek <group_id> --topic <topic> --timestamp <ISO8601> [--partition P]");
                    return Ok(());
                }
                let topic =
                    Self::parse_flag_string(args, "--topic").context("Missing --topic flag")?;
                let timestamp = Self::parse_flag_string(args, "--timestamp")
                    .context("Missing --timestamp flag")?;
                let partition = Self::parse_flag_u32(args, "--partition");

                commands::ConsumerCommands::Seek {
                    group_id: args[1].to_string(),
                    topic,
                    timestamp,
                    partition,
                }
            }
            "delete" => {
                if args.len() < 2 {
                    println!("Usage: consumer delete <group_id> [--force]");
                    return Ok(());
                }
                let force = args.contains(&"--force");

                commands::ConsumerCommands::Delete {
                    group_id: args[1].to_string(),
                    force,
                }
            }
            _ => {
                println!("Unknown consumer command: {}", args[0]);
                return Ok(());
            }
        };

        commands::consumer::handle_consumer_command(command, &self.api_url).await
    }

    /// Print help message
    fn print_help() {
        println!("Available commands:");
        println!();
        println!("  Topic Management:");
        println!("    topic list");
        println!("    topic create <name> [--partitions N] [--retention MS]");
        println!("    topic get <name>");
        println!("    topic delete <name>");
        println!();
        println!("  Schema Registry:");
        println!("    schema list");
        println!("    schema register <subject> <file> [--schema-type TYPE]");
        println!("    schema get <subject> [--version V] | [--id ID]");
        println!("    schema check <subject> <file>");
        println!("    schema evolve <subject> <file>");
        println!("    schema delete <subject> [--version V]");
        println!("    schema config get [<subject>]");
        println!("    schema config set <subject> --compatibility MODE");
        println!();
        println!("  SQL Queries:");
        println!("    sql query \"SELECT * FROM topic LIMIT 10\" [--format table|json|csv]");
        println!("    sql SHOW TOPICS");
        println!("    sql SELECT * FROM orders WHERE partition = 0 LIMIT 50");
        println!("    sql shell                          (interactive SQL shell)");
        println!();
        println!("  Consumer Groups:");
        println!("    consumer list");
        println!("    consumer get <group_id>");
        println!("    consumer lag <group_id>");
        println!("    consumer reset <group_id> [--strategy earliest|latest|specific] [--topic T] [--partition P] [--offset O]");
        println!(
            "    consumer seek <group_id> --topic <topic> --timestamp <ISO8601> [--partition P]"
        );
        println!("    consumer delete <group_id> [--force]");
        println!();
        println!("  Produce/Consume:");
        println!("    produce <topic> --partition N --value <value> [--key <key>]");
        println!("    consume <topic> --partition N [--offset O] [--limit L]");
        println!();
        println!("  Consumer Offsets:");
        println!("    offset commit --group <group> --topic <topic> --partition N --offset O");
        println!("    offset get --group <group> --topic <topic> --partition N");
        println!();
        println!("  Other:");
        println!("    help       Show this help message");
        println!("    exit/quit  Exit the shell");
    }
}
