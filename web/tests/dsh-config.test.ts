import assert from "node:assert/strict"
import test from "node:test"

import { parse } from "yaml"

import {
  dshModels,
  updateDshCredentials,
  updateDshSettings,
} from "../src/lib/dsh-config.ts"

test("writes the NTNL OpenAI DSH provider without replacing other settings", () => {
  const updated = updateDshSettings(
    `# Keep this unrelated configuration.\napp:\n  color: purple\nllm-pi-ai:\n  providers:\n    existing:\n      api: other\n    ntnl-openai:\n      customSetting: keep\n`,
    "sk-example-token",
    dshModels(["gpt-5.6-terra", "gpt-4o-transcribe"])
  )
  const settings = parse(updated) as {
    app: { color: string }
    "llm-pi-ai": { providers: Record<string, Record<string, unknown>> }
  }

  assert.equal(settings.app.color, "purple")
  assert.match(updated, /Keep this unrelated configuration/u)
  assert.deepEqual(settings["llm-pi-ai"].providers.existing, { api: "other" })
  assert.deepEqual(settings["llm-pi-ai"].providers["ntnl-openai"], {
    customSetting: "keep",
    displayName: "NTNL OpenAI",
    apiKeyEnv: "NTNL_OPENAI_API_KEY",
    api: "openai-responses",
    baseURL: "https://deepseek.ntnl.io/v1",
    models: [
      { id: "gpt-5.6-terra", name: "GPT 5.6 Terra" },
      { id: "gpt-4o-transcribe", name: "GPT 4o Transcribe" },
    ],
  })
})

test("writes the generated Consumer key into DSH credentials", () => {
  const updated = updateDshCredentials(
    "OTHER_KEY: keep\nNTNL_OPENAI_API_KEY: sk-old\n",
    "sk-new-token"
  )

  assert.deepEqual(parse(updated), {
    OTHER_KEY: "keep",
    NTNL_OPENAI_API_KEY: "sk-new-token",
  })
})

test("rejects malformed YAML, unsafe structures, and invalid Consumer keys", () => {
  assert.throws(() =>
    updateDshSettings("[", "sk-token", dshModels(["gpt-5.6-terra"]))
  )
  assert.throws(() =>
    updateDshSettings(
      "llm-pi-ai: invalid\n",
      "sk-token",
      dshModels(["gpt-5.6-terra"])
    )
  )
  assert.throws(() => updateDshCredentials("[]\n", "sk-token"))
  assert.throws(() => updateDshCredentials("", "not-a-token"))
})
