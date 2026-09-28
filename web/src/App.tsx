import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import {
  flexRender,
  getCoreRowModel,
  getSortedRowModel,
  useReactTable,
  type ColumnDef,
  type SortingState,
} from "@tanstack/react-table"
import { AuthMiniProvider, useAuthMini } from "auth-mini-react-components"
import {
  Navigate,
  Route,
  Routes,
  useLocation,
  useNavigate,
  useParams,
} from "react-router-dom"
import {
  CartesianGrid,
  Line,
  LineChart,
  Tooltip as ChartTooltip,
  XAxis,
  YAxis,
} from "recharts"
import {
  LinkitMyInfo,
  LinkitProvider,
  LinkitUserInfo,
  LinkitUserPicker,
  useLinkit,
} from "linkit-react-components"
import {
  ActivityIcon,
  ArrowDownUpIcon,
  BookOpenIcon,
  BoxesIcon,
  CheckCircle2Icon,
  CheckIcon,
  ChevronDownIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  ChevronUpIcon,
  CircleGaugeIcon,
  ClipboardIcon,
  CpuIcon,
  DatabaseIcon,
  ExternalLinkIcon,
  HardDriveIcon,
  KeyRoundIcon,
  LanguagesIcon,
  MemoryStickIcon,
  NetworkIcon,
  PencilIcon,
  PlusIcon,
  RefreshCwIcon,
  ScrollTextIcon,
  ServerIcon,
  SettingsIcon,
  ShieldAlertIcon,
  ShieldCheckIcon,
  SlidersHorizontalIcon,
  Trash2Icon,
  UserRoundCogIcon,
  WalletCardsIcon,
  XIcon,
  XCircleIcon,
  type LucideIcon,
} from "lucide-react"
import { toast } from "sonner"
import { compareHeaderSnapshots } from "@/lib/header-comparison"
import { formatAuditHour, formatPercent, type Locale } from "@/lib/format"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { ChartContainer, type ChartConfig } from "@/components/ui/chart"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Separator } from "@/components/ui/separator"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarTrigger,
} from "@/components/ui/sidebar"
import { Skeleton } from "@/components/ui/skeleton"
import { Spinner } from "@/components/ui/spinner"
import { Switch } from "@/components/ui/switch"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { Toaster } from "@/components/ui/sonner"
import { TooltipProvider } from "@/components/ui/tooltip"
import { ResponsesAPIRequestBodyRenderer } from "@/components/responses-api-request-body-renderer"
import { api, type AuthSdk } from "@/lib/api"
import { cn } from "@/lib/utils"
import {
  codexPlatform,
  codexProviderBlock,
  isConsumerToken,
  updateCodexConfig,
  type CodexPlatform,
} from "@/lib/codex-config"
import {
  dshModels,
  updateDshCredentials,
  updateDshSettings,
} from "@/lib/dsh-config"
import { updateOpenCodeConfig } from "@/lib/opencode-config"
import { responseOutputText } from "@/lib/response-output"
import {
  auditFilterSearch,
  auditFiltersFromSearch,
  isAuditError,
  type AuditFilters,
} from "@/lib/audit-filters"
import {
  isUserTableNumericColumn,
  userTableSortValue,
} from "@/lib/user-table-sort"
import { midasTransferUrl, parseUsdNanos } from "@/lib/midas-transfer"

type Page =
  | "dashboard"
  | "providers"
  | "consumers"
  | "codex-integration"
  | "dsh-integration"
  | "opencode-integration"
  | "direct-api-integration"
  | "usage"
  | "audit"
  | "topups"
  | "model-prices"
  | "system-resources"
  | "admin-audit"
  | "provider-audit"
  | "request-detail"
  | "users"
  | "settings"
type NavPage = Exclude<Page, "request-detail">
type NavigationItem = [NavPage, LucideIcon]
type NavigationGroup = { label: string; items: NavigationItem[] }
type Role = "root" | "admin" | "user"
type User = { id: string; role: Role }
type SystemResources = {
  sampled_at: number
  sample_interval_ms: number
  cpu: {
    usage_percent: number
    load_1m: number
    logical_cpus: number
  }
  memory: {
    used_bytes: number
    total_bytes: number
    available_bytes: number
    process_used_bytes: number
    other_used_bytes: number
    usage_percent: number
    swap_used_bytes: number
    swap_total_bytes: number
  }
  network: {
    receive_bytes_per_second: number
    transmit_bytes_per_second: number
    total_received_bytes: number
    total_transmitted_bytes: number
    interfaces: number
  }
  disk: {
    mount_point: string
    used_bytes: number
    total_bytes: number
    available_bytes: number
    usage_percent: number
  } | null
  sqlite: {
    main_bytes: number
    wal_bytes: number
    shm_bytes: number
    total_bytes: number
    freelist_bytes: number
    freelist_percent: number
  }
}
type ManagedUser = {
  id: string
  role: Role
  allow_debt: boolean
  created_at: number
  topup_usd_nanos: number
  consumed_usd_nanos: number
  provided_usd_nanos: number
  available_usd_nanos: number
}
type PublicConfig = {
  setup_required: boolean
  auth_issuer?: string
  auth_audience?: string
}
type Consumer = {
  id: string
  name: string
  prefix: string
  created_at: number
  last_used_at?: number
  request_archive: boolean
  is_disabled: boolean
}
type ProviderUsageStats = {
  requests: number
  errors: number
  peak_requests: number
  input_tokens: number
  output_tokens: number
  cached_tokens: number
  actual_cost_usd_nanos: number
}
type Provider = {
  id: string
  name: string
  owner_id?: string
  status: string
  manual_disabled: number
  cooldown_until?: number
  last_error?: string
  visibility: ProviderVisibility
  created_at: number
  updated_at: number
  last_used_at?: number
  inflight: number
  queued: number
  concurrency_limit: number
  official_provided_usd_nanos: number
  actual_provided_usd_nanos: number
  usage?: ProviderUsageStats | null
}
const providerVisibilities = ["private", "public"] as const
type ProviderVisibility = (typeof providerVisibilities)[number]

function providerVisibilityLabel(
  value: ProviderVisibility,
  t: (typeof copy)[Locale]
): string {
  switch (value) {
    case "public":
      return t.providerVisibilityPublic
    default:
      return t.providerVisibilityPrivate
  }
}

type ProviderBalance = {
  is_available?: boolean | null
  balance_infos?: Array<{
    currency?: string
    total_balance?: string
    granted_balance?: string
    topped_up_balance?: string
  }> | null
}
type ProviderTestState = {
  provider: Provider
  status: "loading" | "success" | "error"
  latency_ms?: number
  model?: string
  error?: string
}
type DashboardStats = {
  active_consumers: number
  active_providers: number
  calls_24h: number
  errors_24h: number
  peak_calls_24h: number
  input_tokens_24h: number
  output_tokens_24h: number
  cached_tokens_24h: number
  official_cost_usd_nanos_24h: number
  actual_cost_usd_nanos_24h: number
  official_consumed_usd_nanos: number
  consumed_usd_nanos: number
  peak_now: boolean
  available_model_ids: string[]
}
type UsagePeriod = "24h" | "7d"
type UsageRow = {
  user_id: string
  consumer_id: string
  consumer_name: string
  consumer_prefix: string
  model: string
  date: string
  requests: number
  input_tokens: number
  cached_tokens: number
  output_tokens: number
  network_transport_bytes: number
  official_cost_usd_nanos: number
  actual_cost_usd_nanos: number
  peak_requests: number
}
type UsageResponse = { period: UsagePeriod; since: number; rows: UsageRow[] }
type PivotCell = Pick<
  UsageRow,
  | "requests"
  | "input_tokens"
  | "cached_tokens"
  | "output_tokens"
  | "network_transport_bytes"
  | "official_cost_usd_nanos"
  | "actual_cost_usd_nanos"
