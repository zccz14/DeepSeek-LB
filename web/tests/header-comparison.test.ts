import assert from "node:assert/strict"
import test from "node:test"
import { compareHeaderSnapshots } from "../src/lib/header-comparison.ts"

const upstream = JSON.stringify([
  ["Content-Type", "text/event-stream"],
  ["x-codex-turn-state", "turn-1"],
])

test("missing downstream diagnostics do not report every upstream header as removed", () => {
  const result = compareHeaderSnapshots(upstream, null)
  assert.equal(result.leftAvailable, true)
  assert.equal(result.rightAvailable, false)
  assert.equal(result.rows.length, 2)
  assert.ok(result.rows.every((row) => !row.differs && row.left !== undefined))
})

test("missing upstream diagnostics do not report downstream headers as added", () => {
  const result = compareHeaderSnapshots(undefined, upstream)
  assert.equal(result.leftAvailable, false)
  assert.equal(result.rightAvailable, true)
  assert.ok(result.rows.every((row) => !row.differs))
})

test("a captured empty snapshot still reports genuinely removed headers", () => {
  const result = compareHeaderSnapshots(upstream, "[]")
  assert.equal(result.rightAvailable, true)
  assert.ok(result.rows.every((row) => row.differs))
})

test("compares available values case-insensitively and retains real differences", () => {
  const result = compareHeaderSnapshots(
    upstream,
    JSON.stringify([
      ["content-type", "text/event-stream"],
      ["x-codex-turn-state", "turn-2"],
    ])
  )
  assert.deepEqual(
    result.rows.map((row) => row.differs),
    [false, true]
  )
})

test("unreadable and absent snapshots remain unavailable", () => {
  for (const missing of [
    null,
    undefined,
    "",
    "{broken",
    "null",
    "[null]",
    "{}",
  ]) {
    const result = compareHeaderSnapshots(upstream, missing)
    assert.equal(result.rightAvailable, false)
    assert.ok(result.rows.every((row) => !row.differs))
  }
})
