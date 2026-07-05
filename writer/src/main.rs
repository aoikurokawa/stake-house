use std::{fs, io::BufWriter, path::PathBuf, str::FromStr, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use futures::StreamExt;
use serde::Serialize;
use solana_account_decoder::{UiAccountEncoding, UiDataSliceConfig};
use solana_client::{
    nonblocking::{pubsub_client::PubsubClient, rpc_client::RpcClient},
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::{Memcmp, RpcFilterType},
};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};

const JITOSOL_MINT: &str = "J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn";
const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const JITOSOL_DECIMALS: i32 = 9;
const SPL_TOKEN_ACCOUNT_SIZE: u64 = 165;
// mint(32) + owner(32) + amount(8) — all we need from each token account
const DATA_SLICE_LEN: usize = 72;

#[derive(Parser)]
#[command(about = "Snapshot JitoSOL holders at each epoch boundary")]
struct Cli {
    /// JSON-RPC endpoint
    #[arg(
        long,
        env = "RPC_URL",
        global = true,
        default_value = "https://api.mainnet-beta.solana.com"
    )]
    rpc_url: String,

    /// Websocket endpoint (derived from --rpc-url if not set)
    #[arg(long, env = "WS_URL", global = true)]
    ws_url: Option<String>,

    /// Directory to write snapshot JSON files into
    #[arg(long, global = true, default_value = "snapshots")]
    out_dir: PathBuf,

    /// Attempts before giving up on a snapshot
    #[arg(long, global = true, default_value_t = 5)]
    snapshot_retries: u32,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Subscribe to slots and snapshot holders when the epoch turns over
    Watch,
    /// Take a snapshot of current holders immediately
    Snapshot,
}

#[derive(Serialize)]
struct Holder {
    token_account: String,
    owner: String,
    amount: u64,
    ui_amount: f64,
}

#[derive(Serialize)]
struct Snapshot {
    mint: String,
    epoch: u64,
    trigger_slot: u64,
    taken_at: String,
    num_holders: usize,
    num_zero_balance_skipped: usize,
    total_amount: u64,
    total_ui_amount: f64,
    holders: Vec<Holder>,
}

#[derive(Clone)]
struct Snapshotter {
    rpc: Arc<RpcClient>,
    out_dir: PathBuf,
    retries: u32,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let ws_url = cli
        .ws_url
        .clone()
        .unwrap_or_else(|| derive_ws_url(&cli.rpc_url));

    let snapshotter = Snapshotter {
        rpc: Arc::new(RpcClient::new_with_timeout_and_commitment(
            cli.rpc_url.clone(),
            Duration::from_secs(600),
            CommitmentConfig::confirmed(),
        )),
        out_dir: cli.out_dir,
        retries: cli.snapshot_retries,
    };

    match cli.command {
        Command::Snapshot => {
            let info = snapshotter.rpc.get_epoch_info().await?;
            println!(
                "taking immediate snapshot (epoch {}, slot {})",
                info.epoch, info.absolute_slot
            );
            snapshotter
                .take_snapshot_with_retries(info.epoch, info.absolute_slot)
                .await?;
        }
        Command::Watch => watch(snapshotter, &ws_url).await?,
    }
    Ok(())
}

fn derive_ws_url(rpc_url: &str) -> String {
    if let Some(rest) = rpc_url.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = rpc_url.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        rpc_url.to_string()
    }
}

