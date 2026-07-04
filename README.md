# Stake House

Snapshot all JitoSOL holders at each epoch boundary.

`stake-house-writer` subscribes to Solana slots over websocket, detects when the
epoch turns over, and dumps every JitoSOL token account with a non-zero balance
to a JSON file via `getProgramAccounts`.

## Usage

Watch for the next epoch boundary and snapshot when it happens:

```bash
cargo run --release -- watch --rpc-url 
```

Take a snapshot of current holders immediately (useful for testing):

```bash
cargo run --release -- snapshot
```

### Options

| Flag | Env | Default | Description |
|------|-----|---------|-------------|
| `--rpc-url` | `RPC_URL` | `https://api.mainnet-beta.solana.com` | JSON-RPC endpoint |
| `--ws-url` | `WS_URL` | derived from `--rpc-url` | Websocket endpoint |
| `--out-dir` | | `snapshots` | Directory for snapshot JSON files |
| `--snapshot-retries` | | `5` | Attempts before giving up on a snapshot |

The RPC endpoint must allow `getProgramAccounts` against the SPL Token program
(the public mainnet endpoint currently does).

## Output

Snapshots are written to `<out-dir>/jitosol_holders_epoch_<epoch>.json`
(~36MB, ~186k holders as of epoch 996):

```json
{
  "mint": "J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn",
  "epoch": 997,
  "trigger_slot": 430704000,
  "taken_at": "2026-07-04T09:00:00+00:00",
  "num_holders": 186058,
  "num_zero_balance_skipped": 363114,
  "total_amount": 7759707752980159,
  "total_ui_amount": 7759707.75,
  "holders": [
    {
      "token_account": "6sga1yRArgQRqa8Darhm54EBromEpV3z8iDAvMTVYXB3",
      "owner": "9DrvZvyWh1HuAoZxvYWMvkf2XCzryCpGgHqrMjyDWpmo",
      "amount": 695455348478765,
      "ui_amount": 695455.348478765
    }
  ]
}
```

Holders are sorted by balance, descending. `amount` is in raw units (JitoSOL
has 9 decimals); zero-balance token accounts are counted but excluded.

## How it works

- `watch` subscribes via `slotSubscribe` and maps each slot to an epoch using
  the on-chain epoch schedule. The boundary check is `epoch > last_epoch`, so
  skipped slots or a websocket reconnect across the boundary won't miss it.
  The websocket reconnects automatically if it drops.
- On a boundary, the snapshot runs in a spawned task (with retries) while the
  watcher keeps consuming slot notifications.
- The holder fetch is a `getProgramAccounts` call filtered by
  `dataSize = 165` and `memcmp(offset 0, mint)`, with a data slice so only
  mint/owner/amount are transferred.

## Caveats

- The snapshot is **approximately** at the boundary: the `getProgramAccounts`
  scan takes seconds to minutes, during which balances keep moving. For a
  slot-exact snapshot you'd need a Geyser-fed index.
- `owner` is the token-account owner. JitoSOL held inside DeFi protocols
  (Kamino, Orca, etc.) appears under the protocol's account, not the end user.
- The stake pool's exchange-rate update (`UpdateStakePoolBalance`) lands
  shortly *after* the boundary, so a snapshot at slot 0 still reflects the
  previous epoch's JitoSOL/SOL rate.
