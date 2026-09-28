export type HeaderComparisonRow = {
  name: string
  left?: string
  right?: string
  differs: boolean
}

function readSnapshot(value?: string | null) {
  if (!value) return undefined
  let parsed: unknown
  try {
    parsed = JSON.parse(value)
  } catch {
    return undefined
  }
  if (!Array.isArray(parsed)) return undefined
  const headers = new Map<string, { name: string; value: string }>()
  for (const entry of parsed) {
    if (
      !Array.isArray(entry) ||
      typeof entry[0] !== "string" ||
      typeof entry[1] !== "string"
    )
      return undefined
    headers.set(entry[0].toLowerCase(), { name: entry[0], value: entry[1] })
  }
  return headers
}

export function compareHeaderSnapshots(
  left?: string | null,
  right?: string | null
) {
  const leftSnapshot = readSnapshot(left)
  const rightSnapshot = readSnapshot(right)
  // Missing diagnostics are unknown, not a captured empty header list ("[]").
  const comparable = leftSnapshot !== undefined && rightSnapshot !== undefined
  const names = new Set([
    ...(leftSnapshot?.keys() ?? []),
    ...(rightSnapshot?.keys() ?? []),
  ])
  const rows: HeaderComparisonRow[] = [...names]
    .map((name) => {
      const leftValue = leftSnapshot?.get(name)
      const rightValue = rightSnapshot?.get(name)
      return {
        name: leftValue?.name || rightValue?.name || name,
        left: leftValue?.value,
        right: rightValue?.value,
        differs: comparable && leftValue?.value !== rightValue?.value,
      }
    })
    .sort((a, b) => a.name.localeCompare(b.name))
  return {
    rows,
    leftAvailable: leftSnapshot !== undefined,
    rightAvailable: rightSnapshot !== undefined,
  }
}
