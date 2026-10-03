# Request cost queries

Downstream services can look up the billed amount of one inference request, or
of a batch of them, after the fact. The query key is the
`x-deepseek-lb-request-id` response header that every inference response
carries, so a caller records that header while proxying or consuming responses
and resolves costs later — including requests it cancelled, lost, or never
finished reading.

Pricing stays inside each load balancer (peak/off-peak windows, price
multipliers, model tables); clients consume final amounts only, and never need
to model per-upstream pricing rules. Other load balancers in this ecosystem
(OpenAI-LB) are expected to expose the same contract, so one client
implementation covers every upstream.

## Single record

```http
GET /v1/requests/{id}
Authorization: Bearer <CONSUMER_KEY>
```

```json
{
  "request_id": "0f10ab1c-8d2f-4c0a-9b1e-3f5a7c9d1e2f",
  "status": 200,
  "model": "deepseek-flash",
  "input_tokens": 11,
  "output_tokens": 5,
  "cached_tokens": 3,
  "cost_usd_nanos": 4209,
  "created_at": 1759360000
}
```

The handler waits up to 2 seconds for a just-finished request to settle and
persist before answering. If the record is still unavailable it returns HTTP
404 with `reason: request_not_found`; a client that knows the request has just
finished may retry with backoff.

## Batch records

```http
POST /v1/requests/query
Authorization: Bearer <CONSUMER_KEY>
Content-Type: application/json

{"ids": ["0f10ab1c-8d2f-4c0a-9b1e-3f5a7c9d1e2f", "4eef28a0-1a2b-4c3d-8e9f-0a1b2c3d4e5f"]}
```

```json
{
  "requests": [
    {"request_id": "0f10ab1c-…", "status": 200, "model": "deepseek-flash",
     "input_tokens": 11, "output_tokens": 5, "cached_tokens": 3,
     "cost_usd_nanos": 4209, "created_at": 1759360000}
  ],
  "missing": ["4eef28a0-…"]
}
```

- The batch handler never waits: `requests` holds settled records in input
  order (duplicate IDs are collapsed), `missing` holds IDs that are not settled
  yet or unknown. Retry `missing` on the next backfill round.
- Up to 1000 IDs per call; an empty `ids` array or more than 1000 IDs is
  rejected with HTTP 400. The request body is capped at 256 KiB.
- The response is HTTP 200 even when every ID is missing. The call is
  read-only, so retrying is safe.

Both endpoints accept any active consumer credential (`Authorization: Bearer
sk-*`). Records are addressed by request ID alone — the querying credential is
not matched against the request's consumer — so a platform that aggregates
requests across several credentials can use any one of them. Treat the request
ID as the capability: whoever holds it can read that record's amount and usage
metadata. IDs are random UUIDs and cannot be enumerated; the console audit
remains scoped per tenant.

## Backfill loop

```text
pending := recorded request IDs (from x-deepseek-lb-request-id headers)

loop, every N minutes or once pending is large enough:
    batch    := up to 1000 IDs taken from pending
    response := POST {lb}/v1/requests/query {"ids": batch}
    store response.requests locally (amount + usage)
    pending  := (pending - batch) ∪ response.missing
    give up on IDs that stay missing for too long: settlement completes
    milliseconds after the stream ends, so a permanently missing ID was most
    likely dropped by a saturated audit queue or never existed
```

## Field reference

| field | meaning |
| --- | --- |
| `request_id` | the `x-deepseek-lb-request-id` header value of the original call |
| `status` | recorded HTTP status (499 is a client-cancelled stream) |
| `model` | requested model; `null` when the request failed before parsing |
| `input_tokens`, `output_tokens`, `cached_tokens` | billed token usage |
| `cost_usd_nanos` | the amount actually charged, in USD nanodollars (1e-9 USD) |
| `created_at` | request start time, Unix seconds |

`cost_usd_nanos` equals `api_calls.actual_cost_usd_nanos` for the call — the
same value the console audit shows and that user consumption accounting uses.
It is read from the ledger at query time, never recomputed.

## Notes

- A request dropped by a saturated audit queue is neither billed nor queryable.
- The single-record wait exists for interactive lookups; batch callers should
  rely on the `missing` list instead.

