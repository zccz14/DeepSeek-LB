import assert from "node:assert/strict"
import test from "node:test"

import {
  auditFilterSearch,
  auditFiltersFromSearch,
  emptyAuditFilters,
  isAuditError,
} from "../src/lib/audit-filters.ts"

test("treats a client-cancelled 499 as a normal audit result", () => {
  assert.equal(isAuditError(499, "client_cancelled", null), false)
  assert.equal(isAuditError(499, "client_cancelled"), false)
  assert.equal(isAuditError(499, "client_cancelled", "upstream_failure"), true)
  assert.equal(isAuditError(499, "other_error", null), true)
})

test("restores audit filters from a HashRouter route search", () => {
  assert.deepEqual(
    auditFiltersFromSearch(
      "?user_id=user-1&consumer=desktop+app&provider=provider-1&model=gpt-5&error_code=server_is_overloaded&status=error"
    ),
    {
      user_id: "user-1",
      consumer: "desktop app",
      provider: "provider-1",
      model: "gpt-5",
      error_code: "server_is_overloaded",
      status: "error",
    }
  )
})

test("ignores unsupported audit filter parameters and statuses", () => {
  assert.deepEqual(
    auditFiltersFromSearch("?status=429&unexpected=value"),
    emptyAuditFilters
  )
})

test("serializes only active audit filters", () => {
  const search = auditFilterSearch({
    user_id: "user-1",
    consumer: "",
    provider: "provider-1",
    model: "",
    error_code: "",
    status: "success",
  })

  assert.deepEqual(Object.fromEntries(new URLSearchParams(search)), {
    user_id: "user-1",
    provider: "provider-1",
    status: "success",
  })
  assert.equal(auditFilterSearch(emptyAuditFilters), "")
})
