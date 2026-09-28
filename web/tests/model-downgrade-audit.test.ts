import assert from "node:assert/strict"
import test from "node:test"

import {
  isModelDowngrade,
  modelDowngradeFlows,
  modelDowngradeRatePoints,
  modelDowngradeSankey,
  unknownProviderId,
  type ModelDowngradeRow,
} from "../src/lib/model-downgrade-audit.ts"

function namedLinks(
  nodes: { name: string }[],
  links: {
    source: number
    target: number
    value: number
    downgradedRequests: number
  }[]
) {
  return links
    .map((link) => [
      nodes[link.source].name,
      nodes[link.target].name,
      link.value,
      link.downgradedRequests,
    ])
    .sort((left, right) =>
      String(left[0] + left[1]).localeCompare(String(right[0] + right[1]))
    )
}

const rows: ModelDowngradeRow[] = [
  {
    hour_start: 3600,
    provider_id: "m1",
    provider_name: "m1",
    downstream_model: "gpt-6-astra",
    upstream_model: "gpt-5.6-luna",
    requests: 3,
  },
  {
    hour_start: 3600,
    provider_id: "m1",
    provider_name: "m1",
    downstream_model: "gpt-6-astra",
    upstream_model: "gpt-6-astra",
    requests: 1,
  },
  {
    hour_start: 7200,
    provider_id: null,
    provider_name: null,
    downstream_model: "gpt-5.6-sol",
    upstream_model: "gpt-5.6-sol",
    requests: 2,
  },
]

test("treats only a different upstream model as a downgrade", () => {
  assert.equal(
    isModelDowngrade({
      downstream_model: "gpt-6-astra",
      upstream_model: "gpt-5.6-luna",
    }),
    true
  )
  assert.equal(
    isModelDowngrade({
      downstream_model: "gpt-6-astra",
      upstream_model: "gpt-6-astra",
    }),
    false
  )
})

test("aggregates flows per provider, upstream model, and downstream model", () => {
  assert.deepEqual(
    modelDowngradeFlows(rows, "all").map((flow) => [
      flow.provider_id,
      flow.upstream_model,
      flow.downstream_model,
      flow.requests,
    ]),
    [
      ["m1", "gpt-5.6-luna", "gpt-6-astra", 3],
      [null, "gpt-5.6-sol", "gpt-5.6-sol", 2],
      ["m1", "gpt-6-astra", "gpt-6-astra", 1],
    ]
  )
  assert.equal(modelDowngradeFlows(rows, "m1").length, 2)
  assert.deepEqual(
    modelDowngradeFlows(rows, unknownProviderId).map((flow) => flow.requests),
    [2]
  )
})

test("keeps one rate point per hour and leaves unobserved hours empty", () => {
  const points = modelDowngradeRatePoints(rows, "all", 0, 7200)
  assert.deepEqual(
    points.map((point) => [
      point.hour_start,
      point.requests,
      point.downgraded,
      point.downgrade_rate,
    ]),
    [
      [0, 0, 0, null],
      [3600, 4, 3, 0.75],
      [7200, 2, 0, 0],
    ]
  )
  assert.deepEqual(
    modelDowngradeRatePoints(rows, "m1", 0, 7200).map(
      (point) => point.downgrade_rate
    ),
    [null, 0.75, null]
  )
})

test("builds a downstream to provider to upstream sankey with downgraded links", () => {
  const { nodes, links } = modelDowngradeSankey(
    modelDowngradeFlows(rows, "all"),
    "unknown provider"
  )
  const named = namedLinks(nodes, links)
  assert.deepEqual(named, [
    ["gpt-5.6-sol", "unknown provider", 2, 0],
    ["gpt-6-astra", "m1", 4, 3],
    ["m1", "gpt-5.6-luna", 3, 3],
    ["m1", "gpt-6-astra", 1, 0],
    ["unknown provider", "gpt-5.6-sol", 2, 0],
  ])
  assert.equal(nodes.length, 7)
})

test("merges flows that share a sankey link", () => {
  const { nodes, links } = modelDowngradeSankey(
    modelDowngradeFlows(
      [
        ...rows.slice(0, 1),
        {
          hour_start: 7200,
          provider_id: "m1",
          provider_name: "m1",
          downstream_model: "gpt-5.6-sol",
          upstream_model: "gpt-5.6-luna",
          requests: 4,
        },
      ],
      "all"
    ),
    "unknown provider"
  )
  assert.deepEqual(namedLinks(nodes, links), [
    ["gpt-5.6-sol", "m1", 4, 4],
    ["gpt-6-astra", "m1", 3, 3],
    ["m1", "gpt-5.6-luna", 7, 7],
  ])
})
