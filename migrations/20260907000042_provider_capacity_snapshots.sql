CREATE TABLE provider_capacity_snapshots (
    provider_id TEXT PRIMARY KEY REFERENCES providers(id) ON DELETE CASCADE,
    plan_type TEXT,
    remaining_basis_points INTEGER CHECK(remaining_basis_points BETWEEN 0 AND 10000),
    last_success_at INTEGER,
    last_attempt_at INTEGER NOT NULL,
    last_error TEXT
);
