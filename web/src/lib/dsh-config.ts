import { isMap, parseDocument } from "yaml"

import { isConsumerToken } from "./codex-config.ts"

const providerId = "ntnl-openai"

export type DshModel = { id: string; name: string }

export function dshModels(modelIds: string[]): DshModel[] {
  return modelIds.map((id) => ({ id, name: dshModelName(id) }))
}

export function updateDshSettings(
  content: string,
  token: string,
  models: DshModel[]
) {
  assertConsumerToken(token)
  if (models.length === 0)
    throw new Error("At least one DSH model is required.")

  const document = parseYaml(content, "settings.yaml")
  assertMapOrMissing(document.getIn(["llm-pi-ai"], true), "llm-pi-ai")
  assertMapOrMissing(
    document.getIn(["llm-pi-ai", "providers"], true),
    "llm-pi-ai.providers"
  )
  const existing = mapValue(
    document.getIn(["llm-pi-ai", "providers", providerId], true),
    `llm-pi-ai.providers.${providerId}`
  )
  document.setIn(["llm-pi-ai", "providers", providerId], {
    ...existing,
    displayName: "NTNL OpenAI",
    apiKeyEnv: "NTNL_OPENAI_API_KEY",
    api: "openai-responses",
    baseURL: "https://openai.ntnl.io/v1",
    models,
  })
  return serializeYaml(document, content)
}

export function updateDshCredentials(content: string, token: string) {
  assertConsumerToken(token)

  const document = parseYaml(content, ".credentials.yaml")
  assertMapOrMissing(document.contents, ".credentials.yaml")
  document.setIn(["NTNL_OPENAI_API_KEY"], token)
  return serializeYaml(document, content)
}

function assertConsumerToken(token: string) {
  if (!isConsumerToken(token)) {
    throw new Error(
      "Consumer token must start with sk- and contain no whitespace."
    )
  }
}

function parseYaml(content: string, name: string) {
  const document = parseDocument(content)
  if (document.errors.length > 0) {
    throw new Error(`${name} could not be parsed.`)
  }
  return document
}

function assertMapOrMissing(value: unknown, name: string) {
  if (value !== undefined && !isMap(value)) {
    throw new Error(`${name} must be a YAML mapping.`)
  }
}

function mapValue(value: unknown, name: string): Record<string, unknown> {
  assertMapOrMissing(value, name)
  return isMap(value) ? (value.toJSON() as Record<string, unknown>) : {}
}

function serializeYaml(
  document: ReturnType<typeof parseDocument>,
  original: string
) {
  const newline = original.includes("\r\n") ? "\r\n" : "\n"
  return document.toString({ lineWidth: 0 }).replace(/\n/g, newline)
}

function dshModelName(modelId: string) {
  const [family, version, ...suffix] = modelId.split("-")
  const familyName = family.toLowerCase() === "gpt" ? "GPT" : title(family)
  return [familyName, version, ...suffix.map(title)].filter(Boolean).join(" ")
}

function title(value: string) {
  return value ? value[0].toUpperCase() + value.slice(1) : value
}
