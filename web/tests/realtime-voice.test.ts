import assert from "node:assert/strict"
import test from "node:test"
import { readFile } from "node:fs/promises"

const app = await readFile(new URL("../src/App.tsx", import.meta.url), "utf8")

test("exposes a WebRTC realtime voice tool backed by the public calls route", () => {
  assert.match(app, /\["realtime", RadioIcon\]/)
  assert.match(app, /path="\/realtime"/)
  assert.match(app, /navigator\.mediaDevices\.getUserMedia/)
  assert.match(app, /new RTCPeerConnection\(\)/)
  assert.match(app, /"\/api\/realtime\/calls"/)
  assert.match(app, /"x-session-id": crypto\.randomUUID\(\)/)
  assert.match(app, /type: "realtime"/)
  assert.match(app, /output_modalities: \["audio"\]/)
  assert.match(app, /noise_reduction: \{ type: "near_field" \}/)
  assert.match(app, /POST \/v1\/realtime\/calls/)
  assert.match(app, /POST https:\/\/api\.openai\.com\/v1\/realtime\/calls/)
})
