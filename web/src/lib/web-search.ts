export const webSearchMaxResultsLimit = 50
export const webSearchMaxUsesLimit = 5
export const webSearchDefaultMaxResults = 8
export const webSearchDefaultMaxUses = 5

export type WebSearchLocation = {
  country: string
  region: string
  city: string
  timezone: string
}

export type WebSearchRequestBody = {
  query: string
  max_results: number
  max_uses: number
  allowed_domains?: string[]
  blocked_domains?: string[]
  user_location?: Partial<WebSearchLocation>
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
  maxUses: number,
  allowedDomains: string,
  blockedDomains: string,
  location: WebSearchLocation
): WebSearchRequestBody {
  const allowed = parseWebSearchDomains(allowedDomains)
  const blocked = parseWebSearchDomains(blockedDomains)
  const userLocation: Partial<WebSearchLocation> = {}
  for (const key of ["country", "region", "city", "timezone"] as const) {
    const value = location[key].trim()
    if (value) userLocation[key] = value
  }
  return {
    query: query.trim(),
    max_results: maxResults,
    max_uses: maxUses,
    ...(allowed.length > 0 ? { allowed_domains: allowed } : {}),
    ...(blocked.length > 0 ? { blocked_domains: blocked } : {}),
    ...(Object.keys(userLocation).length > 0
      ? { user_location: userLocation }
      : {}),
  }
}
