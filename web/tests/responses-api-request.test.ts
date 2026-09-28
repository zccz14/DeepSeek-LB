import assert from "node:assert/strict"
import test from "node:test"

import {
  base64PayloadSize,
  contentText,
  imageUrl,
  inputItems,
  parseJsonValue,
  recordValue,
} from "../src/lib/responses-api-request.ts"

test("parses a retained Responses request body without changing object input", () => {
  const raw = '{"model":"gpt-5","input":[{"role":"user","content":"hello"}]}'
  const parsed = parseJsonValue(raw)
  assert.equal(recordValue(parsed)?.model, "gpt-5")
  assert.equal(inputItems(recordValue(parsed)?.input).length, 1)

  const original = { model: "gpt-5", input: "hello" }
  assert.equal(parseJsonValue(original), original)
})

test("extracts text and safe image sources from Responses input parts", () => {
  assert.equal(
    contentText({ type: "input_text", text: "Describe this image" }),
    "Describe this image"
  )
  assert.equal(
    imageUrl({ url: "https://example.com/image.png" }),
    "https://example.com/image.png"
  )
  assert.equal(
    imageUrl({ url: "data:image/png;base64,aGVsbG8=" }),
    "data:image/png;base64,aGVsbG8="
  )
  assert.equal(imageUrl({ url: "javascript:alert(1)" }), undefined)
})

test("measures base64 attachment payloads without exposing the payload", () => {
  assert.equal(base64PayloadSize("aGVsbG8="), 5)
  assert.equal(base64PayloadSize("data:application/pdf;base64,aGk="), 2)
  assert.equal(base64PayloadSize("not base64!"), undefined)
})

test("preserves additional_tools input entries and their tool definitions", () => {
  const request = recordValue(
    parseJsonValue(`{
      "input": [{
        "role": "developer",
        "type": "additional_tools",
        "tools": [
          {"type": "custom", "name": "exec", "format": "text"},
          {
            "type": "namespace",
            "name": "collaboration",
            "tools": [{"type": "function", "name": "spawn_agent"}]
          }
        ]
      }]
    }`)
  )
  const additionalTools = inputItems(request?.input)
    .map(recordValue)
    .find((item) => item?.type === "additional_tools")

  assert.equal(additionalTools?.role, "developer")
  assert.equal(Array.isArray(additionalTools?.tools), true)
  assert.equal((additionalTools?.tools as unknown[]).length, 2)
  assert.equal(
    ((additionalTools?.tools as unknown[])[1] as { tools: unknown[] }).tools
      .length,
    1
  )
})
