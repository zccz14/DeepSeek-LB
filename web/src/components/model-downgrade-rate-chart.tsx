import {
  CartesianGrid,
  Line,
  LineChart,
  Tooltip as ChartTooltip,
  XAxis,
  YAxis,
} from "recharts"

import { ChartContainer, type ChartConfig } from "@/components/ui/chart"
import type { ModelDowngradeRatePoint } from "@/lib/model-downgrade-audit"
import { formatAuditHour, formatPercent, type Locale } from "@/lib/format"

export function ModelDowngradeRateChart({
  points,
  config,
  rateLabel,
  locale,
}: {
  points: ModelDowngradeRatePoint[]
  config: ChartConfig
  rateLabel: string
  locale: Locale
}) {
  return (
    <ChartContainer config={config} className="aspect-auto h-64 w-full">
      <LineChart accessibilityLayer data={points}>
        <CartesianGrid vertical={false} />
        <ChartTooltip
          labelFormatter={(value) => formatAuditHour(Number(value), locale)}
          formatter={(value, _name, item) => {
            const point = item.payload as ModelDowngradeRatePoint
            return [
              `${formatPercent(Number(value) * 100, locale)} (${point.downgraded.toLocaleString(locale)}/${point.requests.toLocaleString(locale)})`,
              rateLabel,
            ]
          }}
          contentStyle={{
            backgroundColor: "var(--popover)",
            borderColor: "var(--border)",
            borderRadius: "var(--radius)",
            color: "var(--popover-foreground)",
          }}
          itemStyle={{ color: "var(--popover-foreground)" }}
        />
        <XAxis
          axisLine={false}
          dataKey="hour_start"
          minTickGap={44}
          tickFormatter={(value) => formatAuditHour(Number(value), locale)}
          tickLine={false}
          tickMargin={8}
        />
        <YAxis
          axisLine={false}
          domain={[0, 1]}
          tickFormatter={(value) => formatPercent(Number(value) * 100, locale)}
          tickLine={false}
          width={48}
        />
        <Line
          connectNulls={false}
          dataKey="downgrade_rate"
          dot={false}
          stroke="var(--color-downgrade_rate)"
          strokeWidth={2}
          type="monotone"
        />
      </LineChart>
    </ChartContainer>
  )
}
