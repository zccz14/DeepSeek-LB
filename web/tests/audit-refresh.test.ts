import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"

test("inference audit refreshes the current query and prevents duplicate requests", async () => {
  const app = await readFile(new URL("../src/App.tsx", import.meta.url), "utf8")
  const audit = app.slice(
    app.indexOf("function AuditPage"),
    app.indexOf("function CopyableIdentifier")
  )

  assert.match(
    audit,
    /const \{ data, error, loading, refreshing, reload \}\s*=\s*useApiQuery<AuditPageResponse>\(/
  )
  assert.match(audit, /disabled=\{refreshing\}/)
  assert.match(audit, /onClick=\{\(\) => void reload\(\)\}/)
  assert.match(audit, /\{t\.refresh\}/)
  assert.match(app, /refreshing: query\.isFetching/)
})
