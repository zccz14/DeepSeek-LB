import assert from "node:assert/strict"
import test from "node:test"

import { auditImageResponses } from "../src/lib/audit-image-response.ts"

test("renders Images API b64_json response data with the requested image format", () => {
  const images = auditImageResponses(
    JSON.stringify({
      data: [
        {
          b64_json: "iVBORw0KGgo=",
          revised_prompt: "A precise diagram",
        },
      ],
    }),
    JSON.stringify({ output_format: "png" })
  )

  assert.deepEqual(images, [
    {
      src: "data:image/png;base64,iVBORw0KGgo=",
      revisedPrompt: "A precise diagram",
    },
  ])
})

test("uses JPEG and WebP request formats and ignores malformed image data", () => {
  const response = JSON.stringify({
    data: [
      { b64_json: "aGVsbG8=" },
      { b64_json: "not an image" },
    ],
  })

  assert.equal(
    auditImageResponses(response, JSON.stringify({ output_format: "jpeg" }))[0]
      ?.src,
    "data:image/jpeg;base64,aGVsbG8="
  )
  assert.equal(
    auditImageResponses(response, JSON.stringify({ output_format: "webp" }))[0]
      ?.src,
    "data:image/webp;base64,aGVsbG8="
  )
  assert.equal(auditImageResponses(response).length, 1)
})
