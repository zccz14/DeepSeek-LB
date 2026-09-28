import { parse } from "smol-toml"

const providerId = "deepseek-lb"

export type CodexPlatform = "linux" | "macos" | "windows" | "other"

export function isConsumerToken(token: string) {
  return token.startsWith("sk-") && token.length > 3 && !/\s/u.test(token)
}

export function codexPlatform(userAgent: string): CodexPlatform {
  if (/Windows/u.test(userAgent)) return "windows"
  if (/Macintosh|Mac OS X/u.test(userAgent)) return "macos"
  if (/Linux/u.test(userAgent)) return "linux"
  return "other"
}

// Codex talks to models through the Responses API, which DeepSeek serves natively.
export function codexProviderSettings(origin: string, token: string) {
  return [
    ["name", "DeepSeek-LB"],
    ["base_url", `${origin}/v1`],
    ["wire_api", "responses"],
    ["experimental_bearer_token", token],
  ] as const
}

export function codexProviderBlock(origin: string, token: string) {
  const settings = codexProviderSettings(origin, token)
    .map(([key, value]) => `${key} = ${tomlString(value)}`)
    .join("\n")

  return `model_provider = "${providerId}"

[model_providers.${providerId}]
${settings}
`
}

export function updateCodexConfig(content: string, token: string, origin: string) {
  if (!isConsumerToken(token)) {
    throw new Error(
      "Consumer token must start with sk- and contain no whitespace."
    )
  }
  parse(content)
  const settings = codexProviderSettings(origin, token)

  const newline = content.includes("\r\n") ? "\r\n" : "\n"
  const lines = content === "" ? [] : content.split(/\r?\n/u)
  if (/\r?\n$/u.test(content)) lines.pop()

  const rootEnd = firstTableHeader(lines)
  const modelProviderLines = keyLines(lines, "model_provider", 0, rootEnd)
  if (modelProviderLines.length > 1) {
    throw new Error(
      "config.toml contains more than one model_provider setting."
    )
  }
  if (modelProviderLines.length === 1) {
    lines[modelProviderLines[0]] = `model_provider = "${providerId}"`
  } else {
    lines.splice(rootEnd, 0, `model_provider = "${providerId}"`)
  }

  const updatedRootEnd = firstTableHeader(lines)
  if (keyLines(lines, "model_providers", 0, updatedRootEnd).length > 0) {
    throw new Error("config.toml uses an inline model_providers setting.")
  }
  if (lines.some(isDeepSeekLbProviderArrayHeader)) {
    throw new Error("config.toml uses an array of DeepSeek-LB providers.")
  }

  const providerHeader = lines.findIndex(isDeepSeekLbProviderHeader)
  if (providerHeader === -1) {
    appendProviderTable(lines, settings)
  } else {
    updateProviderTable(lines, providerHeader, settings)
  }

  return `${lines.join(newline)}${newline}`
}

function firstTableHeader(lines: string[]) {
  const index = lines.findIndex(isTableHeader)
  return index === -1 ? lines.length : index
}

function isTableHeader(line: string) {
  return /^\s*\[\[?[^\]]+\]\]?\s*(?:#.*)?$/u.test(line)
}

function isDeepSeekLbProviderHeader(line: string) {
  return /^\s*\[\s*model_providers\s*\.\s*deepseek-lb\s*\]\s*(?:#.*)?$/u.test(
    line
  )
}

function isDeepSeekLbProviderArrayHeader(line: string) {
  return /^\s*\[\[\s*model_providers\s*\.\s*deepseek-lb\s*\]\]\s*(?:#.*)?$/u.test(
    line
  )
}

function keyLines(lines: string[], key: string, start: number, end: number) {
  const keyPattern = new RegExp(`^\\s*${key}\\s*=`, "u")
  return lines.flatMap((line, index) =>
    index >= start && index < end && keyPattern.test(line) ? [index] : []
  )
}

function appendProviderTable(
  lines: string[],
  settings: readonly (readonly [string, string])[]
) {
  if (lines.length > 0 && lines.at(-1)?.trim() !== "") lines.push("")
  lines.push(`[model_providers.${providerId}]`)
  for (const [key, value] of settings) {
    lines.push(`${key} = ${tomlString(value)}`)
  }
}

function updateProviderTable(
  lines: string[],
  providerHeader: number,
  settings: readonly (readonly [string, string])[]
) {
  const nextTable = lines.findIndex(
    (line, index) => index > providerHeader && isTableHeader(line)
  )
  const tableEnd = nextTable === -1 ? lines.length : nextTable
  const missing: string[] = []

  for (const [key, value] of settings) {
    const matches = keyLines(lines, key, providerHeader + 1, tableEnd)
    if (matches.length > 1) {
      throw new Error(`config.toml contains more than one ${key} setting.`)
    }
    if (matches.length === 1) {
      lines[matches[0]] = `${key} = ${tomlString(value)}`
    } else {
      missing.push(`${key} = ${tomlString(value)}`)
    }
  }

  lines.splice(tableEnd, 0, ...missing)
}

function tomlString(value: string) {
  return JSON.stringify(value)
}
