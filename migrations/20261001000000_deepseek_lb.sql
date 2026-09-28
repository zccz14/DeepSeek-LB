-- DeepSeek-LB consolidated schema.
--
-- DeepSeek-LB starts from a clean slate: there is no deployed predecessor
-- database to upgrade, so the migration history is a single reviewed snapshot
-- instead of the long compatibility chain inherited from OpenAI-LB.

CREATE TABLE app_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

INSERT INTO app_meta(key,value,updated_at) VALUES
    ('setup_complete','false',unixepoch()),
    ('auth_issuer','',unixepoch()),
    ('auth_audience','',unixepoch()),
    ('upstream_base','https://api.deepseek.com',unixepoch()),
    ('available_model_ids','["deepseek-flash","deepseek-v4-pro"]',unixepoch()),
    ('allow_all_users_debt','false',unixepoch()),
    ('response_body_limit','2097152',unixepoch()),
    ('affinity_ttl_seconds','86400',unixepoch()),
    ('provider_concurrency_limit','3',unixepoch()),
    ('request_archive_retention_days','7',unixepoch()),
    ('model_price_multiplier_nanos','1000000000',unixepoch()),
    ('midas_api_base','https://midas.ntnl.io/api',unixepoch()),
    ('midas_fund_user_id','',unixepoch()),
    ('midas_fund_api_key','',unixepoch());

CREATE TABLE users (
    id TEXT PRIMARY KEY,
    role TEXT NOT NULL CHECK(role IN ('root','admin','user')),
    created_at INTEGER NOT NULL,
    official_consumed_usd_nanos INTEGER NOT NULL DEFAULT 0 CHECK(official_consumed_usd_nanos >= 0),
    consumed_usd_nanos INTEGER NOT NULL DEFAULT 0 CHECK(consumed_usd_nanos >= 0),
    allow_debt INTEGER NOT NULL DEFAULT 0 CHECK(allow_debt IN (0,1)),
    provided_usd_nanos INTEGER NOT NULL DEFAULT 0 CHECK(provided_usd_nanos >= 0)
);
CREATE UNIQUE INDEX users_single_root ON users(role) WHERE role='root';

CREATE TABLE consumers (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    prefix TEXT NOT NULL,
    secret_hash TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER,
    request_archive INTEGER NOT NULL DEFAULT 0 CHECK(request_archive IN (0,1)),
    is_system INTEGER NOT NULL DEFAULT 0 CHECK(is_system IN (0,1)),
    is_deleted INTEGER NOT NULL DEFAULT 0 CHECK(is_deleted IN (0,1)),
    is_disabled INTEGER NOT NULL DEFAULT 0 CHECK(is_disabled IN (0,1))
);
CREATE INDEX consumers_user_idx ON consumers(user_id, created_at DESC);
CREATE INDEX consumers_visible_user_idx
ON consumers(user_id,is_system,is_deleted,created_at DESC);

-- One DeepSeek provider is one API key: `name` plus `api_key`, nothing else.
CREATE TABLE providers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    api_key TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    manual_disabled INTEGER NOT NULL DEFAULT 0,
    cooldown_until INTEGER,
    rate_limit_json TEXT,
    last_error TEXT,
    last_used_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    owner_id TEXT REFERENCES users(id) ON DELETE CASCADE,
    visibility TEXT NOT NULL DEFAULT 'private' CHECK(visibility IN ('public','private')),
    official_provided_usd_nanos INTEGER NOT NULL DEFAULT 0 CHECK(official_provided_usd_nanos >= 0),
    actual_provided_usd_nanos INTEGER NOT NULL DEFAULT 0 CHECK(actual_provided_usd_nanos >= 0),
    is_deleted INTEGER NOT NULL DEFAULT 0 CHECK(is_deleted IN (0,1))
);
CREATE INDEX providers_available_idx ON providers(manual_disabled, status, cooldown_until);
CREATE INDEX providers_owner_idx ON providers(owner_id, created_at DESC);
CREATE INDEX providers_visible_created_idx
ON providers(is_deleted,created_at DESC,id);
CREATE INDEX providers_visible_owner_idx
ON providers(owner_id,is_deleted,created_at DESC);

