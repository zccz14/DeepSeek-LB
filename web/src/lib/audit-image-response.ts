type JsonRecord = Record<string, unknown>

export type AuditImageResponse = {
  revisedPrompt?: string
  src: string
}

export function auditImageResponses(
  responseBody?: string,
  requestBody?: string
): AuditImageResponse[] {
  const response = recordValue(parseJson(responseBody))
  if (!Array.isArray(response?.data)) return []

  const mediaType = imageMediaType(requestBody)
  return response.data.flatMap((value) => {
    const image = recordValue(value)
    if (!image || !isBase64(image.b64_json)) return []
    return [
      {
        src: `data:${mediaType};base64,${image.b64_json}`,
        ...(typeof image.revised_prompt === "string"
          ? { revisedPrompt: image.revised_prompt }
          : {}),
      },
    ]
  })
}

function imageMediaType(requestBody?: string) {
  const format = recordValue(parseJson(requestBody))?.output_format
  if (format === "jpeg") return "image/jpeg"
  if (format === "webp") return "image/webp"
  return "image/png"
}

function isBase64(value: unknown): value is string {
  return (
    typeof value === "string" &&
    value.length > 0 &&
    /^[A-Za-z0-9+/]*={0,2}$/.test(value)
  )
}

function parseJson(value?: string): unknown {
  if (!value) return undefined
  try {
    return JSON.parse(value) as unknown
  } catch {
    return undefined
  }
}

function recordValue(value: unknown): JsonRecord | undefined {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as JsonRecord)
    : undefined
}
