import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"

const app = await readFile(new URL("../src/App.tsx", import.meta.url), "utf8")
const modelDowngradeSankey = await readFile(
  new URL("../src/components/model-downgrade-sankey.tsx", import.meta.url),
  "utf8"
)
const modelDowngradeRateChart = await readFile(
  new URL("../src/components/model-downgrade-rate-chart.tsx", import.meta.url),
  "utf8"
)

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

test("places the model downgrade audit behind the administrator navigation and route", () => {
  const administration = app.slice(
    app.indexOf("label: t.navigationAdministration"),
    app.indexOf("function toggleLocale")
  )

  assert.match(administration, /\["model-downgrade-audit", WorkflowIcon\]/)
  assert.match(
    app,
    /path="\/model-downgrade-audit"[\s\S]*<ModelDowngradeAuditPage/
  )
  assert.match(app, /function ModelDowngradeAuditPage\(/)
  assert.match(app, /model-downgrade-audit\?period=/)
  assert.match(app, /<ModelDowngradeSankey[\s\S]*links=\{sankey\.links\}/)
  assert.match(
    modelDowngradeSankey,
    /<Sankey[\s\S]*data=\{\{ nodes, links \}\}/
  )
  assert.match(app, /<ModelDowngradeRateChart[\s\S]*points=\{ratePoints\}/)
  assert.match(modelDowngradeRateChart, /dataKey="downgrade_rate"/)
  assert.match(modelDowngradeRateChart, /domain=\{\[0, 1\]\}/)
  assert.match(app, /modelDowngradeRateChart/)
  assert.match(app, /modelDowngradeDowngradedRequests/)
})

test("places the experimental 312 turn-state filter in administrator settings", () => {
  const settings = app.slice(
    app.indexOf("function SettingsPage"),
    app.indexOf("function MidasSettings")
  )

  assert.match(
    settings,
    /user\.role === "root" \|\| user\.role === "admin"[\s\S]*<ExperimentalTurnState312FilterSettings/
  )
  assert.match(
    settings,
    /\/api\/settings\/experimental-turn-state-312-filter/
  )
  assert.match(settings, /JSON\.stringify\(\{ enabled \}\)/)
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
})

test("shows the transcription model and token ratios in the account overview", () => {
  const dashboard = app.slice(
    app.indexOf("function Dashboard"),
    app.indexOf("function ModelPricesPage")
  )
  const transcriptions = app.slice(
    app.indexOf("function TranscriptionsPage"),
    app.indexOf("type RealtimeSession")
  )

  assert.match(transcriptions, /t\.transcriptionModelId/)
  assert.match(app, /transcriptionModelId: "gpt-4o-transcribe"/)
  assert.match(dashboard, /t\.accountOverview/)
  assert.match(dashboard, /t\.cacheRate[\s\S]*cacheHitRate\(/)
  assert.match(dashboard, /t\.inputOutputRatio[\s\S]*inputOutputRatio\(/)
  assert.doesNotMatch(dashboard, /<CardTitle>\{t\.operationalStatus\}/)
})
