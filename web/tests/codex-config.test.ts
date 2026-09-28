import assert from "node:assert/strict"
import test from "node:test"

import { parse } from "smol-toml"

import {
  codexPlatform,
  isConsumerToken,
  updateCodexConfig,
} from "../src/lib/codex-config.ts"

const origin = "https://deepseek.ntnl.io"

test("writes the required Codex provider configuration", () => {
  const updated = updateCodexConfig("", "sk-example-token", origin)

  assert.equal(
    updated,
    `model_provider = "deepseek-lb"

[model_providers.deepseek-lb]
name = "DeepSeek-LB"
base_url = "${origin}/v1"
wire_api = "responses"
experimental_bearer_token = "sk-example-token"
`
  )
  assert.equal(parse(updated).model_provider, "deepseek-lb")
})

test("preserves unrelated settings and updates an existing provider", () => {
  const content = `approval_policy = "never"

[features]
unified_exec = true

[model_providers.deepseek-lb]
name = "Old name"
base_url = "https://old.example/v1"
custom_setting = "keep me"
experimental_bearer_token = "sk-old-token"
wire_api = "chat"
`

  assert.equal(
    updateCodexConfig(content, "sk-new-token", origin),
    `approval_policy = "never"

model_provider = "deepseek-lb"
[features]
unified_exec = true

[model_providers.deepseek-lb]
name = "DeepSeek-LB"
base_url = "${origin}/v1"
custom_setting = "keep me"
experimental_bearer_token = "sk-new-token"
wire_api = "responses"
`
  )
})

test("keeps the document newline convention", () => {
  const updated = updateCodexConfig(
    'approval_policy = "never"\r\n',
    "sk-token",
    origin
  )

  assert.match(updated, /\r\n/u)
  assert.doesNotMatch(updated, /(?<!\r)\n/u)
})

test("rejects malformed Consumer tokens and unsafe provider definitions", () => {
  assert.equal(isConsumerToken("sk-valid-token"), true)
  assert.equal(isConsumerToken("token"), false)
  assert.equal(isConsumerToken("sk-two words"), false)
  assert.throws(() => updateCodexConfig("", "token", origin))
  assert.throws(() =>
    updateCodexConfig('model_providers = { legacy = "value" }\n', "sk-token", origin)
  )
  assert.throws(() =>
    updateCodexConfig("[[model_providers.deepseek-lb]]\n", "sk-token", origin)
  )
  assert.throws(() => updateCodexConfig("this is not = TOML\n", "sk-token", origin))
})

test("identifies the desktop platform from the browser user agent", () => {
  assert.equal(
    codexPlatform("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)"),
    "macos"
  )
  assert.equal(
    codexPlatform("Mozilla/5.0 (Windows NT 10.0; Win64; x64)"),
    "windows"
  )
  assert.equal(codexPlatform("Mozilla/5.0 (X11; Linux x86_64)"), "linux")
  assert.equal(
    codexPlatform("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0)"),
    "other"
  )
})
