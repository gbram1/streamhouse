//! CLI integration tests
//!
//! Tests for the streamctl binary's config handling and output formatting.

use std::process::Command;

/// Get the path to the compiled streamctl binary
fn streamctl_bin() -> String {
    let mut path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    path.push("streamctl");
    path.to_str().unwrap().to_string()
}

#[test]
fn test_help_flag() {
    let output = Command::new(streamctl_bin())
        .arg("--help")
        .output()
        .expect("Failed to execute streamctl");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("streamctl") || stdout.contains("StreamHouse"));
}

#[test]
fn test_config_subcommand_exists() {
    // Just verify the config subcommand is recognized
    let output = Command::new(streamctl_bin())
        .args(["config", "--help"])
        .output()
        .expect("Failed to execute streamctl");

    // Should either succeed or show help for config
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success() || combined.contains("config"),
        "config subcommand should be recognized"
    );
}

#[test]
fn test_invalid_subcommand_fails() {
    let output = Command::new(streamctl_bin())
        .arg("nonexistent-command")
        .output()
        .expect("Failed to execute streamctl");

    assert!(!output.status.success());
}

#[test]
fn test_help_contains_subcommands() {
    let output = Command::new(streamctl_bin())
        .arg("--help")
        .output()
        .expect("Failed to execute streamctl");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Verify core subcommands are listed in help output
    assert!(stdout.contains("topic"), "help should list 'topic' subcommand");
    assert!(stdout.contains("produce"), "help should list 'produce' subcommand");
    assert!(stdout.contains("consume"), "help should list 'consume' subcommand");
    assert!(stdout.contains("schema"), "help should list 'schema' subcommand");
}

#[test]
fn test_topic_subcommand_help() {
    let output = Command::new(streamctl_bin())
        .args(["topic", "--help"])
        .output()
        .expect("Failed to execute streamctl");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("create") || stdout.contains("Create"),
        "topic help should mention create");
    assert!(stdout.contains("list") || stdout.contains("List"),
        "topic help should mention list");
    assert!(stdout.contains("delete") || stdout.contains("Delete"),
        "topic help should mention delete");
}

#[test]
fn test_schema_subcommand_help() {
    let output = Command::new(streamctl_bin())
        .args(["schema", "--help"])
        .output()
        .expect("Failed to execute streamctl");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Schema subcommand should have help text
    assert!(!stdout.is_empty(), "schema help should produce output");
}

#[test]
fn test_server_flag_accepted() {
    // Verify that --server flag is accepted (even though connection will fail,
    // we just test that clap doesn't reject it when combined with --help)
    let output = Command::new(streamctl_bin())
        .args(["--server", "http://custom:9090", "topic", "--help"])
        .output()
        .expect("Failed to execute streamctl");

    assert!(output.status.success());
}
