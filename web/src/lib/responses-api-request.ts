export type JsonRecord = Record<string, unknown>

export function parseJsonValue(value: unknown): unknown {
  if (typeof value !== "string") return value
  try {
    return JSON.parse(value)
  } catch {
    return value
  }
}

export function recordValue(value: unknown): JsonRecord | undefined {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as JsonRecord)
    : undefined
}

export function inputItems(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [value]
}

export function contentParts(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [value]
}

export function contentText(value: unknown): string | undefined {
  if (typeof value === "string") return value
  const record = recordValue(value)
  return typeof record?.text === "string" ? record.text : undefined
}

export function imageUrl(value: unknown): string | undefined {
  if (typeof value === "string") return imageSource(value) ? value : undefined
  const record = recordValue(value)
  const url =
    typeof record?.url === "string"
      ? record.url
      : typeof record?.image_url === "string"
        ? record.image_url
        : undefined
  return url && imageSource(url) ? url : undefined
}

function imageSource(value: string) {
  return /^(https?:|data:image\/)/i.test(value)
}

export function displayValue(value: unknown): string {
  if (typeof value === "string") return value
  if (value === undefined || value === null) return "—"
  return JSON.stringify(value, null, 2) || "—"
}

export function base64PayloadSize(value: unknown): number | undefined {
  if (typeof value !== "string") return undefined
  const payload = value.includes(",")
    ? value.slice(value.indexOf(",") + 1)
    : value
  if (!/^[A-Za-z0-9+/]*={0,2}$/.test(payload)) return undefined
  return (
    Math.floor((payload.length * 3) / 4) -
    (payload.endsWith("==") ? 2 : payload.endsWith("=") ? 1 : 0)
  )
}
