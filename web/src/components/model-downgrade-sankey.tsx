import {
  Sankey,
  Tooltip as ChartTooltip,
  type SankeyLinkProps,
  type SankeyNodeProps,
} from "recharts"

import { ChartContainer, type ChartConfig } from "@/components/ui/chart"
import type {
  ModelDowngradeSankeyLink,
  ModelDowngradeSankeyNode,
} from "@/lib/model-downgrade-audit"

function SankeyNodeShape({ x, y, width, height, payload }: SankeyNodeProps) {
  const labelLeft = payload.depth === 2
  return (
    <g>
      <rect
        fill="var(--chart-2)"
        height={height}
        rx={2}
        width={width}
        x={x}
        y={y}
      />
      <text
        className="fill-muted-foreground text-xs"
        dominantBaseline="middle"
        textAnchor={labelLeft ? "end" : "start"}
        x={labelLeft ? x - 6 : x + width + 6}
        y={y + height / 2}
      >
        {payload.name}
      </text>
    </g>
  )
}

type SankeyLinkData = { downgradedRequests: number }

// recharts forwards the sankey link object as the renderer `payload` and nests the same
// object one level deeper inside the tooltip entry; neither is part of its SankeyLink type.
function rendererLinkData(payload: SankeyLinkProps["payload"]) {
  return payload as unknown as SankeyLinkData
}

function tooltipLinkData(item: { payload?: unknown }) {
  return (item.payload as { payload?: SankeyLinkData } | undefined)
    ?.payload as SankeyLinkData
}

function SankeyLinkShape({
  sourceX,
  sourceY,
  sourceControlX,
  targetX,
  targetY,
  targetControlX,
  linkWidth,
  payload,
}: SankeyLinkProps) {
  const downgraded = rendererLinkData(payload).downgradedRequests > 0
  return (
    <path
      d={`M${sourceX},${sourceY}C${sourceControlX},${sourceY} ${targetControlX},${targetY} ${targetX},${targetY}`}
      fill="none"
      stroke={downgraded ? "var(--destructive)" : "var(--chart-3)"}
      strokeOpacity={downgraded ? 0.6 : 0.25}
      strokeWidth={Math.max(1, linkWidth)}
    />
  )
}

export function ModelDowngradeSankey({
  nodes,
  links,
  config,
  requestsLabel,
  downgradedLabel,
  locale,
}: {
  nodes: ModelDowngradeSankeyNode[]
  links: ModelDowngradeSankeyLink[]
  config: ChartConfig
  requestsLabel: string
  downgradedLabel: string
  locale: string
}) {
  return (
    <ChartContainer config={config} className="aspect-auto h-96 w-full">
      <Sankey
        data={{ nodes, links }}
        link={SankeyLinkShape}
        margin={{ top: 8, right: 160, bottom: 8, left: 8 }}
        nameKey="name"
        node={SankeyNodeShape}
        nodePadding={16}
        nodeWidth={12}
      >
        <ChartTooltip
          contentStyle={{
            backgroundColor: "var(--popover)",
            borderColor: "var(--border)",
            borderRadius: "var(--radius)",
            color: "var(--popover-foreground)",
          }}
          formatter={(value, _name, item) => {
            const requests = `${Number(value).toLocaleString(locale)} ${requestsLabel}`
            const downgraded = tooltipLinkData(item).downgradedRequests
            return downgraded
              ? `${requests} · ${downgraded.toLocaleString(locale)} ${downgradedLabel}`
              : requests
          }}
          itemStyle={{ color: "var(--popover-foreground)" }}
        />
      </Sankey>
    </ChartContainer>
  )
}