CREATE TABLE affinities (
    affinity_hash TEXT PRIMARY KEY,
    provider_id TEXT NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    expires_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX affinities_expiry_idx ON affinities(expires_at);

CREATE TABLE api_calls (
    id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL,
    thread_id TEXT,
    consumer_id TEXT NOT NULL REFERENCES consumers(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id),
    provider_id TEXT REFERENCES providers(id) ON DELETE SET NULL,
    affinity_hash TEXT,
    affinity_source TEXT,
    method TEXT NOT NULL,
    path TEXT NOT NULL,
    model TEXT,
    reasoning_effort TEXT,
    -- 1 when the request started inside DeepSeek peak hours, 0 for off-peak.
    peak INTEGER NOT NULL DEFAULT 0 CHECK(peak IN (0,1)),
    status INTEGER NOT NULL,
    first_byte_latency_ms INTEGER,
    latency_ms INTEGER NOT NULL,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cached_tokens INTEGER NOT NULL DEFAULT 0,
    request_bytes INTEGER NOT NULL DEFAULT 0,
    response_bytes INTEGER NOT NULL DEFAULT 0,
    request_transport_bytes INTEGER NOT NULL DEFAULT 0,
    response_transport_bytes INTEGER NOT NULL DEFAULT 0,
    downstream_accept_encoding TEXT,
    downstream_content_encoding TEXT,
    upstream_accept_encoding TEXT,
    upstream_content_encoding TEXT,
    upstream_http_version TEXT,
    official_cost_usd_nanos INTEGER NOT NULL DEFAULT 0 CHECK(official_cost_usd_nanos >= 0),
    actual_cost_usd_nanos INTEGER NOT NULL DEFAULT 0 CHECK(actual_cost_usd_nanos >= 0),
    price_multiplier_nanos INTEGER NOT NULL DEFAULT 1000000000 CHECK(price_multiplier_nanos > 0),
    official_consumed_usd_before_nanos INTEGER NOT NULL DEFAULT 0 CHECK(official_consumed_usd_before_nanos >= 0),
    official_consumed_usd_after_nanos INTEGER NOT NULL DEFAULT 0 CHECK(official_consumed_usd_after_nanos >= official_consumed_usd_before_nanos),
    actual_consumed_usd_before_nanos INTEGER NOT NULL DEFAULT 0 CHECK(actual_consumed_usd_before_nanos >= 0),
    actual_consumed_usd_after_nanos INTEGER NOT NULL DEFAULT 0 CHECK(actual_consumed_usd_after_nanos >= actual_consumed_usd_before_nanos),
    official_provided_usd_before_nanos INTEGER NOT NULL DEFAULT 0 CHECK(official_provided_usd_before_nanos >= 0),
    official_provided_usd_after_nanos INTEGER NOT NULL DEFAULT 0 CHECK(official_provided_usd_after_nanos >= official_provided_usd_before_nanos),
    actual_provided_usd_before_nanos INTEGER NOT NULL DEFAULT 0 CHECK(actual_provided_usd_before_nanos >= 0),
    actual_provided_usd_after_nanos INTEGER NOT NULL DEFAULT 0 CHECK(actual_provided_usd_after_nanos >= actual_provided_usd_before_nanos),
    error TEXT,
    error_code TEXT,
    client_ip TEXT,
    created_at INTEGER NOT NULL
);
CREATE INDEX api_calls_consumer_time_idx ON api_calls(consumer_id, created_at DESC);
CREATE INDEX api_calls_user_time_idx ON api_calls(user_id, created_at DESC);
CREATE INDEX api_calls_affinity_time_idx ON api_calls(affinity_hash,created_at DESC);
CREATE INDEX api_calls_created_at_idx ON api_calls(created_at DESC);
CREATE INDEX api_calls_thread_time_idx ON api_calls(thread_id, created_at DESC);
CREATE INDEX api_calls_error_code_time_idx ON api_calls(error_code, created_at DESC)
WHERE error_code IS NOT NULL;

CREATE TABLE request_archives (
    api_call_id TEXT PRIMARY KEY REFERENCES api_calls(id) ON DELETE CASCADE,
    request_headers_json TEXT NOT NULL,
    upstream_request_headers_json TEXT,
    request_body BLOB NOT NULL,
    request_body_truncated INTEGER NOT NULL CHECK(request_body_truncated IN (0,1)),
    response_headers_json TEXT,
    downstream_response_headers_json TEXT,
    response_body BLOB,
    response_body_truncated INTEGER NOT NULL CHECK(response_body_truncated IN (0,1)),
    bodies_deleted INTEGER NOT NULL DEFAULT 0 CHECK(bodies_deleted IN (0,1)),
    created_at INTEGER NOT NULL
);
CREATE INDEX request_archives_created_at_idx ON request_archives(created_at);

CREATE TABLE admin_audit (
    id TEXT PRIMARY KEY,
    admin_user_id TEXT NOT NULL REFERENCES users(id),
    action TEXT NOT NULL,
    target_id TEXT,
    client_ip TEXT,
    created_at INTEGER NOT NULL
);
CREATE INDEX admin_audit_time_idx ON admin_audit(created_at DESC);
