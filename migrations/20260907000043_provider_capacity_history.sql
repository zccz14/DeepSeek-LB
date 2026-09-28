CREATE TABLE provider_capacity_history (
    sampled_at INTEGER PRIMARY KEY,
    plus_equivalent_remaining_basis_points INTEGER NOT NULL,
    included_provider_count INTEGER NOT NULL,
    provider_count INTEGER NOT NULL
);
