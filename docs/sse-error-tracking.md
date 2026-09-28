# SSE failure tracking

An HTTP 200 response can contain a failed Responses request. The proxy inspects
SSE alongside raw byte forwarding using `sse-core::SseDecoder`. The decoder handles
chunk boundaries, UTF-8, BOM, LF/CRLF/CR, comments, and multiline `data:` fields.
The original status, headers, and response body continue to the client.

The observer recognizes `response.failed` from the SSE event name or JSON `type`,
and also records a non-null `response.error` object on other response events:

| Upstream field | SQLite column on `api_calls` |
| --- | --- |
| `response.error.code` | `error_code` (nullable text, exact upstream string) |
| `response.error.message` | `error` (exact upstream message) |
| HTTP status | `status` (200 remains 200 for a normally ended failed SSE response) |

A failed event without an error message records `response.failed` as its reason.
Codes are open-ended: every string is accepted, including previously unseen codes.
`server_is_overloaded` has been observed in production with the message
`Our servers are currently overloaded. Please try again later.`

## Collected code values

The published [OpenAI JavaScript SDK 7.15.0 source archive](https://registry.npmjs.org/openai/-/openai-7.15.0.tgz),
`resources/responses/responses.d.ts`, lists these `ResponseError.code` values
(checked 2026-09-14):

| Group | Codes |
| --- | --- |
| Server and capacity | `server_error`, `rate_limit_exceeded`, `vector_store_timeout` |
| Request and policy | `invalid_prompt`, `data_residency_mismatch`, `bio_policy`, `misalignment_policy_violation` |
| Image input | `invalid_image`, `invalid_image_format`, `invalid_base64_image`, `invalid_image_url`, `image_too_large`, `image_too_small`, `image_parse_error`, `image_content_policy_violation`, `invalid_image_mode`, `image_file_too_large`, `unsupported_image_media_type`, `empty_image_file`, `failed_to_download_image`, `image_file_not_found` |

Production also emitted `server_is_overloaded`, which this SDK union does not list.
The table is a reference for investigation; database ingestion does not use it as
an allowlist. Official OpenAI documentation pages returned HTTP 403 during this
check, so the catalog above is sourced from the published SDK and production events.

## Persistence and queries

Basic error metadata is queued for persistence when the stream finishes or its
consumer disconnects. It is independent of optional body archives and their
preview limits. A captured upstream failure takes precedence over a later
transport error or client cancellation. Usage already seen on the stream is kept.
The same inspection is applied to buffered SSE and the image response path.

Audit success means `status < 400` and both error columns are null. The one normal
disconnect exception is `status = 499`, `error = client_cancelled`, and a null
`error_code`; this is treated as a successful request for audit filters, dashboard
error counts, and the console. A 499 with any upstream `error_code` remains an
error so captured `response.failed` events are still tracked. The audit list and
detail APIs return `error_code`; `/api/audit?status=error&error_code=server_is_overloaded`
filters by the exact code, with existing tenant visibility rules.

To collect codes over a time window:

```sql
SELECT error_code, COUNT(*) AS requests, MAX(created_at) AS last_seen
FROM api_calls
WHERE error_code IS NOT NULL AND created_at >= unixepoch() - 86400
GROUP BY error_code
ORDER BY requests DESC;
```

Inspection limits each SSE data/name/id buffer to 8 MiB. If the limit is exceeded,
the library discards that event and resumes at the next event boundary. The audit
records an inspection failure so uninspected data is not counted as a success;
raw forwarding continues, and a later parsed upstream error supplies its code and
message. `[DONE]` and non-JSON events do not reset a captured failure.

This migration adds metadata for newly observed requests. Older rows retain null
codes; historical body archives may be incomplete or have expired. A stream that
has started forwarding is not retried in response to a failed SSE event.
