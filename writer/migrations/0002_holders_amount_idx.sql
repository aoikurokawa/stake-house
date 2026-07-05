-- serve "top holders" queries (ORDER BY amount DESC per snapshot) from an index
CREATE INDEX IF NOT EXISTS lst_holders_snapshot_amount_idx
ON lst_holders (snapshot_id, amount DESC);
