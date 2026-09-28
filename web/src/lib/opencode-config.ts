import { applyEdits, modify, parse, type ParseError } from "jsonc-parser"

import { isConsumerToken } from "./codex-config.ts"

type JsonObject = Record<string, unknown>

export function updateOpenCodeConfig(
  content: string,
  token: string,
  origin: string
) {
  if (!isConsumerToken(token)) {
    throw new Error(
      "Consumer token must start with sk- and contain no whitespace."
    )
  }

  const document = parseDocument(content)
  const provider = objectValue(objectValue(document.provider)["openai-lb"])
  const nextProvider = {
    ...provider,
    npm: "@ai-sdk/openai-compatible",
    name: "OpenAI-LB",
    options: {
      ...objectValue(provider.options),
      baseURL: `${origin}/v1`,
      apiKey: token,
    },
    models: {
      ...objectValue(provider.models),
      "gpt-5.4": {
        ...objectValue(objectValue(provider.models)["gpt-5.4"]),
        name: "gpt-5.4",
      },
    },
  }
  const newline = content.includes("\r\n") ? "\r\n" : "\n"
  const edits = modify(content, ["provider", "openai-lb"], nextProvider, {
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
    throw new Error("OpenAI-LB provider must be a JSON object.")
  }
  return value as JsonObject
}