>
type PivotTableRow = {
  id: string
  values: UsageRow
  cells: Record<string, PivotCell>
}
type PivotColumn = { id: string; label: string }
type Audit = {
  id: string
  request_id: string
  thread_id?: string
  user_id: string
  consumer_name: string
  provider_id?: string
  provider_name?: string
  path: string
  model?: string
  reasoning_effort?: string
  peak: boolean
  status: number
  first_byte_latency_ms?: number
  request_bytes: number
  response_bytes: number
  request_transport_bytes: number
  response_transport_bytes: number
  latency_ms: number
  input_tokens: number
  output_tokens: number
  cached_tokens: number
  official_cost_usd_nanos: number
  actual_cost_usd_nanos: number
  price_multiplier_nanos: number
  official_consumed_usd_before_nanos: number
  official_consumed_usd_after_nanos: number
  actual_consumed_usd_before_nanos: number
  actual_consumed_usd_after_nanos: number
  official_provided_usd_before_nanos: number
  official_provided_usd_after_nanos: number
  actual_provided_usd_before_nanos: number
  actual_provided_usd_after_nanos: number
  error_code?: string | null
  error?: string
  created_at: number
}
type AuditPageResponse = { rows: Audit[]; total: number }
type AuditNavigation = { id: string; request_id: string; created_at: number }
type AuditDetail = Audit & {
  upstream_http_version?: string
  downstream_accept_encoding?: string
  downstream_content_encoding?: string
  upstream_accept_encoding?: string
  upstream_content_encoding?: string
  method: string
  client_ip?: string
  affinity_hash?: string
  affinity_source?: string
  archive_available: boolean
  bodies_available: boolean
  request_headers?: string | null
  upstream_request_headers?: string | null
  request_body?: string
  request_body_truncated: boolean
  response_headers?: string | null
  downstream_response_headers?: string | null
  response_body?: string
  response_body_truncated: boolean
  previous?: AuditNavigation
  next?: AuditNavigation
}
type AdminAudit = {
  id: string
  admin_user_id: string
  action: string
  target_id?: string
  client_ip?: string
  created_at: number
}
type ProviderAuditRow = {
  hour_start: number
  provider_id?: string | null
  provider_name?: string | null
  model: string
  requests: number
  successful_requests: number
  failed_requests: number
  input_tokens: number
  actual_cost_usd_nanos: number
  peak_requests: number
  success_rate: number
  failure_rate: number
}
type ProviderAuditResponse = {
  period: UsagePeriod
  since: number
  until: number
  rows: ProviderAuditRow[]
}
type SettingsData = {
  role: Role
  auth_issuer?: string
  auth_audience?: string
  upstream_base: string
  available_model_ids: string[]
  allow_all_users_debt: boolean
  response_body_limit: number
  affinity_ttl_seconds: number
  provider_concurrency_limit: number
  request_archive_retention_days: number
  model_price_multiplier: string
  midas_api_base?: string
  midas_fund_user_id?: string | null
  midas_fund_api_key_configured?: boolean
}
type PaymentSummary = {
  topup_usd_nanos: number
  available_usd_nanos: number
  official_consumed_usd_nanos: number
  consumed_usd_nanos: number
  provided_usd_nanos: number
  enforcement_enabled: boolean
  midas_configured: boolean
  midas_fund_user_id?: string
}
type MidasSettingsData = {
  midas_api_base: string
  midas_fund_user_id?: string
  midas_fund_api_key_configured: boolean
}
type ModelPriceRates = {
  cache_hit_usd_nanos: number
  cache_miss_usd_nanos: number
  output_usd_nanos: number
}
type ModelPrice = {
  model: string
  peak: ModelPriceRates
  off_peak: ModelPriceRates
}
type ModelPricesResponse = {
  source_url: string
  source_as_of: string
  unit: string
  rows: ModelPrice[]
}
const copy = {
  zh: {
    dashboard: "总览",
    providers: "上游提供商",
    consumers: "下游消费者",
    "codex-integration": "Codex",
    "dsh-integration": "DSH（DeepSeek Harness）",
    "opencode-integration": "OpenCode",
    "direct-api-integration": "直接 API",
    usage: "用量",
    audit: "推理审计",
    topups: "充值",
    "model-prices": "模型价格",
    "system-resources": "系统资源",
    "admin-audit": "管理审计",
    "provider-audit": "提供商审计",
    users: "用户",
    settings: "设置",
    title: "DeepSeek-LB",
    subtitle: "DeepSeek API 反向代理与负载均衡器",
    navigationWorkspace: "工作区",
    navigationIntegrations: "下游接入",
    navigationData: "数据",
    navigationAdministration: "管理员",
    english: "English",
    roleLoading: "加载中",
    continueAuthMini: "前往 Auth Mini 登录",
    loading: "正在加载 DeepSeek-LB…",
    pageDashboard: "查看当前账户的 24 小时运行摘要。",
    pageProviders:
      "管理自己拥有的 DeepSeek 上游提供商、运行状态与近 7 天用量。",
    pageConsumers: "按 AI App 隔离下游消费者，分别跟踪调用量并独立吊销凭据。",
    pageCodexIntegration:
      "手动或授权浏览器配置本机 Codex 文件，并使用独立的下游 Consumer。",
    pageDshIntegration:
      "手动或授权浏览器自动配置本机 DSH，并为它创建独立的下游 Consumer。",
    pageOpenCodeIntegration:
      "手动或授权浏览器配置本机 OpenCode 文件，并使用独立的下游 Consumer。",
    pageDirectApiIntegration:
      "使用独立的下游 Consumer 经 DeepSeek-LB 调用 DeepSeek API。",
    pageUsage: "按消费者核算请求、Token、官方费用与实际费用。",
    pageAudit:
      "逐次追踪推理请求、上游提供商、费用快照与累计消费；诊断内容按配置期限保留。",
    pageTopups: "通过 Midas 管理入金、协议授权与可用额度。",
    pageModelPrices:
      "查看 DeepSeek 官方标准 Token 价格快照，价格按每百万 Token 展示。",
    pageSystemResources:
      "查看宿主机当前负载和 DeepSeek-LB 数据占用；仅 root 和管理员可访问。",
    pageAdminAudit: "查看 root 与管理员执行的管理操作记录。",
    pageProviderAudit:
      "按上游提供商、模型和小时查看最近 7 天请求成功与失败情况。",
    pageRequestDetail: "查看调用上下文、消息结构与同一 Thread ID 的相邻请求。",
    pageUsers:
      "由 root 管理本地角色与全局 Provider 权限；用户身份仍由 Auth Mini 提供。",
    pageSettings: "确认身份边界、上游与部署限制。",
    accountOverview: "本账户运行概要",
    accountOverviewDescription: "当前账户的运行状态、调用与 Token 用量。",
    tokenUsage24h: "24 小时 Token 用量",
    tokenUsage24hDescription:
      "已审计请求的输入、输出与缓存 Token 汇总；缓存率 = 缓存输入 Token / 输入 Token，输入输出比 = 输入 Token / 输出 Token。",
    cacheRate: "缓存率",
    inputOutputRatio: "输入输出比",
    activeConsumers: "有效消费者",
    calls24h: "24 小时调用",
    errors24h: "24 小时错误",
    officialCost24h: "24 小时官方费用 (USD)",
    actualCost24h: "24 小时实际费用 (USD)",
    officialConsumedUsd: "官方累计费用 (USD)",
    actualConsumedUsd: "实际累计消费 (USD)",
    providedValue: "累计提供价值 (USD)",
    officialCost: "官方费用 (USD)",
    actualCost: "实际费用 (USD)",
    priceMultiplier: "价格倍率",
    officialConsumedUsdBefore: "官方累计费用前 (USD)",
    officialConsumedUsdAfter: "官方累计费用后 (USD)",
    actualConsumedUsdBefore: "实际累计消费前 (USD)",
    actualConsumedUsdAfter: "实际累计消费后 (USD)",
    officialProvidedUsdBefore: "上游官方累计提供价值前 (USD)",
    officialProvidedUsdAfter: "上游官方累计提供价值后 (USD)",
    actualProvidedUsdBefore: "上游实际累计提供价值前 (USD)",
    actualProvidedUsdAfter: "上游实际累计提供价值后 (USD)",
    topupsTitle: "充值与额度",
    topupsDescription:
      "可用额度 = Midas 公共账户累计转入 + 累计提供价值 - 累计消费。",
    creditedUsd: "累计充值 (USD)",
    usageUsd: "累计消费 (USD)",
    availableCredit: "可用额度 (USD)",
    cumulativeTopups: "累计充值 (USD)",
    cumulativeConsumption: "累计消费 (USD)",
    requestNotEnforced: "当前不因可用额度不足拦截请求。",
    requestEnforced: "可用额度不足时会拦截请求。",
    midasFundTitle: "转入 DeepSeek-LB 公共账户",
    midasFundDescription:
      "输入金额后，Midas 会在新窗口中显示不可编辑的收款账户和金额；登录并确认后，即可完成充值。",
    topupAmount: "充值金额 (USD)",
    topupAmountHint: "最多 9 位小数。Midas 将按精确 USD 纳美元金额转账。",
    continueToMidas: "继续前往 Midas",
    publicWalletUserId: "DeepSeek-LB 公共账户用户 ID",
    copyMidasUserId: "复制用户 ID",
    midasTransferRefresh:
      "完成转账后刷新本页，累计充值会从 Midas 当前累计转入中显示。",
    midasUnavailable: "Midas 尚未配置",
    midasUnavailableDescription:
      "请由 root 在设置中填写 Midas 公共账户用户 ID 与 fund API key。",
    midasSettings: "Midas",
    midasSettingsDescription:
      "Midas 是充值唯一账本。公共账户 user ID 可安全展示给付款用户；fund API key 仅保存在服务器 SQLite 中，留空会保留现有密钥。",
    midasApiBase: "API 地址",
    midasFundUserId: "公共账户 User ID",
    midasFundApiKey: "Fund API key",
    midasConfigured: "已配置",
    midasNotConfigured: "未配置",
    midasSettingsSaved: "Midas 设置已保存",
    modelPricesTitle: "DeepSeek 官方价格",
    modelPricesDescription:
      "用于费用核算的 DeepSeek 官方价格快照，单位为美元/百万 Token；高峰与非高峰两档并列。",
    pricingUnit: "计价单位",
    pricingAsOf: "价格快照日期",
    officialPricingSource: "打开 DeepSeek 官方价格页",
    officialModelPriceTable: "Token 价格",
    modelPricesTableDescription:
      "按高峰（peak）与非高峰（off-peak）分别列示。非高峰为高峰价格的 50%，中国法定节假日全天按非高峰计费。",
    activeProviders: "可用上游提供商",
    peakCalls24h: "峰时请求（24 小时）",
    pricingTariffTitle: "当前计费时段",
    pricingTariffDescription:
      "DeepSeek 分高峰与非高峰两档计费：非高峰价格为高峰的 50%，中国法定节假日全天按非高峰计费。",
    pricingPeak: "高峰",
    pricingTariff: "计费时段",
    pricingOffPeak: "非高峰",
    pricingModels: "可用模型",
    cacheHitTokens: "命中缓存",
    cacheMissTokens: "未命中缓存",
    systemResources: "系统资源",
    systemResourcesDescription:
      "宿主机当前负载与 DeepSeek-LB 数据占用，仅 root 和管理员可见。",
    refreshEvery5s: "每 5 秒刷新",
    sampledAt: "采样于",
    resourceUnavailable: "无法读取系统资源",
    resourceUnavailableDescription: "自动刷新会继续重试，无需重新加载页面。",
    cpu: "CPU",
    memory: "内存",
    network: "网络",
    disk: "磁盘",
    sqlite: "SQLite 数据库",
    load1m: "1 分钟负载",
    logicalCpus: "逻辑核心",
    available: "可用",
    lbRss: "DeepSeek-LB RSS",
    otherSystemMemory: "其他系统占用",
    systemAvailableMemory: "系统可用内存",
    swap: "Swap",
    received: "接收",
    transmitted: "发送",
    totalReceived: "累计接收",
    totalTransmitted: "累计发送",
    networkInterfaces: "网络接口",
    mountPoint: "挂载点",
    mainFile: "主文件",
    walFile: "WAL",
    shmFile: "SHM",
    reclaimableSpace: "可回收空间（VACUUM）",
    vacuumDatabase: "执行 VACUUM",
    vacuumDatabaseTitle: "整理 SQLite 数据库？",
    vacuumDatabaseDescription:
      "此操作会重写数据库文件以回收可回收空间。执行期间数据库写入可能短暂等待。",
    vacuumDatabaseComplete: "SQLite 数据库已整理，资源指标已刷新。",
    providerPool: "上游提供商池",
    providerDescription:
      "每个上游提供商就是一个 DeepSeek API Key。私有提供商只有你自己的 Consumer 可以使用，公开提供商对所有 Consumer 开放。",
    providerApiKey: "DeepSeek API Key",
    providerApiKeyHelp:
      "DeepSeek 平台签发的 API Key，保存在本机 SQLite 中，只有 root 与管理员可以再次读取。",
    providerApiKeyInvalid: "API Key 需要以 sk- 开头且不含空白字符。",
    providerKeyTitle: "上游提供商 API Key",
    providerKeyDescription:
      "读取或替换该提供商的 DeepSeek API Key。更换后调度池会立即使用新 Key。",
    providerKeySaved: "上游提供商 API Key 已保存",
    saveProviderKey: "保存 API Key",
    providerEnable: "启用",
    providerDisable: "停用",
    providerLastError: "最近错误",
    providerCooldownUntil: "冷却至",
    providerUsage7d: "近 7 天用量",
    providerUsageRequests: "请求",
    providerUsageErrors: "失败",
    providerUsageTokens: "Token（输入 / 输出 / 命中缓存）",
    providerUsageCost: "费用",
    providerNoUsage: "近 7 天没有请求",
    providerTestTitle: "上游连通性测试",
    providerTestDescription:
      "服务端使用该提供商的 API Key 向 DeepSeek 发送一次最小请求，Key 不会返回浏览器。",
    providerTestResult: "结果",
    providerTestSucceeded: "连通正常",
    providerTestFailed: "上游返回错误",
    providerTestModel: "测试模型",
    providerTestLatency: "耗时",
    providerBalance: "余额",
    providerBalanceTitle: "上游余额",
    providerBalanceDescription:
      "直接读取 DeepSeek 账户余额；余额为 0 时该提供商会返回 402 并暂停调度。",
    providerBalanceAvailable: "可继续调用",
    providerBalanceUnavailable: "DeepSeek 未返回余额信息。",
    yes: "是",
    no: "否",
    addProvider: "添加上游提供商",
    noProviders: "尚无上游提供商",
    noProvidersDescription:
      "点击“添加上游提供商”，填入 DeepSeek 平台签发的 API Key，即可加入调度池。",
    name: "名称",
    owner: "所有者",
    status: "状态",
    actions: "操作",
    refresh: "刷新",
    providerUpdated: "上游提供商已更新",
    providerAdded: "上游提供商已添加",
    addProviderTitle: "添加上游提供商",
    nameOptional: "名称（可选）",
    providerNameHelp: "留空时使用提供商 UUID。",
    providerVisibility: "可见性",
    providerVisibilityPrivate: "私有",
    providerVisibilityPublic: "公开",
    providerVisibilityHelp:
      "私有：只有本用户名下的 Consumer 可以使用该上游提供商。公开：所有 Consumer 都可以使用。",
    makeProviderPublic: "改为公开",
    makeProviderPrivate: "改为私有",
    editProviderName: "编辑提供商名称",
    saveProviderName: "保存提供商名称",
    cancelProviderName: "取消编辑提供商名称",
    deleteProvider: "删除",
    deleteProviderTitle: "删除上游提供商？",
    deleteProviderDescription:
      "此操作会将上游提供商从管理列表和调度池中隐藏。其 API Key、亲和性及历史记录都会保留。",
    confirmDeleteProvider: "隐藏上游提供商",
    providerDeleted: "上游提供商已隐藏",
    testProvider: "测试",
    testingProvider: "正在测试上游连通性…",
    consumersTitle: "租户消费者",
    consumersDescription:
      "每个 AI App 建议使用一个独立消费者；这样用量、错误和密钥轮换都能按 App 隔离。消费者凭据只在创建或轮换后显示一次。",
    create: "创建",
    noConsumers: "尚无消费者",
    noConsumersDescription:
      "为每个 AI App 创建一个独立消费者，再开始调用代理。",
    prefix: "前缀",
    apiKey: "API KEY",
    createdAt: "创建时间",
    lastUsed: "最近使用",
    requestArchive: "诊断入库",
    requestArchiveHelp:
      "打开后，该 Consumer 的请求/响应诊断预览才会保存到 SQLite。",
    requestArchiveUpdated: "诊断入库开关已更新",
    consumerEnabled: "消费者已启用",
    consumerDisabledUpdated: "消费者状态已更新",
    rotateConsumer: "轮换并复制",
    rotateConsumerTitle: "轮换 API KEY？",
    rotateConsumerDescription:
      "轮换后当前 API KEY 会立即失效，并将新的 API KEY 复制到剪贴板。确定继续吗？",
    confirmRotateConsumer: "确认轮换",
    consumerRotated: "新的 API 密钥已复制到剪贴板",
    consumerRotateCopyFailed:
      "API 密钥已轮换，但无法复制到剪贴板。请再次轮换后重试。",
    editConsumer: "编辑",
    editConsumerAriaLabel: "编辑消费者",
    saveConsumer: "保存",
    consumerUpdated: "消费者已更新",
    deleteConsumer: "删除",
    deleteConsumerTitle: "删除消费者？",
    deleteConsumerDescription:
      "此操作会从列表和鉴权中隐藏该消费者。所有调用记录和已保存的诊断内容都会保留。",
    confirmDeleteConsumer: "删除消费者",
    consumerDeleted: "消费者已删除，历史已保留",
    cancel: "取消",
    consumerAppHelp:
      "请为每个 AI App 单独创建一个消费者，并用 App 名称命名，便于隔离用量、排障和密钥轮换。",
    saveConsumerTitle: "立即保存消费者",
    saveConsumerDescription:
      "关闭后无法再次查看。不要将它写入浏览器代码、日志或聊天记录。",
    savedConsumer: "我已安全保存",
    copied: "已复制",
    usageTitle: "用量透视",
    usageDescription:
      "按用户、脱敏 API Token、模型和日期交叉汇总请求 Token、官方费用与实际费用。",
    noUsage: "暂无用量",
    noUsageDescription: "在所选时间段发起 API 调用后，这里会按维度汇总 Token。",
    requests: "请求",
    errors: "错误",
    last24Hours: "最近 24 小时",
    last7Days: "最近 7 天",
    allUsers: "全部用户",
    allConsumers: "全部消费者",
    allModels: "全部模型",
    userLabel: "用户",
    model: "模型",
    consumerLabel: "消费者",
    date: "日期",
    rows: "行",
    columns: "列",
    data: "数据",
    hidden: "隐藏",
    inputTokens: "输入 Token",
    cachedInputTokens: "缓存输入 Token",
    outputTokens: "输出 Token",
    requestCount: "请求次数",
    usageRows: "条聚合记录",
    sortColumn: "排序",
    total: "总计",
    clearFilters: "清除筛选",
    pivotFields: "透视字段",
    dataOrder: "数据列顺序",
    auditTitle: "推理审计",
    auditDescription:
      "请求/响应诊断预览保存在 SQLite；不记录 Authorization 或上游 API Key。",
    noAudit: "暂无推理审计记录",
    noAuditDescription: "每次推理调用结束后都会写入基础审计记录。",
    time: "时间",
    requestId: "请求 ID",
    threadId: "Thread ID",
    copyThreadId: "复制 Thread ID",
    provider: "上游提供商",
    firstByteLatency: "首字节",
    totalLatency: "总耗时",
    requestSize: "请求大小",
    responseSize: "响应大小",
    requestTransportSize: "请求传输量（压缩后）",
    responseTransportSize: "响应传输量（压缩后）",
    compressionRatio: "压缩率",
    downstreamAcceptEncoding: "下游接受压缩",
    downstreamContentEncoding: "下游响应压缩",
    upstreamAcceptEncoding: "上游请求压缩",
    upstreamContentEncoding: "上游响应压缩",
    networkTransport: "网络传输量（压缩后）",
    reasoningEffort: "推理强度",
    cachedInput: "缓存输入",
    details: "详情",
    filter: "筛选",
    filterUserId: "用户 ID",
    filterConsumer: "消费者",
    filterProvider: "提供商",
    filterModel: "模型",
    errorCode: "错误码",
    errorMessage: "错误信息",
    allStatuses: "全部状态",
    successfulCalls: "成功",
    failedCalls: "失败",
    auditResults: "条调用",
    page: "第",
    previousPage: "上一页",
    nextPage: "下一页",
    requestDetail: "请求详情",
    requestSummary: "请求概要",
    backToAudit: "返回推理审计",
    requestHeaders: "请求头",
    requestBody: "请求正文",
    responseHeaders: "响应头",
    headerSnapshotMissing: "未采集",
    headerComparisonUnavailable: "缺少一侧的头信息，无法比较差异。",
    responseBody: "响应正文",
    tokenUsage: "Token 用量",
    cacheHitRate: "缓存命中率",
    upstreamHttpProtocol: "上游 HTTP 协议",
    httpStatusCode: "HTTP 状态码",
    finalResponse: "最终回复",
    finalResponseUnavailable: "响应正文中未找到 response.output。",
    diagnosticData: "请求与响应正文",
    diagnosticDataDescription:
      "仅在该 Consumer 开启诊断入库时保存；敏感凭据不会记录。",
    deleteAuditBodies: "删除请求/响应正文",
    deleteAuditBodiesTitle: "删除请求和响应正文？",
    deleteAuditBodiesDescription:
      "此操作不可撤销。调用状态、用量和请求/响应头诊断将保留。",
    confirmDeleteAuditBodies: "删除正文",
    auditBodiesDeleted: "请求/响应正文已删除",
    auditBodiesDeletedDescription:
      "此审计记录的请求与响应正文已永久删除；调用状态、用量和请求/响应头诊断仍会保留。",
    headerName: "头字段",
    downstreamToLb: "下游 → LB",
    lbToUpstream: "LB → 上游",
    upstreamToLb: "上游 → LB",
    lbToDownstream: "LB → 下游",
    different: "不同",
    previewTruncated: "预览已截断",
    affinitySource: "亲和来源",
    affinityRequestId: "亲和请求 ID",
    affinityHash: "亲和哈希",
    previousRequest: "上一个请求",
    nextRequest: "下一个请求",
    tools: "工具",
    identityPermissions: "身份与权限",
    identityDescription: "浏览器会话由 Auth Mini 管理；后端只验证 access JWT。",
    proxyBoundary: "代理边界",
    proxyDescription: "仅代理 DeepSeek 能力，不提供其他厂商兼容协议。",
    unableLoad: "无法加载",
    unknownError: "未知错误",
    close: "关闭",
    inflight: "处理中",
    providerLoad: "处理中 / 上限",
    providerQueued: "排队中",
    providerConcurrency: "上游并发控制",
    providerConcurrencyDescription:
      "统一设置每个上游提供商的并发上限，默认为 3。",
    providerConcurrencyLimit: "每个提供商的并发上限",
    providerConcurrencyHelp:
      "从路由到提供商起计数，直到响应结束或请求取消，流式响应包含在内。超出上限的请求按先后顺序排队。保存后立即生效；降低上限不会中断正在处理的请求。",
    providerConcurrencyInvalid: "请输入大于 0 的整数。",
    consumer: "消费者",
    input: "输入",
    output: "输出",
    userId: "用户 ID",
    role: "角色",
    authIssuer: "认证签发方",
    upstream: "上游",
    bodyLimit: "请求体限制",
    affinityTtl: "亲和 TTL",
    upstreamHint: "DeepSeek API 基址，默认 https://api.deepseek.com。",
    archiveRetention: "请求与响应诊断记录保留天数",
    statusActive: "可用",
    statusCooldown: "冷却中",
    statusAuthError: "认证错误",
    statusDisabled: "已禁用",
    statusUnknown: "未知",
    roleRoot: "超级管理员",
    roleAdmin: "管理员",
    roleUser: "租户用户",
    adminAuditTitle: "管理操作审计",
    adminAuditDescription:
      "记录 root 与管理员执行的提供商、密钥、系统与审计管理操作。",
    providerAuditTitle: "提供商审计",
    providerAuditDescription:
      "按上游提供商、模型和小时汇总请求结果，帮助定位失败集中在哪个上游或模型。",
    providerAuditChartDescription:
      "当前筛选范围内按小时观察失败率、请求数和输入 Token。",
    providerAuditFailureRateChart: "时间—失败率",
    providerAuditRequestCountChart: "时间—请求数",
    providerAuditInputTokensChart: "时间—Input Token 数",
    providerAuditPeriod: "数据范围",
    providerAuditTableDescription:
      "每行代表一个小时、一个上游提供商和一个模型。",
    providerAuditNoData: "所选时间范围暂无请求",
    providerAuditNoDataDescription:
      "产生请求后，这里会按小时显示各个上游和模型的结果。",
    providerAuditAllProviders: "全部提供商",
    providerAuditUnknownProvider: "未识别提供商",
    providerAuditHour: "小时",
    providerAuditSuccessRate: "成功率",
    providerAuditFailureRate: "失败率",
    providerAuditSuccessfulRequests: "成功请求",
    providerAuditFailedRequests: "失败请求",
    providerAuditTotalRequests: "请求总数",
    providerAuditInputTokens: "Input Token 数",
    administrator: "操作用户",
    action: "操作",
    target: "目标",
    clientIp: "客户端 IP",
    setupTitle: "初始化 DeepSeek-LB",
    setupDescription: "连接品牌 Auth Mini，并将首个已验证用户绑定为唯一 root。",
    setupIssuer: "Auth Mini issuer",
    setupIssuerHelp:
      "填写品牌提供的 Auth Mini HTTPS 地址。DeepSeek-LB 只连接该实例，不会部署或管理它。",
    setupAudience: "JWT audience",
    connectAuth: "连接 Auth Mini",
    setupLogin: "验证 root 身份",
    setupLoginHelp:
      "登录成功后，当前 Auth Mini user_id 将成为 DeepSeek-LB root。",
    finishSetup: "绑定 root 并完成初始化",
    finishingSetup: "正在完成初始化",
    setupStepConnect: "连接认证实例",
    setupAuthenticated: "身份已验证",
    setupSecurity:
      "Setup 完成后初始化入口会立即关闭；后续登录用户默认为 user。",
    usersTitle: "用户与请求权限",
    usersDescription:
      "所有用户默认可以使用全局 Provider，但仅能查看和管理自己拥有的 Provider；管理员和 root 可以查看全部。列表显示每位用户的累计充值、累计消费、累计提供价值与可用额度。允许欠费的用户可在余额用尽后继续发送请求。",
    noUsers: "暂无用户",
    noUsersDescription: "用户首次通过 Auth Mini 登录后会自动出现在这里。",
    roleUpdated: "用户角色已更新",
    allowDebt: "允许欠费",
    allowDebtUpdated: "允许欠费设置已更新",
    runtimeSettings: "运行配置",
    runtimeSettingsDescription:
      "这些值保存在 SQLite app_meta 中；地址与模型立即生效，请求体上限重启后生效。",
    availableModels: "可用模型",
    availableModelsDescription:
      "控制下游可调用的模型。此列表会立即同步到 /v1/models、模型价格页和 Responses 请求校验。",
    availableModelsHelp: "每行填写一个 Model ID；留空或重复的 ID 无法保存。",
    availableModelsSaved: "可用模型已保存",
    availableModelsRequired: "请至少填写一个 Model ID",
    modelPriceMultiplier: "默认价格倍率",
    modelPriceMultiplierHint:
      "未来请求按官方费用乘以此倍率计费；每条请求都会保存当时的倍率快照。",
    allowAllUsersDebt: "允许所有人欠费",
    allowAllUsersDebtHint: "打开后，所有用户均可在可用额度不足时继续发送请求。",
    saveSettings: "保存配置",
    settingsSaved: "配置已保存",
    responseLimit: "请求体与响应预览上限 (bytes)",
  },
  en: {
    dashboard: "Overview",
    providers: "Providers",
    consumers: "Consumers",
    "codex-integration": "Codex",
    "dsh-integration": "DSH (DeepSeek Harness)",
    "opencode-integration": "OpenCode",
    "direct-api-integration": "Direct API",
    usage: "Usage",
    audit: "Inference audit",
    topups: "Top up",
    "model-prices": "Model prices",
    "system-resources": "System resources",
    "admin-audit": "Admin audit",
    "provider-audit": "Provider audit",
    users: "Users",
    settings: "Settings",
    title: "DeepSeek-LB",
    subtitle: "DeepSeek API reverse proxy and load balancer",
    navigationWorkspace: "Workspace",
    navigationIntegrations: "Downstream integrations",
    navigationData: "Data",
    navigationAdministration: "Administration",
    english: "简体中文",
    roleLoading: "Loading",
    continueAuthMini: "Continue to Auth Mini",
    loading: "Loading DeepSeek-LB…",
    pageDashboard: "Review the current account's 24-hour operating summary.",
    pageProviders:
      "Manage the DeepSeek Providers you own, their runtime state, and 7-day usage.",
    pageConsumers:
      "Give each AI app its own downstream Consumer so usage, errors, and revocation stay isolated.",
    pageCodexIntegration:
      "Configure the local Codex file manually or via the browser, with a dedicated downstream Consumer.",
    pageDshIntegration:
      "Configure local DSH manually or authorize the browser to configure it with a dedicated downstream Consumer.",
    pageOpenCodeIntegration:
      "Configure the local OpenCode file manually or in the browser with a dedicated downstream Consumer.",
    pageDirectApiIntegration:
      "Call the DeepSeek API through DeepSeek-LB with a dedicated downstream Consumer.",
    pageUsage:
      "Attribute requests, tokens, official cost, and actual cost to each Consumer.",
    pageAudit:
      "Trace each inference request, provider, pricing snapshot, and cumulative consumption; diagnostics follow the configured retention.",
    pageTopups:
      "Manage Midas funding, agreement authorization, and available credit.",
    pageModelPrices:
      "Review the official DeepSeek token-pricing snapshot, shown per 1M tokens.",
    pageSystemResources:
      "Review current host load and DeepSeek-LB data usage. Available only to root and administrators.",
    pageAdminAudit:
      "Review management operations performed by root and administrators.",
    pageProviderAudit:
      "Review successful and failed requests by upstream provider, model, and hour over the last 7 days.",
    pageRequestDetail:
      "Review call context, message structure, and adjacent requests with the same Thread ID.",
    pageUsers:
      "Root manages local roles and global Provider access while Auth Mini remains the identity provider.",
    pageSettings:
      "Confirm identity boundaries, upstream, and deployment limits.",
    accountOverview: "Account operating summary",
    accountOverviewDescription:
      "Current account status, calls, and Token usage.",
    tokenUsage24h: "Token usage in 24h",
    tokenUsage24hDescription:
      "Input, output, and cached Token totals from audited requests; cache rate = cached input / input, input/output ratio = input / output.",
    cacheRate: "Cache rate",
    inputOutputRatio: "Input/output ratio",
    activeConsumers: "Active Consumers",
    calls24h: "Calls in 24h",
    errors24h: "Errors in 24h",
    officialCost24h: "Official cost in 24h (USD)",
    actualCost24h: "Actual cost in 24h (USD)",
    officialConsumedUsd: "Official cumulative cost (USD)",
    actualConsumedUsd: "Actual cumulative consumption (USD)",
    providedValue: "Cumulative provided value (USD)",
    officialCost: "Official cost (USD)",
    actualCost: "Actual cost (USD)",
    priceMultiplier: "Price multiplier",
    officialConsumedUsdBefore: "Official cumulative cost before (USD)",
    officialConsumedUsdAfter: "Official cumulative cost after (USD)",
    actualConsumedUsdBefore: "Actual cumulative consumption before (USD)",
    actualConsumedUsdAfter: "Actual cumulative consumption after (USD)",
    officialProvidedUsdBefore:
      "Upstream official cumulative provided value before (USD)",
    officialProvidedUsdAfter:
      "Upstream official cumulative provided value after (USD)",
    actualProvidedUsdBefore:
      "Upstream actual cumulative provided value before (USD)",
    actualProvidedUsdAfter:
      "Upstream actual cumulative provided value after (USD)",
    topupsTitle: "Top-ups and credit",
    topupsDescription:
      "Available credit = cumulative transfers into the Midas public account + cumulative provided value - cumulative consumption.",
    creditedUsd: "Cumulative top-ups (USD)",
    usageUsd: "Cumulative consumption (USD)",
    availableCredit: "Available credit (USD)",
    cumulativeTopups: "Cumulative top-ups (USD)",
    cumulativeConsumption: "Cumulative consumption (USD)",
    requestNotEnforced:
      "Requests are currently not blocked for insufficient available credit.",
    requestEnforced:
      "Requests are blocked when available credit is insufficient.",
    midasFundTitle: "Transfer to the DeepSeek-LB public account",
    midasFundDescription:
      "Enter an amount to open Midas in a new window. It shows the recipient and amount read-only; sign in and confirm to complete the top-up.",
    topupAmount: "Top-up amount (USD)",
    topupAmountHint:
      "Up to 9 decimal places. Midas transfers this exact amount in USD nanodollars.",
    continueToMidas: "Continue to Midas",
    publicWalletUserId: "DeepSeek-LB public-account user ID",
    copyMidasUserId: "Copy user ID",
    midasTransferRefresh:
      "Refresh this page after the transfer; cumulative top-ups are read from your current Midas inbound transfers.",
    midasUnavailable: "Midas is not configured",
    midasUnavailableDescription:
      "Ask root to configure the Midas public-account user ID and fund API key in Settings.",
    midasSettings: "Midas",
    midasSettingsDescription:
      "Midas is the only top-up ledger. The public-account user ID is safe to share with payers; the fund API key stays in server SQLite, and a blank key retains the current key.",
    midasApiBase: "API base URL",
    midasFundUserId: "Public-account user ID",
    midasFundApiKey: "Fund API key",
    midasConfigured: "Configured",
    midasNotConfigured: "Not configured",
    midasSettingsSaved: "Midas settings saved",
    modelPricesTitle: "Official DeepSeek prices",
    modelPricesDescription:
      "The official DeepSeek price snapshot used for cost accounting, in USD per million tokens, with peak and off-peak rates side by side.",
    pricingUnit: "Unit",
    pricingAsOf: "Pricing snapshot date",
    officialPricingSource: "Open official DeepSeek pricing",
    officialModelPriceTable: "Token pricing",
    modelPricesTableDescription:
      "Peak and off-peak rates are shown separately. Off-peak costs 50% of peak, and Chinese public holidays are billed off-peak all day.",
    activeProviders: "Active providers",
    peakCalls24h: "Peak-hour requests (24h)",
    pricingTariffTitle: "Current billing window",
    pricingTariffDescription:
      "DeepSeek bills two tariffs: off-peak costs 50% of peak, and Chinese public holidays are billed off-peak all day.",
    pricingPeak: "Peak",
    pricingTariff: "Billing window",
    pricingOffPeak: "Off-peak",
    pricingModels: "Available models",
    cacheHitTokens: "Cache hit",
    cacheMissTokens: "Cache miss",
    systemResources: "System resources",
    systemResourcesDescription:
      "Current host load and DeepSeek-LB data footprint. Visible to root and administrators only.",
    refreshEvery5s: "Refreshes every 5 seconds",
    sampledAt: "Sampled",
    resourceUnavailable: "System resources unavailable",
    resourceUnavailableDescription:
      "Automatic refresh will keep retrying; there is no need to reload the page.",
    cpu: "CPU",
    memory: "Memory",
    network: "Network",
    disk: "Disk",
    sqlite: "SQLite database",
    load1m: "1-minute load",
    logicalCpus: "Logical CPUs",
    available: "Available",
    lbRss: "DeepSeek-LB RSS",
    otherSystemMemory: "Other system usage",
    systemAvailableMemory: "System available memory",
    swap: "Swap",
    received: "Received",
    transmitted: "Transmitted",
    totalReceived: "Total received",
    totalTransmitted: "Total transmitted",
    networkInterfaces: "Network interfaces",
    mountPoint: "Mount point",
    mainFile: "Main file",
    walFile: "WAL",
    shmFile: "SHM",
    reclaimableSpace: "Reclaimable (VACUUM)",
    vacuumDatabase: "Run VACUUM",
    vacuumDatabaseTitle: "Compact the SQLite database?",
    vacuumDatabaseDescription:
      "This rewrites the database file to reclaim free space. Database writes may wait briefly while it runs.",
    vacuumDatabaseComplete:
      "SQLite database compacted and resource metrics refreshed.",
    providerPool: "Provider pool",
    providerDescription:
      "Every provider is one DeepSeek API key. Private providers serve only your own Consumers; public providers serve every Consumer.",
    providerApiKey: "DeepSeek API key",
    providerApiKeyHelp:
      "The API key issued by the DeepSeek platform. It is stored in the local SQLite database and can only be read again by root and administrators.",
    providerApiKeyInvalid:
      "The API key must start with sk- and contain no whitespace.",
    providerKeyTitle: "Provider API key",
    providerKeyDescription:
      "Read or replace this provider's DeepSeek API key. The scheduler picks up the new key immediately.",
    providerKeySaved: "Provider API key saved",
    saveProviderKey: "Save API key",
    providerEnable: "Enable",
    providerDisable: "Disable",
    providerLastError: "Last error",
    providerCooldownUntil: "Cooling down until",
    providerUsage7d: "Last 7 days",
    providerUsageRequests: "Requests",
    providerUsageErrors: "Failed",
    providerUsageTokens: "Tokens (input / output / cached)",
    providerUsageCost: "Cost",
    providerNoUsage: "No requests in the last 7 days",
    providerTestTitle: "Upstream connectivity test",
    providerTestDescription:
      "The server sends one minimal request to DeepSeek with this provider's API key. The key never reaches the browser.",
    providerTestResult: "Result",
    providerTestSucceeded: "Upstream reachable",
    providerTestFailed: "Upstream returned an error",
    providerTestModel: "Test model",
    providerTestLatency: "Latency",
    providerBalance: "Balance",
    providerBalanceTitle: "Upstream balance",
    providerBalanceDescription:
      "Reads the DeepSeek account balance directly. A provider with no balance returns 402 and is parked.",
    providerBalanceAvailable: "Can serve requests",
    providerBalanceUnavailable: "DeepSeek did not return balance information.",
    yes: "Yes",
    no: "No",
    addProvider: "Add provider",
    noProviders: "No providers",
    noProvidersDescription:
      "Add a Provider with a DeepSeek API key to join the scheduling pool.",
    name: "Name",
    owner: "Owner",
    status: "Status",
    actions: "Actions",
    refresh: "Refresh",
    providerUpdated: "Provider updated",
    providerAdded: "Provider added",
    addProviderTitle: "Add provider",
    nameOptional: "Name (optional)",
    providerNameHelp: "Falls back to the Provider UUID when empty.",
    providerVisibility: "Visibility",
    providerVisibilityPrivate: "Private",
    providerVisibilityPublic: "Public",
    providerVisibilityHelp:
      "Private: only Consumers owned by this user may route to the provider. Public: every Consumer may route to it.",
    makeProviderPublic: "Publish to everyone",
    makeProviderPrivate: "Restrict to my Consumers",
    editProviderName: "Edit provider name",
    saveProviderName: "Save provider name",
    cancelProviderName: "Cancel provider name edit",
    deleteProvider: "Delete",
    deleteProviderTitle: "Delete provider?",
    deleteProviderDescription:
      "This hides the Provider from management and scheduling. Its API key, affinities, and history are retained.",
    confirmDeleteProvider: "Hide provider",
    providerDeleted: "Provider hidden",
    testProvider: "Test",
    testingProvider: "Testing upstream connectivity…",
    consumersTitle: "Tenant Consumers",
    consumersDescription:
      "Create one downstream Consumer per AI app so usage, errors, and key rotation remain isolated. Secrets are shown after creation and rotation only.",
    create: "Create",
    noConsumers: "No Consumers",
    noConsumersDescription:
      "Create a separate Consumer for each AI app before calling the proxy.",
    prefix: "Prefix",
    apiKey: "API KEY",
    createdAt: "Created",
    lastUsed: "Last used",
    requestArchive: "Archive diagnostics",
    requestArchiveHelp:
      "When enabled, this Consumer's request/response diagnostic previews are saved to SQLite.",
    requestArchiveUpdated: "Diagnostic archive setting updated",
    consumerEnabled: "Consumer enabled",
    consumerDisabledUpdated: "Consumer status updated",
    rotateConsumer: "Rotate and copy",
    rotateConsumerTitle: "Rotate API key?",
    rotateConsumerDescription:
      "The current API key will stop working immediately, and the new key will be copied to your clipboard. Continue?",
    confirmRotateConsumer: "Confirm rotation",
    consumerRotated: "New API key copied to clipboard",
    consumerRotateCopyFailed:
      "The API key was rotated, but could not be copied. Rotate it again to retry.",
    editConsumer: "Edit",
    editConsumerAriaLabel: "Edit consumer",
    saveConsumer: "Save",
    consumerUpdated: "Consumer updated",
    deleteConsumer: "Delete",
    deleteConsumerTitle: "Delete Consumer?",
    deleteConsumerDescription:
      "This hides the Consumer from the list and authentication. All call history and saved diagnostics are retained.",
    confirmDeleteConsumer: "Delete Consumer",
    consumerDeleted: "Consumer deleted; history retained",
    cancel: "Cancel",
    consumerAppHelp:
      "Create one Consumer per AI app and name it after the app so usage, troubleshooting, and key rotation stay isolated.",
    saveConsumerTitle: "Save this Consumer now",
    saveConsumerDescription:
      "It cannot be viewed again after closing. Do not put it in browser code, logs, or chat.",
    savedConsumer: "I stored it safely",
    copied: "Copied",
    usageTitle: "Usage pivot",
    usageDescription:
      "Cross-tabulate request Tokens by user, masked API token, model, and date. Adjust rows, columns, and aggregate data to isolate usage.",
    noUsage: "No usage yet",
    noUsageDescription:
      "Usage appears here by dimension after API calls in the selected period.",
    requests: "Requests",
    errors: "Errors",
    last24Hours: "Last 24 hours",
    last7Days: "Last 7 days",
    allUsers: "All users",
    allConsumers: "All Consumers",
    allModels: "All models",
    userLabel: "User",
    model: "Model",
    consumerLabel: "Consumer",
    date: "Date",
    rows: "Rows",
    columns: "Columns",
    data: "Data",
    hidden: "Hidden",
    inputTokens: "Input Tokens",
    cachedInputTokens: "Cached Input Tokens",
    outputTokens: "Output Tokens",
    requestCount: "Request count",
    usageRows: "aggregated rows",
    sortColumn: "Sort",
    total: "Total",
    clearFilters: "Clear filters",
    pivotFields: "Pivot fields",
    dataOrder: "Data column order",
    auditTitle: "Inference audit",
    auditDescription:
      "Request/response diagnostic previews are stored in SQLite; Authorization headers and upstream API keys are excluded.",
    noAudit: "No inference audit records",
    noAuditDescription:
      "A basic inference audit record is written when each proxy call terminates.",
    time: "Time",
    requestId: "Request ID",
    threadId: "Thread ID",
    copyThreadId: "Copy Thread ID",
    provider: "Provider",
    firstByteLatency: "First byte",
    totalLatency: "Total",
    requestSize: "Request size",
    responseSize: "Response size",
    requestTransportSize: "Request transport (compressed)",
    responseTransportSize: "Response transport (compressed)",
    compressionRatio: "Compression ratio",
    downstreamAcceptEncoding: "Downstream accepts",
    downstreamContentEncoding: "Downstream response encoding",
    upstreamAcceptEncoding: "Upstream request encoding",
    upstreamContentEncoding: "Upstream response encoding",
    networkTransport: "Network transport (compressed)",
    reasoningEffort: "Reasoning effort",
    cachedInput: "Cached input",
    details: "Details",
    filter: "Filter",
    filterUserId: "User ID",
    filterConsumer: "Consumer",
    filterProvider: "Provider",
    filterModel: "Model",
    errorCode: "Error code",
    errorMessage: "Error message",
    allStatuses: "All statuses",
    successfulCalls: "Successful",
    failedCalls: "Failed",
    auditResults: "calls",
    page: "Page",
    previousPage: "Previous",
    nextPage: "Next",
    requestDetail: "Request details",
    requestSummary: "Request summary",
    backToAudit: "Back to inference audit",
    requestHeaders: "Request headers",
    requestBody: "Request body",
    responseHeaders: "Response headers",
    headerSnapshotMissing: "Not captured",
    headerComparisonUnavailable:
      "One header snapshot was not captured; differences cannot be determined.",
    responseBody: "Response body",
    tokenUsage: "Token usage",
    cacheHitRate: "Cache hit rate",
    upstreamHttpProtocol: "Upstream HTTP protocol",
    httpStatusCode: "HTTP status code",
    finalResponse: "Final response",
    finalResponseUnavailable:
      "No response.output was found in the response body.",
    diagnosticData: "Request and response bodies",
    diagnosticDataDescription:
      "Saved only when diagnostics are enabled for this Consumer; sensitive credentials are excluded.",
    deleteAuditBodies: "Delete request and response bodies",
    deleteAuditBodiesTitle: "Delete request and response bodies?",
    deleteAuditBodiesDescription:
      "This cannot be undone. Call status, usage, and request/response header diagnostics will remain.",
    confirmDeleteAuditBodies: "Delete bodies",
    auditBodiesDeleted: "Request and response bodies deleted",
    auditBodiesDeletedDescription:
      "The request and response bodies for this audit record were permanently deleted. Call status, usage, and request/response header diagnostics remain available.",
    headerName: "Header",
    downstreamToLb: "Downstream → LB",
    lbToUpstream: "LB → Upstream",
    upstreamToLb: "Upstream → LB",
    lbToDownstream: "LB → Downstream",
    different: "Different",
    previewTruncated: "Preview truncated",
    affinitySource: "Affinity source",
    affinityRequestId: "Affinity request ID",
    affinityHash: "Affinity hash",
    previousRequest: "Previous request",
    nextRequest: "Next request",
    tools: "Tools",
    identityPermissions: "Identity and permissions",
    identityDescription:
      "Auth Mini manages the browser session; the backend only verifies access JWTs.",
    proxyBoundary: "Proxy boundary",
    proxyDescription:
      "Proxies DeepSeek capabilities only; no other vendor protocol compatibility.",
    unableLoad: "Unable to load",
    unknownError: "Unknown error",
    close: "Close",
    inflight: "Inflight",
    providerLoad: "In progress / limit",
    providerQueued: "Queued",
    providerConcurrency: "Upstream concurrency",
    providerConcurrencyDescription:
      "Set one concurrency limit for every upstream provider. The default is 3.",
    providerConcurrencyLimit: "Concurrent requests per provider",
    providerConcurrencyHelp:
      "A request occupies a slot from routing until the response ends or the request is cancelled, including streaming responses. Extra requests queue in arrival order. Changes apply immediately; lowering the limit lets active requests finish.",
    providerConcurrencyInvalid: "Enter a whole number greater than 0.",
    consumer: "Consumer",
    input: "Input",
    output: "Output",
    userId: "User ID",
    role: "Role",
    authIssuer: "Auth issuer",
    upstream: "Upstream",
    bodyLimit: "Body limit",
    affinityTtl: "Affinity TTL",
    upstreamHint:
      "DeepSeek API base URL; defaults to https://api.deepseek.com.",
    archiveRetention: "Request and response diagnostic retention (days)",
    statusActive: "Available",
    statusCooldown: "Cooling down",
    statusAuthError: "Authentication error",
    statusDisabled: "Disabled",
    statusUnknown: "Unknown",
    roleRoot: "Root",
    roleAdmin: "Administrator",
    roleUser: "Tenant user",
    adminAuditTitle: "Administrative operation audit",
    adminAuditDescription:
      "Provider, key, system, and audit management operations performed by root and administrators.",
    providerAuditTitle: "Provider audit",
    providerAuditDescription:
      "Summarize request outcomes by upstream provider, model, and hour to locate concentrated failures.",
    providerAuditChartDescription:
      "Hourly failure rate, request count, and input Tokens for the current filters.",
    providerAuditFailureRateChart: "Time — failure rate",
    providerAuditRequestCountChart: "Time — requests",
    providerAuditInputTokensChart: "Time — Input Tokens",
    providerAuditPeriod: "Data range",
    providerAuditTableDescription:
      "Each row represents one hour, one upstream provider, and one model.",
    providerAuditNoData: "No requests in the selected range",
    providerAuditNoDataDescription:
      "Hourly provider and model outcomes will appear here after requests are made.",
    providerAuditAllProviders: "All providers",
    providerAuditUnknownProvider: "Unidentified provider",
    providerAuditHour: "Hour",
    providerAuditSuccessRate: "Success rate",
    providerAuditFailureRate: "Failure rate",
    providerAuditSuccessfulRequests: "Successful requests",
    providerAuditFailedRequests: "Failed requests",
    providerAuditTotalRequests: "Total requests",
    providerAuditInputTokens: "Input Tokens",
    administrator: "Actor",
    action: "Action",
    target: "Target",
    clientIp: "Client IP",
    setupTitle: "Initialize DeepSeek-LB",
    setupDescription:
      "Connect the brand Auth Mini instance and bind the first verified user as the only root.",
    setupIssuer: "Auth Mini issuer",
    setupIssuerHelp:
      "Enter the Auth Mini HTTPS URL supplied by the brand. DeepSeek-LB connects to it; it does not deploy or manage it.",
    setupAudience: "JWT audience",
    connectAuth: "Connect Auth Mini",
    setupLogin: "Verify the root identity",
    setupLoginHelp:
      "After sign-in, this Auth Mini user_id becomes the DeepSeek-LB root.",
    finishSetup: "Bind root and finish setup",
    finishingSetup: "Finishing setup",
    setupStepConnect: "Connect identity",
    setupAuthenticated: "Identity verified",
    setupSecurity:
      "The setup endpoint closes immediately after completion. Later first-time users receive the user role.",
    usersTitle: "Users and request permissions",
    usersDescription:
      "Every user can use global Providers by default, while users can view and manage only their own Providers. Administrators and root can view all Providers. The list shows each user's cumulative top-ups, consumption, provided value, and available credit. Allow debt lets a user continue making requests after their prepaid balance is exhausted.",
    noUsers: "No users",
    noUsersDescription:
      "Users appear here after their first Auth Mini sign-in.",
    roleUpdated: "User role updated",
    allowDebt: "Allow debt",
    allowDebtUpdated: "Allow-debt setting updated",
    runtimeSettings: "Runtime settings",
    runtimeSettingsDescription:
      "These values live in SQLite app_meta. URLs and models apply immediately; body limits apply after restart.",
    availableModels: "Available models",
    availableModelsDescription:
      "Controls downstream-callable models. This list immediately drives /v1/models, the model-prices page, and Responses validation.",
    availableModelsHelp:
      "Enter one Model ID per line. Empty or duplicate IDs cannot be saved.",
    availableModelsSaved: "Available models saved",
    availableModelsRequired: "Enter at least one Model ID",
    modelPriceMultiplier: "Default price multiplier",
    modelPriceMultiplierHint:
      "Future requests charge official cost multiplied by this value; every request keeps the multiplier snapshot used for settlement.",
    allowAllUsersDebt: "Allow debt for all users",
    allowAllUsersDebtHint:
      "When enabled, every user can continue making requests after available credit is exhausted.",
    saveSettings: "Save settings",
    settingsSaved: "Settings saved",
    responseLimit: "Request & response body limit (bytes)",
  },
} satisfies Record<Locale, Record<string, string>>

function App() {
  const [locale, setLocale] = useState<Locale>(
    () => (localStorage.getItem("locale") as Locale) || "zh"
  )
  const configQuery = useQuery({
    queryKey: ["config"],
    queryFn: async ({ signal }) => {
      const response = await fetch("/api/config", { signal })
      if (!response.ok)
        throw new Error(`Configuration request failed (${response.status})`)
      return response.json() as Promise<PublicConfig>
    },
  })
  const config = configQuery.data

  useEffect(() => {
    document.documentElement.lang = locale === "zh" ? "zh-CN" : "en"
  }, [locale])
  const changeLocale = (next: Locale) => {
    localStorage.setItem("locale", next)
    setLocale(next)
  }

  const configError = configQuery.error ? message(configQuery.error) : ""
  if (!config && !configError) return <CenteredLoading />
  if (configError)
    return (
      <main className="mx-auto flex min-h-svh w-full max-w-2xl items-center p-4">
        <ErrorState message={configError} />
      </main>
    )
  if (!config) return <CenteredLoading />
  if (config?.setup_required)
    return (
      <TooltipProvider>
        <Setup locale={locale} setLocale={changeLocale} />
        <Toaster richColors />
      </TooltipProvider>
    )
  if (!config.auth_issuer || !config.auth_audience)
    return (
      <main className="mx-auto flex min-h-svh w-full max-w-2xl items-center p-4">
        <ErrorState message="Auth Mini issuer is missing" />
      </main>
    )
  return (
    <TooltipProvider>
      <AuthMiniProvider
        authMiniBaseUrl={config.auth_issuer}
        audiences={[config.auth_audience, "linkit.ntnl.io"]}
        autoRedirectToLogin
      >
        <LinkitProvider linkitBaseUrl="https://linkit.ntnl.io" lang={locale}>
          <LinkitLanguageSync setLocale={changeLocale} />
          <AuthenticatedConsole locale={locale} />
        </LinkitProvider>
      </AuthMiniProvider>
      <Toaster richColors />
    </TooltipProvider>
  )
}

function LinkitLanguageSync({
  setLocale,
}: {
  setLocale: (locale: Locale) => void
}) {
  const { languages } = useLinkit()
  useEffect(() => {
    const next = negotiateLocale(languages)
    if (next) setLocale(next)
  }, [languages, setLocale])
  return null
}

function negotiateLocale(languages: readonly string[]): Locale | undefined {
  for (const language of languages) {
    const base = language.toLowerCase().split("-")[0]
    if (base === "zh") return "zh"
    if (base === "en") return "en"
  }
  return undefined
}

