//! Auth CLI commands
//!
//! Provides subcommands for authentication management:
//! login, logout, whoami, status, switch, and instances.

use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum AuthCommands {
    /// Login to a StreamHouse server
    Login {
        /// Server URL to authenticate against
        #[arg(long)]
        server: Option<String>,
        /// Instance name (derived from URL if not provided)
        #[arg(long)]
        name: Option<String>,
    },
    /// Logout from the current instance
    Logout,
    /// Show the current authenticated user
    Whoami,
    /// Show authentication status
    Status,
    /// Switch to a different instance
    Switch {
        /// Instance name to switch to
        name: String,
    },
    /// List all known instances
    Instances,
}
