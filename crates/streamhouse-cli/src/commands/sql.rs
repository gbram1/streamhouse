//! SQL query commands
//!
//! Execute SQL queries against StreamHouse topics via the REST API.
//!
//! ## Examples
//!
//! ```bash
//! # List all topics
//! streamctl sql "SHOW TOPICS"
//!
//! # Query messages from a topic
//! streamctl sql "SELECT * FROM orders LIMIT 10"
//!
//! # Filter by key
//! streamctl sql "SELECT * FROM orders WHERE key = 'customer-123' LIMIT 50"
//!
//! # Extract JSON fields
//! streamctl sql "SELECT key, json_extract(value, '$.amount') as amount FROM orders LIMIT 10"
//!
//! # Interactive mode
//! streamctl sql --interactive
//! ```

use anyhow::{Context, Result};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};

use crate::rest_client::RestClient;

#[derive(Subcommand)]
pub enum SqlCommands {
    /// Execute a SQL query
    Query {
        /// SQL query to execute
        query: String,
        /// Timeout in milliseconds (default: 30000)
        #[arg(short, long, default_value = "30000")]
        timeout: u64,
        /// Output format: table, json, csv (default: table)
        #[arg(short, long, default_value = "table")]
        format: String,
    },
    /// Start interactive SQL shell
    Shell,
}

// API Request/Response types

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SqlQueryRequest {
    query: String,
    timeout_ms: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SqlQueryResponse {
    columns: Vec<ColumnInfo>,
    rows: Vec<Vec<serde_json::Value>>,
    row_count: usize,
    execution_time_ms: u64,
    truncated: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ColumnInfo {
    name: String,
    data_type: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SqlErrorResponse {
    error: String,
    message: String,
}

/// Handle SQL commands
pub async fn handle_sql_command(command: SqlCommands, api_url: &str) -> Result<()> {
    match command {
        SqlCommands::Query {
            query,
            timeout,
            format,
        } => {
            execute_and_display_query(api_url, &query, timeout, &format).await?;
        }
        SqlCommands::Shell => {
            run_interactive_shell(api_url).await?;
        }
    }

    Ok(())
}

/// Execute a single SQL query and display results
async fn execute_and_display_query(
    api_url: &str,
    query: &str,
    timeout_ms: u64,
    format: &str,
) -> Result<()> {
    let client = RestClient::new(api_url);

    let request = SqlQueryRequest {
        query: query.to_string(),
        timeout_ms,
    };

    let response: SqlQueryResponse = client
        .post("/api/v1/sql", &request)
        .await
        .context("Failed to execute SQL query")?;

    match format {
        "json" => print_json(&response)?,
        "csv" => print_csv(&response)?,
        _ => print_table(&response)?,
    }

    // Print execution info
    eprintln!();
    eprintln!(
        "({} rows in {}ms{})",
        response.row_count,
        response.execution_time_ms,
        if response.truncated {
            ", truncated"
        } else {
            ""
        }
    );

    Ok(())
}

/// Print results as a formatted table
fn print_table(response: &SqlQueryResponse) -> Result<()> {
    if response.columns.is_empty() {
        println!("Query returned no columns");
        return Ok(());
    }

    // Calculate column widths
    let mut widths: Vec<usize> = response
        .columns
        .iter()
        .map(|c| c.name.len().max(10))
        .collect();

    for row in &response.rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                let cell_width = format_cell(cell).len();
                widths[i] = widths[i].max(cell_width.min(50)); // Cap at 50 chars
            }
        }
    }

    // Print header
    let header: Vec<String> = response
        .columns
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{:width$}", c.name, width = widths[i]))
        .collect();
    println!("{}", header.join(" | "));

    // Print separator
    let separator: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    println!("{}", separator.join("-+-"));

    // Print rows
    for row in &response.rows {
        let formatted: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, cell)| {
                let width = widths.get(i).copied().unwrap_or(10);
                let value = format_cell(cell);
                if value.len() > width {
                    format!("{}...", &value[..width.saturating_sub(3)])
                } else {
                    format!("{:width$}", value, width = width)
                }
            })
            .collect();
        println!("{}", formatted.join(" | "));
    }

    Ok(())
}

