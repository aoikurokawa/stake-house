pub mod cli;
pub mod holder;
pub mod snapshot;
pub mod snapshotter;

pub const JITOSOL_MINT: &str = "J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn";
pub const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const JITOSOL_DECIMALS: i32 = 9;
pub const SPL_TOKEN_ACCOUNT_SIZE: u64 = 165;
// mint(32) + owner(32) + amount(8) — all we need from each token account
pub const DATA_SLICE_LEN: usize = 72;
