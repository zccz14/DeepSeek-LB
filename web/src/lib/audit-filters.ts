export type AuditFilters = {
  user_id: string
  consumer: string
  provider: string
  model: string
  error_code: string
  status: "all" | "success" | "error"
}

const auditFilterKeys = [
  "user_id",
  "consumer",
  "provider",
  "model",
  "error_code",
] as const

export const emptyAuditFilters: AuditFilters = {
  user_id: "",
  consumer: "",
  provider: "",
  model: "",
  error_code: "",
  status: "all",
}

export function isAuditError(
  status: number,
  error?: string | null,
  errorCode?: string | null
) {
  const clientCancelled =
    status === 499 && error === "client_cancelled" && errorCode == null
  return (
    !clientCancelled && (status >= 400 || error != null || errorCode != null)
  )
}

export function auditFiltersFromSearch(search: string): AuditFilters {
  const params = new URLSearchParams(search)
  const status = params.get("status")

  return {
    ...emptyAuditFilters,
    ...Object.fromEntries(
      auditFilterKeys.flatMap((key) => {
        const value = params.get(key)
        return value ? [[key, value]] : []
      })
    ),
    status: status === "success" || status === "error" ? status : "all",
  }
}

export function auditFilterSearch(filters: AuditFilters) {
  const params = new URLSearchParams()

  for (const key of auditFilterKeys) {
    if (filters[key]) params.set(key, filters[key])
  }
  if (filters.status !== "all") params.set("status", filters.status)

  const search = params.toString()
  return search ? `?${search}` : ""
}