function AuthenticatedConsole({ locale }: { locale: Locale }) {
  const { isAuthenticated, isReady, sdk } = useAuthMini()
  if (!isReady || !isAuthenticated || !sdk) return <CenteredLoading />
  return <Console sdk={sdk} locale={locale} />
}

function Setup({
  locale,
  setLocale,
}: {
  locale: Locale
  setLocale: (locale: Locale) => void
}) {
  const t = copy[locale]
  return (
    <main className="min-h-svh bg-muted/40 p-4 sm:p-6">
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-6">
        <header className="flex items-start justify-between gap-4 py-2">
          <div className="flex max-w-2xl flex-col gap-1">
            <h1 className="text-2xl font-semibold tracking-tight text-balance">
              {t.setupTitle}
            </h1>
            <p className="text-sm text-pretty text-muted-foreground">
              {t.setupDescription}
            </p>
          </div>
          <Button
            size="sm"
            variant="ghost"
            onClick={() => setLocale(locale === "zh" ? "en" : "zh")}
          >
            <LanguagesIcon data-icon="inline-start" />
            {t.english}
          </Button>
        </header>
        <div className="grid items-start gap-5 lg:grid-cols-[minmax(0,1fr)_minmax(22rem,1.15fr)]">
          <Card>
            <CardHeader>
              <CardTitle>{t.title}</CardTitle>
              <CardDescription>{t.setupSecurity}</CardDescription>
            </CardHeader>
            <CardContent>
              <p className="text-sm text-muted-foreground">
                {t.setupDescription}
              </p>
            </CardContent>
          </Card>
          <SetupForm locale={locale} />
        </div>
      </div>
    </main>
  )
}

function SetupForm({ locale }: { locale: Locale }) {
  const t = copy[locale]
  const [issuer, setIssuer] = useState("")
  const [audience, setAudience] = useState(
    window.location.hostname.replace(/^\[|\]$/g, "")
  )
  const [issuerError, setIssuerError] = useState("")
  const [configuredIssuer, setConfiguredIssuer] = useState<string | null>(null)
  const [pending, setPending] = useState(false)
  const [error, setError] = useState("")
  return (
    <Card>
      <CardHeader>
        <CardTitle>
          {configuredIssuer ? t.setupLogin : t.setupStepConnect}
        </CardTitle>
        <CardDescription>
          {configuredIssuer ? t.setupLoginHelp : t.setupIssuerHelp}
        </CardDescription>
      </CardHeader>
      <CardContent>
        {configuredIssuer ? (
          <AuthMiniProvider
            authMiniBaseUrl={configuredIssuer}
            audience={audience.trim()}
            autoRedirectToLogin={false}
          >
            <SetupIdentity
              audience={audience}
              issuer={configuredIssuer}
              locale={locale}
              pending={pending}
              setError={setError}
              setPending={setPending}
            />
          </AuthMiniProvider>
        ) : (
          <form
            onSubmit={(event) => {
              event.preventDefault()
              try {
                setConfiguredIssuer(new URL(issuer).origin)
                setIssuerError("")
              } catch {
                setIssuerError(t.setupIssuerHelp)
              }
            }}
          >
            <FieldGroup>
              <Field data-invalid={Boolean(issuerError)}>
                <FieldLabel htmlFor="setup-issuer">{t.setupIssuer}</FieldLabel>
                <Input
                  id="setup-issuer"
                  type="url"
                  inputMode="url"
                  autoComplete="url"
                  placeholder="https://auth.example.com"
                  value={issuer}
                  onChange={(event) => setIssuer(event.target.value)}
                  aria-invalid={Boolean(issuerError)}
                  required
                />
                <FieldError>{issuerError}</FieldError>
              </Field>
              <Field>
                <FieldLabel htmlFor="setup-audience">
                  {t.setupAudience}
                </FieldLabel>
                <Input
                  id="setup-audience"
                  value={audience}
                  onChange={(event) => setAudience(event.target.value)}
                  placeholder={window.location.hostname}
                  required
                />
              </Field>
              <Button type="submit" disabled={!issuer.trim()}>
                {t.connectAuth}
                <ChevronRightIcon data-icon="inline-end" />
              </Button>
            </FieldGroup>
          </form>
        )}
        {error && (
          <Alert className="mt-5" variant="destructive">
            <ShieldAlertIcon />
            <AlertTitle>{t.unableLoad}</AlertTitle>
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        )}
      </CardContent>
    </Card>
  )
}

function SetupIdentity({
  audience,
  issuer,
  locale,
  pending,
  setError,
  setPending,
}: {
  audience: string
  issuer: string
  locale: Locale
  pending: boolean
  setError: (error: string) => void
  setPending: (pending: boolean) => void
}) {
  const t = copy[locale]
  const { isAuthenticated, sdk, signIn } = useAuthMini()
  if (!isAuthenticated || !sdk)
    return (
      <div className="flex flex-col gap-5">
        <Alert>
          <CheckCircle2Icon />
          <AlertTitle>{t.setupLogin}</AlertTitle>
          <AlertDescription>{t.setupLoginHelp}</AlertDescription>
        </Alert>
        <Button onClick={signIn}>{t.continueAuthMini}</Button>
      </div>
    )
  return (
    <div className="flex flex-col gap-5">
      <Alert>
        <CheckCircle2Icon />
        <AlertTitle>{t.setupAuthenticated}</AlertTitle>
        <AlertDescription>{t.setupLoginHelp}</AlertDescription>
      </Alert>
      <Button
        disabled={pending}
        onClick={() => {
          setPending(true)
          setError("")
          void api(sdk, "/api/setup", {
            method: "POST",
            body: JSON.stringify({
              auth_issuer: issuer,
              auth_audience: audience.trim(),
            }),
          })
            .then(() => window.location.reload())
            .catch((cause) => {
              setError(message(cause, t))
              setPending(false)
            })
        }}
      >
        {pending && <Spinner data-icon="inline-start" />}
        {pending ? t.finishingSetup : t.finishSetup}
      </Button>
    </div>
  )
}

function Console({ sdk, locale }: { sdk: AuthSdk; locale: Locale }) {
  const location = useLocation()
  const navigate = useNavigate()
  const {
    data: user,
    error: userError,
    loading: userLoading,
  } = useApiQuery<User>(sdk, "/api/me")
  const t = copy[locale]
  const isAdministrator = user?.role === "root" || user?.role === "admin"
  const requestedPage = pageForPath(location.pathname)
  const page =
    ["system-resources", "admin-audit", "provider-audit"].includes(
      requestedPage
    ) && !isAdministrator
      ? "dashboard"
      : requestedPage
  const navigationGroups: NavigationGroup[] = [
    {
      label: t.navigationWorkspace,
      items: [
        ["dashboard", CircleGaugeIcon],
        ["providers", BoxesIcon],
        ["consumers", KeyRoundIcon],
        ["topups", WalletCardsIcon],
      ],
    },
    {
      label: t.navigationIntegrations,
      items: [
        ["codex-integration", PencilIcon],
        ["dsh-integration", KeyRoundIcon],
        ["opencode-integration", KeyRoundIcon],
        ["direct-api-integration", BookOpenIcon],
      ],
    },
    {
      label: t.navigationData,
      items: [
        ["usage", ActivityIcon],
        ["audit", ScrollTextIcon],
        ["model-prices", CpuIcon],
      ],
    },
    ...(isAdministrator
      ? [
          {
            label: t.navigationAdministration,
            items: [
              ["system-resources", ServerIcon],
              ["users", UserRoundCogIcon],
              ["admin-audit", ShieldCheckIcon],
              ["provider-audit", ActivityIcon],
            ],
          } satisfies NavigationGroup,
        ]
      : []),
  ]
  return (
    <SidebarProvider>
      <Sidebar collapsible="icon">
        <SidebarHeader className="border-b p-3">
          <div className="flex min-w-0 items-center gap-2">
            <img
              alt=""
              aria-hidden="true"
              className="size-7 shrink-0"
              src="/deepseek.svg"
            />
            <div className="flex min-w-0 flex-col gap-0.5 group-data-[collapsible=icon]:hidden">
              <strong className="truncate text-sm">{t.title}</strong>
              <span className="truncate text-xs text-muted-foreground">
                {t.subtitle}
              </span>
            </div>
          </div>
        </SidebarHeader>
        <SidebarContent>
          {navigationGroups.map((group) => (
            <SidebarGroup key={group.label}>
              <SidebarGroupLabel>{group.label}</SidebarGroupLabel>
              <SidebarGroupContent>
                <SidebarMenu>
                  {group.items.map(([item, Icon]) => (
                    <SidebarMenuItem key={item}>
                      <SidebarMenuButton
                        isActive={page === item}
                        onClick={() => navigate(`/${item}`)}
                      >
                        <Icon />
                        <span>{t[item]}</span>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  ))}
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
          ))}
        </SidebarContent>
        <SidebarFooter className="border-t p-3">
          <SidebarMenu>
            <SidebarMenuItem>
              <SidebarMenuButton
                isActive={page === "settings"}
                onClick={() => navigate("/settings")}
              >
                <SettingsIcon />
                <span>{t.settings}</span>
              </SidebarMenuButton>
            </SidebarMenuItem>
          </SidebarMenu>
        </SidebarFooter>
      </Sidebar>
      <SidebarInset className="overflow-auto">
        <header className="sticky top-0 flex h-14 items-center gap-3 border-b bg-background px-4">
          <SidebarTrigger />
          <Separator orientation="vertical" className="h-5!" />
          <Badge variant="outline">
            {user ? roleLabel(user.role, locale) : t.roleLoading}
          </Badge>
          <div className="ml-auto">
            <LinkitMyInfo />
          </div>
        </header>
        <div className="flex flex-1 flex-col gap-5 p-4 md:p-6">
          <PageHeader
            title={pageTitle(page, locale)}
            description={pageDescription(page, locale)}
          />
          {userError ? (
            <ErrorState message={message(userError)} />
          ) : userLoading || !user ? (
            <LoadingTable />
          ) : (
            <Routes>
              <Route path="/" element={<Navigate replace to="/dashboard" />} />
              <Route
                path="/dashboard"
                element={<Dashboard sdk={sdk} locale={locale} />}
              />
              <Route
                path="/providers"
                element={<Providers sdk={sdk} locale={locale} user={user} />}
              />
              <Route
                path="/consumers"
                element={<Consumers sdk={sdk} locale={locale} />}
              />
              <Route
                path="/codex-integration"
                element={<CodexIntegrationPage sdk={sdk} locale={locale} />}
              />
              <Route
                path="/dsh-integration"
                element={<DshIntegrationPage sdk={sdk} locale={locale} />}
              />
              <Route
                path="/opencode-integration"
                element={<OpenCodeIntegrationPage sdk={sdk} locale={locale} />}
              />
              <Route
                path="/direct-api-integration"
                element={<DirectApiIntegrationPage locale={locale} />}
              />
              <Route
                path="/usage"
                element={<UsagePage sdk={sdk} locale={locale} user={user} />}
              />
              <Route
                path="/audit"
                element={
                  <AuditPage
                    sdk={sdk}
                    locale={locale}
                    onOpenDetail={(id) =>
                      navigate({
                        pathname: `/audit/${id}`,
                        search: location.search,
                      })
                    }
                  />
                }
              />
              <Route
                path="/audit/:auditId"
                element={<RequestDetailPage sdk={sdk} locale={locale} />}
              />
              <Route
                path="/topups"
                element={<TopupsPage sdk={sdk} locale={locale} />}
              />
              <Route
                path="/model-prices"
                element={<ModelPricesPage sdk={sdk} locale={locale} />}
              />
              <Route
                path="/system-resources"
                element={
                  isAdministrator ? (
                    <SystemResourcesCard sdk={sdk} locale={locale} />
                  ) : (
                    <Navigate replace to="/dashboard" />
                  )
                }
              />
              <Route
                path="/admin-audit"
                element={
                  isAdministrator ? (
                    <AdminAuditPage sdk={sdk} locale={locale} />
                  ) : (
                    <Navigate replace to="/dashboard" />
                  )
                }
              />
              <Route
                path="/provider-audit"
                element={
                  isAdministrator ? (
                    <ProviderAuditPage sdk={sdk} locale={locale} />
                  ) : (
                    <Navigate replace to="/dashboard" />
                  )
                }
              />
              <Route
                path="/users"
                element={<UsersPage sdk={sdk} locale={locale} user={user} />}
              />
              <Route
                path="/settings"
                element={<SettingsPage sdk={sdk} user={user} locale={locale} />}
              />
              <Route path="*" element={<Navigate replace to="/dashboard" />} />
            </Routes>
          )}
        </div>
      </SidebarInset>
    </SidebarProvider>
  )
}

function PageHeader({
  title,
  description,
}: {
  title: string
  description: string
}) {
  return (
    <div className="flex flex-col gap-1">
      <h1 className="text-2xl font-semibold tracking-tight text-balance">
        {title}
      </h1>
      <p className="max-w-3xl text-sm text-pretty text-muted-foreground">
        {description}
      </p>
    </div>
  )
}

type CodexPlatformInstruction = {
  configPath: string
  hint: string
}

type FileIntegrationContent = {
  integration: string
  consumerName: string
  description: string
  preparationTitle: string
  preparationDescription: string
  manualTitle: string
  manualDescription: string
  manualConfigInstruction: string
  manualTokenInstruction: string
  manualVerifyTitle: string
  manualVerifyDescription: string
  copyLabel: string
  copiedLabel: string
  automaticTitle: string
  automaticDescription: string
  findConfig: string
  findConfigDescription: string
  platformInstructions: Record<CodexPlatform, CodexPlatformInstruction>
  selectConfig: string
  selectConfigOnly: string
  configInvalid: string
  restartTitle: string
  restartDescription: string
  configWritten: string
  configError: string
  configWriteFailed: string
  consumerCleanupFailed: string
  browserRequired: string
  browserRequiredDescription: string
  fileName: string
  pickerDescription: string
  accept: Record<string, string[]>
}

type DirectApiContent = {
  preparationTitle: string
  preparationDescription: string
  title: string
  description: string
  firstStep: string
  secondStep: string
  modelPlaceholder: string
  sensitiveTitle: string
  sensitiveDescription: string
}

type DshIntegrationContent = {
  integration: string
  consumerName: string
  description: string
  preparationTitle: string
  preparationDescription: string
  manualTitle: string
  manualInstruction: string
  apiKeyInstruction: string
  modelCatalogInstruction: string
  automaticTitle: string
  automaticDescription: string
  directoryPath: string
  selectDirectory: string
  settingsMissing: string
  configInvalid: string
  modelListUnavailable: string
  configWritten: string
  configError: string
  configWriteFailed: string
  configRollbackFailed: string
  consumerCleanupFailed: string
  browserRequiredDescription: string
}

const integrationCopy: Record<
  Locale,
  {
    codex: FileIntegrationContent
    dsh: DshIntegrationContent
    opencode: FileIntegrationContent
    direct: DirectApiContent
  }
> = {
  zh: {
    codex: {
      integration: "Codex",
      consumerName: "Codex",
      description:
        "手动编辑本机 Codex 配置，或授权浏览器读取和写入配置文件并自动创建专用 Consumer。",
      preparationTitle: "专用 Consumer",
      preparationDescription:
        "先在“下游消费者”中为 Codex 创建独立 Consumer 并保存只展示一次的密钥。手动配置需要把密钥粘贴到配置文件；自动配置只会把新密钥写入你已授权的本地文件。",
      manualTitle: "手动配置",
      manualDescription:
        "编辑用户级 ~/.codex/config.toml，将 DeepSeek-LB 设为 Codex 的模型提供方。",
      manualConfigInstruction:
        "将以下内容合并到 ~/.codex/config.toml；如果已有其他设置，只更新 model_provider 与 model_providers.deepseek-lb。",
      manualTokenInstruction:
        "将 <YOUR_CONSUMER_KEY> 替换为“下游消费者”页面创建的 Consumer 密钥。不要把真实密钥提交到 Git 或共享配置仓库。",
      manualVerifyTitle: "重启并验证",
      manualVerifyDescription:
        "保存配置后重启 Codex。若请求失败，先用 /v1/models 验证该 Consumer 仍有效，再检查 config.toml 中的 base_url、wire_api 和密钥。",
      copyLabel: "复制",
      copiedLabel: "已复制",
      automaticTitle: "浏览器自动配置",
      automaticDescription:
        "授权浏览器读取 config.toml 后，页面会创建专用 Consumer 并将配置写回该文件；其他 TOML 配置会保留。",
      findConfig: "在系统文件选择器中找到 config.toml。",
      findConfigDescription: "打开以下本机文件：",
      platformInstructions: {
        macos: {
          configPath: "~/.codex/config.toml",
          hint: "按 ⌘⇧G，输入上方路径后回车；若手动浏览目录，按 ⌘⇧. 显示隐藏文件。",
        },
        windows: {
          configPath: "%USERPROFILE%\\.codex\\config.toml",
          hint: "将上方路径粘贴到文件名框或地址栏；若未显示 .codex，请在“查看”中启用“隐藏的项目”。",
        },
        linux: {
          configPath: "~/.codex/config.toml",
          hint: "按 Ctrl+L 输入上方路径；若手动浏览目录，按 Ctrl+H 显示隐藏文件。",
        },
        other: {
          configPath: "~/.codex/config.toml",
          hint: "在用户主目录中打开 .codex，再选择 config.toml。必要时在文件选择器中显示隐藏文件。",
        },
      },
      selectConfig: "选择 config.toml、创建 Consumer 并写入",
      selectConfigOnly: "请只选择名为 config.toml 的文件。",
      configInvalid:
        "config.toml 无法解析或包含当前流程无法安全更新的配置。文件未修改，也没有创建 Consumer。",
      restartTitle: "重启 Codex",
      restartDescription:
        "页面只会更新 model_provider 与 model_providers.deepseek-lb；其他 TOML 配置保持不变。写入完成后重启 Codex。",
      configWritten: "已写入本机 Codex 配置；请重启 Codex。",
      configError: "配置未完成",
      configWriteFailed:
        "无法完成配置。请确认已授权文件访问，并选择用户目录下的 .codex/config.toml。",
      consumerCleanupFailed:
        "无法写入 config.toml，且自动撤销新建 Consumer 失败。请在“下游消费者”中撤销名称为 Codex 的新记录。",
      browserRequired: "需要 Chrome 或 Edge",
      browserRequiredDescription:
        "此操作依赖浏览器的本地文件访问能力。请用最新版 Chrome 或 Edge 打开此页面后重试。",
      fileName: "config.toml",
      pickerDescription: "Codex config.toml",
      accept: { "application/toml": [".toml"] },
    },
    dsh: {
      integration: "DSH（DeepSeek Harness）",
      consumerName: "DSH (DeepSeek Harness)",
      description:
        "手动添加 DeepSeek-LB Provider，或授权浏览器读取和更新 ~/.dsh 中的 YAML 配置。自动流程会创建一个专用 Consumer，并只把密钥写入本机凭据文件。",
      preparationTitle: "专用 Consumer",
      preparationDescription:
        "先在“下游消费者”中为 DSH 创建独立 Consumer 并保存只展示一次的密钥。手动配置需要粘贴该密钥；自动配置只会把新密钥写入你授权的 ~/.dsh/.credentials.yaml。",
      manualTitle: "手动配置",
      manualInstruction:
        "在 DSH 中打开 设置 → 模型 → 添加自定义提供方，然后填写以下值：",
      apiKeyInstruction:
        "API 密钥：在“下游消费者”页面创建或生成一条密钥后粘贴。",
      modelCatalogInstruction:
        "保存 Provider 后，点击“获取可用模型”读取当前 Consumer 可调用的模型目录。",
      automaticTitle: "自动写入 ~/.dsh",
      automaticDescription:
        "选择 ~/.dsh 目录后，页面会解析 settings.yaml、创建专用 Consumer，并更新 settings.yaml 与 .credentials.yaml。其他 YAML 配置会保留。",
      directoryPath: "~/.dsh",
      selectDirectory: "选择 ~/.dsh 并自动配置",
      settingsMissing:
        "所选目录中没有 settings.yaml。文件未修改，也没有创建 Consumer。",
      configInvalid:
        "settings.yaml 或 .credentials.yaml 无法解析，或包含当前流程无法安全更新的结构。文件未修改，也没有创建 Consumer。",
      modelListUnavailable: "当前无法读取可用模型，暂不能自动写入 DSH 配置。",
      configWritten: "已写入 DSH 配置与专用 Consumer 密钥；请重启 DSH。",
      configError: "DSH 配置未完成",
      configWriteFailed:
        "无法完成配置。请确认已授权目录访问，并选择用户目录下的 ~/.dsh。",
      configRollbackFailed:
        "写入 .credentials.yaml 失败，且无法恢复 settings.yaml。请检查 ~/.dsh 中的 DeepSeek-LB 配置后再重试。",
      consumerCleanupFailed:
        "无法完成配置，且自动撤销新建 Consumer 失败。请在“下游消费者”中撤销名称为 DSH (DeepSeek Harness) 的新记录。",
      browserRequiredDescription:
        "此操作依赖浏览器的本地目录访问能力。请用最新版 Chrome 或 Edge 打开此页面后重试。",
    },
    opencode: {
      integration: "OpenCode",
      consumerName: "OpenCode",
      description:
        "手动编辑本机 OpenCode 配置，或授权浏览器读取和写入 JSONC 文件并自动创建专用 Consumer。",
      preparationTitle: "专用 Consumer",
      preparationDescription:
        "先在“下游消费者”中为 OpenCode 创建独立 Consumer 并保存只展示一次的密钥。手动配置需要把密钥粘贴到 JSONC 文件；自动配置只会把新密钥写入你已授权的本地文件。",
      manualTitle: "手动配置",
      manualDescription:
        "编辑用户级 ~/.config/opencode/opencode.jsonc，将 DeepSeek-LB 添加为 OpenAI 兼容 Provider。",
      manualConfigInstruction:
        "将以下 provider.deepseek-lb 片段合并到 ~/.config/opencode/opencode.jsonc，并把示例模型替换为 /v1/models 返回的可用 model id。",
      manualTokenInstruction:
        "将 <YOUR_CONSUMER_KEY> 替换为“下游消费者”页面创建的 Consumer 密钥。不要把真实密钥提交到 Git 或共享配置仓库。",
      manualVerifyTitle: "重启并验证",
      manualVerifyDescription:
        "保存配置后重启 OpenCode，并选择 deepseek-lb/deepseek-flash（或模型目录中的其他可用模型）。若请求失败，先用 /v1/models 验证 Consumer 和 baseURL。",
      copyLabel: "复制",
      copiedLabel: "已复制",
      automaticTitle: "浏览器自动配置",
      automaticDescription:
        "授权浏览器读取 opencode.jsonc 后，页面会创建专用 Consumer 并将 provider.deepseek-lb 写回该文件；其他 JSONC 配置会保留。",
      findConfig: "在系统文件选择器中找到 opencode.jsonc。",
      findConfigDescription: "打开以下本机文件：",
      platformInstructions: {
        macos: {
          configPath: "~/.config/opencode/opencode.jsonc",
          hint: "按 ⌘⇧G，输入上方路径后回车；若手动浏览目录，按 ⌘⇧. 显示隐藏文件。",
        },
        windows: {
          configPath: "%APPDATA%\\opencode\\opencode.jsonc",
          hint: "将上方路径粘贴到文件名框或地址栏；AppData 默认隐藏，可在“查看”中启用“隐藏的项目”。",
        },
        linux: {
          configPath: "~/.config/opencode/opencode.jsonc",
          hint: "按 Ctrl+L 输入上方路径；若手动浏览目录，按 Ctrl+H 显示隐藏文件。",
        },
        other: {
          configPath: "~/.config/opencode/opencode.jsonc",
          hint: "在用户主目录中打开 .config/opencode，再选择 opencode.jsonc。必要时在文件选择器中显示隐藏文件。",
        },
      },
      selectConfig: "选择 opencode.jsonc、创建 Consumer 并写入",
      selectConfigOnly: "请只选择名为 opencode.jsonc 的文件。",
      configInvalid:
        "opencode.jsonc 无法解析或包含当前流程无法安全更新的配置。文件未修改，也没有创建 Consumer。",
      restartTitle: "重启 OpenCode",
      restartDescription:
        "页面只会更新 provider.deepseek-lb；其他 JSONC 配置保持不变。写入完成后重启 OpenCode。",
      configWritten: "已写入本机 OpenCode 配置；请重启 OpenCode。",
      configError: "配置未完成",
      configWriteFailed:
        "无法完成配置。请确认已授权文件访问，并选择用户目录下的 .config/opencode/opencode.jsonc。",
      consumerCleanupFailed:
        "无法写入 opencode.jsonc，且自动撤销新建 Consumer 失败。请在“下游消费者”中撤销名称为 OpenCode 的新记录。",
      browserRequired: "需要 Chrome 或 Edge",
      browserRequiredDescription:
        "此操作依赖浏览器的本地文件访问能力。请用最新版 Chrome 或 Edge 打开此页面后重试。",
      fileName: "opencode.jsonc",
      pickerDescription: "OpenCode opencode.jsonc",
      accept: { "application/json": [".jsonc"] },
    },
    direct: {
      preparationTitle: "创建 Consumer",
      preparationDescription:
        "在“下游消费者”中为这类直接 API 调用创建独立 Consumer，并立即保存只展示一次的密钥。以下示例以 <YOUR_CONSUMER_KEY> 表示该密钥。",
      title: "直接 API 调用",
      description:
        "DeepSeek-LB 代理 OpenAI /v1 兼容接口。直接用 Bearer Consumer 密钥调用所需端点，无需编辑任何本地配置文件。",
      firstStep: "先验证服务可访问的模型列表。",
      secondStep:
        "再调用对话补全；Codex 等客户端可使用代理后的 /v1/responses 端点。",
      modelPlaceholder: "替换为可用模型 ID",
      sensitiveTitle: "密钥与排障边界",
      sensitiveDescription:
        "Consumer 密钥只应放在本机安全存储、密钥管理器或受控环境变量中。不要提交到 Git、粘贴到工单或放入浏览器。请求状态、Provider 选择、耗时与用量可在“审计”中追踪。",
    },
  },
  en: {
    codex: {
      integration: "Codex",
      consumerName: "Codex",
      description:
        "Edit the local Codex configuration manually, or authorize the browser to write it and create a dedicated Consumer automatically.",
      preparationTitle: "Dedicated Consumer",
      preparationDescription:
        "First create a dedicated Consumer for Codex on the Consumers page and save its one-time secret. Manual setup pastes it into the config file; automatic setup writes the new secret only to the local file you authorize.",
      manualTitle: "Manual configuration",
      manualDescription:
        "Edit the user-level ~/.codex/config.toml and point Codex at DeepSeek-LB.",
      manualConfigInstruction:
        "Merge this into ~/.codex/config.toml. If other settings already exist, update only model_provider and model_providers.deepseek-lb.",
      manualTokenInstruction:
        "Replace <YOUR_CONSUMER_KEY> with the Consumer secret created on the Consumers page. Never commit a real secret to Git or a shared config repository.",
      manualVerifyTitle: "Restart and verify",
      manualVerifyDescription:
        "Save the file and restart Codex. If a request fails, verify the Consumer with /v1/models, then check base_url, wire_api, and the key in config.toml.",
      copyLabel: "Copy",
      copiedLabel: "Copied",
      automaticTitle: "Configure in the browser",
      automaticDescription:
        "After you authorize config.toml, the page creates a dedicated Consumer and writes the configuration back while retaining other TOML settings.",
      findConfig: "Find config.toml in the system file picker.",
      findConfigDescription: "Open this local file:",
      platformInstructions: {
        macos: {
          configPath: "~/.codex/config.toml",
          hint: "Press ⌘⇧G, enter the path above, then press Return. If you browse manually, press ⌘⇧. to show hidden files.",
        },
        windows: {
          configPath: "%USERPROFILE%\\.codex\\config.toml",
          hint: "Paste the path above into the File name box or address bar. If .codex is not visible, enable View > Hidden items.",
        },
        linux: {
          configPath: "~/.codex/config.toml",
          hint: "Press Ctrl+L to enter the path above. If you browse manually, press Ctrl+H to show hidden files.",
        },
        other: {
          configPath: "~/.codex/config.toml",
          hint: "Open .codex in your home directory, then choose config.toml. Show hidden files in the picker if needed.",
        },
      },
      selectConfig: "Choose config.toml, create Consumer, and write",
      selectConfigOnly: "Choose only the file named config.toml.",
      configInvalid:
        "config.toml could not be parsed or cannot be safely updated by this flow. The file was not changed and no Consumer was created.",
      restartTitle: "Restart Codex",
      restartDescription:
        "Only model_provider and model_providers.deepseek-lb are updated. All other TOML settings stay unchanged. Restart Codex after the write completes.",
      configWritten:
        "Your local Codex configuration is updated. Restart Codex to use it.",
      configError: "Configuration was not completed",
      configWriteFailed:
        "Could not complete configuration. Confirm file access was allowed and select .codex/config.toml from your home directory.",
      consumerCleanupFailed:
        "config.toml could not be written and the new Consumer could not be deleted automatically. Delete the new Consumer named Codex on the Consumers page.",
      browserRequired: "Chrome or Edge is required",
      browserRequiredDescription:
        "This action requires browser access to local files. Open this page in the latest Chrome or Edge and try again.",
      fileName: "config.toml",
      pickerDescription: "Codex config.toml",
      accept: { "application/toml": [".toml"] },
    },
    dsh: {
      integration: "DSH (DeepSeek Harness)",
      consumerName: "DSH (DeepSeek Harness)",
      description:
        "Add a DeepSeek-LB provider manually or authorize the browser to read and update the YAML configuration in ~/.dsh. The automatic flow creates a dedicated Consumer and writes its secret only to the local credentials file.",
      preparationTitle: "Dedicated Consumer",
      preparationDescription:
        "First create a dedicated Consumer for DSH on the Consumers page and save its one-time secret. Manual setup pastes that secret; automatic setup writes the new secret only to the ~/.dsh/.credentials.yaml file you authorize.",
      manualTitle: "Manual configuration",
      manualInstruction:
        "In DSH, open Settings → Models → Add custom provider, then enter these values:",
      apiKeyInstruction:
        "API key: create or generate a key on the Consumers page, then paste it here.",
      modelCatalogInstruction:
        "After saving the Provider, click Get available models to read the models available to this Consumer.",
      automaticTitle: "Write ~/.dsh automatically",
      automaticDescription:
        "After you choose ~/.dsh, this page parses settings.yaml, creates a dedicated Consumer, and updates settings.yaml plus .credentials.yaml. Other YAML configuration is retained.",
      directoryPath: "~/.dsh",
      selectDirectory: "Choose ~/.dsh and configure automatically",
      settingsMissing:
        "The selected directory does not contain settings.yaml. No file was changed and no Consumer was created.",
      configInvalid:
        "settings.yaml or .credentials.yaml could not be parsed, or contains a structure this flow cannot safely update. No file was changed and no Consumer was created.",
      modelListUnavailable:
        "The available model list could not be read, so DSH configuration cannot be written automatically yet.",
      configWritten:
        "DSH configuration and the dedicated Consumer key are written. Restart DSH to use them.",
      configError: "DSH configuration was not completed",
      configWriteFailed:
        "Could not complete configuration. Confirm directory access was allowed and choose ~/.dsh from your home directory.",
      configRollbackFailed:
        "Writing .credentials.yaml failed and settings.yaml could not be restored. Check the DeepSeek-LB configuration in ~/.dsh before retrying.",
      consumerCleanupFailed:
        "Configuration could not be completed and the new Consumer could not be deleted automatically. Delete the Consumer named DSH (DeepSeek Harness) on the Consumers page.",
      browserRequiredDescription:
        "This action requires browser access to local directories. Open this page in the latest Chrome or Edge and try again.",
    },
    opencode: {
      integration: "OpenCode",
      consumerName: "OpenCode",
      description:
        "Edit the local OpenCode configuration manually, or authorize the browser to write the JSONC file and create a dedicated Consumer automatically.",
      preparationTitle: "Dedicated Consumer",
      preparationDescription:
        "First create a dedicated Consumer for OpenCode on the Consumers page and save its one-time secret. Manual setup pastes it into the JSONC file; automatic setup writes the new secret only to the local file you authorize.",
      manualTitle: "Manual configuration",
      manualDescription:
        "Edit the user-level ~/.config/opencode/opencode.jsonc and add DeepSeek-LB as an OpenAI-compatible provider.",
      manualConfigInstruction:
        "Merge this provider.deepseek-lb block into ~/.config/opencode/opencode.jsonc and replace the example with a model ID returned by /v1/models.",
      manualTokenInstruction:
        "Replace <YOUR_CONSUMER_KEY> with the Consumer secret created on the Consumers page. Never commit a real secret to Git or a shared config repository.",
      manualVerifyTitle: "Restart and verify",
      manualVerifyDescription:
        "Save the file, restart OpenCode, and choose deepseek-lb/deepseek-flash (or another available model). If a request fails, verify the Consumer and baseURL with /v1/models.",
      copyLabel: "Copy",
      copiedLabel: "Copied",
      automaticTitle: "Configure in the browser",
      automaticDescription:
        "After you authorize opencode.jsonc, the page creates a dedicated Consumer and writes provider.deepseek-lb back while retaining other JSONC settings.",
      findConfig: "Find opencode.jsonc in the system file picker.",
      findConfigDescription: "Open this local file:",
      platformInstructions: {
        macos: {
          configPath: "~/.config/opencode/opencode.jsonc",
          hint: "Press ⌘⇧G, enter the path above, then press Return. If you browse manually, press ⌘⇧. to show hidden files.",
        },
        windows: {
          configPath: "%APPDATA%\\opencode\\opencode.jsonc",
          hint: "Paste the path above into the File name box or address bar. AppData is hidden by default; enable View > Hidden items.",
        },
        linux: {
          configPath: "~/.config/opencode/opencode.jsonc",
          hint: "Press Ctrl+L to enter the path above. If you browse manually, press Ctrl+H to show hidden files.",
        },
        other: {
          configPath: "~/.config/opencode/opencode.jsonc",
          hint: "Open .config/opencode in your home directory, then choose opencode.jsonc. Show hidden files in the picker if needed.",
        },
      },
      selectConfig: "Choose opencode.jsonc, create Consumer, and write",
      selectConfigOnly: "Choose only the file named opencode.jsonc.",
      configInvalid:
        "opencode.jsonc could not be parsed or cannot be safely updated by this flow. The file was not changed and no Consumer was created.",
      restartTitle: "Restart OpenCode",
      restartDescription:
        "Only provider.deepseek-lb is updated. All other JSONC settings stay unchanged. Restart OpenCode after the write completes.",
      configWritten:
        "Your local OpenCode configuration is updated. Restart OpenCode to use it.",
      configError: "Configuration was not completed",
      configWriteFailed:
        "Could not complete configuration. Confirm file access was allowed and select .config/opencode/opencode.jsonc from your home directory.",
      consumerCleanupFailed:
        "opencode.jsonc could not be written and the new Consumer could not be deleted automatically. Delete the new Consumer named OpenCode on the Consumers page.",
      browserRequired: "Chrome or Edge is required",
      browserRequiredDescription:
        "This action requires browser access to local files. Open this page in the latest Chrome or Edge and try again.",
      fileName: "opencode.jsonc",
      pickerDescription: "OpenCode opencode.jsonc",
      accept: { "application/json": [".jsonc"] },
    },
    direct: {
      preparationTitle: "Create a Consumer",
      preparationDescription:
        "Create a dedicated Consumer for direct API calls on the Consumers page and save its one-time secret immediately. The examples below use <YOUR_CONSUMER_KEY> for that secret.",
      title: "Direct API calls",
      description:
        "DeepSeek-LB proxies the DeepSeek OpenAI-compatible API under /v1. Authenticate with a Bearer Consumer secret; no local configuration file is edited.",
      firstStep: "First confirm the model list the service exposes.",
      secondStep:
        "Then run a chat completion; clients such as Codex use the proxied /v1/responses endpoint.",
      modelPlaceholder: "replace with an available model ID",
      sensitiveTitle: "Key handling and troubleshooting",
      sensitiveDescription:
        "Keep Consumer secrets only in secure local storage, a secrets manager, or controlled environment variables. Never commit them to Git, paste them into tickets, or put them in browser code. Trace request state, provider selection, latency, and usage in Audit.",
    },
  },
}

