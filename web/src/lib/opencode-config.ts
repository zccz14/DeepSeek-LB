import { applyEdits, modify, parse, type ParseError } from "jsonc-parser"

import { isConsumerToken } from "./codex-config.ts"

type JsonObject = Record<string, unknown>

const providerId = "deepseek-lb"

export function updateOpenCodeConfig(
  content: string,
  token: string,
  origin: string,
  modelIds: string[]
) {
  if (!isConsumerToken(token)) {
    throw new Error(
      "Consumer token must start with sk- and contain no whitespace."
    )
  }
  if (modelIds.length === 0) {
    throw new Error("At least one model is required.")
  }

  const document = parseDocument(content)
  const provider = objectValue(objectValue(document.provider)[providerId])
  const models = objectValue(provider.models)
  const nextProvider = {
    ...provider,
    npm: "@ai-sdk/openai-compatible",
    name: "DeepSeek-LB",
    options: {
      ...objectValue(provider.options),
      baseURL: `${origin}/v1`,
      apiKey: token,
    },
    models: Object.fromEntries(
      modelIds.map((id) => [id, { ...objectValue(models[id]), name: id }])
    ),
  }
  const newline = content.includes("\r\n") ? "\r\n" : "\n"
  const edits = modify(content, ["provider", providerId], nextProvider, {
    formattingOptions: { insertSpaces: true, tabSize: 2, eol: newline },
  })

  return applyEdits(content, edits)
}

function parseDocument(content: string): JsonObject {
  if (!content.trim()) return {}
  const errors: ParseError[] = []
  const document = parse(content, errors, {
    allowTrailingComma: true,
    disallowComments: false,
  })
  if (errors.length > 0) throw new Error("opencode.jsonc could not be parsed.")
  return objectValue(document)
}

function objectValue(value: unknown): JsonObject {
  if (value === undefined) return {}
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error("DeepSeek-LB provider must be a JSON object.")
  }
  return value as JsonObject
}
