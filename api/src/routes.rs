use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use sqlx::{Pool, Postgres};

use crate::{
    error::ApiError,
    models::{HolderRow, Lst, OwnerBalance, Pagination, SnapshotMeta},
};

pub fn router(pool: Pool<Postgres>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/lsts", get(list_lsts))
        .route("/lsts/{mint}/snapshots", get(list_snapshots))
        .route("/lsts/{mint}/snapshots/{epoch}/holders", get(list_holders))
        .route("/lsts/{mint}/owners/{owner}", get(owner_history))
        .with_state(pool)
}

async fn health() -> &'static str {
    "ok"
}

async fn list_lsts(State(pool): State<Pool<Postgres>>) -> Result<Json<Vec<Lst>>, ApiError> {
    let lsts = sqlx::query_as("SELECT mint, symbol, decimals FROM lst ORDER BY symbol")
        .fetch_all(&pool)
        .await?;
    Ok(Json(lsts))
}

async fn list_snapshots(
    State(pool): State<Pool<Postgres>>,
    Path(mint): Path<String>,
    Query(page): Query<Pagination>,
) -> Result<Json<Vec<SnapshotMeta>>, ApiError> {
    let lst_id = lst_id_for_mint(&pool, &mint).await?;

    let snapshots = sqlx::query_as(
        "SELECT s.epoch, s.trigger_slot, s.taken_at, \
                (SELECT COUNT(*) FROM lst_holders h WHERE h.snapshot_id = s.id) AS num_holders, \
                s.num_zero_balance_skipped, s.total_amount, \
                s.total_amount::float8 / power(10::float8, l.decimals::float8) AS total_ui_amount \
         FROM lst_snapshots s \
         JOIN lst l ON l.id = s.lst_id \
         WHERE s.lst_id = $1 \
         ORDER BY s.epoch DESC \
         LIMIT $2 OFFSET $3",
    )
    .bind(lst_id)
    .bind(page.limit())
    .bind(page.offset())
    .fetch_all(&pool)
    .await?;
    Ok(Json(snapshots))
}

async fn list_holders(
    State(pool): State<Pool<Postgres>>,
    Path((mint, epoch)): Path<(String, i64)>,
    Query(page): Query<Pagination>,
) -> Result<Json<Vec<HolderRow>>, ApiError> {
    let lst_id = lst_id_for_mint(&pool, &mint).await?;

    let snapshot_id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM lst_snapshots WHERE lst_id = $1 AND epoch = $2")
            .bind(lst_id)
            .bind(epoch)
            .fetch_optional(&pool)
            .await?;
    let snapshot_id = snapshot_id
        .ok_or_else(|| ApiError::NotFound(format!("no snapshot for epoch {epoch}")))?;

    let holders = sqlx::query_as(
        "SELECT h.token_account, h.owner, h.amount, \
                h.amount::float8 / power(10::float8, l.decimals::float8) AS ui_amount \
         FROM lst_holders h \
         JOIN lst l ON l.id = $2 \
         WHERE h.snapshot_id = $1 \
         ORDER BY h.amount DESC, h.token_account \
         LIMIT $3 OFFSET $4",
    )
    .bind(snapshot_id)
    .bind(lst_id)
    .bind(page.limit())
    .bind(page.offset())
    .fetch_all(&pool)
    .await?;
    Ok(Json(holders))
}

async fn owner_history(
    State(pool): State<Pool<Postgres>>,
    Path((mint, owner)): Path<(String, String)>,
) -> Result<Json<Vec<OwnerBalance>>, ApiError> {
    let lst_id = lst_id_for_mint(&pool, &mint).await?;

    let history = sqlx::query_as(
        "SELECT s.epoch, s.taken_at, SUM(h.amount)::bigint AS amount, \
                SUM(h.amount)::float8 / power(10::float8, l.decimals::float8) AS ui_amount \
         FROM lst_holders h \
         JOIN lst_snapshots s ON s.id = h.snapshot_id \
         JOIN lst l ON l.id = s.lst_id \
         WHERE s.lst_id = $1 AND h.owner = $2 \
         GROUP BY s.epoch, s.taken_at, l.decimals \
         ORDER BY s.epoch",
    )
    .bind(lst_id)
    .bind(&owner)
    .fetch_all(&pool)
    .await?;
    Ok(Json(history))
}

async fn lst_id_for_mint(pool: &Pool<Postgres>, mint: &str) -> Result<i64, ApiError> {
    let id: Option<i64> = sqlx::query_scalar("SELECT id FROM lst WHERE mint = $1")
        .bind(mint)
        .fetch_optional(pool)
        .await?;
    id.ok_or_else(|| ApiError::NotFound(format!("unknown mint {mint}")))
}
