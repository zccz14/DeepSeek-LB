import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"

const appPath = new URL("../src/App.tsx", import.meta.url)

test("uses native numeric validation for the price multiplier", async () => {
  const source = await readFile(appPath, "utf8")
  const start = source.indexOf('id="settings-model-price-multiplier"')
  const input = source.slice(start, source.indexOf("</Field>", start))

  assert.match(input, /type="number"/)
  assert.match(input, /min="0\.000000001"/)
  assert.match(input, /max="1000"/)
  assert.match(input, /step="any"/)
  assert.doesNotMatch(input, /pattern=/)
})

test("sends only the DeepSeek runtime settings on save", async () => {
  const source = await readFile(appPath, "utf8")
  const start = source.indexOf("function RuntimeSettings")
  const runtime = source.slice(start, source.indexOf("function AdminAuditPage", start))

  for (const field of [
    "upstream_base",
    "model_price_multiplier",
    "allow_all_users_debt",
    "response_body_limit",
    "affinity_ttl_seconds",
    "request_archive_retention_days",
  ]) {
    assert.match(runtime, new RegExp(field))
  }
  assert.doesNotMatch(runtime, /oauth_/)
  assert.doesNotMatch(runtime, /image_body_limit/)
  assert.doesNotMatch(runtime, /audio_body_limit/)
})

test("keeps the peak and off-peak price table in the model price page", async () => {
  const source = await readFile(appPath, "utf8")
  const start = source.indexOf("function ModelPricesPage")
  const page = source.slice(start, source.indexOf("function TopupsPage", start))

  assert.match(page, /price\.peak\.cache_hit_usd_nanos/)
  assert.match(page, /price\.peak\.cache_miss_usd_nanos/)
  assert.match(page, /price\.off_peak\.output_usd_nanos/)
  assert.match(page, /\{t\.pricingPeak\}/)
  assert.match(page, /\{t\.pricingOffPeak\}/)
})
