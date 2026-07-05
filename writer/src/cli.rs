use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(about = "Snapshot JitoSOL holders at each epoch boundary")]
pub struct Cli {
    /// JSON-RPC endpoint
    #[arg(
        long,
        env = "RPC_URL",
        global = true,
        default_value = "https://api.mainnet-beta.solana.com"
    )]
    pub rpc_url: String,

    /// Websocket endpoint (derived from --rpc-url if not set)
    #[arg(long, env = "WS_URL", global = true)]
    pub ws_url: Option<String>,

    /// Directory to write snapshot JSON files into
    #[arg(long, global = true, default_value = "snapshots")]
    pub out_dir: PathBuf,

    /// Attempts before giving up on a snapshot
    #[arg(long, global = true, default_value_t = 5)]
    pub snapshot_retries: u32,

    /// Postgres host
    #[arg(long, env = "DB_HOST", global = true)]
    pub db_host: String,

    /// Postgres port
    #[arg(long, env = "DB_PORT", global = true)]
    pub db_port: u16,

    /// Postgres user
    #[arg(long, env = "DB_USER", global = true)]
    pub db_user: String,

    /// Postgres password
    #[arg(long, env = "DB_PASSWORD", global = true)]
    pub db_password: String,

    /// Postgres database name
    #[arg(long, env = "DB_NAME", global = true)]
    pub db_name: String,

    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    pub fn ws_url(&self) -> String {
        if let Some(rest) = self.rpc_url.strip_prefix("https://") {
            format!("wss://{rest}")
        } else if let Some(rest) = self.rpc_url.strip_prefix("http://") {
            format!("ws://{rest}")
        } else {
            self.rpc_url.to_string()
        }
    }
}

#[derive(Subcommand)]
pub enum Command {
    /// Subscribe to slots and snapshot holders when the epoch turns over
    Watch,
    /// Take a snapshot of current holders immediately
    Snapshot,
}
