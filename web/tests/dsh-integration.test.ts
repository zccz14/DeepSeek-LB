import assert from "node:assert/strict"
import test from "node:test"
import { readFile } from "node:fs/promises"
import { fileURLToPath } from "node:url"

const appPath = fileURLToPath(new URL("../src/App.tsx", import.meta.url))

test("exposes the DSH manual and automatic configuration guide", async () => {
  const source = await readFile(appPath, "utf8")

  assert.match(source, /path="\/dsh-integration"/)
  assert.match(
    source,
    /<TabsTrigger value="manual">\s*\{content\.manualTitle\}\s*<\/TabsTrigger>/
  )
  assert.match(
    source,
    /<TabsTrigger value="automatic">\s*\{content\.automaticTitle\}\s*<\/TabsTrigger>/
  )
  assert.match(source, /showDirectoryPicker/)
  assert.match(source, /getFileHandle\("settings\.yaml"\)/)
  assert.match(source, /getFileHandle\("\.credentials\.yaml"/)
  assert.match(source, /Provider ID/)
  assert.match(source, /openai-responses/)
  assert.match(source, /Get available models/)
})
