use serde::Serialize;

#[derive(Serialize)]
pub struct Holder {
    /// Token account
    pub token_account: String,

    /// Owner
    pub owner: String,

    /// Amount
    pub amount: u64,

    /// UI Amount
    pub ui_amount: f64,
}
