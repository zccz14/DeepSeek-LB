import assert from "node:assert/strict"
import test from "node:test"

import { parse } from "jsonc-parser"

import { updateOpenCodeConfig } from "../src/lib/opencode-config.ts"

test("writes an OpenAI-LB provider into an OpenCode JSONC configuration", () => {
  const updated = updateOpenCodeConfig(
    "",
    "sk-example-token",
    "https://openai.ntnl.io"
  )

  assert.deepEqual(parse(updated), {
    provider: {
      "openai-lb": {
        npm: "@ai-sdk/openai-compatible",
        name: "OpenAI-LB",
        options: {
          baseURL: "https://openai.ntnl.io/v1",
          apiKey: "sk-example-token",
        },
        models: { "gpt-5.4": { name: "gpt-5.4" } },
      },
    },
  })
})

test("preserves unrelated JSONC settings and OpenAI-LB provider fields", () => {
  const content = `{
  // Keep this comment and existing configuration.
  "theme": "dark",
    "provider": {
      "openai-lb": {
        "models": { "custom": { "name": "custom" } },
        "options": { "timeout": 100 },
        "custom": true
    }
  }
}
`
  const updated = updateOpenCodeConfig(
    content,
    "sk-new-token",
    "https://openai.ntnl.io"
  )
  const config = parse(updated) as {
    theme: string
    provider: { "openai-lb": Record<string, unknown> }
  }

  assert.match(updated, /Keep this comment/u)
  assert.equal(config.theme, "dark")
  assert.equal(config.provider["openai-lb"].custom, true)
  assert.deepEqual(config.provider["openai-lb"].models, {
    custom: { name: "custom" },
    "gpt-5.4": { name: "gpt-5.4" },
  })
  assert.deepEqual(config.provider["openai-lb"].options, {
    timeout: 100,
    baseURL: "https://openai.ntnl.io/v1",
    apiKey: "sk-new-token",
  })
})

test("rejects invalid JSONC and malformed Consumer tokens", () => {
  assert.throws(() =>
    updateOpenCodeConfig("{", "sk-token", "https://openai.ntnl.io")
  )
  assert.throws(() =>
    updateOpenCodeConfig("{}", "not-a-token", "https://openai.ntnl.io")
  )
  assert.throws(() =>
    updateOpenCodeConfig("[]", "sk-token", "https://openai.ntnl.io")
  )
  assert.throws(() =>
    updateOpenCodeConfig(
      '{"provider":"invalid"}',
      "sk-token",
      "https://openai.ntnl.io"
    )
  )
})
