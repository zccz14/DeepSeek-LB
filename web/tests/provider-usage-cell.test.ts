import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"

const app = await readFile(new URL("../src/App.tsx", import.meta.url), "utf8")

test("shows provider usage as labeled token metrics with grouped numbers", () => {
  const cell = app.slice(
    app.indexOf("{t.providerUsageRequests}"),
    app.indexOf("{t.providerNoUsage}")
  )
  assert.match(cell, /usage\.requests\.toLocaleString\(locale\)/)
  assert.match(cell, /usage\.errors\.toLocaleString\(locale\)/)
  assert.match(cell, /label=\{t\.input\}/)
  assert.match(cell, /value=\{usage\.input_tokens\}/)
  assert.match(cell, /label=\{t\.output\}/)
  assert.match(cell, /value=\{usage\.output_tokens\}/)
  assert.match(cell, /label=\{t\.cacheHitTokens\}/)
  assert.match(cell, /value=\{usage\.cached_tokens\}/)
  assert.doesNotMatch(cell, /\{usage\.input_tokens\}\//)
  assert.doesNotMatch(app, /providerUsageTokens/)
})

test("keeps the shared token metric grouped for both locales", () => {
  const metric = app.slice(
    app.indexOf("function TokenMetric("),
    app.indexOf("function DataTable(")
  )
  assert.match(metric, /\{value\.toLocaleString\(locale\)\}/)
})