async fn watch(snapshotter: Snapshotter, ws_url: &str) -> Result<()> {
    let rpc = &snapshotter.rpc;
    let schedule = rpc
        .get_epoch_schedule()
        .await
        .context("get_epoch_schedule")?;
    let info = rpc.get_epoch_info().await.context("get_epoch_info")?;
    let mut last_epoch = info.epoch;
    let remaining = info.slots_in_epoch.saturating_sub(info.slot_index);
    println!(
        "epoch {} — slot {}/{} — ~{} slots (~{:.1}h) until epoch {}",
        info.epoch,
        info.slot_index,
        info.slots_in_epoch,
        remaining,
        remaining as f64 * 0.4 / 3600.0,
        info.epoch + 1
    );

    loop {
        println!("connecting to {ws_url} ...");
        match PubsubClient::new(ws_url).await {
            Ok(client) => match client.slot_subscribe().await {
                Ok((mut stream, _unsub)) => {
                    println!("subscribed to slots");
                    while let Some(slot_info) = stream.next().await {
                        let slot = slot_info.slot;
                        let epoch = schedule.get_epoch(slot);
                        if slot % 1000 == 0 {
                            let (_, slot_index) = schedule.get_epoch_and_slot_index(slot);
                            println!("slot {slot} (epoch {epoch}, index {slot_index})");
                        }
                        if epoch > last_epoch {
                            println!(
                                ">>> epoch boundary crossed: {last_epoch} -> {epoch} at slot {slot}"
                            );
                            last_epoch = epoch;
                            let snapshotter = snapshotter.clone();
                            // spawn so we keep draining slot notifications while gPA runs
                            tokio::spawn(async move {
                                if let Err(e) =
                                    snapshotter.take_snapshot_with_retries(epoch, slot).await
                                {
                                    eprintln!("!!! snapshot for epoch {epoch} FAILED: {e:#}");
                                }
                            });
                        }
                    }
                    eprintln!("slot stream ended; reconnecting in 3s");
                }
                Err(e) => eprintln!("slot_subscribe failed: {e}; retrying in 3s"),
            },
            Err(e) => eprintln!("ws connect failed: {e}; retrying in 3s"),
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

impl Snapshotter {
    async fn take_snapshot_with_retries(&self, epoch: u64, trigger_slot: u64) -> Result<()> {
        let mut last_err = None;
        for attempt in 1..=self.retries {
            match self.take_snapshot(epoch, trigger_slot).await {
                Ok(path) => {
                    println!("snapshot written to {}", path.display());
                    return Ok(());
                }
                Err(e) => {
                    eprintln!("snapshot attempt {attempt}/{} failed: {e:#}", self.retries);
                    last_err = Some(e);
                    tokio::time::sleep(Duration::from_secs(15)).await;
                }
            }
        }
        Err(last_err.unwrap())
    }

    async fn take_snapshot(&self, epoch: u64, trigger_slot: u64) -> Result<PathBuf> {
        let rpc = &self.rpc;
        let mint = Pubkey::from_str(JITOSOL_MINT)?;
        let token_program = Pubkey::from_str(TOKEN_PROGRAM)?;

        let config = RpcProgramAccountsConfig {
            filters: Some(vec![
                RpcFilterType::DataSize(SPL_TOKEN_ACCOUNT_SIZE),
                RpcFilterType::Memcmp(Memcmp::new_base58_encoded(0, mint.as_ref())),
            ]),
            account_config: RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64),
                data_slice: Some(UiDataSliceConfig {
                    offset: 0,
                    length: DATA_SLICE_LEN,
                }),
                commitment: Some(CommitmentConfig::confirmed()),
                ..Default::default()
            },
            ..Default::default()
        };

        println!(
            "fetching all JitoSOL token accounts via getProgramAccounts (can take minutes)..."
        );
        let accounts = rpc
            .get_program_accounts_with_config(&token_program, config)
            .await
            .context("getProgramAccounts for JitoSOL mint")?;
        println!("fetched {} token accounts", accounts.len());

        let mut holders = Vec::with_capacity(accounts.len());
        let mut zero_skipped = 0usize;
        let mut total_amount: u64 = 0;
        for (pubkey, account) in accounts {
            let data = &account.data;
            if data.len() < DATA_SLICE_LEN {
                eprintln!("skipping {pubkey}: short data ({} bytes)", data.len());
                continue;
            }
            let owner = Pubkey::try_from(&data[32..64]).expect("32 bytes");
            let amount = u64::from_le_bytes(data[64..72].try_into().expect("8 bytes"));
            if amount == 0 {
                zero_skipped += 1;
                continue;
            }
            total_amount += amount;
            holders.push(Holder {
                token_account: pubkey.to_string(),
                owner: owner.to_string(),
                amount,
                ui_amount: amount as f64 / 10f64.powi(JITOSOL_DECIMALS),
            });
        }
        holders.sort_by(|a, b| b.amount.cmp(&a.amount));

        let snapshot = Snapshot {
            mint: JITOSOL_MINT.to_string(),
            epoch,
            trigger_slot,
            taken_at: chrono::Utc::now().to_rfc3339(),
            num_holders: holders.len(),
            num_zero_balance_skipped: zero_skipped,
            total_amount,
            total_ui_amount: total_amount as f64 / 10f64.powi(JITOSOL_DECIMALS),
            holders,
        };

        fs::create_dir_all(&self.out_dir)?;
        let path = self
            .out_dir
            .join(format!("jitosol_holders_epoch_{epoch}.json"));
        let file = fs::File::create(&path)?;
        serde_json::to_writer(BufWriter::new(file), &snapshot)?;
        println!(
            "epoch {}: {} holders, {:.2} JitoSOL total ({} zero-balance accounts skipped)",
            epoch, snapshot.num_holders, snapshot.total_ui_amount, zero_skipped
        );
        Ok(path)
    }
}
