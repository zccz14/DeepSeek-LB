import assert from "node:assert/strict"
import test from "node:test"

import { parse } from "yaml"

import {
  dshModels,
  updateDshCredentials,
  updateDshSettings,
} from "../src/lib/dsh-config.ts"

const origin = "https://deepseek.ntnl.io"

test("writes the DeepSeek-LB DSH provider without replacing other settings", () => {
  const updated = updateDshSettings(
    `# Keep this unrelated configuration.\napp:\n  color: purple\nllm-pi-ai:\n  providers:\n    existing:\n      api: other\n    deepseek-lb:\n      customSetting: keep\n`,
    "sk-example-token",
    dshModels(["deepseek-flash", "deepseek-v4-pro"]),
    origin
  )
  const settings = parse(updated) as {
    app: { color: string }
    "llm-pi-ai": { providers: Record<string, Record<string, unknown>> }
  }

  assert.equal(settings.app.color, "purple")
  assert.match(updated, /Keep this unrelated configuration/u)
  assert.deepEqual(settings["llm-pi-ai"].providers.existing, { api: "other" })
  assert.deepEqual(settings["llm-pi-ai"].providers["deepseek-lb"], {
    customSetting: "keep",
    displayName: "DeepSeek-LB",
    apiKeyEnv: "DEEPSEEK_LB_API_KEY",
    api: "openai-responses",
    baseURL: `${origin}/v1`,
    models: [
      { id: "deepseek-flash", name: "Deepseek Flash" },
      { id: "deepseek-v4-pro", name: "Deepseek V4 Pro" },
    ],
  })
})

test("writes the generated Consumer key into DSH credentials", () => {
  const updated = updateDshCredentials(
    "OTHER_KEY: keep\nDEEPSEEK_LB_API_KEY: sk-old\n",
    "sk-new-token"
  )

  assert.deepEqual(parse(updated), {
    OTHER_KEY: "keep",
    DEEPSEEK_LB_API_KEY: "sk-new-token",
  })
})

test("rejects malformed YAML, unsafe structures, and invalid Consumer keys", () => {
  assert.throws(() =>
    updateDshSettings("[", "sk-token", dshModels(["deepseek-flash"]), origin)
  )
  assert.throws(() =>
    updateDshSettings(
      "llm-pi-ai: invalid\n",
      "sk-token",
      dshModels(["deepseek-flash"]),
      origin
    )
  )
  assert.throws(() =>
    updateDshSettings("", "sk-token", [], origin),
    /At least one DSH model/
  )
  assert.throws(() => updateDshCredentials("[]\n", "sk-token"))
  assert.throws(() => updateDshCredentials("", "not-a-token"))
})
