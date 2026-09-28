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

test("renders platform capacity history with the shadcn chart component", async () => {
  const source = await readFile(appPath, "utf8")
  const start = source.indexOf("function PlatformCapacityTrend")
  const trend = source.slice(start, source.indexOf("function UsersPage", start))

  assert.match(source, /@\/components\/ui\/chart/)
  assert.match(trend, /<ChartContainer/)
  assert.match(trend, /<LineChart/)
  assert.doesNotMatch(trend, /<svg/)
})

test("capacity trend exposes the sampled time and localized capacity in its tooltip", async () => {
  const source = await readFile(appPath, "utf8")
  const start = source.indexOf("function PlatformCapacityTrend")
  const trend = source.slice(start, source.indexOf("function UsersPage", start))

  assert.match(source, /Tooltip as ChartTooltip/)
  assert.match(trend, /<LineChart accessibilityLayer data=\{chartData\}>/)
  assert.match(trend, /<ChartTooltip[\s\S]*?<\/LineChart>/)
  assert.match(trend, /labelFormatter=\{\(value\) => formatTime\(Number\(value\), locale\)\}/)
  assert.match(trend, /Number\(value\)\.toLocaleString\(locale, \{ maximumFractionDigits: 2 \}\)/)
  assert.match(trend, /formatter=\{[\s\S]*?%`[\s\S]*?t\.platformPlusCapacity/)
  assert.match(trend, /backgroundColor: "var\(--popover\)"/)
  assert.match(trend, /itemStyle=\{\{ color: "var\(--popover-foreground\)" \}\}/)
})

test("capacity trend requests a seven-day history window", async () => {
  const source = await readFile(new URL("../../src/api.rs", import.meta.url), "utf8")
  assert.match(source, /PROVIDER_CAPACITY_HISTORY_WINDOW_SECONDS: i64 = 7 \* 24 \* 60 \* 60/)
})
