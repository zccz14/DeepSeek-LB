const USD_NANOS = 1_000_000_000n

export function parseUsdNanos(value: string): number | null {
  const normalized = value.trim()
  if (!/^(?:0|[1-9]\d*)(?:\.\d{1,9})?$/.test(normalized)) return null
  const [whole, fraction = ""] = normalized.split(".")
  const nanos = BigInt(whole) * USD_NANOS + BigInt((fraction + "000000000").slice(0, 9))
  if (nanos <= 0n || nanos > BigInt(Number.MAX_SAFE_INTEGER)) return null
  return Number(nanos)
}

export function midasTransferUrl(recipientUserId: string, amountUsdNanos: number): string {
  const search = new URLSearchParams({ recipient_user_id: recipientUserId, amount_usd_nanos: String(amountUsdNanos) })
  return `https://midas.ntnl.io/#/transfer?${search}`
}
