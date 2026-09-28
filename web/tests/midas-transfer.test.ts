import assert from "node:assert/strict"
import test from "node:test"

import { midasTransferUrl, parseUsdNanos } from "../src/lib/midas-transfer.ts"

test("parses USD into exact nanodollars without floating point", () => {
  assert.equal(parseUsdNanos("12.345678901"), 12_345_678_901)
  assert.equal(parseUsdNanos("0.000000001"), 1)
  assert.equal(parseUsdNanos("12.3456789012"), null)
  assert.equal(parseUsdNanos("0"), null)
})

test("builds the Midas hash transfer URL with the exact nanodollar amount", () => {
  assert.equal(
    midasTransferUrl("f02211a4-2bb1-4a87-a70a-8f08a306cd05", 12_345_678_901),
    "https://midas.ntnl.io/#/transfer?recipient_user_id=f02211a4-2bb1-4a87-a70a-8f08a306cd05&amount_usd_nanos=12345678901",
  )
})
