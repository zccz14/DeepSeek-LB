import assert from "node:assert/strict"
import test from "node:test"

import {
  parseWebSearchBound,
  parseWebSearchDomains,
  webSearchRequestBody,
} from "../src/lib/web-search.ts"

const emptyLocation = { country: "", region: "", city: "", timezone: "" }

test("reads a search bound only when it is a whole number in range", () => {
  assert.equal(parseWebSearchBound("1", 50), 1)
  assert.equal(parseWebSearchBound("50", 50), 50)
  assert.equal(parseWebSearchBound("0", 50), undefined)
  assert.equal(parseWebSearchBound("51", 50), undefined)
  assert.equal(parseWebSearchBound("2.5", 50), undefined)
  assert.equal(parseWebSearchBound("", 50), undefined)
  assert.equal(parseWebSearchBound("many", 50), undefined)
})

test("splits domain filters on commas and whitespace", () => {
  assert.deepEqual(parseWebSearchDomains(""), [])
  assert.deepEqual(parseWebSearchDomains(" , \n"), [])
  assert.deepEqual(parseWebSearchDomains("example.com, openai.com"), [
    "example.com",
    "openai.com",
  ])
})

test("builds a request with only the required search controls", () => {
  assert.deepEqual(
    webSearchRequestBody("  rust release  ", 8, 5, "  ", "", emptyLocation),
    { query: "rust release", max_results: 8, max_uses: 5 }
  )
})

test("copies domain filters and trims location fields", () => {
  assert.deepEqual(
    webSearchRequestBody(
      "rust release",
      20,
      2,
      "rust-lang.org,blog.rust-lang.org",
      "",
      {
        country: " CN ",
        region: " Shanghai ",
        city: "",
        timezone: " Asia/Shanghai ",
      }
    ),
    {
      query: "rust release",
      max_results: 20,
      max_uses: 2,
      allowed_domains: ["rust-lang.org", "blog.rust-lang.org"],
      user_location: {
        country: "CN",
        region: "Shanghai",
        timezone: "Asia/Shanghai",
      },
    }
  )
})