type ConfigFileHandle = {
  name: string
  getFile: () => Promise<File>
  createWritable: () => Promise<{
    write: (data: string) => Promise<void>
    close: () => Promise<void>
  }>
}

type FilePickerWindow = Window & {
  showOpenFilePicker?: (options: {
    multiple: boolean
    types: Array<{
      description: string
      accept: Record<string, string[]>
    }>
  }) => Promise<ConfigFileHandle[]>
}

type ConfigDirectoryHandle = {
  getFileHandle: (
    name: string,
    options?: { create?: boolean }
  ) => Promise<ConfigFileHandle>
}

type DirectoryPickerWindow = Window & {
  showDirectoryPicker?: (options: {
    mode: "readwrite"
  }) => Promise<ConfigDirectoryHandle>
}

type FileIntegrationConfig = {
  modelIds: string[]
  updateConfig: (content: string, token: string) => string
  manualConfig: (origin: string) => string
}

function CodexIntegrationPage({
  sdk,
  locale,
}: {
  sdk: AuthSdk
  locale: Locale
}) {
  const content = integrationCopy[locale].codex
  const origin = window.location.origin

  return (
    <FileIntegrationPage
      sdk={sdk}
      content={content}
      config={{
        modelIds: [],
        updateConfig: (config, token) =>
          updateCodexConfig(config, token, origin),
        manualConfig: (manualOrigin) =>
          codexProviderBlock(manualOrigin, "<YOUR_CONSUMER_KEY>"),
      }}
    />
  )
}

function OpenCodeIntegrationPage({
  sdk,
  locale,
}: {
  sdk: AuthSdk
  locale: Locale
}) {
  const content = integrationCopy[locale].opencode
  const origin = window.location.origin
  const { data: settings } = useApiQuery<SettingsData>(sdk, "/api/settings")
  const modelIds = settings?.available_model_ids ?? []

  return (
    <FileIntegrationPage
      sdk={sdk}
      content={content}
      config={{
        modelIds,
        updateConfig: (config, token) =>
          updateOpenCodeConfig(config, token, origin, modelIds),
        manualConfig: (configOrigin) => `{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "deepseek-lb": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "DeepSeek-LB",
      "options": {
        "baseURL": "${configOrigin}/v1",
        "apiKey": "<YOUR_CONSUMER_KEY>"
      },
      "models": {
${modelIds.map((model) => `        "${model}": { "name": "${model}" }`).join(",\n")}
      }
    }
  }
}`,
      }}
    />
  )
}

function DshIntegrationPage({ sdk, locale }: { sdk: AuthSdk; locale: Locale }) {
  const content = integrationCopy[locale].dsh
  const {
    data: settings,
    error: settingsError,
    loading: settingsLoading,
  } = useApiQuery<SettingsData>(sdk, "/api/settings")

  return (
    <DshIntegrationGuide
      sdk={sdk}
      content={content}
      modelIds={settings?.available_model_ids ?? []}
      modelListLoading={settingsLoading}
      modelListError={settingsError}
    />
  )
}

function FileIntegrationPage({
  sdk,
  content,
  config,
}: {
  sdk: AuthSdk
  content: FileIntegrationContent
  config: FileIntegrationConfig
}) {
  const instruction =
    content.platformInstructions[codexPlatform(navigator.userAgent)]
  const manualConfig = config.manualConfig(window.location.origin)

  return (
    <div className="flex max-w-4xl flex-col gap-5">
      <Alert>
        <KeyRoundIcon />
        <AlertTitle>{content.preparationTitle}</AlertTitle>
        <AlertDescription>{content.preparationDescription}</AlertDescription>
      </Alert>
      <IntegrationPanel
        title={content.integration}
        description={content.description}
      >
        <Tabs defaultValue="manual">
          <TabsList
            className="max-w-full overflow-x-auto"
            aria-label={content.integration}
          >
            <TabsTrigger value="manual">{content.manualTitle}</TabsTrigger>
            <TabsTrigger value="automatic">
              {content.automaticTitle}
            </TabsTrigger>
          </TabsList>
          <TabsContent value="manual" className="flex flex-col gap-5 pt-3">
            <p className="text-sm text-muted-foreground">
              {content.manualDescription}
            </p>
            <IntegrationStep
              number={1}
              description={content.manualConfigInstruction}
            >
              <CopyableCode
                code={manualConfig}
                copyLabel={content.copyLabel}
                copiedLabel={content.copiedLabel}
              />
            </IntegrationStep>
            <IntegrationStep
              number={2}
              description={content.manualTokenInstruction}
            />
            <Alert>
              <ShieldCheckIcon />
              <AlertTitle>{content.manualVerifyTitle}</AlertTitle>
              <AlertDescription>
                {content.manualVerifyDescription}
              </AlertDescription>
            </Alert>
          </TabsContent>
          <TabsContent value="automatic" className="flex flex-col gap-5 pt-3">
            <p className="text-sm text-muted-foreground">
              {content.automaticDescription}
            </p>
            <IntegrationStep number={1} description={content.findConfig}>
              <p className="text-sm text-muted-foreground">
                {content.findConfigDescription}
              </p>
              <Alert>
                <PencilIcon />
                <AlertTitle>
                  <code>{instruction.configPath}</code>
                </AlertTitle>
                <AlertDescription>{instruction.hint}</AlertDescription>
              </Alert>
            </IntegrationStep>
            <IntegrationStep number={2} description={content.selectConfig}>
              <FileConfigWriter sdk={sdk} content={content} config={config} />
            </IntegrationStep>
            <Alert>
              <ShieldCheckIcon />
              <AlertTitle>{content.restartTitle}</AlertTitle>
              <AlertDescription>{content.restartDescription}</AlertDescription>
            </Alert>
          </TabsContent>
        </Tabs>
      </IntegrationPanel>
    </div>
  )
}

function FileConfigWriter({
  sdk,
  content,
  config,
}: {
  sdk: AuthSdk
  content: FileIntegrationContent
  config: FileIntegrationConfig
}) {
  const [pending, setPending] = useState(false)
  const [error, setError] = useState("")

  async function configureIntegration() {
    const filePickerWindow = window as FilePickerWindow
    if (!filePickerWindow.showOpenFilePicker) {
      setError(content.browserRequiredDescription)
      return
    }

    setError("")
    setPending(true)
    let consumer: { id: string; secret: string } | undefined

    try {
      const [handle] = await filePickerWindow.showOpenFilePicker({
        multiple: false,
        types: [
          {
            description: content.pickerDescription,
            accept: content.accept,
          },
        ],
      })
      if (!handle || handle.name !== content.fileName) {
        setError(content.selectConfigOnly)
        return
      }

      const source = await (await handle.getFile()).text()
      try {
        config.updateConfig(source, "sk-pending")
      } catch {
        setError(content.configInvalid)
        return
      }

      consumer = await api<{ id: string; secret: string }>(
        sdk,
        "/api/consumers",
        {
          method: "POST",
          body: JSON.stringify({ name: content.consumerName }),
        }
      )
      if (!isConsumerToken(consumer.secret))
        throw new Error("Invalid Consumer token")

      const writable = await handle.createWritable()
      await writable.write(config.updateConfig(source, consumer.secret))
      await writable.close()
      consumer = undefined
      toast.success(content.configWritten)
    } catch (cause) {
      if (isAbortError(cause)) return

      if (consumer) {
        try {
          await api(sdk, "/api/consumers/" + consumer.id, { method: "DELETE" })
        } catch {
          setError(content.consumerCleanupFailed)
          return
        }
      }
      setError(content.configWriteFailed)
    } finally {
      setPending(false)
    }
  }

  return (
    <section
      className="flex max-w-3xl flex-col gap-3"
      aria-label={content.integration}
    >
      {error && (
        <Alert variant="destructive">
          <XCircleIcon />
          <AlertTitle>{content.configError}</AlertTitle>
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}
      <Button disabled={pending} onClick={() => void configureIntegration()}>
        {pending ? (
          <Spinner data-icon="inline-start" />
        ) : (
          <PencilIcon data-icon="inline-start" />
        )}
        {content.selectConfig}
      </Button>
    </section>
  )
}

function DshIntegrationGuide({
  sdk,
  content,
  modelIds,
  modelListLoading,
  modelListError,
}: {
  sdk: AuthSdk
  content: DshIntegrationContent
  modelIds: string[]
  modelListLoading: boolean
  modelListError: string
}) {
  const providerFields = [
    ["Provider ID", "deepseek-lb"],
    ["API URL", `${window.location.origin}/v1`],
    ["API protocol", "openai-responses"],
  ]

  return (
    <div className="flex max-w-4xl flex-col gap-5">
      <Alert>
        <KeyRoundIcon />
        <AlertTitle>{content.preparationTitle}</AlertTitle>
        <AlertDescription>{content.preparationDescription}</AlertDescription>
      </Alert>
      <IntegrationPanel
        title={content.integration}
        description={content.description}
      >
        <Tabs defaultValue="manual">
          <TabsList
            className="max-w-full overflow-x-auto"
            aria-label={content.integration}
          >
            <TabsTrigger value="manual">{content.manualTitle}</TabsTrigger>
            <TabsTrigger value="automatic">
              {content.automaticTitle}
            </TabsTrigger>
          </TabsList>
          <TabsContent value="manual" className="flex flex-col gap-3 pt-3">
            <p className="text-sm text-muted-foreground">
              {content.manualInstruction}
            </p>
            <dl className="grid gap-px overflow-hidden rounded-lg border bg-border sm:grid-cols-3">
              {providerFields.map(([label, value]) => (
                <div
                  key={label}
                  className="flex flex-col gap-1 bg-background p-3"
                >
                  <dt className="text-xs text-muted-foreground">{label}</dt>
                  <dd className="font-mono text-sm">{value}</dd>
                </div>
              ))}
            </dl>
            <p className="text-sm text-muted-foreground">
              {content.apiKeyInstruction}
            </p>
            <p className="text-sm text-muted-foreground">
              {content.modelCatalogInstruction}
            </p>
          </TabsContent>
          <TabsContent value="automatic" className="flex flex-col gap-3 pt-3">
            <p className="text-sm text-muted-foreground">
              {content.automaticDescription}
            </p>
            <Alert>
              <PencilIcon />
              <AlertTitle>
                <code>{content.directoryPath}</code>
              </AlertTitle>
              <AlertDescription>
                settings.yaml · .credentials.yaml
              </AlertDescription>
            </Alert>
            <DshConfigWriter
              sdk={sdk}
              content={content}
              modelIds={modelIds}
              modelListLoading={modelListLoading}
              modelListError={modelListError}
            />
            <Alert>
              <ShieldCheckIcon />
              <AlertTitle>{content.automaticTitle}</AlertTitle>
              <AlertDescription>{content.configWritten}</AlertDescription>
            </Alert>
          </TabsContent>
        </Tabs>
      </IntegrationPanel>
    </div>
  )
}

function DshConfigWriter({
  sdk,
  content,
  modelIds,
  modelListLoading,
  modelListError,
}: {
  sdk: AuthSdk
  content: DshIntegrationContent
  modelIds: string[]
  modelListLoading: boolean
  modelListError: string
}) {
  const [pending, setPending] = useState(false)
  const [error, setError] = useState("")
  const modelListAvailable =
    !modelListLoading && !modelListError && modelIds.length > 0

  async function configureIntegration() {
    const directoryPickerWindow = window as DirectoryPickerWindow
    if (!directoryPickerWindow.showDirectoryPicker) {
      setError(content.browserRequiredDescription)
      return
    }
    if (!modelListAvailable) {
      setError(content.modelListUnavailable)
      return
    }

    setError("")
    setPending(true)
    let consumer: { id: string; secret: string } | undefined
    let settingsHandle: ConfigFileHandle | undefined
    let credentialsHandle: ConfigFileHandle | undefined
    let settingsSource = ""
    let credentialsSource = ""
    let credentialsMissing = false
    let settingsWritten = false

    try {
      const directory = await directoryPickerWindow.showDirectoryPicker({
        mode: "readwrite",
      })
      try {
        settingsHandle = await directory.getFileHandle("settings.yaml")
      } catch {
        setError(content.settingsMissing)
        return
      }

      settingsSource = await readConfigFile(settingsHandle)
      try {
        updateDshSettings(
          settingsSource,
          "sk-pending",
          dshModels(modelIds),
          window.location.origin
        )
        try {
          credentialsHandle = await directory.getFileHandle(".credentials.yaml")
          credentialsSource = await readConfigFile(credentialsHandle)
        } catch (cause) {
          if (
            !(cause instanceof DOMException) ||
            cause.name !== "NotFoundError"
          ) {
            throw cause
          }
          credentialsMissing = true
        }
        updateDshCredentials(credentialsSource, "sk-pending")
      } catch {
        setError(content.configInvalid)
        return
      }

      consumer = await api<{ id: string; secret: string }>(
        sdk,
        "/api/consumers",
        {
          method: "POST",
          body: JSON.stringify({ name: content.consumerName }),
        }
      )
      if (!isConsumerToken(consumer.secret)) {
        throw new Error("Invalid Consumer token")
      }
      if (credentialsMissing) {
        credentialsHandle = await directory.getFileHandle(".credentials.yaml", {
          create: true,
        })
      }
      if (!credentialsHandle) {
        throw new Error("Missing DSH credentials handle")
      }

      await writeConfigFile(
        settingsHandle,
        updateDshSettings(
          settingsSource,
          consumer.secret,
          dshModels(modelIds),
          window.location.origin
        )
      )
      settingsWritten = true
      await writeConfigFile(
        credentialsHandle,
        updateDshCredentials(credentialsSource, consumer.secret)
      )
      consumer = undefined
      toast.success(content.configWritten)
    } catch (cause) {
      if (isAbortError(cause)) return

      if (
        settingsWritten &&
        settingsHandle &&
        credentialsHandle &&
        !(await restoreDshFiles(
          settingsHandle,
          settingsSource,
          credentialsHandle,
          credentialsSource
        ))
      ) {
        setError(content.configRollbackFailed)
        return
      }
      if (consumer) {
        try {
          await api(sdk, "/api/consumers/" + consumer.id, {
            method: "DELETE",
          })
        } catch {
          setError(content.consumerCleanupFailed)
          return
        }
      }
      setError(content.configWriteFailed)
    } finally {
      setPending(false)
    }
  }

  return (
    <section
      className="flex max-w-3xl flex-col gap-3"
      aria-label={content.automaticTitle}
    >
      {error && (
        <Alert variant="destructive">
          <XCircleIcon />
          <AlertTitle>{content.configError}</AlertTitle>
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}
      {!modelListAvailable && !modelListLoading && (
        <Alert variant="destructive">
          <XCircleIcon />
          <AlertTitle>{content.configError}</AlertTitle>
          <AlertDescription>{content.modelListUnavailable}</AlertDescription>
        </Alert>
      )}
      <Button
        disabled={pending || !modelListAvailable}
        onClick={() => void configureIntegration()}
      >
        {pending || modelListLoading ? (
          <Spinner data-icon="inline-start" />
        ) : (
          <PencilIcon data-icon="inline-start" />
        )}
        {content.selectDirectory}
      </Button>
    </section>
  )
}

async function readConfigFile(handle: ConfigFileHandle) {
  return (await handle.getFile()).text()
}

async function writeConfigFile(handle: ConfigFileHandle, content: string) {
  const writable = await handle.createWritable()
  await writable.write(content)
  await writable.close()
}

async function restoreDshFiles(
  settingsHandle: ConfigFileHandle,
  settingsSource: string,
  credentialsHandle: ConfigFileHandle,
  credentialsSource: string
) {
  try {
    await writeConfigFile(settingsHandle, settingsSource)
    await writeConfigFile(credentialsHandle, credentialsSource)
    return true
  } catch {
    return false
  }
}

function DirectApiIntegrationPage({ locale }: { locale: Locale }) {
  const content = integrationCopy[locale].direct
  const copyLabel = locale === "zh" ? "复制" : "Copy"
  const copiedLabel = locale === "zh" ? "已复制" : "Copied"
  const origin = window.location.origin
  const listModels =
    "curl " +
    origin +
    '/v1/models \\\n  -H "Authorization: Bearer <YOUR_CONSUMER_KEY>"'
  const chat =
    "curl " +
    origin +
    '/v1/chat/completions \\\n  -H "Authorization: Bearer <YOUR_CONSUMER_KEY>" \\\n  -H "Content-Type: application/json" \\\n  -d \'{\n    "model": "deepseek-flash",\n    "messages": [{"role": "user", "content": "Explain this Rust error"}]\n  }\''

  return (
    <div className="flex max-w-4xl flex-col gap-5">
      <Alert>
        <KeyRoundIcon />
        <AlertTitle>{content.preparationTitle}</AlertTitle>
        <AlertDescription>{content.preparationDescription}</AlertDescription>
      </Alert>
      <IntegrationPanel title={content.title} description={content.description}>
        <IntegrationStep number={1} description={content.firstStep}>
          <CopyableCode
            code={listModels}
            copyLabel={copyLabel}
            copiedLabel={copiedLabel}
          />
        </IntegrationStep>
        <IntegrationStep number={2} description={content.secondStep}>
          <CopyableCode
            code={chat}
            copyLabel={copyLabel}
            copiedLabel={copiedLabel}
          />
          <p className="text-xs text-muted-foreground">
            {content.modelPlaceholder}: <code>deepseek-flash</code>,{" "}
            <code>deepseek-v4-pro</code>
          </p>
        </IntegrationStep>
      </IntegrationPanel>
      <Alert>
        <ShieldAlertIcon />
        <AlertTitle>{content.sensitiveTitle}</AlertTitle>
        <AlertDescription>{content.sensitiveDescription}</AlertDescription>
      </Alert>
    </div>
  )
}

function IntegrationPanel({
  title,
  description,
  children,
}: {
  title: string
  description: string
  children: ReactNode
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{title}</CardTitle>
        <CardDescription>{description}</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-5">{children}</CardContent>
    </Card>
  )
}

function IntegrationStep({
  number,
  description,
  children,
}: {
  number: number
  description: string
  children?: ReactNode
}) {
  return (
    <section className="grid gap-2 sm:grid-cols-[1.5rem_minmax(0,1fr)]">
      <span className="flex size-6 items-center justify-center rounded-md bg-muted text-xs font-medium tabular-nums">
        {number}
      </span>
      <div className="flex min-w-0 flex-col gap-3">
        <p className="text-sm text-pretty">{description}</p>
        {children}
      </div>
    </section>
  )
}

function CopyableCode({
  code,
  copyLabel,
  copiedLabel,
}: {
  code: string
  copyLabel: string
  copiedLabel: string
}) {
  return (
    <div className="relative min-w-0 rounded-lg border bg-muted/50 p-3 pr-12">
      <pre className="overflow-x-auto text-xs leading-5 break-words whitespace-pre-wrap">
        <code>{code}</code>
      </pre>
      <Button
        className="absolute top-2 right-2"
        size="icon-xs"
        variant="ghost"
        aria-label={copyLabel}
        onClick={() =>
          void navigator.clipboard
            .writeText(code)
            .then(() => toast.success(copiedLabel))
        }
      >
        <ClipboardIcon />
      </Button>
    </div>
  )
}

function Dashboard({ sdk, locale }: { sdk: AuthSdk; locale: Locale }) {
  const t = copy[locale]
  const { data, error, loading } = useApiQuery<DashboardStats>(
    sdk,
    "/api/dashboard"
  )
  if (loading) return <LoadingTable />
  if (error) return <ErrorState message={error} />
  const rows = [
    [t.activeConsumers, data?.active_consumers],
    [t.activeProviders, data?.active_providers],
    [t.calls24h, data?.calls_24h],
    [t.peakCalls24h, data?.peak_calls_24h],
    [t.errors24h, data?.errors_24h],
    [t.officialCost24h, formatUsd(data?.official_cost_usd_nanos_24h, locale)],
    [t.actualCost24h, formatUsd(data?.actual_cost_usd_nanos_24h, locale)],
    [
      t.officialConsumedUsd,
      formatUsd(data?.official_consumed_usd_nanos, locale),
    ],
    [t.actualConsumedUsd, formatUsd(data?.consumed_usd_nanos, locale)],
  ]
  const tokenRows = [
    [t.inputTokens, Number(data?.input_tokens_24h ?? 0).toLocaleString(locale)],
    [
      t.outputTokens,
      Number(data?.output_tokens_24h ?? 0).toLocaleString(locale),
    ],
    [
      t.cachedInputTokens,
      Number(data?.cached_tokens_24h ?? 0).toLocaleString(locale),
    ],
    [
      t.cacheRate,
      cacheHitRate(
        data?.cached_tokens_24h ?? 0,
        data?.input_tokens_24h ?? 0,
        locale
      ),
    ],
    [
      t.inputOutputRatio,
      inputOutputRatio(
        data?.input_tokens_24h ?? 0,
        data?.output_tokens_24h ?? 0,
        locale
      ),
    ],
  ]
  return (
    <div className="flex flex-col gap-5">
      <Card>
        <CardHeader className="flex-row items-start justify-between">
          <div>
            <CardTitle>{t.pricingTariffTitle}</CardTitle>
            <CardDescription>{t.pricingTariffDescription}</CardDescription>
          </div>
          <Badge variant={data?.peak_now ? "destructive" : "secondary"}>
            {data?.peak_now ? t.pricingPeak : t.pricingOffPeak}
          </Badge>
        </CardHeader>
        <CardContent className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
          <span>{t.pricingModels}:</span>
          {(data?.available_model_ids ?? []).map((model) => (
            <Badge key={model} variant="outline">
              {model}
            </Badge>
          ))}
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>{t.accountOverview}</CardTitle>
          <CardDescription>{t.accountOverviewDescription}</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <dl className="grid gap-px overflow-hidden rounded-lg border bg-border sm:grid-cols-2 lg:grid-cols-4">
            {rows.map(([label, value]) => (
              <div
                key={String(label)}
                className="flex flex-col gap-1 bg-background p-4"
              >
                <dt>{label}</dt>
                <dd className="text-2xl font-semibold tabular-nums">
                  {value ?? 0}
                </dd>
              </div>
            ))}
          </dl>
          <Separator />
          <div className="flex flex-col gap-1">
            <h3 className="text-sm font-medium">{t.tokenUsage24h}</h3>
            <p className="text-sm text-muted-foreground">
              {t.tokenUsage24hDescription}
            </p>
          </div>
          <dl className="grid gap-px overflow-hidden rounded-lg border bg-border sm:grid-cols-2 lg:grid-cols-5">
            {tokenRows.map(([label, value]) => (
              <div
                key={String(label)}
                className="flex flex-col gap-1 bg-background p-4"
              >
                <dt>{label}</dt>
                <dd className="text-2xl font-semibold tabular-nums">{value}</dd>
              </div>
            ))}
          </dl>
        </CardContent>
      </Card>
    </div>
  )
}

function ModelPricesPage({ sdk, locale }: { sdk: AuthSdk; locale: Locale }) {
  const t = copy[locale]
  const query = useApiQuery<ModelPricesResponse>(sdk, "/api/model-prices")
  if (query.loading) return <LoadingTable />
  if (query.error) return <ErrorState message={query.error} />
  if (!query.data) return <ErrorState message={t.unknownError} />
  const missingRate = "—"
  const rate = (value: number | undefined) =>
    typeof value === "number" ? formatUsd(value, locale) : missingRate
  return (
    <div className="flex min-w-0 flex-col gap-4">
      <Card>
        <CardHeader>
          <CardTitle>{t.modelPricesTitle}</CardTitle>
          <CardDescription>{t.modelPricesDescription}</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-wrap items-center gap-x-4 gap-y-2 text-sm text-muted-foreground">
          <span>
            {t.pricingUnit}: {query.data.unit}
          </span>
          <span>
            {t.pricingAsOf}: {query.data.source_as_of}
          </span>
          <a
            className="inline-flex items-center gap-1 font-medium text-foreground underline underline-offset-4"
            href={query.data.source_url}
            rel="noreferrer"
            target="_blank"
          >
            {t.officialPricingSource}
            <ExternalLinkIcon aria-hidden="true" className="size-3.5" />
          </a>
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>{t.officialModelPriceTable}</CardTitle>
          <CardDescription>{t.modelPricesTableDescription}</CardDescription>
        </CardHeader>
        <CardContent>
          <DataTable>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead rowSpan={2}>{t.model}</TableHead>
                  <TableHead className="text-center" colSpan={3}>
                    {t.pricingPeak}
                  </TableHead>
                  <TableHead className="text-center" colSpan={3}>
                    {t.pricingOffPeak}
                  </TableHead>
                </TableRow>
                <TableRow>
                  {[t.cacheHitTokens, t.cacheMissTokens, t.output].map(
                    (label) => (
                      <TableHead className="text-right" key={`peak-${label}`}>
                        {label}
                      </TableHead>
                    )
                  )}
                  {[t.cacheHitTokens, t.cacheMissTokens, t.output].map(
                    (label) => (
                      <TableHead
                        className="text-right"
                        key={`off-peak-${label}`}
                      >
                        {label}
                      </TableHead>
                    )
                  )}
                </TableRow>
              </TableHeader>
              <TableBody>
                {query.data.rows.map((price) => (
                  <TableRow key={price.model}>
                    <TableCell>
                      <code>{price.model}</code>
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {rate(price.peak.cache_hit_usd_nanos)}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {rate(price.peak.cache_miss_usd_nanos)}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {rate(price.peak.output_usd_nanos)}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {rate(price.off_peak.cache_hit_usd_nanos)}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {rate(price.off_peak.cache_miss_usd_nanos)}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {rate(price.off_peak.output_usd_nanos)}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </DataTable>
        </CardContent>
      </Card>
    </div>
  )
}

function TopupsPage({ sdk, locale }: { sdk: AuthSdk; locale: Locale }) {
  const t = copy[locale]
  const summary = useApiQuery<PaymentSummary>(sdk, "/api/payments/summary")
  const [amount, setAmount] = useState("")

  if (summary.loading) return <LoadingTable />
  if (summary.error) return <ErrorState message={summary.error} />
  if (!summary.data) return <ErrorState message={t.unknownError} />
  const publicWalletUserId = summary.data.midas_fund_user_id ?? ""
  const amountUsdNanos = parseUsdNanos(amount)
  const amountInvalid = amount.length > 0 && amountUsdNanos === null
  const canContinueToMidas = Boolean(
    summary.data.midas_configured &&
    publicWalletUserId &&
    amountUsdNanos !== null
  )

  const metrics = [
    [t.creditedUsd, summary.data.topup_usd_nanos],
    [t.providedValue, summary.data.provided_usd_nanos],
    [t.usageUsd, summary.data.consumed_usd_nanos],
    [t.availableCredit, summary.data.available_usd_nanos],
  ]
  return (
    <div className="flex max-w-5xl flex-col gap-4">
      <Card>
        <CardHeader>
          <CardTitle>{t.topupsTitle}</CardTitle>
          <CardDescription>{t.topupsDescription}</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <dl className="grid gap-px overflow-hidden rounded-lg border bg-border sm:grid-cols-2 lg:grid-cols-4">
            {metrics.map(([label, value]) => (
              <div
                key={String(label)}
                className="flex flex-col gap-1 bg-background p-4"
              >
                <dt>{label}</dt>
                <dd className="text-lg font-semibold tabular-nums">
                  {formatUsd(Number(value), locale)}
                </dd>
              </div>
            ))}
          </dl>
          <Alert>
            <ShieldCheckIcon />
            <AlertDescription>
              {summary.data.enforcement_enabled
                ? t.requestEnforced
                : t.requestNotEnforced}
            </AlertDescription>
          </Alert>
        </CardContent>
      </Card>
      {summary.data.midas_configured ? (
        <Card>
          <CardHeader>
            <CardTitle>{t.midasFundTitle}</CardTitle>
            <CardDescription>{t.midasFundDescription}</CardDescription>
          </CardHeader>
          <CardContent className="flex flex-col gap-4">
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="midas-public-wallet-user-id">
                  {t.publicWalletUserId}
                </FieldLabel>
                <div className="flex flex-col gap-2 sm:flex-row">
                  <Input
                    id="midas-public-wallet-user-id"
                    readOnly
                    value={publicWalletUserId}
                  />
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() =>
                      navigator.clipboard
                        .writeText(publicWalletUserId)
                        .then(() => toast.success(t.copied))
                    }
                  >
                    <ClipboardIcon data-icon="inline-start" />
                    {t.copyMidasUserId}
                  </Button>
                </div>
                <FieldDescription>{t.midasTransferRefresh}</FieldDescription>
              </Field>
              <Field data-invalid={amountInvalid}>
                <FieldLabel htmlFor="midas-topup-amount">
                  {t.topupAmount}
                </FieldLabel>
                <Input
                  id="midas-topup-amount"
                  inputMode="decimal"
                  value={amount}
                  onChange={(event) => setAmount(event.target.value)}
                  aria-invalid={amountInvalid}
                  placeholder="0.00"
                />
                <FieldDescription>{t.topupAmountHint}</FieldDescription>
              </Field>
            </FieldGroup>
            <Button
              disabled={!canContinueToMidas}
              onClick={() => {
                if (amountUsdNanos === null) return
                window.open(
                  midasTransferUrl(publicWalletUserId, amountUsdNanos),
                  "_blank",
                  "noopener,noreferrer"
                )
              }}
            >
              <ExternalLinkIcon data-icon="inline-start" />
              {t.continueToMidas}
            </Button>
          </CardContent>
        </Card>
      ) : (
        <Alert variant="destructive">
          <ShieldAlertIcon />
          <AlertTitle>{t.midasUnavailable}</AlertTitle>
          <AlertDescription>{t.midasUnavailableDescription}</AlertDescription>
        </Alert>
      )}
    </div>
  )
}

function SystemResourcesCard({
  sdk,
  locale,
}: {
  sdk: AuthSdk
  locale: Locale
}) {
  const t = copy[locale]
  const queryClient = useQueryClient()
  const [vacuumDialogOpen, setVacuumDialogOpen] = useState(false)
  const [vacuumPending, setVacuumPending] = useState(false)
  const query = useQuery({
    queryKey: ["/api/system/resources"],
    queryFn: ({ signal }) =>
      api<SystemResources>(sdk, "/api/system/resources", { signal }),
    refetchInterval: 5_000,
  })
  async function vacuumDatabase() {
    setVacuumPending(true)
    try {
      const snapshot = await api<SystemResources>(
        sdk,
        "/api/system/resources",
        {
          method: "POST",
        }
      )
      queryClient.setQueryData(["/api/system/resources"], snapshot)
      toast.success(t.vacuumDatabaseComplete)
    } catch (cause) {
      toast.error(message(cause, t))
    } finally {
      setVacuumPending(false)
    }
  }
  if (query.isPending) return <SystemResourcesLoading locale={locale} />
  if (query.error || !query.data)
    return (
      <Card>
        <CardHeader>
          <CardTitle>{t.systemResources}</CardTitle>
          <CardDescription>{t.systemResourcesDescription}</CardDescription>
        </CardHeader>
        <CardContent>
          <Alert variant="destructive">
            <ShieldAlertIcon />
            <AlertTitle>{t.resourceUnavailable}</AlertTitle>
            <AlertDescription>
              {message(query.error, t)} {t.resourceUnavailableDescription}
            </AlertDescription>
          </Alert>
        </CardContent>
      </Card>
    )

  const data = query.data
  const disk = data.disk
  const metrics: Array<{
    icon: LucideIcon
    label: string
    value: string
    detail: string
    secondaryDetail?: string
    percent?: number
    action?: ReactNode
  }> = [
    {
      icon: CpuIcon,
      label: t.cpu,
      value: formatPercent(data.cpu.usage_percent, locale),
      detail: `${t.load1m}: ${data.cpu.load_1m.toFixed(2)} · ${t.logicalCpus}: ${data.cpu.logical_cpus}`,
      percent: data.cpu.usage_percent,
    },
    {
      icon: MemoryStickIcon,
      label: t.memory,
      value: `${formatStorageBytes(data.memory.used_bytes, locale)} / ${formatStorageBytes(data.memory.total_bytes, locale)}`,
      detail: `${t.lbRss}: ${formatStorageBytes(data.memory.process_used_bytes, locale)} · ${t.otherSystemMemory}: ${formatStorageBytes(data.memory.other_used_bytes, locale)} · ${t.systemAvailableMemory}: ${formatStorageBytes(data.memory.available_bytes, locale)}`,
      secondaryDetail: `${t.swap}: ${formatStorageBytes(data.memory.swap_used_bytes, locale)} / ${formatStorageBytes(data.memory.swap_total_bytes, locale)}`,
      percent: data.memory.usage_percent,
    },
    {
      icon: NetworkIcon,
      label: t.network,
      value: `${t.received}: ${formatRate(data.network.receive_bytes_per_second, locale)} · ${t.transmitted}: ${formatRate(data.network.transmit_bytes_per_second, locale)}`,
      detail: `${t.totalReceived}: ${formatStorageBytes(data.network.total_received_bytes, locale)} · ${t.totalTransmitted}: ${formatStorageBytes(data.network.total_transmitted_bytes, locale)}`,
      secondaryDetail: `${t.networkInterfaces}: ${data.network.interfaces}`,
    },
    {
      icon: HardDriveIcon,
      label: t.disk,
      value: disk
        ? `${formatStorageBytes(disk.used_bytes, locale)} / ${formatStorageBytes(disk.total_bytes, locale)}`
        : "—",
      detail: disk
        ? `${t.available}: ${formatStorageBytes(disk.available_bytes, locale)} · ${t.mountPoint}: ${disk.mount_point}`
        : t.resourceUnavailable,
      percent: disk?.usage_percent,
    },
    {
      icon: DatabaseIcon,
      label: t.sqlite,
      value: formatStorageBytes(data.sqlite.total_bytes, locale),
      detail: `${t.mainFile}: ${formatStorageBytes(data.sqlite.main_bytes, locale)} · ${t.walFile}: ${formatStorageBytes(data.sqlite.wal_bytes, locale)} · ${t.shmFile}: ${formatStorageBytes(data.sqlite.shm_bytes, locale)}`,
      secondaryDetail: `${t.reclaimableSpace}: ${formatStorageBytes(data.sqlite.freelist_bytes, locale)} · ${formatPercent(data.sqlite.freelist_percent, locale)}`,
      action: (
        <Button
          size="sm"
          variant="outline"
          disabled={vacuumPending}
          onClick={() => setVacuumDialogOpen(true)}
        >
          {vacuumPending ? (
            <Spinner data-icon="inline-start" />
          ) : (
            <DatabaseIcon />
          )}
          {t.vacuumDatabase}
        </Button>
      ),
    },
  ]

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t.systemResources}</CardTitle>
        <CardDescription>{t.systemResourcesDescription}</CardDescription>
        <Badge variant="outline" className="mt-2 w-fit">
          {t.sampledAt} {formatTime(data.sampled_at, locale)} ·{" "}
          {t.refreshEvery5s}
        </Badge>
      </CardHeader>
      <CardContent>
        <dl className="divide-y overflow-hidden rounded-lg border">
          {metrics.map((metric) => {
            const Icon = metric.icon
            return (
              <div
                key={metric.label}
                className="grid gap-3 px-4 py-3 md:grid-cols-[minmax(10rem,0.75fr)_minmax(14rem,1fr)_minmax(16rem,1.5fr)] md:items-center"
              >
                <dt className="flex items-center gap-2 font-medium">
                  <Icon className="size-4 text-muted-foreground" />
                  {metric.label}
                </dt>
                <dd className="font-medium tabular-nums">{metric.value}</dd>
                <dd className="flex min-w-0 flex-col gap-2 text-xs text-muted-foreground">
                  <span className="truncate" title={metric.detail}>
                    {metric.detail}
                  </span>
                  {metric.secondaryDetail && (
                    <span
                      className="truncate font-medium text-foreground"
                      title={metric.secondaryDetail}
                    >
                      {metric.secondaryDetail}
                    </span>
                  )}
                  {metric.percent !== undefined && (
                    <progress
                      aria-label={`${metric.label}: ${formatPercent(metric.percent, locale)}`}
                      className="h-1.5 w-full accent-primary"
                      max={100}
                      value={Math.max(0, Math.min(100, metric.percent))}
                    />
                  )}
                  {metric.action && <div className="pt-1">{metric.action}</div>}
                </dd>
              </div>
            )
          })}
        </dl>
      </CardContent>
      <AlertDialog open={vacuumDialogOpen} onOpenChange={setVacuumDialogOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t.vacuumDatabaseTitle}</AlertDialogTitle>
            <AlertDialogDescription>
              {t.vacuumDatabaseDescription}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={vacuumPending}>
              {t.cancel}
            </AlertDialogCancel>
            <AlertDialogAction
              disabled={vacuumPending}
              onClick={() => void vacuumDatabase()}
            >
              {vacuumPending && <Spinner data-icon="inline-start" />}
              {t.vacuumDatabase}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Card>
  )
}

function SystemResourcesLoading({ locale }: { locale: Locale }) {
  const t = copy[locale]
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t.systemResources}</CardTitle>
        <CardDescription>{t.systemResourcesDescription}</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-2">
        {Array.from({ length: 5 }, (_, index) => (
          <Skeleton key={index} className="h-12 w-full" />
        ))}
      </CardContent>
    </Card>
  )
}

