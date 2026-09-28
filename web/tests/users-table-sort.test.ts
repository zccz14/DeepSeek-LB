import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"
import { fileURLToPath } from "node:url"
import {
  createTable,
  getCoreRowModel,
  getSortedRowModel,
  type ColumnDef,
  type SortingState,
} from "@tanstack/react-table"

import {
  isUserTableNumericColumn,
  userTableSortValue,
  type UserTableSortRecord,
} from "../src/lib/user-table-sort.ts"

const appPath = fileURLToPath(new URL("../src/App.tsx", import.meta.url))

const user = {
  id: "user-id",
  role: "admin",
  allow_debt: true,
  created_at: 1_700_000_000,
  topup_usd_nanos: 3_000_000,
  consumed_usd_nanos: 2_000_000,
  provided_usd_nanos: 4_000_000,
  available_usd_nanos: 1_000_000,
}

test("returns raw user table values for numeric and text sorting", () => {
  assert.equal(userTableSortValue(user, "created_at"), 1_700_000_000)
  assert.equal(userTableSortValue(user, "available_usd_nanos"), 1_000_000)
  assert.equal(userTableSortValue(user, "allow_debt"), 1)
  assert.equal(isUserTableNumericColumn("created_at"), false)
  assert.equal(isUserTableNumericColumn("available_usd_nanos"), true)
  assert.equal(isUserTableNumericColumn("role"), false)
})

test("sorts user rows by their raw numeric values", () => {
  const data: UserTableSortRecord[] = [
    { ...user, id: "first", available_usd_nanos: 10_000_000 },
    { ...user, id: "second", available_usd_nanos: 1_000_000 },
  ]
  const columns: ColumnDef<UserTableSortRecord>[] = [
    {
      id: "available_usd_nanos",
      accessorFn: (row) => userTableSortValue(row, "available_usd_nanos"),
    },
  ]
  const sorting: SortingState = [{ id: "available_usd_nanos", desc: false }]
  const table = createTable({
    data,
    columns,
    state: { sorting },
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
  })

  assert.deepEqual(
    table.getRowModel().rows.map((row) => row.original.id),
    ["second", "first"]
  )
})

test("connects the users table to TanStack sorting with accessible headers", async () => {
  const source = await readFile(appPath, "utf8")

  assert.match(source, /function UsersPage[\s\S]*getSortedRowModel\(\)/)
  assert.match(source, /function UsersPage[\s\S]*onSortingChange: setSorting/)
  assert.match(
    source,
    /function UsersPage[\s\S]*useState<SortingState>\(\[\s*\{ id: "consumed_usd_nanos", desc: true \},?\s*\]\)/
  )
  assert.match(
    source,
    /function UsersPage[\s\S]*userTableSortValue\(row, "available_usd_nanos"\)/
  )
  assert.match(source, /function UsersPage[\s\S]*aria-sort=/)
  assert.match(
    source,
    /id: "user",\s*header: t\.userLabel,\s*enableSorting: false,/
  )
})
