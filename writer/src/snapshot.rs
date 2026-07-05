use serde::Serialize;

use crate::holder::Holder;

#[derive(Serialize)]
pub struct Snapshot {
    pub mint: String,
    pub epoch: u64,
    pub trigger_slot: u64,
    pub taken_at: String,
    pub num_holders: usize,
    pub num_zero_balance_skipped: usize,
    pub total_amount: u64,
    pub total_ui_amount: f64,
    pub holders: Vec<Holder>,
}
