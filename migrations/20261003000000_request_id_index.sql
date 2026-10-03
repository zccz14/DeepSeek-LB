-- The cost query API resolves records by the unified request ID: the value of
-- the inbound `x-normai-request-id` header when a gateway supplies one,
-- otherwise a generated UUID. Duplicate rows for one ID are legitimate (a
-- transport-level re-send can arrive twice), and audit writes must never fail
-- on a collision, so the index is not unique.
CREATE INDEX api_calls_request_id_idx ON api_calls(request_id);
