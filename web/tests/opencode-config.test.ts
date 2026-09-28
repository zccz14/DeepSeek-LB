import assert from "node:assert/strict"
import test from "node:test"

import { parse } from "jsonc-parser"

import { updateOpenCodeConfig } from "../src/lib/opencode-config.ts"

const origin = "https://deepseek.ntnl.io"
const models = ["deepseek-flash", "deepseek-v4-pro"]

test("writes a DeepSeek-LB provider into an OpenCode JSONC configuration", () => {
  const updated = updateOpenCodeConfig("", "sk-example-token", origin, models)

  assert.deepEqual(parse(updated), {
    provider: {
      "deepseek-lb": {
        npm: "@ai-sdk/openai-compatible",
        name: "DeepSeek-LB",
        options: {
          baseURL: `${origin}/v1`,
          apiKey: "sk-example-token",
        },
        models: {
          "deepseek-flash": { name: "deepseek-flash" },
          "deepseek-v4-pro": { name: "deepseek-v4-pro" },
        },
      },
    },
  })
})

test("preserves unrelated JSONC settings and provider fields", () => {
  const content = `{
  // Keep this comment and existing configuration.
  "theme": "dark",
    "provider": {
      "deepseek-lb": {
        "models": { "custom": { "name": "custom" } },
        "options": { "timeout": 100 },
        "custom": true
    }
  }
}
`
  const updated = updateOpenCodeConfig(content, "sk-new-token", origin, models)
  const config = parse(updated) as {
    theme: string
    provider: { "deepseek-lb": Record<string, unknown> }
  }

  assert.match(updated, /Keep this comment/u)
  assert.equal(config.theme, "dark")
  assert.equal(config.provider["deepseek-lb"].custom, true)
  assert.deepEqual(config.provider["deepseek-lb"].models, {
    "deepseek-flash": { name: "deepseek-flash" },
    "deepseek-v4-pro": { name: "deepseek-v4-pro" },
  })
  assert.deepEqual(config.provider["deepseek-lb"].options, {
    timeout: 100,
    baseURL: `${origin}/v1`,
    apiKey: "sk-new-token",
  })
})

test("rejects invalid JSONC, empty model lists and malformed tokens", () => {
  assert.throws(() => updateOpenCodeConfig("{", "sk-token", origin, models))
  assert.throws(() => updateOpenCodeConfig("{", "sk-token", origin, []))
  assert.throws(() => updateOpenCodeConfig("{}", "not-a-token", origin, models))
  assert.throws(() => updateOpenCodeConfig("[]", "sk-token", origin, models))
  assert.throws(() =>
    updateOpenCodeConfig('{"provider":"invalid"}', "sk-token", origin, models)
  )
})