type ProviderBalanceState = {
  provider: Provider
  status: "loading" | "success" | "error"
  balance?: ProviderBalance
  error?: string
}

function Providers({
  sdk,
  locale,
  user,
}: {
  sdk: AuthSdk
  locale: Locale
  user: User
}) {
  const t = copy[locale]
  const queryClient = useQueryClient()
  const isAdministrator = user.role !== "user"
  const [addOpen, setAddOpen] = useState(false)
  const [keyTarget, setKeyTarget] = useState<Provider | null>(null)
  const [editingProviderId, setEditingProviderId] = useState<string | null>(
    null
  )
  const [editingProviderName, setEditingProviderName] = useState("")
  const [editingProviderPending, setEditingProviderPending] = useState(false)
  const [testState, setTestState] = useState<ProviderTestState | null>(null)
  const [balanceState, setBalanceState] = useState<ProviderBalanceState | null>(
    null
  )
  const [deleteTarget, setDeleteTarget] = useState<Provider | null>(null)
  const [deletePending, setDeletePending] = useState(false)
  const [togglePending, setTogglePending] = useState<string | null>(null)
  const testRequest = useRef<AbortController | null>(null)
  const { data, error, loading } = useApiQuery<Provider[]>(
    sdk,
    "/api/providers",
    5_000
  )
  useEffect(
    () => () => {
      testRequest.current?.abort()
    },
    []
  )

  function refreshProviders() {
    void queryClient.invalidateQueries({ queryKey: ["/api/providers"] })
  }

  async function testProvider(provider: Provider) {
    testRequest.current?.abort()
    const controller = new AbortController()
    testRequest.current = controller
    setTestState({ provider, status: "loading" })
    try {
      const result = await api<{
        ok: boolean
        status?: number | null
        latency_ms?: number
        model?: string
        error?: string | null
      }>(sdk, `/api/providers/${provider.id}/test`, {
        method: "POST",
        signal: controller.signal,
      })
      setTestState({
        provider,
        status: result.ok ? "success" : "error",
        latency_ms: result.latency_ms,
        model: result.model,
        error: result.error ?? (result.ok ? undefined : t.providerTestFailed),
      })
      refreshProviders()
    } catch (cause) {
      if (isAbortError(cause)) return
      setTestState({ provider, status: "error", error: message(cause, t) })
    }
  }

  async function loadBalance(provider: Provider) {
    setBalanceState({ provider, status: "loading" })
    try {
      const balance = await api<ProviderBalance>(
        sdk,
        `/api/providers/${provider.id}/balance`
      )
      setBalanceState({ provider, status: "success", balance })
    } catch (cause) {
      setBalanceState({
        provider,
        status: "error",
        error: message(cause, t),
      })
    }
  }

  async function toggleProvider(provider: Provider, enabled: boolean) {
    if (togglePending) return
    setTogglePending(provider.id)
    try {
      await api(sdk, `/api/providers/${provider.id}`, {
        method: "PATCH",
        body: JSON.stringify({ enabled }),
      })
      refreshProviders()
      toast.success(t.providerUpdated)
    } catch (cause) {
      toast.error(message(cause, t))
    } finally {
      setTogglePending(null)
    }
  }

  async function toggleVisibility(provider: Provider) {
    if (togglePending) return
    setTogglePending(provider.id)
    try {
      await api(sdk, `/api/providers/${provider.id}`, {
        method: "PATCH",
        body: JSON.stringify({
          visibility: provider.visibility === "public" ? "private" : "public",
        }),
      })
      refreshProviders()
      toast.success(t.providerUpdated)
    } catch (cause) {
      toast.error(message(cause, t))
    } finally {
      setTogglePending(null)
    }
  }

  function startRename(provider: Provider) {
    setEditingProviderId(provider.id)
    setEditingProviderName(provider.name)
  }

  async function saveName(provider: Provider) {
    if (editingProviderPending) return
    setEditingProviderPending(true)
    try {
      await api(sdk, `/api/providers/${provider.id}`, {
        method: "PATCH",
        body: JSON.stringify({ name: editingProviderName }),
      })
      setEditingProviderId(null)
      refreshProviders()
      toast.success(t.providerUpdated)
    } catch (cause) {
      toast.error(message(cause, t))
    } finally {
      setEditingProviderPending(false)
    }
  }

  async function remove() {
    const target = deleteTarget
    if (!target || deletePending) return
    setDeletePending(true)
    try {
      await api(sdk, `/api/providers/${target.id}`, { method: "DELETE" })
      setDeleteTarget(null)
      refreshProviders()
      toast.success(t.providerDeleted)
    } catch (cause) {
      toast.error(message(cause, t))
    } finally {
      setDeletePending(false)
    }
  }

  if (loading) return <LoadingTable />
  if (error) return <ErrorState message={error} />
  const providers = data ?? []

  return (
    <>
      <Card>
        <CardHeader className="flex-row items-start justify-between">
          <div>
            <CardTitle>{t.providerPool}</CardTitle>
            <CardDescription>{t.providerDescription}</CardDescription>
          </div>
          <Button onClick={() => setAddOpen(true)}>
            <PlusIcon data-icon="inline-start" />
            {t.addProvider}
          </Button>
        </CardHeader>
        <CardContent>
          {providers.length === 0 ? (
            <EmptyState
              icon={<BoxesIcon />}
              title={t.noProviders}
              description={t.noProvidersDescription}
              action={
                <Button onClick={() => setAddOpen(true)}>
                  {t.addProvider}
                </Button>
              }
            />
          ) : (
            <DataTable>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>{t.name}</TableHead>
                    <TableHead>{t.status}</TableHead>
                    <TableHead>{t.providerVisibility}</TableHead>
                    <TableHead>{t.providerLoad}</TableHead>
                    <TableHead>{t.providerUsage7d}</TableHead>
                    <TableHead>{t.lastUsed}</TableHead>
                    <TableHead>{t.actions}</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {providers.map((provider) => {
                    const enabled = provider.manual_disabled === 0
                    const usage = provider.usage
                    return (
                      <TableRow key={provider.id}>
                        <TableCell className="font-medium">
                          <div className="flex flex-col gap-1">
                            <div className="flex items-center gap-1">
                              {editingProviderId === provider.id ? (
                                <Input
                                  className="h-8 w-48"
                                  value={editingProviderName}
                                  onChange={(event) =>
                                    setEditingProviderName(event.target.value)
                                  }
                                />
                              ) : (
                                <span>{provider.name}</span>
                              )}
                              {editingProviderId === provider.id ? (
                                <>
                                  <Button
                                    size="icon-xs"
                                    variant="ghost"
                                    aria-label={t.saveProviderName}
                                    disabled={editingProviderPending}
                                    onClick={() => void saveName(provider)}
                                  >
                                    <CheckIcon />
                                  </Button>
                                  <Button
                                    size="icon-xs"
                                    variant="ghost"
                                    aria-label={t.cancelProviderName}
                                    onClick={() => setEditingProviderId(null)}
                                  >
                                    <XIcon />
                                  </Button>
                                </>
                              ) : (
                                <Button
                                  size="icon-xs"
                                  variant="ghost"
                                  aria-label={`${t.editProviderName}: ${provider.name}`}
                                  onClick={() => startRename(provider)}
                                >
                                  <PencilIcon />
                                </Button>
                              )}
                            </div>
                            {isAdministrator && provider.owner_id ? (
                              <span className="font-mono text-xs text-muted-foreground">
                                {t.owner}: {provider.owner_id}
                              </span>
                            ) : null}
                            {provider.last_error ? (
                              <span className="text-xs text-destructive">
                                {t.providerLastError}: {provider.last_error}
                              </span>
                            ) : null}
                            {provider.cooldown_until ? (
                              <span className="text-xs text-muted-foreground">
                                {t.providerCooldownUntil}{" "}
                                {formatTime(provider.cooldown_until, locale)}
                              </span>
                            ) : null}
                          </div>
                        </TableCell>
                        <TableCell>
                          <div className="flex flex-col items-start gap-2">
                            <StatusBadge
                              status={enabled ? provider.status : "disabled"}
                              locale={locale}
                            />
                            <Button
                              size="sm"
                              variant="outline"
                              disabled={togglePending === provider.id}
                              onClick={() =>
                                void toggleProvider(provider, !enabled)
                              }
                            >
                              {enabled ? t.providerDisable : t.providerEnable}
                            </Button>
                          </div>
                        </TableCell>
                        <TableCell>
                          <div className="flex flex-col items-start gap-2">
                            <Badge
                              variant={
                                provider.visibility === "public"
                                  ? "secondary"
                                  : "outline"
                              }
                            >
                              {providerVisibilityLabel(provider.visibility, t)}
                            </Badge>
                            <Button
                              size="sm"
                              variant="ghost"
                              disabled={togglePending === provider.id}
                              onClick={() => void toggleVisibility(provider)}
                            >
                              {provider.visibility === "public"
                                ? t.makeProviderPrivate
                                : t.makeProviderPublic}
                            </Button>
                          </div>
                        </TableCell>
                        <TableCell>
                          <div className="flex flex-col text-xs">
                            <span>
                              {provider.inflight} / {provider.concurrency_limit}
                            </span>
                            <span className="text-muted-foreground">
                              {t.providerQueued}: {provider.queued}
                            </span>
                          </div>
                        </TableCell>
                        <TableCell>
                          {usage ? (
                            <div className="flex flex-col text-xs">
                              <span>
                                {t.providerUsageRequests}: {usage.requests} (
                                {t.providerUsageErrors}: {usage.errors})
                              </span>
                              <span>
                                {t.providerUsageTokens}: {usage.input_tokens}/
                                {usage.output_tokens}/{usage.cached_tokens}
                              </span>
                              <span className="text-muted-foreground">
                                {t.providerUsageCost}:{" "}
                                {formatUsd(usage.actual_cost_usd_nanos, locale)}
                              </span>
                            </div>
                          ) : (
                            <span className="text-xs text-muted-foreground">
                              {t.providerNoUsage}
                            </span>
                          )}
                        </TableCell>
                        <TableCell>
                          {formatTime(provider.last_used_at, locale)}
                        </TableCell>
                        <TableCell>
                          <div className="flex items-center gap-1">
                            <Button
                              size="sm"
                              variant="outline"
                              onClick={() => void testProvider(provider)}
                            >
                              {t.testProvider}
                            </Button>
                            <Button
                              size="sm"
                              variant="outline"
                              onClick={() => void loadBalance(provider)}
                            >
                              {t.providerBalance}
                            </Button>
                            {isAdministrator ? (
                              <Button
                                size="sm"
                                variant="outline"
                                onClick={() => setKeyTarget(provider)}
                              >
                                {t.providerApiKey}
                              </Button>
                            ) : null}
                            <Button
                              size="sm"
                              variant="ghost"
                              onClick={() => setDeleteTarget(provider)}
                            >
                              <Trash2Icon data-icon="inline-start" />
                              {t.deleteProvider}
                            </Button>
                          </div>
                        </TableCell>
                      </TableRow>
                    )
                  })}
                </TableBody>
              </Table>
            </DataTable>
          )}
        </CardContent>
      </Card>
      <ProviderDialog
        sdk={sdk}
        locale={locale}
        open={addOpen}
        onOpenChange={setAddOpen}
        onDone={refreshProviders}
      />
      <ProviderTestDialog
        locale={locale}
        state={testState}
        onClose={() => setTestState(null)}
      />
      <ProviderBalanceDialog
        locale={locale}
        state={balanceState}
        onClose={() => setBalanceState(null)}
      />
      <ProviderKeyDialog
        sdk={sdk}
        locale={locale}
        provider={keyTarget}
        onClose={() => setKeyTarget(null)}
        onSaved={refreshProviders}
      />
      <AlertDialog
        open={Boolean(deleteTarget)}
        onOpenChange={(next) =>
          !next && !deletePending && setDeleteTarget(null)
        }
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t.deleteProviderTitle}</AlertDialogTitle>
            <AlertDialogDescription>
              {t.deleteProviderDescription}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={deletePending}>
              {t.cancel}
            </AlertDialogCancel>
            <AlertDialogAction
              disabled={deletePending}
              onClick={(event) => {
                event.preventDefault()
                void remove()
              }}
            >
              {t.confirmDeleteProvider}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}

