import assert from "node:assert/strict"
import test from "node:test"
import { readFile } from "node:fs/promises"
import { fileURLToPath } from "node:url"

const appPath = fileURLToPath(new URL("../src/App.tsx", import.meta.url))

test("exposes manual setup tabs for Codex and OpenCode", async () => {
  const source = await readFile(appPath, "utf8")

  assert.match(source, /codexProviderBlock\(manualOrigin, "<YOUR_CONSUMER_KEY>"\)/)
  assert.match(source, /"apiKey": "<YOUR_CONSUMER_KEY>"/)
  assert.match(
    source,
    /<TabsTrigger value="manual">\s*\{content\.manualTitle\}\s*<\/TabsTrigger>/
  )
  assert.match(
    source,
    /<TabsTrigger value="automatic">\s*\{content\.automaticTitle\}\s*<\/TabsTrigger>/
  )
})
