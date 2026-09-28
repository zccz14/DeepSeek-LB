import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"

test("inference audit renders service tier details inside the model cell", async () => {
  const app = await readFile(new URL("../src/App.tsx", import.meta.url), "utf8")
  const audit = app.slice(
    app.indexOf("function AuditPage"),
    app.indexOf("function CopyableIdentifier")
  )

  assert.match(app, /fast_mode: boolean/)
  assert.doesNotMatch(audit, /<TableHead>\{t\.reasoningEffort\}<\/TableHead>/)
  assert.doesNotMatch(audit, /<TableHead>\{t\.fastMode\}<\/TableHead>/)
  assert.match(audit, /\{t\.reasoningEffort\} · \{row\.reasoning_effort\}/)
  assert.match(audit, /row\.fast_mode && \(/)
  assert.match(app, /\[labels\.fastMode, data\.fast_mode \? "Fast" : "—"\]/)
})
