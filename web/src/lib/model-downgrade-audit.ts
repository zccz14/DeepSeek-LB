export type ModelDowngradeRow = {
  hour_start: number
  provider_id: string | null
  provider_name: string | null
  downstream_model: string
  upstream_model: string
  requests: number
}

export type ModelDowngradeFlow = {
  provider_id: string | null
  provider_name: string | null
  upstream_model: string
  downstream_model: string
  requests: number
}

export type ModelDowngradeRatePoint = {
  hour_start: number
  requests: number
  downgraded: number
  downgrade_rate: number | null
}

export type ModelDowngradeSankeyNode = { name: string }
export type ModelDowngradeSankeyLink = {
  source: number
  target: number
  value: number
  downgradedRequests: number
}

export const unknownProviderId = "__unknown__"

export function isModelDowngrade(row: {
  downstream_model: string
  upstream_model: string
}) {
  return row.downstream_model !== row.upstream_model
}

function matchesProvider(row: ModelDowngradeRow, providerFilter: string) {
  return (
    providerFilter === "all" ||
    (row.provider_id ?? unknownProviderId) === providerFilter
  )
}

export function modelDowngradeFlows(
  rows: ModelDowngradeRow[],
  providerFilter: string
): ModelDowngradeFlow[] {
  const flows = new Map<string, ModelDowngradeFlow>()
  for (const row of rows) {
    if (!matchesProvider(row, providerFilter)) continue
    const key = [
      row.provider_id ?? unknownProviderId,
      row.upstream_model,
      row.downstream_model,
    ].join("\u0000")
    const flow = flows.get(key) ?? {
      provider_id: row.provider_id,
      provider_name: row.provider_name,
      upstream_model: row.upstream_model,
      downstream_model: row.downstream_model,
      requests: 0,
    }
    flow.requests += row.requests
    flows.set(key, flow)
  }
  return [...flows.values()].sort(
    (left, right) => right.requests - left.requests
  )
}

export function modelDowngradeRatePoints(
  rows: ModelDowngradeRow[],
  providerFilter: string,
  since: number,
  until: number
): ModelDowngradeRatePoint[] {
  const start = Math.floor(since / 3600) * 3600
  const end = Math.floor(until / 3600) * 3600
  const byHour = new Map<number, { requests: number; downgraded: number }>()
  for (const row of rows) {
    if (!matchesProvider(row, providerFilter)) continue
    const point = byHour.get(row.hour_start) ?? { requests: 0, downgraded: 0 }
    point.requests += row.requests
    if (isModelDowngrade(row)) point.downgraded += row.requests
    byHour.set(row.hour_start, point)
  }
  const hours = Math.max(0, Math.floor((end - start) / 3600) + 1)
  return Array.from({ length: hours }, (_, index) => {
    const hour = start + index * 3600
    const point = byHour.get(hour)
    return {
      hour_start: hour,
      requests: point?.requests ?? 0,
      downgraded: point?.downgraded ?? 0,
      downgrade_rate: point?.requests
        ? point.downgraded / point.requests
        : null,
    }
  })
}

export function modelDowngradeSankey(
  flows: ModelDowngradeFlow[],
  unknownProviderLabel: string
): {
  nodes: ModelDowngradeSankeyNode[]
  links: ModelDowngradeSankeyLink[]
} {
  const nodes: ModelDowngradeSankeyNode[] = []
  const index = new Map<string, number>()
  const links = new Map<string, ModelDowngradeSankeyLink>()
  const nodeIndex = (key: string, name: string) => {
    const existing = index.get(key)
    if (existing !== undefined) return existing
    const created = nodes.push({ name }) - 1
    index.set(key, created)
    return created
  }
  const addLink = (
    source: number,
    target: number,
    value: number,
    downgradedRequests: number
  ) => {
    const key = `${source}-${target}`
    const link = links.get(key) ?? {
      source,
      target,
      value: 0,
      downgradedRequests: 0,
    }
    link.value += value
    link.downgradedRequests += downgradedRequests
    links.set(key, link)
  }
  for (const flow of flows) {
    const downstream = nodeIndex(
      `downstream\u0000${flow.downstream_model}`,
      flow.downstream_model
    )
    const provider = nodeIndex(
      `provider\u0000${flow.provider_id ?? unknownProviderId}`,
      flow.provider_name ?? unknownProviderLabel
    )
    const upstream = nodeIndex(
      `upstream\u0000${flow.upstream_model}`,
      flow.upstream_model
    )
    const downgraded = isModelDowngrade(flow) ? flow.requests : 0
    addLink(downstream, provider, flow.requests, downgraded)
    addLink(provider, upstream, flow.requests, downgraded)
  }
  return { nodes, links: [...links.values()] }
}
