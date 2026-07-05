use std::{fs, io::BufWriter, path::PathBuf, str::FromStr, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use solana_account_decoder::{UiAccountEncoding, UiDataSliceConfig};
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::{Memcmp, RpcFilterType},
};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    Pool, Postgres,
};

use crate::{
    cli::Cli, holder::Holder, snapshot::Snapshot, DATA_SLICE_LEN, JITOSOL_DECIMALS, JITOSOL_MINT,
    SPL_TOKEN_ACCOUNT_SIZE, TOKEN_PROGRAM,
};

#[derive(Clone)]
pub struct Snapshotter {
    /// RPC
    pub rpc: Arc<RpcClient>,

    /// Pool
    pub pool: Pool<Postgres>,

    /// Out dir
    pub out_dir: PathBuf,

    /// Retries
    pub retries: u32,
}

impl Snapshotter {
    pub async fn new(cli: &Cli) -> anyhow::Result<Self> {
        let rpc = Arc::new(RpcClient::new_with_timeout_and_commitment(
            cli.rpc_url.clone(),
            Duration::from_secs(600),
            CommitmentConfig::confirmed(),
        ));

        let opts = PgConnectOptions::new()
            .host(&cli.db_host)
            .port(cli.db_port)
            .username(&cli.db_user)
            .password(&cli.db_password)
            .database(&cli.db_name);

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await?;

        sqlx::migrate!()
            .run(&pool)
            .await
            .context("run database migrations")?;

        Ok(Self {
            rpc,
            pool,
            out_dir: cli.out_dir.clone(),
            retries: cli.snapshot_retries,
        })
    }

    pub async fn take_snapshot_with_retries(&self, epoch: u64, trigger_slot: u64) -> Result<()> {
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

        let taken_at = Utc::now();
        let snapshot = Snapshot {
            mint: JITOSOL_MINT.to_string(),
            epoch,
            trigger_slot,
            taken_at: taken_at.to_rfc3339(),
            num_holders: holders.len(),
            num_zero_balance_skipped: zero_skipped,
            total_amount,
            total_ui_amount: total_amount as f64 / 10f64.powi(JITOSOL_DECIMALS),
            holders,
        };

        self.persist_snapshot(&snapshot, taken_at)
            .await
            .context("persist snapshot to database")?;
        println!(
            "epoch {}: wrote {} holder rows to database",
            epoch, snapshot.num_holders
        );

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

    async fn persist_snapshot(&self, snapshot: &Snapshot, taken_at: DateTime<Utc>) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        let lst_id: i64 = sqlx::query_scalar("SELECT id FROM lst WHERE mint = $1")
            .bind(&snapshot.mint)
            .fetch_one(&mut *tx)
            .await
            .context("look up lst row for mint")?;

        let snapshot_id: i64 = sqlx::query_scalar(
            "INSERT INTO lst_snapshots \
                 (lst_id, epoch, trigger_slot, taken_at, num_zero_balance_skipped, total_amount) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (lst_id, epoch) DO UPDATE SET \
                 trigger_slot = EXCLUDED.trigger_slot, \
                 taken_at = EXCLUDED.taken_at, \
                 num_zero_balance_skipped = EXCLUDED.num_zero_balance_skipped, \
                 total_amount = EXCLUDED.total_amount \
             RETURNING id",
        )
        .bind(lst_id)
        .bind(snapshot.epoch as i64)
        .bind(snapshot.trigger_slot as i64)
        .bind(taken_at)
        .bind(snapshot.num_zero_balance_skipped as i64)
        .bind(snapshot.total_amount as i64)
        .fetch_one(&mut *tx)
        .await?;

        // a re-run for the same epoch replaces the holder set instead of accumulating
        sqlx::query("DELETE FROM lst_holders WHERE snapshot_id = $1")
            .bind(snapshot_id)
            .execute(&mut *tx)
            .await?;

        let mut token_accounts = Vec::with_capacity(snapshot.holders.len());
        let mut owners = Vec::with_capacity(snapshot.holders.len());
        let mut amounts = Vec::with_capacity(snapshot.holders.len());
        for holder in &snapshot.holders {
            token_accounts.push(holder.token_account.as_str());
            owners.push(holder.owner.as_str());
            amounts.push(holder.amount as i64);
        }

        sqlx::query(
            "INSERT INTO lst_holders (snapshot_id, token_account, owner, amount) \
             SELECT $1, t.* FROM UNNEST($2::text[], $3::text[], $4::bigint[]) AS t",
        )
        .bind(snapshot_id)
        .bind(&token_accounts)
        .bind(&owners)
        .bind(&amounts)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }
}
