export const webSearchMaxResultsLimit = 50
export const webSearchDefaultMaxResults = 8

export type WebSearchRequestBody = {
  query: string
  max_results: number
  need_content?: boolean
  allowed_domains?: string[]
  blocked_domains?: string[]
}

/** A whole-number search control within `1..=maximum`, or `undefined`. */
export function parseWebSearchBound(
  value: string,
  maximum: number
): number | undefined {
  const bound = Number(value)
  return Number.isSafeInteger(bound) && bound >= 1 && bound <= maximum
    ? bound
    : undefined
}

export function parseWebSearchDomains(value: string): string[] {
  return value.split(/[\s,]+/).filter(Boolean)
}

export function webSearchRequestBody(
  query: string,
  maxResults: number,
  needContent: boolean,
  allowedDomains: string,
  blockedDomains: string
): WebSearchRequestBody {
  const allowed = parseWebSearchDomains(allowedDomains)
  const blocked = parseWebSearchDomains(blockedDomains)
  return {
    query: query.trim(),
    max_results: maxResults,
    ...(needContent ? { need_content: true } : {}),
    ...(allowed.length > 0 ? { allowed_domains: allowed } : {}),
    ...(blocked.length > 0 ? { blocked_domains: blocked } : {}),
  }
}
