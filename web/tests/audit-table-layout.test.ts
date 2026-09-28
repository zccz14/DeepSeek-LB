import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"

test("inference audit keeps its table columns aligned around user and consumer", async () => {
  const app = await readFile(new URL("../src/App.tsx", import.meta.url), "utf8")
  const audit = app.slice(
    app.indexOf("function AuditPage"),
    app.indexOf("function AuditDetailPage")
  )
  const header = audit.slice(
    audit.indexOf("<TableHeader>"),
    audit.indexOf("</TableHeader>")
  )
  const columns = [
    "{t.userLabel}",
    "{t.consumer}",
    "{t.time}",
    "{t.model}",
    "{t.downstreamUserAgent}",
    "{t.actualCost}",
    "{t.officialCost}",
    "{t.priceMultiplier}",
    "{t.provider}",
    "{t.sessionId}",
    "{t.status}",
    "{t.usage}",
    "{t.actions}",
  ]
  let previous = -1
  for (const column of columns) {
    const position = header.indexOf(column)
    assert.ok(position > previous, `${column} is in the expected column order`)
    previous = position
  }
  for (const hiddenColumn of [
    "downstreamModel",
    "upstreamModel",
    "officialConsumedUsdBefore",
    "officialConsumedUsdAfter",
    "actualConsumedUsdBefore",
    "actualConsumedUsdAfter",
    "officialProvidedUsdBefore",
    "officialProvidedUsdAfter",
    "actualProvidedUsdBefore",
    "actualProvidedUsdAfter",
  ]) {
    assert.doesNotMatch(header, new RegExp(`\\{t\\.${hiddenColumn}\\}`))
  }

  const rowStart = audit.indexOf("<TableRow key={row.id}>")
  const firstRow = audit.slice(rowStart, audit.indexOf("</TableRow>", rowStart))
  assert.match(
    firstRow,
    /<LinkitUserInfo[\s\S]*?<\/TableCell>\s*<TableCell className="font-medium">\s*\{row\.consumer_name\}\s*<\/TableCell>\s*<TableCell className="whitespace-nowrap">/
  )
  assert.ok(firstRow.indexOf("{t.cacheHitRate}") > firstRow.indexOf("{t.cachedInput}"))
  assert.match(
    firstRow,
    /cacheHitRate\(\s*row\.cached_tokens,\s*row\.input_tokens,\s*locale\s*\)/
  )
  assert.match(firstRow, /row\.upstream_model/)
  assert.match(firstRow, /auditTurnStateMayIndicateDowngrade\(row\)/)
  assert.match(firstRow, /bg-amber-500\/10/)
  assert.match(firstRow, /text-destructive/)
})
