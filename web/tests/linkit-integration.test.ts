import assert from "node:assert/strict"
import test from "node:test"
import { readFile } from "node:fs/promises"

test("console delegates Linkit identity UI to the published components", async () => {
  const app = await readFile(new URL("../src/App.tsx", import.meta.url), "utf8")
  const main = await readFile(new URL("../src/main.tsx", import.meta.url), "utf8")
  const indexHtml = await readFile(new URL("../index.html", import.meta.url), "utf8")
  const manifest = JSON.parse(
    await readFile(new URL("../package.json", import.meta.url), "utf8")
  ) as { dependencies: Record<string, string> }

  assert.equal(manifest.dependencies["linkit-react-components"], "0.6.0")
  assert.equal(manifest.dependencies["next-themes"], undefined)
  assert.doesNotMatch(main, /theme-provider/)
  assert.match(indexHtml, /localStorage\.getItem\("linkit\.theme"\)/)
  assert.match(app, /function LinkitToaster[\s\S]{0,160}?theme=\{resolvedTheme\}/)
  assert.match(main, /import "linkit-react-components\/styles\.css"/)
  assert.match(app, /<LinkitProvider linkitBaseUrl="https:\/\/linkit\.ntnl\.io" lang=\{locale\}>/)
  assert.match(app, /audiences=\{authMiniAudiences\(config\.auth_audience\)\}/)
  assert.match(app, /audiences=\{authMiniAudiences\(audience\.trim\(\)\)\}/)
  assert.match(app, /function authMiniAudiences[\s\S]{0,160}?linkit\.ntnl\.io/)
  assert.doesNotMatch(app, /audience=\{audience\.trim\(\)\}/)
  assert.match(app, /<LinkitMyInfo \/>/)
  assert.match(app, /<LinkitLanguageSync setLocale=\{changeLocale\} \/>/)
  assert.match(app, /<LinkitUserInfo/)
  assert.match(app, /<LinkitUserPicker/)
  assert.doesNotMatch(app, /AuthMiniButton/)
  assert.doesNotMatch(app, /display_name/)
  assert.doesNotMatch(app, /function UserDisplay/)
})

test("inference audit leads with one Linkit identity column and users do not duplicate IDs", async () => {
  const app = await readFile(new URL("../src/App.tsx", import.meta.url), "utf8")
  const audit = app.slice(app.indexOf("function AuditPage"), app.indexOf("function AuditDetailPage"))
  const users = app.slice(app.indexOf("function UsersPage"), app.indexOf("function AdminAuditPage"))

  assert.match(
    audit,
    /<TableHead>\{t\.userLabel\}<\/TableHead>\s*<TableHead>\{t\.consumer\}<\/TableHead>\s*<TableHead>\{t\.time\}/
  )
  assert.match(audit, /<TableRow key=\{row\.id\}>\s*<TableCell>\s*<LinkitUserInfo/)
  assert.match(
    audit,
    /<LinkitUserInfo[\s\S]*?<\/TableCell>\s*<TableCell className="font-medium">\s*\{row\.consumer_name\}/
  )
  assert.equal((audit.match(/userId=\{row\.user_id\}/g) ?? []).length, 1)
  assert.match(
    users,
    /id: "user",[\s\S]*header: t\.userLabel,[\s\S]*id: "created_at",[\s\S]*header: t\.createdAt/
  )
  assert.equal((users.match(/<code>\{item\.id\}<\/code>/g) ?? []).length, 0)
  assert.equal((users.match(/<LinkitUserInfo/g) ?? []).length, 1)
})

test("the data navigation and page copy call this view inference audit", async () => {
  const app = await readFile(new URL("../src/App.tsx", import.meta.url), "utf8")
  assert.match(app, /audit: "推理审计"/)
  assert.match(app, /audit: "Inference audit"/)
  assert.match(app, /auditTitle: "推理审计"/)
  assert.match(app, /auditTitle: "Inference audit"/)
  assert.match(app, /backToAudit: "返回推理审计"/)
  assert.match(app, /backToAudit: "Back to inference audit"/)
})
