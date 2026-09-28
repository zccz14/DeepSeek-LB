import assert from "node:assert/strict"
import test from "node:test"
import { api } from "../src/lib/api.ts"

const sdk = {
  session: {
    getState: () => ({ accessToken: "session-token" }),
    refresh: async () => ({ accessToken: "session-token" }),
  },
} as unknown as Parameters<typeof api>[0]

function withFetch(
  handler: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>
) {
  const original = globalThis.fetch
  globalThis.fetch = handler as typeof fetch
  return () => {
    globalThis.fetch = original
  }
}

test("returns parsed JSON and attaches the access token", async () => {
  let authorization = ""
  const restore = withFetch(async (_input, init) => {
    authorization = new Headers(init?.headers).get("authorization") ?? ""
    return new Response(JSON.stringify({ ok: true }), { status: 200 })
  })
  try {
    const payload = await api(sdk, "/api/example")
    assert.deepEqual(payload, { ok: true })
    assert.equal(authorization, "Bearer session-token")
  } finally {
    restore()
  }
})

test("surfaces the JSON error message and reason", async () => {
  const restore = withFetch(
    async () =>
      new Response(
        JSON.stringify({ error: { message: "boom", reason: "bad-input" } }),
        {
          status: 400,
        }
      )
  )
  try {
    await assert.rejects(
      () => api(sdk, "/api/example"),
      (error: Error & { reason?: string }) => {
        assert.equal(error.message, "boom")
        assert.equal(error.reason, "bad-input")
        return true
      }
    )
  } finally {
    restore()
  }
})

test("reports HTML error pages with the status instead of a JSON parse error", async () => {
  const restore = withFetch(
    async () =>
      new Response("<html><head><title>502 Bad Gateway</title></head></html>", {
        status: 502,
      })
  )
  try {
    await assert.rejects(
      () => api(sdk, "/api/example"),
      (error: Error) => {
        assert.equal(error.message, "Request failed (502)")
        assert.ok(!error.message.includes("Unexpected token"))
        return true
      }
    )
  } finally {
    restore()
  }
})

test("reports non-JSON success bodies clearly", async () => {
  const restore = withFetch(
    async () => new Response("<html></html>", { status: 200 })
  )
  try {
    await assert.rejects(
      () => api(sdk, "/api/example"),
      (error: Error) => {
        assert.equal(error.message, "Response was not JSON (HTTP 200)")
        return true
      }
    )
  } finally {
    restore()
  }
})
