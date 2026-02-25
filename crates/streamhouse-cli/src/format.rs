//! Output formatting utilities for streamctl
//!
//! Supports multiple output formats:
//! - Table: ASCII tables with borders (default)
//! - JSON: Machine-readable JSON
//! - YAML: Human-readable YAML
//! - Text: Plain text, one item per line

use crate::config::OutputFormat;
use anyhow::Result;
use colored::*;
use serde::Serialize;
use tabled::{
    settings::{object::Rows, Alignment, Modify, Style},
    Table, Tabled,
};

/// Format and print output based on configured format
pub struct Formatter {
    format: OutputFormat,
    colored: bool,
}

impl Formatter {
    pub fn new(format: OutputFormat, colored: bool) -> Self {
        Self { format, colored }
    }

    /// Print a list of items
    pub fn print_list<T: Serialize + Tabled>(&self, items: Vec<T>) -> Result<()> {
        match self.format {
            OutputFormat::Table => self.print_table(items),
            OutputFormat::Json => self.print_json(&items),
            OutputFormat::Yaml => self.print_yaml(&items),
            OutputFormat::Text => self.print_text(items),
        }
    }

    /// Print a single item
    pub fn print_single<T: Serialize + Tabled>(&self, item: T) -> Result<()> {
        match self.format {
            OutputFormat::Table => self.print_table(vec![item]),
            OutputFormat::Json => self.print_json(&item),
            OutputFormat::Yaml => self.print_yaml(&item),
            OutputFormat::Text => self.print_text(vec![item]),
        }
    }

    /// Print a success message
    pub fn print_success(&self, message: &str) {
        if self.colored {
            println!("{} {}", "✅".green(), message);
        } else {
            println!("✅ {}", message);
        }
    }

    /// Print an error message
    pub fn print_error(&self, message: &str) {
        if self.colored {
            eprintln!("{} {}", "❌".red(), message);
        } else {
            eprintln!("❌ {}", message);
        }
    }

    /// Print an info message
    pub fn print_info(&self, message: &str) {
        if self.colored {
            println!("{} {}", "ℹ️".blue(), message);
        } else {
            println!("ℹ️  {}", message);
        }
    }

    /// Print table format
    fn print_table<T: Tabled>(&self, items: Vec<T>) -> Result<()> {
        if items.is_empty() {
            println!("No items found");
            return Ok(());
        }

        let mut table = Table::new(items);
        table
            .with(Style::rounded())
            .with(Modify::new(Rows::first()).with(Alignment::center()));

        println!("{}", table);
        Ok(())
    }

    /// Print JSON format
    fn print_json<T: Serialize>(&self, items: &T) -> Result<()> {
        let json = serde_json::to_string_pretty(items)?;
        println!("{}", json);
        Ok(())
    }

    /// Print YAML format
    fn print_yaml<T: Serialize>(&self, items: &T) -> Result<()> {
        let yaml = serde_yaml::to_string(items)?;
        println!("{}", yaml);
        Ok(())
    }

    /// Print text format (simple, one per line)
    fn print_text<T: Tabled>(&self, items: Vec<T>) -> Result<()> {
        if items.is_empty() {
            println!("No items found");
            return Ok(());
        }

        // For text format, we'll use a simple representation
        // This is a basic implementation - can be enhanced per type
        for item in items {
            let table = Table::new(vec![item]);
            let table_str = table.to_string();
            let rows: Vec<&str> = table_str.lines().collect();

            // Skip header lines and borders, just print data
            if rows.len() > 2 {
                println!("{}", rows[2].trim());
            }
        }
        Ok(())
    }
}

/// Helper trait for types that can be displayed as key-value pairs
pub trait KeyValue {
    fn to_key_value(&self) -> Vec<(String, String)>;
}

/// Display key-value pairs
pub fn print_key_value(pairs: Vec<(String, String)>, colored: bool) {
    for (key, value) in pairs {
        if colored {
            println!("  {}: {}", key.bold(), value);
        } else {
            println!("  {}: {}", key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Tabled)]
    struct TestItem {
        name: String,
        count: u32,
    }

    #[test]
    fn test_json_format() {
        let formatter = Formatter::new(OutputFormat::Json, false);
        let items = vec![
            TestItem {
                name: "test1".to_string(),
                count: 1,
            },
            TestItem {
                name: "test2".to_string(),
                count: 2,
            },
        ];

        // Should not panic
        formatter.print_list(items).unwrap();
    }

    #[test]
    fn test_table_format() {
        let formatter = Formatter::new(OutputFormat::Table, false);
        let items = vec![TestItem {
            name: "test".to_string(),
            count: 42,
        }];

        // Should not panic
        formatter.print_list(items).unwrap();
    }

    #[test]
    fn test_yaml_format() {
        let formatter = Formatter::new(OutputFormat::Yaml, false);
        let items = vec![TestItem {
            name: "yaml-test".to_string(),
            count: 99,
        }];
        formatter.print_list(items).unwrap();
    }

    #[test]
    fn test_text_format() {
        let formatter = Formatter::new(OutputFormat::Text, false);
        let items = vec![TestItem {
            name: "text-test".to_string(),
            count: 7,
        }];
        formatter.print_list(items).unwrap();
    }

    #[test]
    fn test_empty_list_table() {
        let formatter = Formatter::new(OutputFormat::Table, false);
        let items: Vec<TestItem> = vec![];
        formatter.print_list(items).unwrap();
    }

    #[test]
    fn test_empty_list_text() {
        let formatter = Formatter::new(OutputFormat::Text, false);
        let items: Vec<TestItem> = vec![];
        formatter.print_list(items).unwrap();
    }

    #[test]
    fn test_print_single_json() {
        let formatter = Formatter::new(OutputFormat::Json, false);
        let item = TestItem {
            name: "single".to_string(),
            count: 1,
        };
        formatter.print_single(item).unwrap();
    }

    #[test]
    fn test_print_single_table() {
        let formatter = Formatter::new(OutputFormat::Table, false);
        let item = TestItem {
            name: "single".to_string(),
            count: 1,
        };
        formatter.print_single(item).unwrap();
    }

    #[test]
    fn test_formatter_colored_messages() {
        let formatter = Formatter::new(OutputFormat::Table, true);
        formatter.print_success("Operation succeeded");
        formatter.print_error("Something failed");
        formatter.print_info("Informational message");
    }

    #[test]
    fn test_formatter_uncolored_messages() {
        let formatter = Formatter::new(OutputFormat::Table, false);
        formatter.print_success("Operation succeeded");
        formatter.print_error("Something failed");
        formatter.print_info("Informational message");
    }

    #[test]
    fn test_print_key_value_colored() {
        let pairs = vec![
            ("key1".to_string(), "value1".to_string()),
            ("key2".to_string(), "value2".to_string()),
        ];
        print_key_value(pairs, true);
    }

    #[test]
    fn test_print_key_value_uncolored() {
        let pairs = vec![
            ("key1".to_string(), "value1".to_string()),
        ];
        print_key_value(pairs, false);
    }
}
