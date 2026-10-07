import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"

const app = await readFile(new URL("../src/App.tsx", import.meta.url), "utf8")

test("puts the web search tool in its own navigation group and route", () => {
  const tools = app.slice(
    app.indexOf("label: t.navigationTools"),
    app.indexOf("label: t.navigationData")
  )
  const data = app.slice(
    app.indexOf("label: t.navigationData"),
    app.indexOf("...(isAdministrator")
  )

  assert.match(tools, /\["web-search", SearchIcon\]/)
  assert.doesNotMatch(data, /web-search/)
  assert.match(app, /path="\/web-search"[\s\S]*<WebSearchPage/)
  assert.match(app, /"web-search": t\.pageWebSearch/)
  assert.match(app, /pageWebSearch:/)
})

test("runs console searches with the native search controls", () => {
  const page = app.slice(
    app.indexOf("function WebSearchPage"),
    app.indexOf("function Dashboard")
  )

  assert.match(page, /api<WebSearchEnvelope>\(sdk, "\/api\/web-search"/)
  assert.match(page, /webSearchRequestBody\(/)
  assert.match(page, /parseWebSearchBound\(maxResults, webSearchMaxResultsLimit\)/)
  assert.match(page, /parseWebSearchBound\(maxUses, webSearchMaxUsesLimit\)/)
  assert.match(page, /domainsConflict/)
  assert.match(page, /result\.queries\.map/)
  assert.match(page, /result\.truncated &&/)
  assert.match(page, /result\.usage\.input_tokens/)
  assert.match(page, /result\.usage\.cached_tokens/)
  assert.match(page, /source\.published_at/)
  assert.match(page, /source\.snippet/)
  assert.match(page, /t\.webSearchAuditLink/)
  assert.match(page, /t\.webSearching/)
})

test("keeps the search host model on the administrator settings", () => {
  const settings = app.slice(
    app.indexOf("function WebSearchSettings"),
    app.indexOf("function ProviderConcurrencySettings")
  )

  assert.match(settings, /api\(sdk, "\/api\/settings\/web-search"/)
  assert.match(settings, /web_search_model: model/)
  assert.match(settings, /\/anthropic\/v1\/messages/)

  const settingsPage = app.slice(
    app.indexOf("function SettingsPage"),
    app.indexOf("function WebSearchSettings")
  )
  assert.match(settingsPage, /initial=\{data\.web_search_model\}/)
  assert.match(settingsPage, /upstreamBase=\{data\.upstream_base\}/)
})
