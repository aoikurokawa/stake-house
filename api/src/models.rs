use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Serialize, FromRow)]
pub struct Lst {
    pub mint: String,
    pub symbol: String,
    pub decimals: i16,
}

#[derive(Serialize, FromRow)]
pub struct SnapshotMeta {
    pub epoch: i64,
    pub trigger_slot: i64,
    pub taken_at: DateTime<Utc>,
    pub num_holders: i64,
    pub num_zero_balance_skipped: i64,
    pub total_amount: i64,
    pub total_ui_amount: f64,
}

#[derive(Serialize, FromRow)]
pub struct HolderRow {
    pub token_account: String,
    pub owner: String,
    pub amount: i64,
    pub ui_amount: f64,
}

#[derive(Serialize, FromRow)]
pub struct OwnerBalance {
    pub epoch: i64,
    pub taken_at: DateTime<Utc>,
    pub amount: i64,
    pub ui_amount: f64,
}

#[derive(Deserialize)]
pub struct Pagination {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl Pagination {
    pub fn limit(&self) -> i64 {
        self.limit.unwrap_or(100).clamp(1, 1000)
    }

    pub fn offset(&self) -> i64 {
        self.offset.unwrap_or(0).max(0)
    }
}
