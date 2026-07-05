CREATE TABLE IF NOT EXISTS lst (
    id BIGSERIAL PRIMARY KEY,
    mint TEXT NOT NULL UNIQUE,
    symbol TEXT NOT NULL,
    decimals SMALLINT NOT NULL
);

CREATE TABLE IF NOT EXISTS lst_snapshots (
    id BIGSERIAL PRIMARY KEY,
    lst_id BIGINT NOT NULL REFERENCES lst(id),
    epoch BIGINT NOT NULL,
    trigger_slot BIGINT NOT NULL,
    taken_at TIMESTAMPTZ NOT NULL,
    num_zero_balance_skipped BIGINT NOT NULL,
    total_amount BIGINT NOT NULL,
    UNIQUE (lst_id, epoch)
);

CREATE TABLE IF NOT EXISTS lst_holders (
    snapshot_id BIGINT NOT NULL REFERENCES lst_snapshots(id) ON DELETE CASCADE,
    token_account TEXT NOT NULL,
    owner TEXT NOT NULL,
    amount BIGINT NOT NULL,
    PRIMARY KEY (snapshot_id, token_account)
);

CREATE INDEX IF NOT EXISTS lst_holders_owner_idx ON lst_holders (owner);

INSERT INTO lst (mint, symbol, decimals)
VALUES ('J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn', 'jitosol', 9)
ON CONFLICT (mint) DO NOTHING;
