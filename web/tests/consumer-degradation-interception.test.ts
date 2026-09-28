import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"

const app = await readFile(new URL("../src/App.tsx", import.meta.url), "utf8")
const consumers = app.slice(
  app.indexOf("function Consumers"),
  app.indexOf("function UsagePage")
)

test("exposes the Consumer degradation interception switch", () => {
  assert.match(app, /intercept_degradation: boolean/)
  assert.match(consumers, /intercept_degradation: interceptDegradation/)
  assert.match(
    consumers,
    /JSON\.stringify\(\{ intercept_degradation: checked \}\)/
  )
  assert.match(consumers, /<TableHead>\{t\.interceptDegradation\}<\/TableHead>/)
  assert.match(
    consumers,
    /checked=\{consumer\.intercept_degradation\}[\s\S]*?updateDegradationInterception\(/
  )
  assert.match(consumers, /id="consumer-intercept-degradation"/)
  assert.match(consumers, /toast\.success\(t\.interceptDegradationUpdated\)/)
  assert.match(app, /interceptDegradation: "降级拦截"/)
  assert.match(app, /interceptDegradation: "Degradation interception"/)
})
