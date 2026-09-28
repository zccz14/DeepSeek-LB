import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"

const app = await readFile(new URL("../src/App.tsx", import.meta.url), "utf8")

test("places management audit on its administrator page and keeps inference audit focused", () => {
  const audit = app.slice(
    app.indexOf("function AuditPage"),
    app.indexOf("function CopyableIdentifier")
  )

  assert.match(app, /\["admin-audit", ShieldCheckIcon\]/)
  assert.match(app, /path="\/admin-audit"[\s\S]*<AdminAuditPage/)
  assert.match(app, /function AdminAuditPage\(/)
  assert.doesNotMatch(audit, /AdminAuditPage|AdminAuditSection/)
})

test("places provider audit behind the administrator navigation and route", () => {
  assert.match(app, /\["provider-audit", ActivityIcon\]/)
  assert.match(app, /path="\/provider-audit"[\s\S]*<ProviderAuditPage/)
  assert.match(app, /function ProviderAuditPage\(/)
  assert.match(app, /providerAuditSuccessRate/)
  assert.match(app, /providerAuditFailureRate/)
  assert.match(app, /providerAuditFailureRateChart/)
  assert.match(app, /providerAuditRequestCountChart/)
  assert.match(app, /providerAuditInputTokensChart/)
  assert.match(app, /provider-audit\?period=/)
  assert.match(app, /<TabsTrigger value="24h">/)
  assert.match(app, /dataKey="requests"/)
  assert.match(app, /dataKey="input_tokens"/)
})

test("exposes only the DeepSeek surface in the sidebar navigation", () => {
  const navigation = app.slice(
    app.indexOf("const navigationGroups"),
    app.indexOf("function toggleLocale")
  )
  assert.doesNotMatch(
    navigation,
    /transcriptions|realtime|images|model-downgrade-audit/
  )
  assert.match(app, /\["codex-integration", PencilIcon\]/)
  assert.match(app, /\["dsh-integration", KeyRoundIcon\]/)
  assert.match(app, /\["opencode-integration", KeyRoundIcon\]/)
  assert.match(app, /\["direct-api-integration", BookOpenIcon\]/)
})

test("keeps top-ups in the workspace navigation group", () => {
  const workspace = app.slice(
    app.indexOf("label: t.navigationWorkspace"),
    app.indexOf("label: t.navigationIntegrations")
  )
  const data = app.slice(
    app.indexOf("label: t.navigationData"),
    app.indexOf("...(isAdministrator")
  )

  assert.match(workspace, /\["topups", WalletCardsIcon\]/)
  assert.doesNotMatch(data, /\["topups", WalletCardsIcon\]/)
})

test("keeps consumer actions beside their identifying columns with rotation confirmation", () => {
  const consumers = app.slice(
    app.indexOf("function Consumers"),
    app.indexOf("function UsagePage")
  )

  assert.match(consumers, /<TableHead>\{t\.apiKey\}<\/TableHead>/)
  assert.match(
    consumers,
    /<TableCell className="font-medium">[\s\S]*?openEdit\(consumer\)[\s\S]*?<\/TableCell>\s*<TableCell className="font-mono text-xs">[\s\S]*?setRotateTarget\(consumer\)/
  )
  assert.match(
    consumers,
    /<AlertDialogTitle>\{t\.rotateConsumerTitle\}<\/AlertDialogTitle>/
  )
  assert.match(consumers, /event\.preventDefault\(\)\s*void confirmRotate\(\)/)
  assert.match(consumers, /<Trash2Icon data-icon="inline-start" \/>/)
  assert.doesNotMatch(consumers, /intercept_degradation/)
})

test("shows the billing window and token ratios in the account overview", () => {
  const dashboard = app.slice(
    app.indexOf("function Dashboard"),
    app.indexOf("function ModelPricesPage")
  )

  assert.match(dashboard, /t\.accountOverview/)
  assert.match(dashboard, /t\.pricingTariffTitle/)
  assert.match(dashboard, /data\?\.peak_now \? t\.pricingPeak : t\.pricingOffPeak/)
  assert.match(dashboard, /t\.cacheRate[\s\S]*cacheHitRate\(/)
  assert.match(dashboard, /t\.inputOutputRatio[\s\S]*inputOutputRatio\(/)
  assert.doesNotMatch(dashboard, /<CardTitle>\{t\.operationalStatus\}/)
})
