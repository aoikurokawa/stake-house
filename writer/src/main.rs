use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use futures::StreamExt;
use solana_client::nonblocking::pubsub_client::PubsubClient;
use stake_house_writer::{
    cli::{Cli, Command},
    snapshotter::Snapshotter,
};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let ws_url = cli.ws_url();

    let snapshotter = Snapshotter::new(&cli).await?;

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
