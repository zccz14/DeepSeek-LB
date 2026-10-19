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

test("runs console searches with the search API controls", () => {
  const page = app.slice(
    app.indexOf("function WebSearchPage"),
    app.indexOf("function Dashboard")
  )

  assert.match(page, /api<WebSearchEnvelope>\(sdk, "\/api\/web-search"/)
  assert.match(page, /webSearchRequestBody\(/)
  assert.match(page, /parseWebSearchBound\(maxResults, webSearchMaxResultsLimit\)/)
  assert.match(page, /domainsConflict/)
  assert.match(page, /needContent/)
  assert.match(page, /result\.sources\.map/)
  assert.match(page, /source\.summary/)
  assert.match(page, /source\.published_at/)
  assert.match(page, /source\.snippet/)
  assert.match(page, /t\.webSearchAuditLink/)
  assert.match(page, /t\.webSearching/)
  assert.doesNotMatch(
    page,
    /max_uses|user_location|result\.usage|result\.queries|result\.truncated/
  )
})

test("the console envelope carries sources only", () => {
  const envelope = app.slice(
    app.indexOf("type WebSearchEnvelope = {"),
    app.indexOf("const webSearchAuditModel")
  )

  assert.match(envelope, /sources: WebSearchSource\[\]/)
  assert.doesNotMatch(envelope, /queries|truncated|usage/)
})

test("no admin setting claims a search model", () => {
  assert.doesNotMatch(app, /web_search_model|settings\/web-search/)
})