function ProviderDialog({
  sdk,
  locale,
  open,
  onOpenChange,
  onDone,
}: {
  sdk: AuthSdk
  locale: Locale
  open: boolean
  onOpenChange: (open: boolean) => void
  onDone: () => void
}) {
  const t = copy[locale]
  const [name, setName] = useState("")
  const [apiKey, setApiKey] = useState("")
  const [visibility, setVisibility] = useState<ProviderVisibility>("private")
  const [pending, setPending] = useState(false)

  function close() {
    setName("")
    setApiKey("")
    setVisibility("private")
    onOpenChange(false)
  }

  async function submit(event: FormEvent) {
    event.preventDefault()
    if (pending) return
    if (!isConsumerToken(apiKey)) {
      toast.error(t.providerApiKeyInvalid)
      return
    }
    setPending(true)
    try {
      await api(sdk, "/api/providers", {
        method: "POST",
        body: JSON.stringify({ name, api_key: apiKey, visibility }),
      })
      close()
      onDone()
      toast.success(t.providerAdded)
    } catch (cause) {
      toast.error(message(cause, t))
    } finally {
      setPending(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={(next) => !next && close()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t.addProviderTitle}</DialogTitle>
          <DialogDescription>{t.providerDescription}</DialogDescription>
        </DialogHeader>
        <form onSubmit={submit}>
          <FieldGroup>
            <ProviderNameField
              id="provider-name"
              value={name}
              onChange={setName}
              locale={locale}
            />
            <Field>
              <FieldLabel htmlFor="provider-api-key">
                {t.providerApiKey}
              </FieldLabel>
              <Input
                id="provider-api-key"
                value={apiKey}
                autoComplete="off"
                spellCheck={false}
                onChange={(event) => setApiKey(event.target.value)}
                required
              />
              <FieldDescription>{t.providerApiKeyHelp}</FieldDescription>
            </Field>
            <ProviderVisibilityField
              id="provider-visibility"
              value={visibility}
              onChange={setVisibility}
              locale={locale}
            />
            <DialogFooter>
              <Button type="button" variant="outline" onClick={close}>
                {t.cancel}
              </Button>
              <Button type="submit" disabled={pending || !apiKey.trim()}>
                {t.addProvider}
              </Button>
            </DialogFooter>
          </FieldGroup>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function ProviderKeyDialog({
  sdk,
  locale,
  provider,
  onClose,
  onSaved,
}: {
  sdk: AuthSdk
  locale: Locale
  provider: Provider | null
  onClose: () => void
  onSaved: () => void
}) {
  const t = copy[locale]
  const [name, setName] = useState("")
  const [apiKey, setApiKey] = useState("")
  const [pending, setPending] = useState(false)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState("")

  useEffect(() => {
    if (!provider) return
    let cancelled = false
    setLoading(true)
    setError("")
    api<{ name: string; api_key: string }>(
      sdk,
      `/api/providers/${provider.id}/key`
    )
      .then((value) => {
        if (cancelled) return
        setName(value.name)
        setApiKey(value.api_key)
      })
      .catch((cause) => {
        if (cancelled) return
        setError(message(cause, t))
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [provider, sdk, t])

  async function submit(event: FormEvent) {
    event.preventDefault()
    if (!provider || pending) return
    setPending(true)
    try {
      await api(sdk, `/api/providers/${provider.id}/key`, {
        method: "PUT",
        body: JSON.stringify({ name, api_key: apiKey }),
      })
      onSaved()
      onClose()
      toast.success(t.providerKeySaved)
    } catch (cause) {
      toast.error(message(cause, t))
    } finally {
      setPending(false)
    }
  }

  return (
    <Dialog
      open={Boolean(provider)}
      onOpenChange={(next) => !next && !pending && onClose()}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {t.providerKeyTitle}: {provider?.name}
          </DialogTitle>
          <DialogDescription>{t.providerKeyDescription}</DialogDescription>
        </DialogHeader>
        {loading ? (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Spinner />
            {t.loading}
          </div>
        ) : error ? (
          <ErrorState message={error} />
        ) : (
          <form onSubmit={submit}>
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="provider-key-name">{t.name}</FieldLabel>
                <Input
                  id="provider-key-name"
                  value={name}
                  onChange={(event) => setName(event.target.value)}
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="provider-key-value">
                  {t.providerApiKey}
                </FieldLabel>
                <Input
                  id="provider-key-value"
                  value={apiKey}
                  autoComplete="off"
                  spellCheck={false}
                  onChange={(event) => setApiKey(event.target.value)}
                />
                <FieldDescription>{t.providerApiKeyHelp}</FieldDescription>
              </Field>
              <DialogFooter>
                <Button
                  type="button"
                  variant="outline"
                  disabled={pending}
                  onClick={onClose}
                >
                  {t.cancel}
                </Button>
                <Button type="submit" disabled={pending || !apiKey.trim()}>
                  {t.saveProviderKey}
                </Button>
              </DialogFooter>
            </FieldGroup>
          </form>
        )}
      </DialogContent>
    </Dialog>
  )
}

function ProviderTestDialog({
  locale,
  state,
  onClose,
}: {
  locale: Locale
  state: ProviderTestState | null
  onClose: () => void
}) {
  const t = copy[locale]
  return (
    <Dialog open={Boolean(state)} onOpenChange={(next) => !next && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {t.providerTestTitle}: {state?.provider.name}
          </DialogTitle>
          <DialogDescription>{t.providerTestDescription}</DialogDescription>
        </DialogHeader>
        {state?.status === "loading" ? (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Spinner />
            {t.testingProvider}
          </div>
        ) : state?.status === "error" ? (
          <ErrorState message={state.error || t.unknownError} />
        ) : (
          <Definition
            rows={[
              [t.providerTestResult, t.providerTestSucceeded],
              [t.providerTestModel, state?.model || "—"],
              [t.providerTestLatency, formatLatency(state?.latency_ms)],
            ]}
          />
        )}
        <DialogFooter>
          <Button onClick={onClose}>{t.close}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function ProviderBalanceDialog({
  locale,
  state,
  onClose,
}: {
  locale: Locale
  state: ProviderBalanceState | null
  onClose: () => void
}) {
  const t = copy[locale]
  const balances = state?.balance?.balance_infos ?? []
  return (
    <Dialog open={Boolean(state)} onOpenChange={(next) => !next && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {t.providerBalanceTitle}: {state?.provider.name}
          </DialogTitle>
          <DialogDescription>{t.providerBalanceDescription}</DialogDescription>
        </DialogHeader>
        {state?.status === "loading" ? (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Spinner />
            {t.loading}
          </div>
        ) : state?.status === "error" ? (
          <ErrorState message={state.error || t.unknownError} />
        ) : balances.length === 0 ? (
          <Alert>
            <AlertTitle>{t.providerBalanceUnavailable}</AlertTitle>
          </Alert>
        ) : (
          <Definition
            rows={[
              [
                t.providerBalanceAvailable,
                state?.balance?.is_available ? t.yes : t.no,
              ],
              ...balances.map(
                (info) =>
                  [
                    info.currency || t.providerBalance,
                    info.total_balance ?? "—",
                  ] as [string, unknown]
              ),
            ]}
          />
        )}
        <DialogFooter>
          <Button onClick={onClose}>{t.close}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function ProviderNameField({
  id,
  value,
  onChange,
  locale,
}: {
  id: string
  value: string
  onChange: (value: string) => void
  locale: Locale
}) {
  const t = copy[locale]
  return (
    <Field>
      <FieldLabel htmlFor={id}>{t.nameOptional}</FieldLabel>
      <Input
        id={id}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
      <FieldDescription>{t.providerNameHelp}</FieldDescription>
    </Field>
  )
}

function ProviderVisibilityField({
  id,
  value,
  onChange,
  locale,
}: {
  id: string
  value: ProviderVisibility
  onChange: (value: ProviderVisibility) => void
  locale: Locale
}) {
  const t = copy[locale]
  return (
    <Field>
      <FieldLabel htmlFor={id}>{t.providerVisibility}</FieldLabel>
      <Select
        value={value}
        onValueChange={(next) => onChange(next as ProviderVisibility)}
      >
        <SelectTrigger id={id}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            {providerVisibilities.map((option) => (
              <SelectItem key={option} value={option}>
                {providerVisibilityLabel(option, t)}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
      <FieldDescription>{t.providerVisibilityHelp}</FieldDescription>
    </Field>
  )
}

function Consumers({ sdk, locale }: { sdk: AuthSdk; locale: Locale }) {
  const t = copy[locale]
  const queryClient = useQueryClient()
  const [name, setName] = useState("")
  const [requestArchive, setRequestArchive] = useState(false)
  const [open, setOpen] = useState(false)
  const [secret, setSecret] = useState("")
  const [rotateId, setRotateId] = useState<string | null>(null)
  const [rotateTarget, setRotateTarget] = useState<Consumer | null>(null)
  const [deleteId, setDeleteId] = useState<string | null>(null)
  const [editTarget, setEditTarget] = useState<Consumer | null>(null)
  const [editName, setEditName] = useState("")
  const [editPending, setEditPending] = useState(false)
  const { data, error, loading } = useApiQuery<Consumer[]>(
    sdk,
    "/api/consumers"
  )
  function refreshConsumers() {
    void queryClient.invalidateQueries({ queryKey: ["/api/consumers"] })
  }
  async function create(event: FormEvent) {
    event.preventDefault()
    try {
      const value = await api<{ secret: string }>(sdk, "/api/consumers", {
        method: "POST",
        body: JSON.stringify({
          name,
          request_archive: requestArchive,
        }),
      })
      setSecret(value.secret)
      setName("")
      setRequestArchive(false)
      refreshConsumers()
    } catch (cause) {
      toast.error(message(cause, t))
    }
  }
  async function updateArchive(id: string, checked: boolean) {
    try {
      await api(sdk, `/api/consumers/${id}`, {
        method: "PATCH",
        body: JSON.stringify({ request_archive: checked }),
      })
      refreshConsumers()
      toast.success(t.requestArchiveUpdated)
    } catch (cause) {
      toast.error(message(cause, t))
    }
  }
  async function updateDisabled(id: string, checked: boolean) {
    try {
      await api(sdk, `/api/consumers/${id}`, {
        method: "PATCH",
        body: JSON.stringify({ is_disabled: checked }),
      })
      refreshConsumers()
      toast.success(t.consumerDisabledUpdated)
    } catch (cause) {
      toast.error(message(cause, t))
    }
  }
  async function rotate(id: string) {
    if (rotateId) return
    setRotateId(id)
    try {
      const value = await api<{ secret: string }>(
        sdk,
        `/api/consumers/${id}/rotate`,
        {
          method: "POST",
        }
      )
      refreshConsumers()
      try {
        await navigator.clipboard.writeText(value.secret)
        toast.success(t.consumerRotated)
      } catch {
        toast.error(t.consumerRotateCopyFailed)
      }
    } catch (cause) {
      toast.error(message(cause, t))
    } finally {
      setRotateId(null)
    }
  }
  async function confirmRotate() {
    const target = rotateTarget
    if (!target) return
    await rotate(target.id)
    setRotateTarget(null)
  }
  async function remove() {
    if (!deleteId) return
    try {
      await api(sdk, `/api/consumers/${deleteId}`, { method: "DELETE" })
      refreshConsumers()
      setDeleteId(null)
      toast.success(t.consumerDeleted)
    } catch (cause) {
      toast.error(message(cause, t))
    }
  }
  function openEdit(consumer: Consumer) {
    setEditTarget(consumer)
    setEditName(consumer.name)
  }
  async function update(event: FormEvent) {
    event.preventDefault()
    if (!editTarget || editPending) return
    setEditPending(true)
    try {
      await api(sdk, `/api/consumers/${editTarget.id}`, {
        method: "PATCH",
        body: JSON.stringify({ name: editName }),
      })
      setEditTarget(null)
      refreshConsumers()
      toast.success(t.consumerUpdated)
    } catch (cause) {
      toast.error(message(cause, t))
    } finally {
      setEditPending(false)
    }
  }
  if (loading) return <LoadingTable />
  if (error) return <ErrorState message={error} />
  return (
    <>
      <Card>
        <CardHeader className="flex-row items-start justify-between">
          <div>
            <CardTitle>{t.consumersTitle}</CardTitle>
            <CardDescription>{t.consumersDescription}</CardDescription>
          </div>
          <Button onClick={() => setOpen(true)}>
            <PlusIcon data-icon="inline-start" />
            {t.create}
          </Button>
        </CardHeader>
        <CardContent>
          {!data?.length ? (
            <EmptyState
              icon={<KeyRoundIcon />}
              title={t.noConsumers}
              description={t.noConsumersDescription}
              action={<Button onClick={() => setOpen(true)}>{t.create}</Button>}
            />
          ) : (
            <DataTable>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>{t.name}</TableHead>
                    <TableHead>{t.apiKey}</TableHead>
                    <TableHead>{t.createdAt}</TableHead>
                    <TableHead>{t.lastUsed}</TableHead>
                    <TableHead>{t.requestArchive}</TableHead>
                    <TableHead>{t.consumerEnabled}</TableHead>
                    <TableHead />
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {data.map((consumer) => (
                    <TableRow key={consumer.id}>
                      <TableCell className="font-medium">
                        <div className="flex items-center gap-1">
                          <span>{consumer.name}</span>
                          <Button
                            size="icon-xs"
                            variant="ghost"
                            aria-label={`${t.editConsumerAriaLabel}: ${consumer.name}`}
                            onClick={() => openEdit(consumer)}
                          >
                            <PencilIcon />
                          </Button>
                        </div>
                      </TableCell>
                      <TableCell className="font-mono text-xs">
                        <div className="flex items-center gap-1">
                          <span>{consumer.prefix}…</span>
                          <Button
                            size="icon-xs"
                            variant="ghost"
                            aria-label={`${t.rotateConsumer}: ${consumer.name}`}
                            disabled={rotateId === consumer.id}
                            onClick={() => setRotateTarget(consumer)}
                          >
                            {rotateId === consumer.id ? (
                              <Spinner />
                            ) : (
                              <RefreshCwIcon />
                            )}
                          </Button>
                        </div>
                      </TableCell>
                      <TableCell>
                        {formatTime(consumer.created_at, locale)}
                      </TableCell>
                      <TableCell>
                        {formatTime(consumer.last_used_at, locale)}
                      </TableCell>
                      <TableCell>
                        <Switch
                          aria-label={`${t.requestArchive}: ${consumer.name}`}
                          checked={consumer.request_archive}
                          onCheckedChange={(checked) =>
                            void updateArchive(consumer.id, checked)
                          }
                        />
                      </TableCell>
                      <TableCell>
                        <Switch
                          aria-label={`${t.consumerEnabled}: ${consumer.name}`}
                          checked={!consumer.is_disabled}
                          onCheckedChange={(checked) =>
                            void updateDisabled(consumer.id, !checked)
                          }
                        />
                      </TableCell>
                      <TableCell className="text-right">
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() => setDeleteId(consumer.id)}
                        >
                          <Trash2Icon data-icon="inline-start" />
                          {t.deleteConsumer}
                        </Button>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </DataTable>
          )}
        </CardContent>
      </Card>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {t.create} {t.consumer}
            </DialogTitle>
            <DialogDescription>{t.consumerAppHelp}</DialogDescription>
          </DialogHeader>
          <form onSubmit={create}>
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="consumer-name">{t.name}</FieldLabel>
                <Input
                  id="consumer-name"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  required
                />
              </Field>
              <Field orientation="horizontal">
                <FieldContent>
                  <FieldLabel htmlFor="consumer-request-archive">
                    {t.requestArchive}
                  </FieldLabel>
                  <FieldDescription>{t.requestArchiveHelp}</FieldDescription>
                </FieldContent>
                <Switch
                  id="consumer-request-archive"
                  checked={requestArchive}
                  onCheckedChange={setRequestArchive}
                />
              </Field>
              <DialogFooter>
                <Button type="submit" disabled={!name.trim()}>
                  {t.create}
                </Button>
              </DialogFooter>
            </FieldGroup>
          </form>
        </DialogContent>
      </Dialog>
      <Dialog
        open={Boolean(editTarget)}
        onOpenChange={(next) => !next && !editPending && setEditTarget(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {t.editConsumer} {t.consumer}
            </DialogTitle>
          </DialogHeader>
          <form onSubmit={update}>
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="consumer-edit-name">{t.name}</FieldLabel>
                <Input
                  id="consumer-edit-name"
                  value={editName}
                  onChange={(event) => setEditName(event.target.value)}
                  required
                />
              </Field>
              <DialogFooter>
                <Button
                  type="submit"
                  disabled={editPending || !editName.trim()}
                >
                  {editPending && <Spinner data-icon="inline-start" />}
                  {t.saveConsumer}
                </Button>
              </DialogFooter>
            </FieldGroup>
          </form>
        </DialogContent>
      </Dialog>
      <Dialog
        open={Boolean(secret)}
        onOpenChange={(next) => !next && setSecret("")}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t.saveConsumerTitle}</DialogTitle>
            <DialogDescription>{t.saveConsumerDescription}</DialogDescription>
          </DialogHeader>
          <div className="flex items-center gap-2 rounded-lg border bg-muted p-3">
            <code className="min-w-0 flex-1 text-xs break-all">{secret}</code>
            <Button
              size="icon-sm"
              variant="outline"
              aria-label={t.copied}
              onClick={() =>
                void navigator.clipboard
                  .writeText(secret)
                  .then(() => toast.success(t.copied))
              }
            >
              <ClipboardIcon />
            </Button>
          </div>
          <DialogFooter>
            <Button onClick={() => setSecret("")}>{t.savedConsumer}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <AlertDialog
        open={Boolean(rotateTarget)}
        onOpenChange={(next) => !next && !rotateId && setRotateTarget(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t.rotateConsumerTitle}</AlertDialogTitle>
            <AlertDialogDescription>
              {t.rotateConsumerDescription}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={Boolean(rotateId)}>
              {t.cancel}
            </AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              disabled={Boolean(rotateId)}
              onClick={(event) => {
                event.preventDefault()
                void confirmRotate()
              }}
            >
              {rotateId && <Spinner data-icon="inline-start" />}
              {t.confirmRotateConsumer}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
      <AlertDialog
        open={Boolean(deleteId)}
        onOpenChange={(next) => !next && setDeleteId(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t.deleteConsumerTitle}</AlertDialogTitle>
            <AlertDialogDescription>
              {t.deleteConsumerDescription}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t.cancel}</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => void remove()}
            >
              {t.confirmDeleteConsumer}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}

function UsagePage({
  sdk,
  locale,
  user,
}: {
  sdk: AuthSdk
  locale: Locale
  user: User
}) {
  const t = copy[locale]
  const [period, setPeriod] = useState<UsagePeriod>("7d")
  const [userFilter, setUserFilter] = useState("all")
  const [consumerFilter, setConsumerFilter] = useState("all")
  const [modelFilter, setModelFilter] = useState("all")
  const [rowDimensions, setRowDimensions] = useState<PivotDimension[]>(["user"])
  const [columnDimensions, setColumnDimensions] = useState<PivotDimension[]>([
    "date",
  ])
  const [dataFields, setDataFields] = useState<UsageMetric[]>(["input_tokens"])
  const [sorting, setSorting] = useState<SortingState>([])
  const { data, error, loading } = useApiQuery<UsageResponse>(
    sdk,
    `/api/usage?period=${period}`
  )
  const rows = useMemo(() => data?.rows ?? [], [data])
  const fieldLabels = useMemo<Record<PivotDimension, string>>(
    () => ({
      user: t.userLabel,
      consumer: t.consumerLabel,
      model: t.model,
      date: t.date,
    }),
    [t]
  )
  const consumers = useMemo(
    () =>
      uniqueUsageOptions(rows, (row) => row.consumer_id, usageConsumerLabel),
    [rows]
  )
  const models = useMemo(
    () =>
      uniqueUsageOptions(
        rows,
        (row) => row.model,
        (row) => row.model
      ),
    [rows]
  )
  const filteredRows = useMemo(
    () =>
      rows.filter(
        (row) =>
          (userFilter === "all" || row.user_id === userFilter) &&
          (consumerFilter === "all" || row.consumer_id === consumerFilter) &&
          (modelFilter === "all" || row.model === modelFilter)
      ),
    [rows, userFilter, consumerFilter, modelFilter]
  )
  const pivot = useMemo(
    () =>
      pivotUsageRows(
        filteredRows,
        rowDimensions,
        columnDimensions,
        (row) =>
          columnDimensions
            .map((dimension) => usageDimensionLabel(row, dimension))
            .join(" / ") || t.total
      ),
    [columnDimensions, filteredRows, rowDimensions, t.total]
  )
  const columns = useMemo<ColumnDef<PivotTableRow>[]>(() => {
    const result: ColumnDef<PivotTableRow>[] = rowDimensions.map(
      (dimension) => ({
        id: dimension,
        accessorFn: (row) => usageDimensionLabel(row.values, dimension),
        header: fieldLabels[dimension],
        cell: ({ row }) => (
          <UsageDimensionCell dimension={dimension} row={row.original.values} />
        ),
      })
    )
    if (!result.length)
      result.push({ id: "total", header: t.total, cell: () => t.total })
    for (const pivotColumn of pivot.columns) {
      result.push({
        id: `column-${pivotColumn.id}`,
        header: pivotColumn.label,
        columns: dataFields.map((field) => ({
          id: `${pivotColumn.id}-${field}`,
          accessorFn: (row) => row.cells[pivotColumn.id]?.[field] ?? 0,
          header: usageAggregateLabel(field, t),
          cell: (info) => (
            <span className="block text-right tabular-nums">
              {field === "official_cost_usd_nanos" ||
              field === "actual_cost_usd_nanos"
                ? formatUsd(Number(info.getValue()), locale)
                : Number(info.getValue()).toLocaleString(locale)}
            </span>
          ),
        })),
      })
    }
    return result
  }, [dataFields, fieldLabels, locale, pivot.columns, rowDimensions, t])
  // TanStack Table owns mutable table state, so React Compiler must not memoize this hook.
  // eslint-disable-next-line react-hooks/incompatible-library
  const table = useReactTable({
    data: pivot.rows,
    columns,
    state: { sorting },
    onSortingChange: setSorting,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
  })
  function assignDimension(
    dimension: PivotDimension,
    placement: PivotPlacement
  ) {
    setRowDimensions((current) =>
      placement === "rows"
        ? appendPivotDimension(current, dimension)
        : current.filter((item) => item !== dimension)
    )
    setColumnDimensions((current) =>
      placement === "columns"
        ? appendPivotDimension(current, dimension)
        : current.filter((item) => item !== dimension)
    )
  }
  const clearFilters = () => {
    setUserFilter("all")
    setConsumerFilter("all")
    setModelFilter("all")
  }
  if (loading) return <LoadingTable />
  if (error) return <ErrorState message={error} />
  return (
    <Card>
      <CardHeader className="gap-4">
        <div>
          <CardTitle>{t.usageTitle}</CardTitle>
          <CardDescription>{t.usageDescription}</CardDescription>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {(["24h", "7d"] as UsagePeriod[]).map((value) => (
            <Button
              key={value}
              size="sm"
              variant={period === value ? "default" : "outline"}
              aria-pressed={period === value}
              onClick={() => setPeriod(value)}
            >
              {value === "24h" ? t.last24Hours : t.last7Days}
            </Button>
          ))}
          {user.role !== "user" && (
            <LinkitUserPicker
              lang={locale}
              label={t.userLabel}
              placeholder={t.allUsers}
              value={userFilter === "all" ? "" : userFilter}
              onValueChange={(user_id) => setUserFilter(user_id || "all")}
            />
          )}
          <UsageSelect
            id="usage-consumer-filter"
            label={t.consumerLabel}
            value={consumerFilter}
            onValueChange={setConsumerFilter}
            allLabel={t.allConsumers}
            options={consumers}
          />
          <UsageSelect
            id="usage-model-filter"
            label={t.model}
            value={modelFilter}
            onValueChange={setModelFilter}
            allLabel={t.allModels}
            options={models}
          />
        </div>
        <div className="grid gap-3 border-t pt-3 text-xs xl:grid-cols-[1fr_1fr_auto] xl:items-start">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-muted-foreground">{t.pivotFields}</span>
            {pivotDimensions.map((dimension) => (
              <PivotFieldAssignment
                key={dimension}
                label={fieldLabels[dimension]}
                placement={pivotPlacement(
                  dimension,
                  rowDimensions,
                  columnDimensions
                )}
                onChange={(placement) => assignDimension(dimension, placement)}
                rowsLabel={t.rows}
                columnsLabel={t.columns}
                hiddenLabel={t.hidden}
              />
            ))}
          </div>
          <div className="flex flex-col gap-1.5">
            <PivotOrder
              label={t.rows}
              items={rowDimensions}
              itemLabel={(dimension) => fieldLabels[dimension]}
              onChange={setRowDimensions}
            />
            <PivotOrder
              label={t.columns}
              items={columnDimensions}
              itemLabel={(dimension) => fieldLabels[dimension]}
              onChange={setColumnDimensions}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <div className="flex flex-wrap items-center gap-1.5">
              <span className="text-muted-foreground">{t.data}</span>
              {usageMetrics.map((field) => (
                <Button
                  key={field}
                  size="xs"
                  variant={dataFields.includes(field) ? "secondary" : "ghost"}
                  aria-pressed={dataFields.includes(field)}
                  onClick={() =>
                    setDataFields((current) =>
                      toggleUsageMetric(current, field)
                    )
                  }
                >
                  {usageAggregateLabel(field, t)}
                </Button>
              ))}
            </div>
            <PivotOrder
              label={t.dataOrder}
              items={dataFields}
              itemLabel={(field) => usageAggregateLabel(field, t)}
              onChange={setDataFields}
            />
          </div>
        </div>
        <span className="text-xs text-muted-foreground tabular-nums">
          {pivot.rows.length.toLocaleString(locale)} {t.usageRows}
        </span>
      </CardHeader>
      <CardContent>
        {!rows.length || !filteredRows.length ? (
          <EmptyState
            icon={<SlidersHorizontalIcon />}
            title={t.noUsage}
            description={t.noUsageDescription}
            action={
              filteredRows.length ? undefined : (
                <Button variant="outline" onClick={clearFilters}>
                  {t.clearFilters}
                </Button>
              )
            }
          />
        ) : (
          <DataTable>
            <Table>
              <TableHeader>
                {table.getHeaderGroups().map((headerGroup) => (
                  <TableRow key={headerGroup.id}>
                    {headerGroup.headers.map((header) => (
                      <TableHead key={header.id} colSpan={header.colSpan}>
                        {header.isPlaceholder ? null : header.column.getCanSort() ? (
                          <Button
                            variant="ghost"
                            size="sm"
                            className="-ml-2"
                            onClick={header.column.getToggleSortingHandler()}
                            aria-label={`${t.sortColumn}: ${String(header.column.columnDef.header)}`}
                          >
                            <span>
                              {flexRender(
                                header.column.columnDef.header,
                                header.getContext()
                              )}
                            </span>
                            <ArrowDownUpIcon />
                          </Button>
                        ) : (
                          <span className="px-0.5">
                            {flexRender(
                              header.column.columnDef.header,
                              header.getContext()
                            )}
                          </span>
                        )}
                      </TableHead>
                    ))}
                  </TableRow>
                ))}
              </TableHeader>
              <TableBody>
                {table.getRowModel().rows.map((row) => (
                  <TableRow key={row.id}>
                    {row.getVisibleCells().map((cell) => (
                      <TableCell key={cell.id}>
                        {flexRender(
                          cell.column.columnDef.cell,
                          cell.getContext()
                        )}
                      </TableCell>
                    ))}
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </DataTable>
        )}
      </CardContent>
    </Card>
  )
}

function UsageSelect({
  id,
  label,
  value,
  onValueChange,
  allLabel,
  options,
}: {
  id: string
  label: string
  value: string
  onValueChange: (value: string) => void
  allLabel: string
  options: [string, string][]
}) {
  return (
    <Field orientation="horizontal" className="w-auto">
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      <Select
        value={value}
        onValueChange={(next) => {
          if (next) onValueChange(next)
        }}
      >
        <SelectTrigger id={id} className="max-w-52">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            <SelectItem value="all">{allLabel}</SelectItem>
            {options.map(([optionValue, optionLabel]) => (
              <SelectItem key={optionValue} value={optionValue}>
                {optionLabel}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
    </Field>
  )
}

function PivotFieldAssignment({
  label,
  placement,
  onChange,
  rowsLabel,
  columnsLabel,
  hiddenLabel,
}: {
  label: string
  placement: PivotPlacement
  onChange: (placement: PivotPlacement) => void
  rowsLabel: string
  columnsLabel: string
  hiddenLabel: string
}) {
  return (
    <Select
      value={placement}
      onValueChange={(value) => {
        if (value) onChange(value as PivotPlacement)
      }}
    >
      <SelectTrigger size="sm" aria-label={label}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectGroup>
          <SelectItem value="rows">
            {label} · {rowsLabel}
          </SelectItem>
          <SelectItem value="columns">
            {label} · {columnsLabel}
          </SelectItem>
          <SelectItem value="hidden">
            {label} · {hiddenLabel}
          </SelectItem>
        </SelectGroup>
      </SelectContent>
    </Select>
  )
}

function PivotOrder<T extends string>({
  label,
  items,
  itemLabel,
  onChange,
}: {
  label: string
  items: T[]
  itemLabel: (item: T) => string
  onChange: (items: T[]) => void
}) {
  return (
    <div className="flex flex-wrap items-center gap-1">
      <span className="mr-1 text-muted-foreground">{label}</span>
      {items.length ? (
        items.map((item, index) => (
          <span
            key={item}
            className="inline-flex items-center rounded-md border bg-muted/50 pl-2"
          >
            <span>{itemLabel(item)}</span>
            <Button
              size="icon-xs"
              variant="ghost"
              aria-label={`${label}: ${itemLabel(item)} ←`}
              disabled={index === 0}
              onClick={() => onChange(movePivotItem(items, index, -1))}
            >
              <ChevronLeftIcon />
            </Button>
            <Button
              size="icon-xs"
              variant="ghost"
              aria-label={`${label}: ${itemLabel(item)} →`}
              disabled={index === items.length - 1}
              onClick={() => onChange(movePivotItem(items, index, 1))}
            >
              <ChevronRightIcon />
            </Button>
          </span>
        ))
      ) : (
        <span className="text-muted-foreground">—</span>
      )}
    </div>
  )
}

function UsageDimensionCell({
  dimension,
  row,
}: {
  dimension: PivotDimension
  row: UsageRow
}) {
  if (dimension === "user") return <LinkitUserInfo userId={row.user_id} />
  if (dimension === "consumer")
    return (
      <div className="flex flex-col">
        <span className="font-medium">{row.consumer_name}</span>
        <code>{row.consumer_prefix}…</code>
      </div>
    )
  return <code>{usageDimensionLabel(row, dimension)}</code>
}

function AuditPage({
  sdk,
  locale,
  onOpenDetail,
}: {
  sdk: AuthSdk
  locale: Locale
  onOpenDetail: (id: string) => void
}) {
  const location = useLocation()
  const navigate = useNavigate()
  const t = copy[locale]
  const pageSize = 25
  const [page, setPage] = useState(0)
  const filters = useMemo(
    () => auditFiltersFromSearch(location.search),
    [location.search]
  )
  const [draftFilters, setDraftFilters] = useState<AuditFilters>(() => filters)
  useEffect(() => {
    setDraftFilters(filters)
    setPage(0)
  }, [filters])
  const query = useMemo(() => {
    const params = new URLSearchParams({
      limit: String(pageSize),
      offset: String(page * pageSize),
    })
    for (const [key, value] of Object.entries(filters)) {
      if (value && value !== "all") params.set(key, value)
    }
    return `/api/audit?${params}`
  }, [filters, page])
  const { data, error, loading, refreshing, reload } =
    useApiQuery<AuditPageResponse>(sdk, query)
  const rows = data?.rows ?? []
  const total = data?.total ?? 0
  const totalPages = Math.max(1, Math.ceil(total / pageSize))

  function applyFilters(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setPage(0)
    navigate({
      pathname: "/audit",
      search: auditFilterSearch(draftFilters),
    })
  }

  function clearFilters() {
    setPage(0)
    navigate({ pathname: "/audit", search: "" })
  }

  async function openDetail(row: Audit) {
    onOpenDetail(row.id)
  }
  if (loading) return <LoadingTable />
  if (error) return <ErrorState message={error} />
  return (
    <div className="flex flex-col gap-5">
      <Card>
        <CardHeader className="flex-row items-start justify-between gap-3">
          <div>
            <CardTitle>{t.auditTitle}</CardTitle>
            <CardDescription>{t.auditDescription}</CardDescription>
          </div>
          <Button
            variant="outline"
            disabled={refreshing}
            onClick={() => void reload()}
          >
            {refreshing ? (
              <Spinner data-icon="inline-start" />
            ) : (
              <RefreshCwIcon data-icon="inline-start" />
            )}
            {t.refresh}
          </Button>
        </CardHeader>
        <CardContent>
          <form
            className="mb-4 grid gap-3 border-b pb-4 md:grid-cols-2 xl:grid-cols-6"
            onSubmit={applyFilters}
          >
            <LinkitUserPicker
              lang={locale}
              label={t.filterUserId}
              value={draftFilters.user_id}
              onValueChange={(user_id) =>
                setDraftFilters((current) => ({ ...current, user_id }))
              }
            />
            <Field>
              <FieldLabel htmlFor="audit-consumer-filter">
                {t.filterConsumer}
              </FieldLabel>
              <Input
                id="audit-consumer-filter"
                value={draftFilters.consumer}
                onChange={(event) =>
                  setDraftFilters((current) => ({
                    ...current,
                    consumer: event.target.value,
                  }))
                }
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="audit-provider-filter">
                {t.filterProvider}
              </FieldLabel>
              <Input
                id="audit-provider-filter"
                value={draftFilters.provider}
                onChange={(event) =>
                  setDraftFilters((current) => ({
                    ...current,
                    provider: event.target.value,
                  }))
                }
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="audit-model-filter">
                {t.filterModel}
              </FieldLabel>
              <Input
                id="audit-model-filter"
                value={draftFilters.model}
                onChange={(event) =>
                  setDraftFilters((current) => ({
                    ...current,
                    model: event.target.value,
                  }))
                }
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="audit-error-code-filter">
                {t.errorCode}
              </FieldLabel>
              <Input
                id="audit-error-code-filter"
                value={draftFilters.error_code}
                onChange={(event) =>
                  setDraftFilters((current) => ({
                    ...current,
                    error_code: event.target.value,
                  }))
                }
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="audit-status-filter">{t.status}</FieldLabel>
              <Select
                value={draftFilters.status}
                onValueChange={(status) =>
                  status &&
                  setDraftFilters((current) => ({ ...current, status }))
                }
              >
                <SelectTrigger id="audit-status-filter">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    <SelectItem value="all">{t.allStatuses}</SelectItem>
                    <SelectItem value="success">{t.successfulCalls}</SelectItem>
                    <SelectItem value="error">{t.failedCalls}</SelectItem>
                  </SelectGroup>
                </SelectContent>
              </Select>
            </Field>
            <div className="flex flex-wrap items-end gap-2 xl:col-span-6">
              <Button type="submit">{t.filter}</Button>
              <Button type="button" variant="outline" onClick={clearFilters}>
                {t.clearFilters}
              </Button>
              <span className="self-center text-sm text-muted-foreground tabular-nums">
                {total.toLocaleString(locale)} {t.auditResults}
              </span>
            </div>
          </form>
          {!rows.length ? (
            <EmptyState
              icon={<ScrollTextIcon />}
              title={t.noAudit}
              description={t.noAuditDescription}
              action={
                Object.values(filters).some(
                  (value) => value && value !== "all"
                ) ? (
                  <Button variant="outline" onClick={clearFilters}>
                    {t.clearFilters}
                  </Button>
                ) : undefined
              }
            />
          ) : (
            <>
              <DataTable>
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>{t.userLabel}</TableHead>
                      <TableHead>{t.consumer}</TableHead>
                      <TableHead>{t.time}</TableHead>
                      <TableHead>{t.model}</TableHead>
                      <TableHead className="text-right">
                        {t.actualCost}
                      </TableHead>
                      <TableHead className="text-right">
                        {t.officialCost}
                      </TableHead>
                      <TableHead className="text-right">
                        {t.priceMultiplier}
                      </TableHead>
                      <TableHead>{t.provider}</TableHead>
                      <TableHead>{t.threadId}</TableHead>
                      <TableHead>{t.status}</TableHead>
                      <TableHead>{t.usage}</TableHead>
                      <TableHead className="text-right">{t.actions}</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {rows.map((row) => (
                      <TableRow key={row.id}>
                        <TableCell>
                          <LinkitUserInfo userId={row.user_id} />
                        </TableCell>
                        <TableCell className="font-medium">
                          {row.consumer_name}
                        </TableCell>
                        <TableCell className="whitespace-nowrap">
                          {formatTime(row.created_at, locale)}
                        </TableCell>
                        <TableCell>
                          <div className="flex min-w-52 flex-col gap-1">
                            <code>{row.model || "—"}</code>
                            <div className="flex flex-wrap gap-1">
                              <Badge
                                variant={row.peak ? "destructive" : "secondary"}
                              >
                                {row.peak ? t.pricingPeak : t.pricingOffPeak}
                              </Badge>
                              {row.reasoning_effort && (
                                <Badge variant="outline">
                                  {t.reasoningEffort} · {row.reasoning_effort}
                                </Badge>
                              )}
                            </div>
                          </div>
                        </TableCell>
                        <TableCell className="text-right tabular-nums">
                          {formatUsd(row.actual_cost_usd_nanos, locale)}
                        </TableCell>
                        <TableCell className="text-right tabular-nums">
                          {formatUsd(row.official_cost_usd_nanos, locale)}
                        </TableCell>
                        <TableCell className="text-right tabular-nums">
                          {formatMultiplier(row.price_multiplier_nanos, locale)}
                        </TableCell>
                        <TableCell title={row.provider_id}>
                          {row.provider_name || "—"}
                        </TableCell>
                        <TableCell>
                          {row.thread_id ? (
                            <CopyableIdentifier
                              value={row.thread_id}
                              label={t.threadId}
                              copyLabel={t.copyThreadId}
                              copiedLabel={t.copied}
                            />
                          ) : (
                            "—"
                          )}
                        </TableCell>
                        <TableCell>
                          <div className="flex max-w-48 flex-col gap-1">
                            <Badge
                              className="w-fit"
                              variant={
                                isAuditError(
                                  row.status,
                                  row.error,
                                  row.error_code
                                )
                                  ? "destructive"
                                  : "secondary"
                              }
                            >
                              {isAuditError(
                                row.status,
                                row.error,
                                row.error_code
                              )
                                ? `${t.failedCalls} · HTTP ${row.status}`
                                : row.status}
                            </Badge>
                            <div className="flex flex-col text-xs text-muted-foreground tabular-nums">
                              <span>
                                {t.firstByteLatency} ·{" "}
                                {formatLatency(row.first_byte_latency_ms)}
                              </span>
                              <span>
                                {t.totalLatency} ·{" "}
                                {formatLatency(row.latency_ms)}
                              </span>
                            </div>
                            {row.error_code != null && (
                              <code className="text-xs break-all whitespace-normal text-destructive">
                                {row.error_code}
                              </code>
                            )}
                            {row.error && (
                              <span className="text-xs break-words whitespace-normal text-destructive">
                                {row.error}
                              </span>
                            )}
                          </div>
                        </TableCell>
                        <TableCell>
                          <div className="flex items-center gap-3 text-xs tabular-nums">
                            <div className="flex flex-col gap-0.5">
                              <span className="text-muted-foreground">
                                {t.input}
                              </span>
                              <span>{row.input_tokens.toLocaleString()}</span>
                            </div>
                            <div className="flex flex-col gap-0.5">
                              <span className="text-muted-foreground">
                                {t.cachedInput}
                              </span>
                              <span>{row.cached_tokens.toLocaleString()}</span>
                            </div>
                            <div className="flex flex-col gap-0.5">
                              <span className="text-muted-foreground">
                                {t.cacheHitRate}
                              </span>
                              <span>
                                {cacheHitRate(
                                  row.cached_tokens,
                                  row.input_tokens,
                                  locale
                                )}
                              </span>
                            </div>
                            <div className="flex flex-col gap-0.5">
                              <span className="text-muted-foreground">
                                {t.output}
                              </span>
                              <span>{row.output_tokens.toLocaleString()}</span>
                            </div>
                            <div className="flex flex-col gap-0.5">
                              <span className="text-muted-foreground">
                                {t.requestSize}
                              </span>
                              <span>
                                {formatBytes(row.request_bytes, locale)}
                              </span>
                            </div>
                            <div className="flex flex-col gap-0.5">
                              <span className="text-muted-foreground">
                                {t.responseSize}
                              </span>
                              <span>
                                {formatBytes(row.response_bytes, locale)}
                              </span>
                            </div>
                            <div className="flex flex-col gap-0.5">
                              <span className="text-muted-foreground">
                                {t.networkTransport}
                              </span>
                              <span>
                                {formatBytes(
                                  row.request_transport_bytes +
                                    row.response_transport_bytes,
                                  locale
                                )}
                              </span>
                            </div>
                          </div>
                        </TableCell>
                        <TableCell className="text-right">
                          <Button
                            size="sm"
                            variant="outline"
                            onClick={() => void openDetail(row)}
                          >
                            {t.details}
                          </Button>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </DataTable>
              <div className="mt-4 flex flex-wrap items-center justify-end gap-2">
                <span className="mr-auto text-sm text-muted-foreground tabular-nums">
                  {t.page} {page + 1} / {totalPages}
                </span>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={page === 0}
                  onClick={() => setPage((current) => current - 1)}
                >
                  <ChevronLeftIcon data-icon="inline-start" />
                  {t.previousPage}
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={page + 1 >= totalPages}
                  onClick={() => setPage((current) => current + 1)}
                >
                  {t.nextPage}
                  <ChevronRightIcon data-icon="inline-end" />
                </Button>
              </div>
            </>
          )}
        </CardContent>
      </Card>
    </div>
  )
}

function CopyableIdentifier({
  value,
  label,
  copyLabel,
  copiedLabel,
}: {
  value: string
  label: string
  copyLabel: string
  copiedLabel: string
}) {
  return (
    <div className="flex items-center gap-1">
      <code title={value} aria-label={`${label}: ${value}`}>
        {value.slice(0, 12)}
        {value.length > 12 && "…"}
      </code>
      <Button
        size="icon-xs"
        variant="ghost"
        aria-label={`${copyLabel}: ${value}`}
        onClick={() =>
          void navigator.clipboard
            .writeText(value)
            .then(() => toast.success(copiedLabel))
        }
      >
        <ClipboardIcon />
      </Button>
    </div>
  )
}

function RequestDetailPage({ sdk, locale }: { sdk: AuthSdk; locale: Locale }) {
  const { auditId } = useParams()
  const location = useLocation()
  const navigate = useNavigate()
  const t = copy[locale]
  const { data, error, loading, reload } = useApiQuery<AuditDetail>(
    sdk,
    "/api/audit/" + (auditId || "")
  )
  const [deleteBodiesOpen, setDeleteBodiesOpen] = useState(false)
  const [deleteBodiesPending, setDeleteBodiesPending] = useState(false)
  if (!auditId) return <ErrorState message={t.unknownError} />
  if (loading) return <LoadingTable />
  if (error) return <ErrorState message={error} />
  if (!data) return <ErrorState message={t.unknownError} />
  const audit = data
  const request = parseJson(data.request_body)
  const affinityId = affinityRequestId(request, data.affinity_source)

  async function deleteBodies() {
    if (deleteBodiesPending) return
    setDeleteBodiesPending(true)
    try {
      await api(sdk, `/api/audit/${audit.id}/bodies`, { method: "DELETE" })
      await reload()
      setDeleteBodiesOpen(false)
      toast.success(t.auditBodiesDeleted)
    } catch (cause) {
      toast.error(message(cause, t))
    } finally {
      setDeleteBodiesPending(false)
    }
  }

  return (
    <div className="flex max-w-6xl flex-col gap-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <Button
          variant="outline"
          onClick={() =>
            navigate({ pathname: "/audit", search: location.search })
          }
        >
          <ChevronLeftIcon data-icon="inline-start" />
          {t.backToAudit}
        </Button>
        <div className="flex flex-wrap gap-2">
          <Button
            variant="outline"
            disabled={!data.previous}
            onClick={() =>
              data.previous &&
              navigate({
                pathname: `/audit/${data.previous.id}`,
                search: location.search,
              })
            }
          >
            <ChevronLeftIcon data-icon="inline-start" />
            {t.previousRequest}
          </Button>
          <Button
            variant="outline"
            disabled={!data.next}
            onClick={() =>
              data.next &&
              navigate({
                pathname: `/audit/${data.next.id}`,
                search: location.search,
              })
            }
          >
            {t.nextRequest}
            <ChevronRightIcon data-icon="inline-end" />
          </Button>
          {data.bodies_available && (
            <Button
              variant="destructive"
              onClick={() => setDeleteBodiesOpen(true)}
            >
              <Trash2Icon data-icon="inline-start" />
              {t.deleteAuditBodies}
            </Button>
          )}
        </div>
      </div>
      <RequestSummary
        data={data}
        affinityId={affinityId}
        labels={t}
        locale={locale}
      />
      {data.archive_available && !data.bodies_available && (
        <Alert>
          <Trash2Icon />
          <AlertTitle>{t.auditBodiesDeleted}</AlertTitle>
          <AlertDescription>{t.auditBodiesDeletedDescription}</AlertDescription>
        </Alert>
      )}
      {data.path === "/v1/responses" && data.bodies_available && (
        <>
          <FinalResponse
            responseBody={data.response_body}
            truncated={data.response_body_truncated}
            labels={t}
          />
          <ResponsesAPIRequestBodyRenderer
            request={data.request_body}
            locale={locale}
            truncated={data.request_body_truncated}
          />
        </>
      )}
      {data.bodies_available && (
        <DiagnosticBodies
          data={data}
          labels={t}
          showRequestBody={data.path !== "/v1/responses"}
        />
      )}
      <AlertDialog
        open={deleteBodiesOpen}
        onOpenChange={(next) =>
          !next && !deleteBodiesPending && setDeleteBodiesOpen(false)
        }
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t.deleteAuditBodiesTitle}</AlertDialogTitle>
            <AlertDialogDescription>
              {t.deleteAuditBodiesDescription}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={deleteBodiesPending}>
              {t.cancel}
            </AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              disabled={deleteBodiesPending}
              onClick={() => void deleteBodies()}
            >
              {deleteBodiesPending && <Spinner data-icon="inline-start" />}
              {t.confirmDeleteAuditBodies}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}

function RequestSummary({
  data,
  affinityId,
  labels,
  locale,
}: {
  data: AuditDetail
  affinityId?: string
  labels: (typeof copy)[Locale]
  locale: Locale
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{labels.requestSummary}</CardTitle>
        <CardDescription>
          {data.method} {data.path} · {formatTime(data.created_at, locale)} ·{" "}
          {labels.firstByteLatency} {formatLatency(data.first_byte_latency_ms)}{" "}
          · {labels.totalLatency} {formatLatency(data.latency_ms)}
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-6">
        <Definition
          rows={[
            [labels.requestId, data.request_id],
            [labels.threadId, data.thread_id || "—"],
            [labels.consumer, data.consumer_name],
            [labels.userId, data.user_id],
            [labels.provider, data.provider_name || data.provider_id || "—"],
            [labels.model, data.model || "—"],
            [
              labels.pricingTariff,
              data.peak ? labels.pricingPeak : labels.pricingOffPeak,
            ],
            [labels.reasoningEffort, data.reasoning_effort || "—"],
            [labels.upstreamHttpProtocol, data.upstream_http_version || "—"],
            [labels.httpStatusCode, data.status],
            [
              labels.status,
              isAuditError(data.status, data.error, data.error_code)
                ? labels.failedCalls
                : labels.successfulCalls,
            ],
            [labels.errorCode, data.error_code ?? "—"],
            [labels.errorMessage, data.error ?? "—"],
            [labels.requestSize, formatBytes(data.request_bytes, locale)],
            [labels.responseSize, formatBytes(data.response_bytes, locale)],
            [
              labels.requestTransportSize,
              formatBytes(data.request_transport_bytes, locale),
            ],
            [
              labels.responseTransportSize,
              formatBytes(data.response_transport_bytes, locale),
            ],
            [
              labels.compressionRatio,
              compressionRatio(
                data.response_bytes,
                data.response_transport_bytes
              ),
            ],
            [
              labels.downstreamAcceptEncoding,
              data.downstream_accept_encoding || "identity",
            ],
            [
              labels.downstreamContentEncoding,
              data.downstream_content_encoding || "identity",
            ],
            [
              labels.upstreamAcceptEncoding,
              data.upstream_accept_encoding || "identity",
            ],
            [
              labels.upstreamContentEncoding,
              data.upstream_content_encoding || "identity",
            ],
            [labels.affinitySource, data.affinity_source || "—"],
            [labels.affinityRequestId, affinityId || "—"],
            [labels.affinityHash, data.affinity_hash || "—"],
          ]}
        />
        <section className="flex flex-col gap-3">
          <h3 className="text-sm font-medium">{labels.tokenUsage}</h3>
          <Definition
            rows={[
              [labels.inputTokens, data.input_tokens.toLocaleString(locale)],
              [
                labels.cachedInputTokens,
                data.cached_tokens.toLocaleString(locale),
              ],
              [labels.outputTokens, data.output_tokens.toLocaleString(locale)],
              [
                labels.officialCost,
                formatUsd(data.official_cost_usd_nanos, locale),
              ],
              [
                labels.priceMultiplier,
                formatMultiplier(data.price_multiplier_nanos, locale),
              ],
              [
                labels.actualCost,
                formatUsd(data.actual_cost_usd_nanos, locale),
              ],
              [
                labels.officialConsumedUsdBefore,
                formatUsd(data.official_consumed_usd_before_nanos, locale),
              ],
              [
                labels.officialConsumedUsdAfter,
                formatUsd(data.official_consumed_usd_after_nanos, locale),
              ],
              [
                labels.actualConsumedUsdBefore,
                formatUsd(data.actual_consumed_usd_before_nanos, locale),
              ],
              [
                labels.actualConsumedUsdAfter,
                formatUsd(data.actual_consumed_usd_after_nanos, locale),
              ],
              [
                labels.officialProvidedUsdBefore,
                formatUsd(data.official_provided_usd_before_nanos, locale),
              ],
              [
                labels.officialProvidedUsdAfter,
                formatUsd(data.official_provided_usd_after_nanos, locale),
              ],
              [
                labels.actualProvidedUsdBefore,
                formatUsd(data.actual_provided_usd_before_nanos, locale),
              ],
              [
                labels.actualProvidedUsdAfter,
                formatUsd(data.actual_provided_usd_after_nanos, locale),
              ],
              [
                labels.cacheHitRate,
                cacheHitRate(data.cached_tokens, data.input_tokens, locale),
              ],
            ]}
          />
        </section>
        {data.archive_available && (
          <>
            <section className="flex flex-col gap-3">
              <h3 className="text-sm font-medium">{labels.requestHeaders}</h3>
              <HeaderComparison
                left={data.request_headers}
                right={data.upstream_request_headers}
                leftLabel={labels.downstreamToLb}
                rightLabel={labels.lbToUpstream}
                labels={labels}
              />
            </section>
            <section className="flex flex-col gap-3">
              <h3 className="text-sm font-medium">{labels.responseHeaders}</h3>
              <HeaderComparison
                left={data.response_headers}
                right={data.downstream_response_headers}
                leftLabel={labels.upstreamToLb}
                rightLabel={labels.lbToDownstream}
                labels={labels}
              />
            </section>
          </>
        )}
      </CardContent>
    </Card>
  )
}

function DiagnosticBodies({
  data,
  labels,
  showRequestBody,
}: {
  data: AuditDetail
  labels: (typeof copy)[Locale]
  showRequestBody: boolean
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{labels.diagnosticData}</CardTitle>
        <CardDescription>{labels.diagnosticDataDescription}</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-5">
        {showRequestBody && (
          <DiagnosticPreview
            title={labels.requestBody}
            value={data.request_body}
            truncated={data.request_body_truncated}
          />
        )}
        <DiagnosticPreview
          title={labels.responseBody}
          value={data.response_body}
          truncated={data.response_body_truncated}
        />
      </CardContent>
    </Card>
  )
}

function FinalResponse({
  responseBody,
  truncated,
  labels,
}: {
  responseBody?: string
  truncated: boolean
  labels: (typeof copy)[Locale]
}) {
  const output = responseOutputText(responseBody)
  return (
    <Card>
      <CardHeader>
        <CardTitle>{labels.finalResponse}</CardTitle>
        {truncated && (
          <CardDescription>{labels.previewTruncated}</CardDescription>
        )}
      </CardHeader>
      <CardContent>
        {output ? (
          <div className="max-h-80 overflow-auto rounded-md border bg-muted p-3 text-sm leading-6 break-words whitespace-pre-wrap">
            {output}
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">
            {labels.finalResponseUnavailable}
          </p>
        )}
      </CardContent>
    </Card>
  )
}

function HeaderComparison({
  left,
  right,
  leftLabel,
  rightLabel,
  labels,
}: {
  left?: string | null
  right?: string | null
  leftLabel: string
  rightLabel: string
  labels: (typeof copy)[Locale]
}) {
  const { rows, leftAvailable, rightAvailable } = compareHeaderSnapshots(
    left,
    right
  )
  return (
    <ScrollArea className="w-full rounded-md border">
      <Table className="min-w-[720px]">
        {(!leftAvailable || !rightAvailable) && (
          <TableCaption className="px-3 pb-3">
            {labels.headerComparisonUnavailable}
          </TableCaption>
        )}
        <TableHeader>
          <TableRow>
            <TableHead className="w-48">{labels.headerName}</TableHead>
            <TableHead>
              <div className="flex items-center gap-2">
                {leftLabel}
                {!leftAvailable && (
                  <Badge variant="outline">
                    {labels.headerSnapshotMissing}
                  </Badge>
                )}
              </div>
            </TableHead>
            <TableHead>
              <div className="flex items-center gap-2">
                {rightLabel}
                {!rightAvailable && (
                  <Badge variant="outline">
                    {labels.headerSnapshotMissing}
                  </Badge>
                )}
              </div>
            </TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.length === 0 ? (
            <TableRow>
              <TableCell
                colSpan={3}
                className="py-8 text-center text-muted-foreground"
              >
                —
              </TableCell>
            </TableRow>
          ) : (
            rows.map((row) => (
              <TableRow
                key={row.name}
                className={row.differs ? "bg-amber-500/10" : undefined}
              >
                <TableCell className="font-mono text-xs">
                  <div className="flex items-center gap-2">
                    <span>{row.name}</span>
                    {row.differs && (
                      <Badge variant="secondary">{labels.different}</Badge>
                    )}
                  </div>
                </TableCell>
                <TableCell className="font-mono text-xs break-all whitespace-pre-wrap">
                  {row.left || "—"}
                </TableCell>
                <TableCell className="font-mono text-xs break-all whitespace-pre-wrap">
                  {row.right || "—"}
                </TableCell>
              </TableRow>
            ))
          )}
        </TableBody>
      </Table>
    </ScrollArea>
  )
}

function DiagnosticPreview({
  title,
  value,
  truncated = false,
}: {
  title: string
  value?: string
  truncated?: boolean
}) {
  const t = currentMessages()
  return (
    <section className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <h3 className="text-sm font-medium">{title}</h3>
        {truncated && <Badge variant="secondary">{t.previewTruncated}</Badge>}
      </div>
      <pre className="max-h-80 overflow-auto rounded-md border bg-muted p-3 font-mono text-xs break-all whitespace-pre-wrap">
        {value || "—"}
      </pre>
    </section>
  )
}

function UsersPage({
  sdk,
  locale,
  user,
}: {
  sdk: AuthSdk
  locale: Locale
  user: User
}) {
  const t = copy[locale]
  const queryClient = useQueryClient()
  const { data, error, loading } = useApiQuery<ManagedUser[]>(sdk, "/api/users")
  const [sorting, setSorting] = useState<SortingState>([
    { id: "consumed_usd_nanos", desc: true },
  ])
  const refreshUsers = useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: ["/api/users"] })
  }, [queryClient])
  const updateRole = useCallback(
    async (id: string, role: "admin" | "user") => {
      try {
        await api(sdk, `/api/users/${id}`, {
          method: "PATCH",
          body: JSON.stringify({ role }),
        })
        refreshUsers()
        toast.success(t.roleUpdated)
      } catch (cause) {
        toast.error(message(cause, t))
      }
    },
    [refreshUsers, sdk, t]
  )
  const updateAllowDebt = useCallback(
    async (id: string, allow_debt: boolean) => {
      try {
        await api(sdk, `/api/users/${id}`, {
          method: "PATCH",
          body: JSON.stringify({ allow_debt }),
        })
        refreshUsers()
        toast.success(t.allowDebtUpdated)
      } catch (cause) {
        toast.error(message(cause, t))
      }
    },
    [refreshUsers, sdk, t]
  )
  const columns = useMemo<ColumnDef<ManagedUser>[]>(
    () => [
      {
        id: "user",
        header: t.userLabel,
        enableSorting: false,
        cell: ({ row }) => <LinkitUserInfo userId={row.original.id} />,
      },
      {
        id: "created_at",
        accessorFn: (row) => userTableSortValue(row, "created_at"),
        header: t.createdAt,
        cell: ({ row }) => formatTime(row.original.created_at, locale),
      },
      {
        id: "topup_usd_nanos",
        accessorFn: (row) => userTableSortValue(row, "topup_usd_nanos"),
        header: t.cumulativeTopups,
        cell: ({ row }) => (
          <span className="block text-right tabular-nums">
            {formatUsd(row.original.topup_usd_nanos, locale)}
          </span>
        ),
      },
      {
        id: "consumed_usd_nanos",
        accessorFn: (row) => userTableSortValue(row, "consumed_usd_nanos"),
        header: t.cumulativeConsumption,
        cell: ({ row }) => (
          <span className="block text-right tabular-nums">
            {formatUsd(row.original.consumed_usd_nanos, locale)}
          </span>
        ),
      },
      {
        id: "provided_usd_nanos",
        accessorFn: (row) => userTableSortValue(row, "provided_usd_nanos"),
        header: t.providedValue,
        cell: ({ row }) => (
          <span className="block text-right tabular-nums">
            {formatUsd(row.original.provided_usd_nanos, locale)}
          </span>
        ),
      },
      {
        id: "available_usd_nanos",
        accessorFn: (row) => userTableSortValue(row, "available_usd_nanos"),
        header: t.availableCredit,
        cell: ({ row }) => (
          <span className="block text-right tabular-nums">
            {formatUsd(row.original.available_usd_nanos, locale)}
          </span>
        ),
      },
      {
        id: "role",
        accessorFn: (row) => userTableSortValue(row, "role"),
        header: t.role,
        cell: ({ row }) => {
          const item = row.original
          return user.role === "root" && item.role !== "root" ? (
            <Select
              value={item.role}
              onValueChange={(value) =>
                void updateRole(item.id, value as "admin" | "user")
              }
            >
              <SelectTrigger aria-label={`${t.role}: ${item.id}`}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem value="admin">{t.roleAdmin}</SelectItem>
                  <SelectItem value="user">{t.roleUser}</SelectItem>
                </SelectGroup>
              </SelectContent>
            </Select>
          ) : (
            <Badge variant="secondary">{roleLabel(item.role, locale)}</Badge>
          )
        },
      },
      {
        id: "allow_debt",
        accessorFn: (row) => userTableSortValue(row, "allow_debt"),
        header: t.allowDebt,
        cell: ({ row }) => {
          const item = row.original
          return (
            <Switch
              aria-label={`${t.allowDebt}: ${item.id}`}
              checked={item.allow_debt}
              onCheckedChange={(checked) =>
                void updateAllowDebt(item.id, checked)
              }
            />
          )
        },
      },
    ],
    [locale, t, updateAllowDebt, updateRole, user.role]
  )
  // TanStack Table owns mutable table state, so React Compiler must not memoize this hook.
  // eslint-disable-next-line react-hooks/incompatible-library
  const table = useReactTable({
    data: data ?? [],
    columns,
    state: { sorting },
    onSortingChange: setSorting,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
  })
  if (loading) return <LoadingTable />
  if (error) return <ErrorState message={error} />
  return (
    <div className="flex min-w-0 flex-col gap-4">
      <Card>
        <CardHeader>
          <CardTitle>{t.usersTitle}</CardTitle>
          <CardDescription>{t.usersDescription}</CardDescription>
        </CardHeader>
        <CardContent>
          {!data?.length ? (
            <EmptyState
              icon={<UserRoundCogIcon />}
              title={t.noUsers}
              description={t.noUsersDescription}
            />
          ) : (
            <DataTable>
              <Table>
                <TableHeader>
                  {table.getHeaderGroups().map((headerGroup) => (
                    <TableRow key={headerGroup.id}>
                      {headerGroup.headers.map((header) => {
                        const direction = header.column.getIsSorted()
                        const numeric = isUserTableNumericColumn(
                          header.column.id
                        )
                        return (
                          <TableHead
                            key={header.id}
                            aria-sort={
                              direction === "asc"
                                ? "ascending"
                                : direction === "desc"
                                  ? "descending"
                                  : "none"
                            }
                            className={cn(numeric && "text-right")}
                          >
                            <Button
                              variant="ghost"
                              size="sm"
                              className={cn(numeric ? "-mr-2" : "-ml-2")}
                              aria-label={`${t.sortColumn}: ${String(header.column.columnDef.header)}`}
                              aria-pressed={Boolean(direction)}
                              onClick={header.column.getToggleSortingHandler()}
                            >
                              <span>
                                {String(header.column.columnDef.header)}
                              </span>
                              {direction === "asc" ? (
                                <ChevronUpIcon data-icon="inline-end" />
                              ) : direction === "desc" ? (
                                <ChevronDownIcon data-icon="inline-end" />
                              ) : (
                                <ArrowDownUpIcon data-icon="inline-end" />
                              )}
                            </Button>
                          </TableHead>
                        )
                      })}
                    </TableRow>
                  ))}
                </TableHeader>
                <TableBody>
                  {table.getRowModel().rows.map((row) => (
                    <TableRow key={row.id}>
                      {row.getVisibleCells().map((cell) => (
                        <TableCell key={cell.id}>
                          {flexRender(
                            cell.column.columnDef.cell,
                            cell.getContext()
                          )}
                        </TableCell>
                      ))}
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </DataTable>
          )}
        </CardContent>
      </Card>
    </div>
  )
}

function SettingsPage({
  sdk,
  user,
  locale,
}: {
  sdk: AuthSdk
  user: User
  locale: Locale
}) {
  const t = copy[locale]
  const { data, error, loading } = useApiQuery<SettingsData>(
    sdk,
    "/api/settings"
  )
  if (loading) return <LoadingTable />
  if (error) return <ErrorState message={error} />
  if (!data) return <ErrorState message={t.unknownError} />
  return (
    <div className="flex max-w-4xl flex-col gap-4">
      <Card>
        <CardHeader>
          <CardTitle>{t.identityPermissions}</CardTitle>
          <CardDescription>{t.identityDescription}</CardDescription>
        </CardHeader>
        <CardContent>
          <Definition
            rows={[
              [t.userId, user.id],
              [t.role, roleLabel(user.role, locale)],
              [t.authIssuer, data.auth_issuer],
            ]}
          />
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>{t.proxyBoundary}</CardTitle>
          <CardDescription>{t.proxyDescription}</CardDescription>
        </CardHeader>
        <CardContent>
          <Definition
            rows={[
              [t.upstream, data.upstream_base],
              [t.bodyLimit, `${data.response_body_limit} bytes`],
              [t.affinityTtl, `${data.affinity_ttl_seconds} s`],
              [t.pricingModels, data.available_model_ids.join(", ") || "—"],
            ]}
          />
        </CardContent>
      </Card>
      {(user.role === "root" || user.role === "admin") && (
        <>
          <ProviderConcurrencySettings
            sdk={sdk}
            locale={locale}
            initial={data.provider_concurrency_limit}
          />
          <AvailableModelsSettings
            sdk={sdk}
            locale={locale}
            initial={data.available_model_ids}
          />
        </>
      )}
      {user.role === "root" && (
        <>
          <RuntimeSettings sdk={sdk} locale={locale} initial={data} />
          <MidasSettings sdk={sdk} locale={locale} />
        </>
      )}
    </div>
  )
}

function ProviderConcurrencySettings({
  sdk,
  locale,
  initial,
}: {
  sdk: AuthSdk
  locale: Locale
  initial: number
}) {
  const t = copy[locale]
  const queryClient = useQueryClient()
  const [value, setValue] = useState(String(initial))
  const [pending, setPending] = useState(false)
  const limit = Number(value)
  const invalid = !Number.isSafeInteger(limit) || limit < 1
  useEffect(() => setValue(String(initial)), [initial])

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (invalid || pending) return
    setPending(true)
    try {
      await api(sdk, "/api/settings/provider-concurrency", {
        method: "PATCH",
        body: JSON.stringify({ provider_concurrency_limit: limit }),
      })
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["/api/settings"] }),
        queryClient.invalidateQueries({ queryKey: ["/api/providers"] }),
      ])
      toast.success(t.settingsSaved)
    } catch (cause) {
      toast.error(message(cause, t))
    } finally {
      setPending(false)
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t.providerConcurrency}</CardTitle>
        <CardDescription>{t.providerConcurrencyDescription}</CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={save}>
          <FieldGroup>
            <Field data-invalid={invalid} data-disabled={pending}>
              <FieldLabel htmlFor="settings-provider-concurrency">
                {t.providerConcurrencyLimit}
              </FieldLabel>
              <Input
                id="settings-provider-concurrency"
                type="number"
                min={1}
                step={1}
                value={value}
                onChange={(event) => setValue(event.target.value)}
                disabled={pending}
                aria-invalid={invalid}
                aria-describedby="settings-provider-concurrency-help"
                required
              />
              <FieldDescription id="settings-provider-concurrency-help">
                {invalid
                  ? t.providerConcurrencyInvalid
                  : t.providerConcurrencyHelp}
              </FieldDescription>
            </Field>
            <Button type="submit" disabled={pending || invalid}>
              {pending && <Spinner data-icon="inline-start" />}
              {t.saveSettings}
            </Button>
          </FieldGroup>
        </form>
      </CardContent>
    </Card>
  )
}

function AvailableModelsSettings({
  sdk,
  locale,
  initial,
}: {
  sdk: AuthSdk
  locale: Locale
  initial: string[]
}) {
  const t = copy[locale]
  const [value, setValue] = useState(initial.join("\n"))
  const [pending, setPending] = useState(false)

  useEffect(() => setValue(initial.join("\n")), [initial])

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const available_model_ids = value.split(/[\s,]+/).filter(Boolean)
    if (available_model_ids.length === 0) {
      toast.error(t.availableModelsRequired)
      return
    }
    setPending(true)
    try {
      await api(sdk, "/api/settings/available-models", {
        method: "PATCH",
        body: JSON.stringify({ available_model_ids }),
      })
      toast.success(t.availableModelsSaved)
    } catch (cause) {
      toast.error(message(cause, t))
    } finally {
      setPending(false)
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t.availableModels}</CardTitle>
        <CardDescription>{t.availableModelsDescription}</CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={save}>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="settings-available-models">
                {t.availableModels}
              </FieldLabel>
              <Textarea
                id="settings-available-models"
                className="min-h-56 font-mono"
                spellCheck={false}
                value={value}
                onChange={(event) => setValue(event.target.value)}
                required
              />
              <FieldDescription>{t.availableModelsHelp}</FieldDescription>
            </Field>
            <Button type="submit" disabled={pending}>
              {pending && <Spinner data-icon="inline-start" />}
              {t.saveSettings}
            </Button>
          </FieldGroup>
        </form>
      </CardContent>
    </Card>
  )
}

function MidasSettings({ sdk, locale }: { sdk: AuthSdk; locale: Locale }) {
  const t = copy[locale]
  const query = useApiQuery<MidasSettingsData>(sdk, "/api/payments/settings")
  const [settings, setSettings] = useState({
    midas_api_base: "",
    midas_fund_user_id: "",
    midas_fund_api_key: "",
  })
  const [pending, setPending] = useState(false)

  useEffect(() => {
    if (!query.data) return
    setSettings({
      midas_api_base: query.data.midas_api_base,
      midas_fund_user_id: query.data.midas_fund_user_id || "",
      midas_fund_api_key: "",
    })
  }, [query.data])

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setPending(true)
    try {
      await api(sdk, "/api/payments/settings", {
        method: "PATCH",
        body: JSON.stringify(settings),
      })
      await query.reload()
      toast.success(t.midasSettingsSaved)
    } catch (cause) {
      toast.error(message(cause, t))
    } finally {
      setPending(false)
    }
  }

  if (query.loading) return <LoadingTable />
  if (query.error) return <ErrorState message={query.error} />
  if (!query.data) return <ErrorState message={t.unknownError} />
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t.midasSettings}</CardTitle>
        <CardDescription>{t.midasSettingsDescription}</CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={save}>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="midas-api-base">{t.midasApiBase}</FieldLabel>
              <Input
                id="midas-api-base"
                type="url"
                value={settings.midas_api_base}
                onChange={(event) =>
                  setSettings((current) => ({
                    ...current,
                    midas_api_base: event.target.value,
                  }))
                }
                required
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="midas-fund-user-id">
                {t.midasFundUserId}
              </FieldLabel>
              <Input
                id="midas-fund-user-id"
                value={settings.midas_fund_user_id}
                onChange={(event) =>
                  setSettings((current) => ({
                    ...current,
                    midas_fund_user_id: event.target.value,
                  }))
                }
                required
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="midas-fund-api-key">
                {t.midasFundApiKey}
              </FieldLabel>
              <Input
                id="midas-fund-api-key"
                type="password"
                autoComplete="new-password"
                value={settings.midas_fund_api_key}
                onChange={(event) =>
                  setSettings((current) => ({
                    ...current,
                    midas_fund_api_key: event.target.value,
                  }))
                }
              />
              <FieldDescription>
                {query.data.midas_fund_api_key_configured
                  ? t.midasConfigured
                  : t.midasNotConfigured}
              </FieldDescription>
            </Field>
            <Button type="submit" disabled={pending}>
              {pending && <Spinner data-icon="inline-start" />}
              {t.saveSettings}
            </Button>
          </FieldGroup>
        </form>
      </CardContent>
    </Card>
  )
}

function RuntimeSettings({
  sdk,
  locale,
  initial,
}: {
  sdk: AuthSdk
  locale: Locale
  initial: SettingsData
}) {
  const t = copy[locale]
  const [settings, setSettings] = useState({
    upstream_base: initial.upstream_base,
    model_price_multiplier: initial.model_price_multiplier,
    allow_all_users_debt: initial.allow_all_users_debt,
    response_body_limit: initial.response_body_limit,
    affinity_ttl_seconds: initial.affinity_ttl_seconds,
    request_archive_retention_days: initial.request_archive_retention_days,
  })
  const [pending, setPending] = useState(false)
  function update<K extends keyof typeof settings>(
    key: K,
    value: (typeof settings)[K]
  ) {
    setSettings((current) => ({ ...current, [key]: value }))
  }
  async function save(event: FormEvent) {
    event.preventDefault()
    setPending(true)
    try {
      await api(sdk, "/api/settings", {
        method: "PATCH",
        body: JSON.stringify(settings),
      })
      toast.success(t.settingsSaved)
    } catch (cause) {
      toast.error(message(cause, t))
    } finally {
      setPending(false)
    }
  }
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t.runtimeSettings}</CardTitle>
        <CardDescription>{t.runtimeSettingsDescription}</CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={save}>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="settings-upstream">{t.upstream}</FieldLabel>
              <Input
                id="settings-upstream"
                type="url"
                value={settings.upstream_base}
                onChange={(event) =>
                  update("upstream_base", event.target.value)
                }
                required
              />
              <FieldDescription>{t.upstreamHint}</FieldDescription>
            </Field>
            <Field>
              <FieldLabel htmlFor="settings-model-price-multiplier">
                {t.modelPriceMultiplier}
              </FieldLabel>
              <Input
                id="settings-model-price-multiplier"
                type="number"
                inputMode="decimal"
                min="0.000000001"
                max="1000"
                step="any"
                value={settings.model_price_multiplier}
                onChange={(event) =>
                  update("model_price_multiplier", event.target.value)
                }
                required
              />
              <FieldDescription>{t.modelPriceMultiplierHint}</FieldDescription>
            </Field>
            <Field orientation="horizontal">
              <FieldContent>
                <FieldLabel htmlFor="settings-allow-all-users-debt">
                  {t.allowAllUsersDebt}
                </FieldLabel>
                <FieldDescription>{t.allowAllUsersDebtHint}</FieldDescription>
              </FieldContent>
              <Switch
                id="settings-allow-all-users-debt"
                checked={settings.allow_all_users_debt}
                onCheckedChange={(checked) =>
                  update("allow_all_users_debt", checked)
                }
              />
            </Field>
            <FieldGroup className="grid gap-4 sm:grid-cols-2">
              <Field>
                <FieldLabel htmlFor="settings-response-limit">
                  {t.responseLimit}
                </FieldLabel>
                <Input
                  id="settings-response-limit"
                  type="number"
                  min={1024}
                  max={16777216}
                  value={settings.response_body_limit}
                  onChange={(event) =>
                    update("response_body_limit", event.target.valueAsNumber)
                  }
                  required
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="settings-affinity-ttl">
                  {t.affinityTtl}
                </FieldLabel>
                <Input
                  id="settings-affinity-ttl"
                  type="number"
                  min={60}
                  max={2592000}
                  value={settings.affinity_ttl_seconds}
                  onChange={(event) =>
                    update("affinity_ttl_seconds", event.target.valueAsNumber)
                  }
                  required
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="settings-archive-retention">
                  {t.archiveRetention}
                </FieldLabel>
                <Input
                  id="settings-archive-retention"
                  type="number"
                  min={1}
                  max={365}
                  value={settings.request_archive_retention_days}
                  onChange={(event) =>
                    update(
                      "request_archive_retention_days",
                      event.target.valueAsNumber
                    )
                  }
                  required
                />
              </Field>
            </FieldGroup>
            <Button className="self-start" type="submit" disabled={pending}>
              {pending && <Spinner data-icon="inline-start" />}
              {t.saveSettings}
            </Button>
          </FieldGroup>
        </form>
      </CardContent>
    </Card>
  )
}

function AdminAuditPage({ sdk, locale }: { sdk: AuthSdk; locale: Locale }) {
  const t = copy[locale]
  const { data, error, loading } = useApiQuery<AdminAudit[]>(
    sdk,
    "/api/admin-audit?limit=200"
  )
  if (loading) return <LoadingTable />
  if (error) return <ErrorState message={error} />
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t.adminAuditTitle}</CardTitle>
        <CardDescription>{t.adminAuditDescription}</CardDescription>
      </CardHeader>
      <CardContent>
        {!data?.length ? (
          <EmptyState
            icon={<ShieldAlertIcon />}
            title={t.noAudit}
            description={t.noAuditDescription}
          />
        ) : (
          <DataTable>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t.time}</TableHead>
                  <TableHead>{t.administrator}</TableHead>
                  <TableHead>{t.action}</TableHead>
                  <TableHead>{t.target}</TableHead>
                  <TableHead>{t.clientIp}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.map((row) => (
                  <TableRow key={row.id}>
                    <TableCell>{formatTime(row.created_at, locale)}</TableCell>
                    <TableCell>
                      <LinkitUserInfo userId={row.admin_user_id} />
                    </TableCell>
                    <TableCell>
                      <code>{row.action}</code>
                    </TableCell>
                    <TableCell>
                      <code>{row.target_id || "—"}</code>
                    </TableCell>
                    <TableCell>
                      <code>{row.client_ip || "—"}</code>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </DataTable>
        )}
      </CardContent>
    </Card>
  )
}

type ProviderAuditChartPoint = {
  hour_start: number
  failure_rate: number | null
  requests: number
  input_tokens: number
}

type ProviderAuditMetricKey = "failure_rate" | "requests" | "input_tokens"

function ProviderAuditMetricChart({
  chartData,
  chartConfig,
  dataKey,
  label,
  formatValue,
  locale,
  domain,
  yAxisWidth,
}: {
  chartData: ProviderAuditChartPoint[]
  chartConfig: ChartConfig
  dataKey: ProviderAuditMetricKey
  label: string
  formatValue: (value: number) => string
  locale: Locale
  domain?: [number, number]
  yAxisWidth: number
}) {
  return (
    <figure>
      <figcaption className="mb-3 text-sm font-medium">{label}</figcaption>
      <ChartContainer config={chartConfig} className="aspect-auto h-64 w-full">
        <LineChart accessibilityLayer data={chartData}>
          <CartesianGrid vertical={false} />
          <ChartTooltip
            labelFormatter={(value) => formatAuditHour(Number(value), locale)}
            formatter={(value) => [formatValue(Number(value)), label]}
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
            domain={domain}
            tickFormatter={(value) => formatValue(Number(value))}
            tickLine={false}
            width={yAxisWidth}
          />
          <Line
            connectNulls={false}
            dot={false}
            dataKey={dataKey}
            stroke={"var(--color-" + dataKey + ")"}
            strokeWidth={2}
            type="monotone"
          />
        </LineChart>
      </ChartContainer>
    </figure>
  )
}

function ProviderAuditPage({ sdk, locale }: { sdk: AuthSdk; locale: Locale }) {
  const t = copy[locale]
  const [period, setPeriod] = useState<UsagePeriod>("7d")
  const [providerFilter, setProviderFilter] = useState("all")
  const [modelFilter, setModelFilter] = useState("all")
  const { data, error, loading, refreshing, reload } =
    useApiQuery<ProviderAuditResponse>(
      sdk,
      "/api/provider-audit?period=" + period
    )
  const rows = useMemo(() => data?.rows ?? [], [data])
  const providerOptions = useMemo(
    () =>
      [
        ...new Map(
          rows.map((row) => [
            row.provider_id ?? "__unknown__",
            row.provider_name ?? t.providerAuditUnknownProvider,
          ])
        ),
      ].sort((left, right) => left[1].localeCompare(right[1], locale)),
    [locale, rows, t.providerAuditUnknownProvider]
  )
  const modelOptions = useMemo(
    () =>
      [...new Set(rows.map((row) => row.model))]
        .sort((left, right) => left.localeCompare(right, locale))
        .map((model) => [model, model] as [string, string]),
    [locale, rows]
  )
  const filteredRows = useMemo(
    () =>
      rows.filter(
        (row) =>
          (providerFilter === "all" ||
            (row.provider_id ?? "__unknown__") === providerFilter) &&
          (modelFilter === "all" || row.model === modelFilter)
      ),
    [modelFilter, providerFilter, rows]
  )
  const totals = useMemo(
    () =>
      filteredRows.reduce(
        (summary, row) => ({
          requests: summary.requests + row.requests,
          successful: summary.successful + row.successful_requests,
          failed: summary.failed + row.failed_requests,
          input_tokens: summary.input_tokens + row.input_tokens,
        }),
        { requests: 0, successful: 0, failed: 0, input_tokens: 0 }
      ),
    [filteredRows]
  )
  const successRate = totals.requests ? totals.successful / totals.requests : 0
  const failureRate = totals.requests ? totals.failed / totals.requests : 0
  const chartData = useMemo<ProviderAuditChartPoint[]>(() => {
    if (!data) return []
    const start = Math.floor(data.since / 3600) * 3600
    const end = Math.floor(data.until / 3600) * 3600
    const byHour = new Map<
      number,
      { requests: number; failed: number; input_tokens: number }
    >()
    for (const row of filteredRows) {
      const value = byHour.get(row.hour_start) ?? {
        requests: 0,
        failed: 0,
        input_tokens: 0,
      }
      value.requests += row.requests
      value.failed += row.failed_requests
      value.input_tokens += row.input_tokens
      byHour.set(row.hour_start, value)
    }
    return Array.from(
      { length: Math.max(0, Math.floor((end - start) / 3600) + 1) },
      (_, index) => {
        const hour = start + index * 3600
        const value = byHour.get(hour)
        return {
          hour_start: hour,
          failure_rate: value?.requests ? value.failed / value.requests : null,
          requests: value?.requests ?? 0,
          input_tokens: value?.input_tokens ?? 0,
        }
      }
    )
  }, [data, filteredRows])
  const chartConfig = {
    failure_rate: {
      label: t.providerAuditFailureRate,
      color: "var(--destructive)",
    },
    requests: {
      label: t.providerAuditTotalRequests,
      color: "var(--chart-2)",
    },
    input_tokens: {
      label: t.providerAuditInputTokens,
      color: "var(--chart-3)",
    },
  } satisfies ChartConfig

  if (loading) return <LoadingTable />
  if (error) return <ErrorState message={error} />
  return (
    <div className="flex flex-col gap-5">
      <Card>
        <CardHeader className="flex-row items-start justify-between gap-3">
          <div>
            <CardTitle>{t.providerAuditTitle}</CardTitle>
            <CardDescription>{t.providerAuditDescription}</CardDescription>
          </div>
          <Button
            variant="outline"
            disabled={refreshing}
            onClick={() => void reload()}
          >
            {refreshing ? (
              <Spinner data-icon="inline-start" />
            ) : (
              <RefreshCwIcon data-icon="inline-start" />
            )}
            {t.refresh}
          </Button>
        </CardHeader>
        <CardContent className="flex flex-col gap-5">
          <div className="grid gap-px overflow-hidden rounded-lg border bg-border sm:grid-cols-2 xl:grid-cols-4">
            <div className="bg-background p-4">
              <dt>{t.providerAuditTotalRequests}</dt>
              <dd className="mt-1 text-2xl font-semibold tabular-nums">
                {totals.requests.toLocaleString(locale)}
              </dd>
            </div>
            <div className="bg-background p-4">
              <dt>{t.providerAuditSuccessRate}</dt>
              <dd className="mt-1 text-2xl font-semibold tabular-nums">
                {formatPercent(successRate * 100, locale)}
              </dd>
              <p className="mt-1 text-xs text-muted-foreground tabular-nums">
                {totals.successful.toLocaleString(locale)}{" "}
                {t.providerAuditSuccessfulRequests}
              </p>
            </div>
            <div className="bg-background p-4">
              <dt>{t.providerAuditFailureRate}</dt>
              <dd className="mt-1 text-2xl font-semibold tabular-nums">
                {formatPercent(failureRate * 100, locale)}
              </dd>
              <p className="mt-1 text-xs text-muted-foreground tabular-nums">
                {totals.failed.toLocaleString(locale)}{" "}
                {t.providerAuditFailedRequests}
              </p>
            </div>
            <div className="bg-background p-4">
              <dt>{t.providerAuditInputTokens}</dt>
              <dd className="mt-1 text-2xl font-semibold tabular-nums">
                {totals.input_tokens.toLocaleString(locale)}
              </dd>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-3 border-t pt-4">
            <Tabs
              value={period}
              onValueChange={(value) =>
                value && setPeriod(value as UsagePeriod)
              }
            >
              <TabsList aria-label={t.providerAuditPeriod}>
                <TabsTrigger value="7d">{t.last7Days}</TabsTrigger>
                <TabsTrigger value="24h">{t.last24Hours}</TabsTrigger>
              </TabsList>
            </Tabs>
            <UsageSelect
              id="provider-audit-provider-filter"
              label={t.provider}
              value={providerFilter}
              onValueChange={setProviderFilter}
              allLabel={t.providerAuditAllProviders}
              options={providerOptions}
            />
            <UsageSelect
              id="provider-audit-model-filter"
              label={t.model}
              value={modelFilter}
              onValueChange={setModelFilter}
              allLabel={t.allModels}
              options={modelOptions}
            />
          </div>
          {!filteredRows.length ? (
            <EmptyState
              icon={<ActivityIcon />}
              title={t.providerAuditNoData}
              description={t.providerAuditNoDataDescription}
            />
          ) : (
            <div className="grid gap-8 xl:grid-cols-3">
              <ProviderAuditMetricChart
                chartData={chartData}
                chartConfig={chartConfig}
                dataKey="failure_rate"
                label={t.providerAuditFailureRateChart}
                formatValue={(value) => formatPercent(value * 100, locale)}
                locale={locale}
                domain={[0, 1]}
                yAxisWidth={48}
              />
              <ProviderAuditMetricChart
                chartData={chartData}
                chartConfig={chartConfig}
                dataKey="requests"
                label={t.providerAuditRequestCountChart}
                formatValue={(value) => value.toLocaleString(locale)}
                locale={locale}
                yAxisWidth={64}
              />
              <ProviderAuditMetricChart
                chartData={chartData}
                chartConfig={chartConfig}
                dataKey="input_tokens"
                label={t.providerAuditInputTokensChart}
                formatValue={(value) => value.toLocaleString(locale)}
                locale={locale}
                yAxisWidth={80}
              />
            </div>
          )}
          <p className="text-xs text-muted-foreground">
            {t.providerAuditChartDescription}
          </p>
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>{t.providerAuditTitle}</CardTitle>
          <CardDescription>{t.providerAuditTableDescription}</CardDescription>
        </CardHeader>
        <CardContent>
          {!filteredRows.length ? (
            <EmptyState
              icon={<ActivityIcon />}
              title={t.providerAuditNoData}
              description={t.providerAuditNoDataDescription}
            />
          ) : (
            <DataTable>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>{t.providerAuditHour}</TableHead>
                    <TableHead>{t.provider}</TableHead>
                    <TableHead>{t.model}</TableHead>
                    <TableHead className="text-right">
                      {t.providerAuditTotalRequests}
                    </TableHead>
                    <TableHead className="text-right">
                      {t.providerAuditInputTokens}
                    </TableHead>
                    <TableHead className="text-right">
                      {t.providerAuditSuccessfulRequests}
                    </TableHead>
                    <TableHead className="text-right">
                      {t.providerAuditFailedRequests}
                    </TableHead>
                    <TableHead className="text-right">
                      {t.providerAuditSuccessRate}
                    </TableHead>
                    <TableHead className="text-right">
                      {t.providerAuditFailureRate}
                    </TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {filteredRows.map((row) => (
                    <TableRow
                      key={[
                        row.hour_start,
                        row.provider_id ?? "unknown",
                        row.model,
                      ].join("-")}
                    >
                      <TableCell className="tabular-nums">
                        {formatAuditHour(row.hour_start, locale)}
                      </TableCell>
                      <TableCell>
                        <code>
                          {row.provider_name ?? t.providerAuditUnknownProvider}
                        </code>
                      </TableCell>
                      <TableCell>
                        <code>{row.model}</code>
                      </TableCell>
                      <TableCell className="text-right tabular-nums">
                        {row.requests.toLocaleString(locale)}
                      </TableCell>
                      <TableCell className="text-right tabular-nums">
                        {row.input_tokens.toLocaleString(locale)}
                      </TableCell>
                      <TableCell className="text-right tabular-nums">
                        {row.successful_requests.toLocaleString(locale)}
                      </TableCell>
                      <TableCell className="text-right tabular-nums">
                        {row.failed_requests.toLocaleString(locale)}
                      </TableCell>
                      <TableCell className="text-right tabular-nums">
                        {formatPercent(row.success_rate * 100, locale)}
                      </TableCell>
                      <TableCell className="text-right tabular-nums">
                        {formatPercent(row.failure_rate * 100, locale)}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </DataTable>
          )}
        </CardContent>
      </Card>
    </div>
  )
}

type PivotDimension = "user" | "consumer" | "model" | "date"
type PivotPlacement = "rows" | "columns" | "hidden"
type UsageMetric = keyof PivotCell

const pivotDimensions: PivotDimension[] = ["user", "consumer", "model", "date"]
const usageMetrics: UsageMetric[] = [
  "requests",
  "input_tokens",
  "output_tokens",
  "cached_tokens",
  "official_cost_usd_nanos",
  "actual_cost_usd_nanos",
  "network_transport_bytes",
]

function usageConsumerLabel(row: UsageRow) {
  return `${row.consumer_name} · ${row.consumer_prefix}…`
}
function usageDimensionLabel(row: UsageRow, dimension: PivotDimension) {
  return dimension === "user"
    ? row.user_id
    : dimension === "consumer"
      ? usageConsumerLabel(row)
      : row[dimension]
}
function usageAggregateLabel(metric: UsageMetric, t: typeof copy.zh) {
  if (metric === "requests") return `COUNT(${t.requestCount})`
  return `SUM(${metric === "input_tokens" ? t.inputTokens : metric === "cached_tokens" ? t.cachedInputTokens : metric === "output_tokens" ? t.outputTokens : metric === "official_cost_usd_nanos" ? t.officialCost : metric === "actual_cost_usd_nanos" ? t.actualCost : t.networkTransport})`
}
function uniqueUsageOptions(
  rows: UsageRow[],
  value: (row: UsageRow) => string,
  label: (row: UsageRow) => string
): [string, string][] {
  return [...new Map(rows.map((row) => [value(row), label(row)])).entries()]
}
function pivotPlacement(
  dimension: PivotDimension,
  rows: PivotDimension[],
  columns: PivotDimension[]
): PivotPlacement {
  return rows.includes(dimension)
    ? "rows"
    : columns.includes(dimension)
      ? "columns"
      : "hidden"
}
function appendPivotDimension(
  dimensions: PivotDimension[],
  dimension: PivotDimension
) {
  return dimensions.includes(dimension)
    ? dimensions
    : [...dimensions, dimension]
}
function movePivotItem<T>(items: T[], index: number, distance: number) {
  const next = [...items]
  const target = index + distance
  ;[next[index], next[target]] = [next[target], next[index]]
  return next
}
function toggleUsageMetric(metrics: UsageMetric[], metric: UsageMetric) {
  return metrics.length === 1 && metrics.includes(metric)
    ? metrics
    : metrics.includes(metric)
      ? metrics.filter((item) => item !== metric)
      : [...metrics, metric]
}
function pivotUsageRows(
  rows: UsageRow[],
  rowDimensions: PivotDimension[],
  columnDimensions: PivotDimension[],
  label: (row: UsageRow) => string
) {
  const columns = new Map<string, PivotColumn>()
  const grouped = new Map<string, PivotTableRow>()
  for (const row of rows) {
    const rowId = usagePivotKey(row, rowDimensions)
    const columnId = usagePivotKey(row, columnDimensions)
    columns.set(columnId, { id: columnId, label: label(row) })
    const pivotRow = grouped.get(rowId) ?? { id: rowId, values: row, cells: {} }
    const cell = pivotRow.cells[columnId] ?? {
      requests: 0,
      input_tokens: 0,
      cached_tokens: 0,
      output_tokens: 0,
      official_cost_usd_nanos: 0,
      actual_cost_usd_nanos: 0,
      network_transport_bytes: 0,
    }
    cell.requests += row.requests
    cell.input_tokens += row.input_tokens
    cell.cached_tokens += row.cached_tokens
    cell.output_tokens += row.output_tokens
    cell.official_cost_usd_nanos += row.official_cost_usd_nanos
    cell.actual_cost_usd_nanos += row.actual_cost_usd_nanos
    cell.network_transport_bytes += row.network_transport_bytes
    pivotRow.cells[columnId] = cell
    grouped.set(rowId, pivotRow)
  }
  return {
    rows: [...grouped.values()],
    columns: [...columns.values()].sort((left, right) =>
      left.label.localeCompare(right.label)
    ),
  }
}
function usagePivotKey(row: UsageRow, dimensions: PivotDimension[]) {
  return (
    dimensions
      .map((dimension) => usageDimensionLabel(row, dimension))
      .join("\u001f") || "total"
  )
}
function parseJson(value?: string): unknown {
  if (!value) return undefined
  try {
    return JSON.parse(value)
  } catch {
    return value
  }
}

function recordValue(value: unknown): Record<string, unknown> | undefined {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined
}
function affinityRequestId(
  request: unknown,
  source?: string
): string | undefined {
  const value = source ? recordValue(request)?.[source] : undefined
  return typeof value === "string" ? value : undefined
}
function Definition({ rows }: { rows: [string, unknown][] }) {
  return (
    <dl className="grid gap-3">
      {rows.map(([label, value]) => (
        <div
          key={label}
          className="grid gap-1 border-b pb-3 last:border-0 last:pb-0 sm:grid-cols-[10rem_1fr]"
        >
          <dt>{label}</dt>
          <dd className="font-mono text-xs break-all">
            {String(value ?? "—")}
          </dd>
        </div>
      ))}
    </dl>
  )
}
function StatusBadge({ status, locale }: { status: string; locale: Locale }) {
  const bad =
    status === "auth_error" || status === "cooldown" || status === "disabled"
  return (
    <Badge variant={bad ? "destructive" : "secondary"}>
      {status === "active" ? (
        <CheckCircle2Icon />
      ) : status === "cooldown" ? (
        <CircleGaugeIcon />
      ) : (
        <XCircleIcon />
      )}
      {statusLabel(status, locale)}
    </Badge>
  )
}
function DataTable({ children }: { children: ReactNode }) {
  return (
    <ScrollArea className="w-full whitespace-nowrap">
      <div className="min-w-180">{children}</div>
    </ScrollArea>
  )
}
function EmptyState({
  icon,
  title,
  description,
  action,
}: {
  icon: ReactNode
  title: string
  description: string
  action?: ReactNode
}) {
  return (
    <Empty className="border">
      <EmptyHeader>
        <EmptyMedia variant="icon">{icon}</EmptyMedia>
        <EmptyTitle>{title}</EmptyTitle>
        <EmptyDescription>{description}</EmptyDescription>
      </EmptyHeader>
      {action && <EmptyContent>{action}</EmptyContent>}
    </Empty>
  )
}
function ErrorState({ message: detail }: { message: string }) {
  const t = currentMessages()
  return (
    <Alert variant="destructive">
      <ShieldAlertIcon />
      <AlertTitle>{t.unableLoad}</AlertTitle>
      <AlertDescription>{detail}</AlertDescription>
    </Alert>
  )
}
function LoadingTable() {
  return (
    <Card>
      <CardHeader>
        <Skeleton className="h-5 w-40" />
        <Skeleton className="h-4 w-72 max-w-full" />
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {Array.from({ length: 5 }, (_, i) => (
          <Skeleton key={i} className="h-10 w-full" />
        ))}
      </CardContent>
    </Card>
  )
}
function CenteredLoading() {
  const t = currentMessages()
  return (
    <main className="flex min-h-svh items-center justify-center">
      <div className="flex items-center gap-2 text-sm text-muted-foreground">
        <Spinner />
        {t.loading}
      </div>
    </main>
  )
}

function useApiQuery<T>(sdk: AuthSdk, path: string, refetchInterval?: number) {
  const query = useQuery({
    queryKey: [path],
    refetchInterval,
    queryFn: ({ signal }) => api<T>(sdk, path, { signal }),
  })
  return {
    data: query.data ?? null,
    error: query.error ? message(query.error) : "",
    loading: query.isPending,
    refreshing: query.isFetching,
    reload: query.refetch,
  }
}
function currentMessages() {
  return copy[document.documentElement.lang.startsWith("zh") ? "zh" : "en"]
}
function message(cause: unknown, t = currentMessages()) {
  if (cause instanceof Error) {
    return cause.message
  }
  return t.unknownError
}
function isAbortError(cause: unknown) {
  return cause instanceof DOMException && cause.name === "AbortError"
}
function formatTime(timestamp: number | undefined, locale: Locale) {
  return timestamp === undefined || timestamp === null
    ? "—"
    : new Intl.DateTimeFormat(locale === "zh" ? "zh-CN" : "en-US", {
        dateStyle: "short",
        timeStyle: "medium",
      }).format(timestamp * 1000)
}
function formatStorageBytes(bytes: number, locale: Locale) {
  const units = ["B", "KiB", "MiB", "GiB", "TiB"]
  let value = Math.max(0, bytes)
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  return `${new Intl.NumberFormat(locale === "zh" ? "zh-CN" : "en-US", {
    maximumFractionDigits: value >= 100 || unit === 0 ? 0 : 1,
  }).format(value)} ${units[unit]}`
}
function formatRate(bytesPerSecond: number, locale: Locale) {
  return `${formatStorageBytes(bytesPerSecond, locale)}/s`
}
function formatLatency(milliseconds: number | undefined) {
  return typeof milliseconds === "number"
    ? `${(milliseconds / 1000).toFixed(1)} s`
    : "—"
}
function formatBytes(bytes: number, locale: Locale) {
  return `${bytes.toLocaleString(locale)} B`
}
function formatUsd(nanos: number | undefined, locale: Locale) {
  if (typeof nanos !== "number") return "—"
  return new Intl.NumberFormat(locale === "zh" ? "zh-CN" : "en-US", {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits: 9,
  }).format(nanos / 1_000_000_000)
}

function formatMultiplier(nanos: number, locale: Locale) {
  return `${(nanos / 1_000_000_000).toLocaleString(
    locale === "zh" ? "zh-CN" : "en-US",
    { maximumFractionDigits: 9 }
  )}×`
}

function compressionRatio(contentBytes: number, transportBytes: number) {
  if (contentBytes <= 0) return "—"
  return `${((1 - transportBytes / contentBytes) * 100).toFixed(1)}%`
}

function cacheHitRate(
  cachedTokens: number,
  inputTokens: number,
  locale: Locale
) {
  if (inputTokens <= 0) return "—"
  return (cachedTokens / inputTokens).toLocaleString(locale, {
    style: "percent",
    maximumFractionDigits: 1,
  })
}

function inputOutputRatio(
  inputTokens: number,
  outputTokens: number,
  locale: Locale
) {
  if (inputTokens <= 0 && outputTokens <= 0) return "—"
  if (outputTokens <= 0) return "∞:1"
  return `${(inputTokens / outputTokens).toLocaleString(locale, {
    maximumFractionDigits: 2,
  })}:1`
}

function roleLabel(role: User["role"], locale: Locale) {
  const t = copy[locale]
  return role === "root"
    ? t.roleRoot
    : role === "admin"
      ? t.roleAdmin
      : t.roleUser
}
function statusLabel(status: string, locale: Locale) {
  const t = copy[locale]
  return (
    {
      active: t.statusActive,
      cooldown: t.statusCooldown,
      auth_error: t.statusAuthError,
      disabled: t.statusDisabled,
    }[status] || t.statusUnknown
  )
}
function pageForPath(pathname: string): Page {
  if (pathname.startsWith("/audit/")) return "request-detail"
  return (
    (
      {
        "/dashboard": "dashboard",
        "/providers": "providers",
        "/consumers": "consumers",
        "/codex-integration": "codex-integration",
        "/dsh-integration": "dsh-integration",
        "/opencode-integration": "opencode-integration",
        "/direct-api-integration": "direct-api-integration",
        "/usage": "usage",
        "/audit": "audit",
        "/topups": "topups",
        "/model-prices": "model-prices",
        "/system-resources": "system-resources",
        "/admin-audit": "admin-audit",
        "/provider-audit": "provider-audit",
        "/users": "users",
        "/settings": "settings",
      } as const
    )[pathname] ?? "dashboard"
  )
}
function pageTitle(page: Page, locale: Locale) {
  const t = copy[locale]
  return page === "request-detail" ? t.requestDetail : t[page]
}
function pageDescription(page: Page, locale: Locale) {
  const t = copy[locale]
  return {
    dashboard: t.pageDashboard,
    providers: t.pageProviders,
    consumers: t.pageConsumers,
    "codex-integration": t.pageCodexIntegration,
    "dsh-integration": t.pageDshIntegration,
    "opencode-integration": t.pageOpenCodeIntegration,
    "direct-api-integration": t.pageDirectApiIntegration,
    usage: t.pageUsage,
    audit: t.pageAudit,
    topups: t.pageTopups,
    "model-prices": t.pageModelPrices,
    "system-resources": t.pageSystemResources,
    "admin-audit": t.pageAdminAudit,
    "provider-audit": t.pageProviderAudit,
    "request-detail": t.pageRequestDetail,
    users: t.pageUsers,
    settings: t.pageSettings,
  }[page]
}

export default App