/// Print results as JSON
fn print_json(response: &SqlQueryResponse) -> Result<()> {
    // Build a JSON array of objects
    let rows: Vec<serde_json::Value> = response
        .rows
        .iter()
        .map(|row| {
            let obj: serde_json::Map<String, serde_json::Value> = response
                .columns
                .iter()
                .enumerate()
                .map(|(i, col)| {
                    (
                        col.name.clone(),
                        row.get(i).cloned().unwrap_or(serde_json::Value::Null),
                    )
                })
                .collect();
            serde_json::Value::Object(obj)
        })
        .collect();

    println!("{}", serde_json::to_string_pretty(&rows)?);
    Ok(())
}

/// Print results as CSV
fn print_csv(response: &SqlQueryResponse) -> Result<()> {
    // Print header
    let header: Vec<String> = response
        .columns
        .iter()
        .map(|c| escape_csv(&c.name))
        .collect();
    println!("{}", header.join(","));

    // Print rows
    for row in &response.rows {
        let formatted: Vec<String> = row
            .iter()
            .map(|cell| escape_csv(&format_cell(cell)))
            .collect();
        println!("{}", formatted.join(","));
    }

    Ok(())
}

/// Format a JSON value for display
fn format_cell(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "NULL".to_string(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            serde_json::to_string(value).unwrap_or_else(|_| "?".to_string())
        }
    }
}

/// Escape a string for CSV output
fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Run an interactive SQL shell
async fn run_interactive_shell(api_url: &str) -> Result<()> {
    println!("StreamHouse SQL Shell");
    println!("Connected to: {}", api_url);
    println!("Type 'help' for help, 'exit' to quit.");
    println!();

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("sql> ");
        stdout.flush()?;

        let mut input = String::new();
        if stdin.read_line(&mut input)? == 0 {
            // EOF
            println!();
            break;
        }

        let query = input.trim();

        if query.is_empty() {
            continue;
        }

        match query.to_lowercase().as_str() {
            "exit" | "quit" | "\\q" => {
                println!("Goodbye!");
                break;
            }
            "help" | "\\h" | "\\?" => {
                print_shell_help();
                continue;
            }
            "clear" | "\\c" => {
                print!("\x1B[2J\x1B[1;1H");
                stdout.flush()?;
                continue;
            }
            _ => {}
        }

        // Execute query
        match execute_and_display_query(api_url, query, 30000, "table").await {
            Ok(()) => {}
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }

        println!();
    }

    Ok(())
}

/// Print help for the interactive shell
fn print_shell_help() {
    println!("StreamHouse SQL Help");
    println!();
    println!("Commands:");
    println!("  exit, quit, \\q    Exit the shell");
    println!("  help, \\h, \\?      Show this help");
    println!("  clear, \\c         Clear the screen");
    println!();
    println!("SQL Statements:");
    println!("  SHOW TOPICS                       List all topics");
    println!("  DESCRIBE <topic>                  Show topic structure");
    println!("  SELECT * FROM <topic> LIMIT n     Query messages");
    println!("  SELECT COUNT(*) FROM <topic>      Count messages");
    println!();
    println!("Built-in Columns:");
    println!("  topic, partition, offset, key, value, timestamp");
    println!();
    println!("WHERE Clauses:");
    println!("  WHERE key = 'value'");
    println!("  WHERE partition = 0");
    println!("  WHERE offset >= 100 AND offset < 200");
    println!("  WHERE timestamp >= '2026-01-01T00:00:00Z'");
    println!();
    println!("Functions:");
    println!("  json_extract(value, '$.field')    Extract JSON field");
    println!();
    println!("Examples:");
    println!("  SELECT * FROM orders LIMIT 10;");
    println!("  SELECT key, json_extract(value, '$.amount') as amt FROM orders LIMIT 50;");
    println!("  SELECT * FROM orders WHERE partition = 0 AND offset >= 0 AND offset < 100;");
    println!();
}
