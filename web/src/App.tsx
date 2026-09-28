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
  DownloadIcon,
  ExternalLinkIcon,
  FileAudioIcon,
  GlobeIcon,
  HardDriveIcon,
  ImageIcon,
  KeyRoundIcon,
  LanguagesIcon,
  LockIcon,
  LogInIcon,
  MemoryStickIcon,
  MicIcon,
  NetworkIcon,
  PencilIcon,
  PlusIcon,
  RadioIcon,
  RefreshCwIcon,
  ScrollTextIcon,
  ServerIcon,
  SettingsIcon,
  ShieldAlertIcon,
  ShieldCheckIcon,
  SlidersHorizontalIcon,
  SquareIcon,
  Trash2Icon,
  UserRoundCogIcon,
  UploadIcon,
  WalletCardsIcon,
  WorkflowIcon,
  XIcon,
  XCircleIcon,
  type LucideIcon,
} from "lucide-react"
import { toast } from "sonner"
import { compareHeaderSnapshots } from "@/lib/header-comparison"
import { formatAuditHour, formatPercent, type Locale } from "@/lib/format"
import {
  isModelDowngrade,
  modelDowngradeFlows,
  modelDowngradeRatePoints,
  modelDowngradeSankey,
  unknownProviderId,
  type ModelDowngradeRow,
} from "@/lib/model-downgrade-audit"

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
import { ModelDowngradeRateChart } from "@/components/model-downgrade-rate-chart"
import { ModelDowngradeSankey } from "@/components/model-downgrade-sankey"
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
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet"
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
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { ResponsesAPIRequestBodyRenderer } from "@/components/responses-api-request-body-renderer"
import { api, apiForm, type AuthSdk } from "@/lib/api"
import { cn } from "@/lib/utils"
import {
  codexPlatform,
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
import {
  rateLimitResetExpiryStatus,
  rateLimitResetTimestampSeconds,
  sortRateLimitResetCreditsByExpiry,
  type RateLimitResetTimestamp,
} from "@/lib/rate-limit-reset-expiry"
import { responseOutputText } from "@/lib/response-output"
import { auditImageResponses } from "@/lib/audit-image-response"
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
  | "transcriptions"
  | "realtime"
  | "images"
  | "usage"
  | "audit"
  | "topups"
  | "model-prices"
  | "system-resources"
  | "admin-audit"
  | "provider-audit"
  | "model-downgrade-audit"
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
  intercept_degradation: boolean
  is_disabled: boolean
}
type Provider = {
  id: string
  name: string
  account_id: string
  owner_id?: string
  status: string
  manual_disabled: number
  cooldown_until?: number
  rate_limit_json?: string
  last_error?: string
  originator: ProviderOriginator
  allow_other_originator: boolean
  visibility: ProviderVisibility
  inflight: number
  queued: number
  concurrency_limit: number
  official_provided_usd_nanos: number
  actual_provided_usd_nanos: number
  updated_at: number
  http_proxy_configured?: boolean
}
type ProviderProxyHealth = {
  proxy_configured: boolean
  lb_to_proxy_ms?: number
  proxy_to_openai_ms?: number
  location?: { ip?: string; city?: string; region?: string; country?: string; org?: string }
  error?: string
}
type ProviderTokens = { access_key: string; refresh_key: string }
type OAuthFlow = { state: string; authorize_url: string }
// Client families a provider can be authorized as; the value is presented to OpenAI as the
// `originator` and decides which downstream clients may use the provider.
const providerOriginators = ["codex_cli_rs", "pi", "opencode"] as const
type ProviderOriginator = (typeof providerOriginators)[number]

// Who may route to a provider: every Consumer of this proxy, or only the owner's own Consumers.
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

function providerOriginatorLabel(
  value: ProviderOriginator,
  t: (typeof copy)[Locale]
): string {
  switch (value) {
    case "pi":
      return t.providerOriginatorPi
    case "opencode":
      return t.providerOriginatorOpencode
    default:
      return t.providerOriginatorCodex
  }
}
type UsageWindow = {
  used_percent?: number
  reset_at?: number
  reset_after_seconds?: number
}
type ProviderUsage = {
  email?: string
  account_email?: string
  account?: { email?: string }
  plan_type?: string
  rate_limit?: { primary_window?: UsageWindow; secondary_window?: UsageWindow }
  credits?: { balance?: number | string | null; unlimited?: boolean }
  [key: string]: unknown
}
type ProviderUsageEntry = { usage?: ProviderUsage; error?: string }
type ProviderUsageResponse = { providers: Record<string, ProviderUsageEntry> }
type ProviderCapacityHistoryPoint = {
  sampled_at: number
  plus_equivalent_remaining_basis_points: number
}
type ProviderCapacity = {
  provider_count: number
  included_provider_count: number
  plus_equivalent_remaining_basis_points: number
  last_sampled_at?: number
  history: ProviderCapacityHistoryPoint[]
}
type ProviderRateLimitResetCredit = {
  id: string
  reset_type?: string
  status?: string
  granted_at?: RateLimitResetTimestamp
  expires_at?: RateLimitResetTimestamp
  title?: string | null
  description?: string | null
}
type ProviderRateLimitResets = {
  available_count?: number
  credits?: ProviderRateLimitResetCredit[] | null
}
type ProviderRateLimitResetEntry = {
  resets?: ProviderRateLimitResets
  error?: string
}
type ProviderRateLimitResetsResponse = {
  providers: Record<string, ProviderRateLimitResetEntry>
}
type ProviderCircuitEvent = {
  id: string
  provider_id: string
  cause: string
  rate_limit_json: string
  opened_at: number
  cooldown_until: number
  closed_at?: number
  resolution?: string
}
type ProviderCircuitSummaryResponse = {
  providers: Record<string, ProviderCircuitEvent>
}
type ProviderRateLimitResetResult = { code?: string }
type ProviderRateLimitResetTarget = {
  provider: Provider
  credit?: ProviderRateLimitResetCredit
  redeemRequestId: string
}
type ProviderTokenDialogState = {
  provider: Provider
  loading: boolean
  tokens?: ProviderTokens
  error?: string
}
type ProviderTestState = {
  provider: Provider
  status: "loading" | "success" | "error"
  usage?: ProviderUsage
  error?: string
}
type ProviderProxyDialogState = { provider: Provider }
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
  session_id?: string
  user_id: string
  consumer_name: string
  provider_id?: string
  provider_name?: string
  path: string
  model?: string
  upstream_model?: string | null
  downstream_user_agent?: string | null
  downstream_originator?: string | null
  upstream_originator?: string | null
  originator_fallback_reason?: string | null
  reasoning_effort?: string
  fast_mode: boolean
  status: number
  first_byte_latency_ms?: number
  request_bytes: number
  response_bytes: number
  request_transport_bytes: number
  response_transport_bytes: number
  codex_turn_state_length?: number | null
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
  auth_issuer?: string
  upstream_base: string
  upstream_openai_beta?: string
  upstream_user_agent?: string | null
  upstream_user_agents?: {
    codex_cli_rs?: string | null
    pi?: string | null
    opencode?: string | null
  }
  experimental_filter_codex_turn_state_312?: boolean
  image_host_model: string
  available_model_ids: string[]
  allow_all_users_debt: boolean
  oauth_authorize_url: string
  oauth_token_url: string
  oauth_redirect_uri: string
  oauth_client_id: string
  response_body_limit: number
  image_body_limit: number
  audio_body_limit: number
  affinity_ttl_seconds: number
  provider_concurrency_limit: number
  request_archive_retention_days: number
  model_price_multiplier: string
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
  input_usd_nanos: number
  cached_input_usd_nanos?: number
  cache_write_usd_nanos?: number
  output_usd_nanos: number
}
type ModelPrice = {
  model: string
  short: ModelPriceRates
  long?: ModelPriceRates
}
type ModelPricesResponse = {
  source_url: string
  source_as_of: string
  unit: string
  rows: ModelPrice[]
}
type ReferenceImage = {
  id: string
  name: string
  size: number
  dataUrl: string
}

const copy = {
  zh: {
    dashboard: "总览",
    providers: "上游提供商",
    consumers: "下游消费者",
    "codex-integration": "ChatGPT（CodeX）",
    "dsh-integration": "DSH（DeepSeek Harness）",
    "opencode-integration": "OpenCode",
    "direct-api-integration": "直接 API",
    transcriptions: "语音转文字",
    realtime: "实时语音",
    images: "图片生成",
    usage: "用量",
    audit: "推理审计",
    topups: "充值",
    "model-prices": "模型价格",
    "system-resources": "系统资源",
    "admin-audit": "管理审计",
    "provider-audit": "提供商审计",
    "model-downgrade-audit": "模型降级审计",
    users: "用户",
    settings: "设置",
    signout: "退出登录",
    title: "OpenAI-LB",
    subtitle: "CodeX OAuth 负载均衡器",
    console: "控制台",
    navigationWorkspace: "工作区",
    navigationIntegrations: "下游接入",
    navigationTools: "工具",
    navigationData: "数据",
    navigationAdministration: "管理员",
    english: "English",
    roleLoading: "加载中",
    loginDescription: "使用 Auth Mini 登录运维控制台",
    email: "邮箱",
    authRedirectTitle: "在 Auth Mini 完成身份验证",
    authRedirectHelp:
      "邮箱验证码、Passkey 和 ED25519 登录均在 Auth Mini 页面完成；成功后会自动返回 OpenAI-LB。",
    continueAuthMini: "前往 Auth Mini 登录",
    loading: "正在加载 OpenAI-LB…",
    pageDashboard: "查看当前账户的 24 小时运行摘要。",
    pageProviders: "管理自己拥有的 CodeX OAuth Provider、运行状态与用户授权。",
    pageConsumers: "按 AI App 隔离下游消费者，分别跟踪调用量并独立吊销凭据。",
    pageCodexIntegration:
      "手动或授权浏览器配置本机 ChatGPT（CodeX）文件，并使用独立的下游 Consumer。",
    pageDshIntegration:
      "手动或授权浏览器自动配置本机 DSH，并为它创建独立的下游 Consumer。",
    pageOpenCodeIntegration:
      "手动或授权浏览器配置本机 OpenCode 文件，并使用独立的下游 Consumer。",
    pageDirectApiIntegration:
      "使用独立的下游 Consumer 直接调用 OpenAI-LB 的 OpenAI 兼容 API。",
    pageTranscriptions:
      "录制或上传音频，使用当前用户可访问的 CodeX OAuth Provider 转写为文字。",
    pageRealtime:
      "通过 WebRTC 直接与实时语音模型对话；连接和控制信令由 OpenAI-LB 安全代理到 OpenAI Realtime API。",
    pageImages:
      "通过文字提示和可选参考图片生成图像，使用当前用户可访问的 CodeX OAuth Provider。",
    pageUsage: "按消费者核算请求、Token、官方费用与实际费用。",
    pageAudit:
      "逐次追踪推理请求、上游提供商、费用快照与累计消费；诊断内容按配置期限保留。",
    pageTopups: "通过 Midas 管理入金、协议授权与可用额度。",
    pageModelPrices:
      "查看 OpenAI 官方标准 Token 价格快照，价格按每百万 Token 展示。",
    pageSystemResources:
      "查看宿主机当前负载和 OpenAI-LB 数据占用；仅 root 和管理员可访问。",
    pageAdminAudit: "查看 root 与管理员执行的管理操作记录。",
    pageProviderAudit:
      "按上游提供商、模型和小时查看最近 7 天请求成功与失败情况。",
    pageModelDowngradeAudit:
      "对比下游请求模型与上游实际返回的模型，定位被上游静默替换、降级交付的调用。",
    pageRequestDetail: "查看调用上下文、消息结构与同一 Session ID 的相邻请求。",
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
    officialProvidedValue: "官方累计提供价值 (USD)",
    actualProvidedValue: "实际累计提供价值 (USD)",
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
    midasFundTitle: "转入 OpenAI-LB 公共账户",
    midasFundDescription: "输入金额后，Midas 会在新窗口中显示不可编辑的收款账户和金额；登录并确认后，即可完成充值。",
    topupAmount: "充值金额 (USD)",
    topupAmountHint: "最多 9 位小数。Midas 将按精确 USD 纳美元金额转账。",
    continueToMidas: "继续前往 Midas",
    publicWalletUserId: "OpenAI-LB 公共账户用户 ID",
    copyMidasUserId: "复制用户 ID",
    midasTransferRefresh: "完成转账后刷新本页，累计充值会从 Midas 当前累计转入中显示。",
    midasUnavailable: "Midas 尚未配置",
    midasUnavailableDescription: "请由 root 在设置中填写 Midas 公共账户用户 ID 与 fund API key。",
    midasSettings: "Midas",
    midasSettingsDescription: "Midas 是充值唯一账本。公共账户 user ID 可安全展示给付款用户；fund API key 仅保存在服务器 SQLite 中，留空会保留现有密钥。",
    midasApiBase: "API 地址",
    midasFundUserId: "公共账户 User ID",
    midasFundApiKey: "Fund API key",
    midasConfigured: "已配置",
    midasNotConfigured: "未配置",
    midasSettingsSaved: "Midas 设置已保存",
    modelPricesTitle: "模型官方价格",
    modelPricesDescription:
      "用于费用核算的 OpenAI 官方标准价格快照；仅显示当前允许调用且有 Token 标准价格的模型。",
    pricingUnit: "计价单位",
    pricingAsOf: "价格快照日期",
    officialPricingSource: "打开 OpenAI 官方价格页",
    officialModelPriceTable: "标准 Token 价格",
    modelPricesTableDescription:
      "短上下文和长上下文分别列示；“—”表示官方未单列该价格。",
    shortContext: "短上下文",
    longContext: "长上下文（≥272K）",
    cacheWrite: "缓存写入",
    systemResources: "系统资源",
    systemResourcesDescription:
      "宿主机当前负载与 OpenAI-LB 数据占用，仅 root 和管理员可见。",
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
    openaiLbRss: "OpenAI-LB RSS",
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
    providerPool: "OAuth 上游提供商池",
    providerDescription:
      "管理自己拥有的 OAuth 上游提供商；root 与管理员可管理全局 Provider。Token 读取与变更仅对管理员开放，并会进入操作审计。",
    addProvider: "添加上游提供商",
    noProviders: "尚无上游提供商",
    noProvidersDescription:
      "从浏览器重新登录 OpenAI OAuth，或导入已有的 Access Key 与 Refresh Key。",
    name: "名称",
    account: "账户",
    owner: "所有者",
    status: "状态",
    circuitBreaker: "熔断",
    circuitOpen: "已熔断",
    circuitRecovered: "已恢复",
    circuitNeverOpened: "无熔断记录",
    circuitUntil: "熔断至",
    circuitOpenedAt: "触发时间",
    circuitClosedAt: "恢复时间",
    circuitReason: "触发原因",
    circuitResolution: "结束方式",
    circuitRateLimitHeaders: "限流响应头",
    circuitHistory: "熔断记录",
    circuitHistoryTitle: "上游熔断记录",
    circuitHistoryDescription:
      "429 触发、熔断截止时间和恢复结果均持久化在 SQLite。",
    actions: "操作",
    refresh: "刷新",
    providerUpdated: "上游提供商已更新",
    providerAdded: "上游提供商已添加",
    oauthProviderAdded: "OAuth 上游提供商已添加",
    addProviderTitle: "添加 CodeX OAuth 上游提供商",
    addProviderDescription:
      "选择完成添加所需的方式。Token 将以明文保存在 SQLite，仅 root 与管理员可再次读取和编辑。",
    addProviderOAuth: "从浏览器重新登录 OpenAI OAuth",
    addProviderOAuthDescription:
      "在浏览器中完成 OpenAI 登录后，粘贴完整 callback URL 以完成添加。",
    addProviderCredentials: "已有 Access Key 和 Refresh Key",
    addProviderCredentialsDescription:
      "直接导入已知凭据；适用于已从其他位置安全取得 Token 的情况。",
    addProviderOAuthTitle: "重新登录 OpenAI OAuth",
    addProviderOAuthInProgressDescription:
      "在新标签页中完成 OpenAI 登录，然后将浏览器地址栏中的完整 callback URL 粘贴到这里。认证进行中只能使用取消按钮退出。",
    addProviderCredentialsTitle: "导入 OpenAI OAuth 凭据",
    addProviderCredentialsTitleDescription:
      "填写已知的 Access Key 与 Refresh Key。",
    nameOptional: "名称（可选）",
    providerNameHelp: "留空时会使用上游渠道 UUID。",
    providerOriginator: "客户端身份（originator）",
    providerOriginatorCodex: "CodeX CLI（codex_cli_rs）",
    providerOriginatorPi: "Pi Agent（pi）",
    allowOtherOriginator: "允许为其他 originator 兜底",
    allowOtherOriginatorHelp: "仅当请求没有可用的同身份提供商时，承接其他或未知来源的请求。发往上游的身份仍使用此提供商的 originator。",
    originatorFallback: "跨来源兜底",
    downstreamOriginator: "下游 originator",
    upstreamOriginator: "上游 originator",
    originatorFallbackReason: "跨来源兜底原因",
    providerOriginatorOpencode: "OpenCode（opencode）",
    providerOriginatorHelp:
      "该身份在 OAuth 授权与每次上游请求中声明，优先服务相同身份的下游客户端。创建后不可修改；可单独开启跨来源兜底。",
    providerOriginatorLockedHelp:
      "本次授权已按此身份发起；如需更换，请重新发起 OAuth。",
    providerVisibility: "可见性",
    providerVisibilityPrivate: "私有",
    providerVisibilityPublic: "公开",
    providerVisibilityHelp:
      "私有：只有本用户名下的 Consumer 可以使用该上游提供商。公开：所有 Consumer 都可以使用。",
    makeProviderPublic: "改为公开",
    makeProviderPrivate: "改为私有",
    accessClaimHelp: "必须是包含 CodeX account_id claim 的 JWT。",
    callbackUrl: "Callback URL",
    callbackUrlHelp: "粘贴完整 URL，系统会自动解析 code 与 state。",
    oauthStateHelp: "OAuth state 已在服务器中一次性保存，有效期 10 分钟。",
    startOauth: "打开 OpenAI 登录",
    completeOauth: "完成添加",
    importCredentials: "导入凭据",
    editProvider: "编辑凭据",
    editProviderName: "编辑提供商名称",
    saveProviderName: "保存提供商名称",
    cancelProviderName: "取消编辑提供商名称",
    tokenTitle: "编辑上游提供商凭据",
    tokenDescription:
      "读取并更新 SQLite 中明文存储的上游 OAuth Token。名称请直接在名称列中编辑。",
    loadingTokens: "正在读取 Token…",
    saveTokens: "保存",
    tokensSaved: "上游提供商已保存",
    deleteProvider: "删除",
    deleteProviderTitle: "删除 OAuth 上游提供商？",
    deleteProviderDescription:
      "此操作会将上游提供商从管理列表和调度池中隐藏。其 Token、亲和性及历史记录都会保留。",
    confirmDeleteProvider: "隐藏上游提供商",
    providerDeleted: "上游提供商已隐藏",
    testProvider: "测试",
    testingProvider: "正在获取上游提供商 Usage…",
    testTitle: "上游提供商 Usage 测试",
    testDescription:
      "服务端使用该上游提供商 OAuth Token 调用 Usage API，Token 不会随测试结果返回浏览器。",
    testSucceeded: "上游提供商测试成功",
    httpProxy: "HTTP 代理",
    upstreamProxy: "上游代理",
    proxyConfigured: "已配置",
    proxyNotConfigured: "未配置",
    proxyConfigTitle: "上游代理",
    proxyConfigDescription:
      "此代理只用于当前上游提供商。已保存的地址和认证信息不会回显到浏览器。",
    proxyUrl: "代理地址",
    proxyUrlHelp: "仅支持 http://，例如 http://username:password@host:port。",
    saveProxy: "保存代理",
    removeProxy: "移除代理",
    proxyDiagnostics: "连接诊断",
    proxyDiagnosticsTitle: "上游代理诊断",
    proxyDiagnosticsDescription:
      "检测由 LB 发起。代理凭据不会返回浏览器。",
    testingProxy: "正在检测代理链路…",
    testAgain: "重新检测",
    proxyExitLocation: "出口位置",
    proxyNetworkQuality: "链路质量",
    lbToProxy: "LB → 代理",
    lbViaProxyToOpenAi: "LB 经代理 → OpenAI",
    proxyLatencyHelp: "显示收到响应为止的耗时，不代表单向网络时延。",
    proxyLocationUnavailable: "未能获得出口位置",
    proxyNotConfiguredDescription: "请先配置 HTTP 代理后再进行诊断。",
    usageEmail: "邮箱",
    usagePlan: "套餐",
    quotaRemaining: "剩余额度",
    resetsIn: "重置倒计时",
    quotaUnavailable: "未返回额度",
    credits: "Credits",
    rawUsage: "Usage 原始字段",
    usageUnavailable: "Usage API 未返回可识别的额度字段，请查看原始字段。",
    rateLimitResetsAvailable: "可用重置",
    viewRateLimitResets: "重置机会",
    rateLimitResetsTitle: "限额重置机会",
    rateLimitResetsDescription:
      "每次兑换会消耗一项已获得的额度，并只能重置当前符合条件的 ChatGPT / Codex 限额窗口。",
    rateLimitResetGrantedAt: "获得时间",
    rateLimitResetExpiresAt: "失效时间",
    rateLimitResetNextExpiresAt: "重置机会倒计时",
    rateLimitResetExpiresIn: "剩余",
    rateLimitResetNoExpiry: "未提供到期时间",
    rateLimitResetExpired: "已过期",
    rateLimitResetUnknownCredit:
      "服务端未提供逐项详情；确认后将使用下一项可用机会。",
    rateLimitResetUse: "使用此机会",
    rateLimitResetUseNext: "使用下一项机会",
    rateLimitResetConfirmTitle: "使用限额重置机会？",
    rateLimitResetConfirmDescription:
      "此操作将为当前 Provider 消耗一项已获得的重置额度。额度不可恢复。",
    confirmRateLimitReset: "确认重置",
    rateLimitResetting: "正在重置…",
    rateLimitResetSuccess: "限额已重置，当前状态已刷新。",
    rateLimitResetAlreadyRedeemed: "本次重置已完成，当前状态已刷新。",
    rateLimitResetNothingToReset: "当前没有符合条件的限额窗口可重置。",
    rateLimitResetNoCredit: "账户没有可用的重置机会。",
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
    interceptDegradation: "降级拦截",
    interceptDegradationHelp:
      "打开后，上游返回 x-codex-turn-state=312 的成功响应（可能发生模型降级）会被取消，并以 503 错误码返回。",
    interceptDegradationUpdated: "降级拦截开关已更新",
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
    averageLatency: "平均延迟",
    last24Hours: "最近 24 小时",
    last7Days: "最近 7 天",
    allUsers: "全部用户",
    allConsumers: "全部消费者",
    allModels: "全部模型",
    userLabel: "用户",
    model: "模型",
    downstreamModel: "下游模型",
    upstreamModel: "上游模型",
    downstreamUserAgent: "下游 UA",
    modelDowngraded: "模型降级",
    modelMismatchHint: "上游返回模型与下游请求模型不一致。",
    upstreamUserAgent: "上游 UA",
    upstreamUserAgentHint:
      "按 originator 分别覆盖发往上游的 User-Agent，保存后立即生效。留空保持原有行为；下游 UA 仍记录请求原值。",
    experimentalTurnState312Filter: "实验性拦截 312 Turn State",
    experimentalTurnState312FilterHint:
      "打开后，长度恰为 312 的 x-codex-turn-state 请求头不会转发给上游；默认关闭。",
    upstreamUserAgentCodex: "codex_cli_rs UA",
    upstreamUserAgentPi: "pi UA",
    upstreamUserAgentOpencode: "opencode UA",
    consumerLabel: "消费者",
    date: "日期",
    rows: "行",
    columns: "列",
    data: "数据",
    hidden: "隐藏",
    inputTokens: "输入 Token",
    cachedInputTokens: "缓存输入 Token",
    outputTokens: "输出 Token",
    cost: "费用 (USD)",
    requestCount: "请求次数",
    usageRows: "条聚合记录",
    sortColumn: "排序",
    total: "总计",
    clearFilters: "清除筛选",
    pivotFields: "透视字段",
    groupOrder: "分组顺序",
    dataOrder: "数据列顺序",
    sum: "求和",
    auditTitle: "推理审计",
    auditDescription:
      "请求/响应诊断预览保存在 SQLite；不记录 Authorization 或 OAuth 凭据。",
    noAudit: "暂无推理审计记录",
    noAuditDescription: "每次推理调用结束后都会写入基础审计记录。",
    time: "时间",
    requestId: "请求 ID",
    threadId: "Thread ID",
    copyThreadId: "复制 Thread ID",
    sessionId: "Session ID",
    copySessionId: "复制 Session ID",
    copyUserId: "复制用户 ID",
    provider: "上游提供商",
    latency: "延迟",
    firstByteLatency: "首字节",
    totalLatency: "总耗时",
    requestSize: "请求大小",
    responseSize: "响应大小",
    codexTurnStateLength: "x-codex-turn-state 字符串长度",
    codexTurnStateLengthWarning: "x-codex-turn-state=312，可能发生模型降级",
    requestTransportSize: "请求传输量（压缩后）",
    responseTransportSize: "响应传输量（压缩后）",
    compressionRatio: "压缩率",
    downstreamAcceptEncoding: "下游接受压缩",
    downstreamContentEncoding: "下游响应压缩",
    upstreamAcceptEncoding: "上游请求压缩",
    upstreamContentEncoding: "上游响应压缩",
    networkTransport: "网络传输量（压缩后）",
    reasoningEffort: "推理强度",
    fastMode: "Fast 模式",
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
    auditDetailDescription:
      "展示已保留的请求与响应诊断预览；敏感凭据不会记录。",
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
    responseImages: "响应图片",
    revisedPrompt: "修订后的提示词",
    auditArchiveUnavailable: "此请求的诊断记录已过期或不可用。",
    affinitySource: "亲和来源",
    affinityRequestId: "亲和请求 ID",
    affinityHash: "亲和哈希",
    previousRequest: "上一个请求",
    nextRequest: "下一个请求",
    messages: "消息结构",
    requestSettings: "请求参数",
    instructions: "Instructions",
    tools: "工具",
    toolsDescription: "以列表查看工具定义，或切换到原始 JSON 检查完整 schema。",
    toolsList: "工具列表",
    rawJson: "原始 JSON",
    toolParameters: "参数",
    toolRequired: "必填",
    toolOptional: "可选",
    noTools: "未配置工具",
    unnamedTool: "未命名工具",
    rawRequest: "原始请求",
    identityPermissions: "身份与权限",
    identityDescription: "浏览器会话由 Auth Mini 管理；后端只验证 access JWT。",
    proxyBoundary: "代理边界",
    proxyDescription: "仅 OpenAI / CodeX 能力，不提供其他厂商兼容协议。",
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
    providerQueueRefresh: "并发与排队数据每 2 秒更新。",
    accessKey: "Access key",
    refreshKey: "Refresh key",
    consumer: "消费者",
    input: "输入",
    output: "输出",
    userId: "用户 ID",
    role: "角色",
    authIssuer: "认证签发方",
    upstream: "上游",
    upstreamOpenaiBeta: "上游 OpenAI-Beta",
    upstreamOpenaiBetaHint:
      "留空时 LB 不注入此请求头；客户端自行携带的同名头仍按透传规则处理。",
    bodyLimit: "请求体限制",
    affinityTtl: "亲和 TTL",
    statusActive: "可用",
    statusCooldown: "冷却中",
    statusAuthError: "认证错误",
    statusDisabled: "已禁用",
    statusUnknown: "未知",
    roleRoot: "超级管理员",
    roleAdmin: "管理员",
    roleUser: "租户用户",
    loginUnknown: "认证失败，请重试。",
    adminAuditTitle: "管理操作审计",
    adminAuditDescription:
      "记录 root 与管理员执行的 Provider、OAuth、Token、授权、系统和审计管理操作。",
    providerAuditTitle: "提供商审计",
    providerAuditDescription:
      "按上游提供商、模型和小时汇总请求结果，帮助定位失败集中在哪个上游或模型。",
    providerAuditChartDescription: "当前筛选范围内按小时观察失败率、请求数和输入 Token。",
    providerAuditFailureRateChart: "时间—失败率",
    providerAuditRequestCountChart: "时间—请求数",
    providerAuditInputTokensChart: "时间—Input Token 数",
    providerAuditPeriod: "数据范围",
    providerAuditTableDescription: "每行代表一个小时、一个上游提供商和一个模型。",
    providerAuditNoData: "所选时间范围暂无请求",
    providerAuditNoDataDescription: "产生请求后，这里会按小时显示各个上游和模型的结果。",
    providerAuditAllProviders: "全部提供商",
    providerAuditUnknownProvider: "未识别提供商",
    providerAuditHour: "小时",
    providerAuditSuccessRate: "成功率",
    providerAuditFailureRate: "失败率",
    providerAuditSuccessfulRequests: "成功请求",
    providerAuditFailedRequests: "失败请求",
    providerAuditTotalRequests: "请求总数",
    providerAuditInputTokens: "Input Token 数",
    providerAuditProviders: "提供商数",
    providerAuditModels: "模型数",
    modelDowngradeTitle: "模型降级审计",
    modelDowngradeDescription:
      "上游有时会用其他模型完成请求；这里汇总下游请求模型与上游实际返回模型的映射关系。",
    modelDowngradeFlowChart: "下游模型 → 上游提供商 → 上游模型",
    modelDowngradeFlowDescription:
      "流量从下游请求模型流向承接请求的上游提供商，再流向上游实际返回的模型；红色连接表示该链路上存在上游返回与下游请求不一致的请求，精确数量见下方表格。",
    modelDowngradeRateChart: "时间—降级率",
    modelDowngradeRate: "降级率",
    modelDowngradeRateDescription:
      "降级率 = 上游实际模型与下游请求模型不一致的请求 / 已记录上游模型的请求。",
    modelDowngradePeriod: "数据范围",
    modelDowngradeTableDescription:
      "每行代表一个上游提供商、上游实际模型与下游请求模型的组合。",
    modelDowngradeAuditedRequests: "已记录上游模型的请求",
    modelDowngradeDowngradedRequests: "降级请求",
    modelDowngradeConsistent: "一致",
    modelDowngradeDowngraded: "降级",
    modelDowngradeUpstreamModel: "上游实际模型",
    modelDowngradeDownstreamModel: "下游请求模型",
    modelDowngradeNoData: "所选时间范围暂无上游模型记录",
    modelDowngradeNoDataDescription:
      "上游在完成或失败事件中带上模型后，这里会显示模型映射与降级率。",
    administrator: "操作用户",
    action: "操作",
    target: "目标",
    clientIp: "客户端 IP",
    setupTitle: "初始化 OpenAI-LB",
    setupDescription: "连接品牌 Auth Mini，并将首个已验证用户绑定为唯一 root。",
    setupIssuer: "Auth Mini issuer",
    setupIssuerHelp:
      "填写品牌提供的 Auth Mini HTTPS 地址。OpenAI-LB 只连接该实例，不会部署或管理它。",
    setupAudience: "JWT audience",
    connectAuth: "连接 Auth Mini",
    changeAuth: "更换实例",
    setupLogin: "验证 root 身份",
    setupLoginHelp:
      "登录成功后，当前 Auth Mini user_id 将成为 OpenAI-LB root。",
    finishSetup: "绑定 root 并完成初始化",
    finishingSetup: "正在完成初始化",
    setupStepConnect: "连接认证实例",
    setupStepLogin: "验证首个用户",
    setupStepFinish: "绑定 root",
    setupConnected: "已连接",
    setupWaiting: "待完成",
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
    platformCapacityTitle: "平台上游可用额度",
    platformCapacityDescription:
      "每 5 分钟轮询全部上游的套餐与主限额窗口；Plus 按 1×、Pro Lite 按 5×、Pro 按 20× 归一。仅纳入已成功采样且额度可比的 Plus、Pro Lite、Pro 账户。",
    platformPlusCapacity: "Plus 等价剩余额度",
    platformCapacitySampledAt: "最近采样",
    platformCapacityPending: "等待首次采样",
    platformCapacityTrend: "近 7 天额度趋势",
    platformCapacityTrendDescription:
      "展示每次采样的平台 Plus 等价剩余额度。",
    platformCapacityTrendPending: "等待历史采样数据。",
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
    allowAllUsersDebtHint:
      "打开后，所有用户均可在可用额度不足时继续发送请求。",
    saveSettings: "保存配置",
    settingsSaved: "配置已保存",
    imageHostModel: "图像宿主模型",
    oauthAuthorizeUrl: "OAuth 授权地址",
    oauthTokenUrl: "OAuth Token 地址",
    oauthRedirectUri: "OAuth 回调地址",
    oauthClientId: "OAuth Client ID",
    responseLimit: "Responses 限制",
    imageLimit: "图像请求限制",
    audioLimit: "音频请求限制",
    transcriptionInput: "音频输入",
    transcriptionInputHelp: "上传音频文件，或直接使用浏览器麦克风录音。",
    transcriptionModel: "模型 ID",
    transcriptionModelId: "gpt-4o-transcribe",
    selectAudio: "选择音频",
    startRecording: "开始录音",
    stopRecording: "停止录音",
    recording: "正在录音",
    languageHint: "语言提示",
    languageAuto: "自动检测",
    languageChinese: "中文",
    languageEnglish: "英文",
    transcribe: "转写为文字",
    transcribing: "正在转写",
    transcript: "转写结果",
    transcriptEmpty: "选择或录制音频后，转写文本会显示在这里。",
    copyTranscript: "复制转写结果",
    microphoneUnavailable: "当前浏览器不支持麦克风录音，请上传音频文件。",
    microphoneDenied: "无法访问麦克风，请检查浏览器权限后重试。",
    noAudioSelected: "请先选择或录制音频。",
    realtimeStart: "开始实时语音",
    realtimeStop: "结束对话",
    realtimeIdle: "准备就绪",
    realtimeConnecting: "正在建立安全语音连接…",
    realtimeLive: "实时对话中",
    realtimeConnectionFailed: "实时语音连接已中断，请重试。",
    realtimeInstruction: "对话指令",
    realtimeInstructionHelp:
      "这段指令会随会话发送给实时模型。默认是简洁、自然的中文语音助手。",
    realtimeTranscript: "实时转写",
    realtimeTranscriptEmpty: "开始说话后，输入与输出的实时转写会显示在这里。",
    realtimeRouteTitle: "实时语音链路",
    realtimeRouteDescription:
      "浏览器媒体通过 WebRTC 建立；LB 只代理受保护的会话创建与 sideband 控制信令。",
    realtimePublicEndpoint: "公开接口",
    realtimeUpstreamEndpoint: "OpenAI Realtime 上游",
    realtimeMicrophoneHelp:
      "开始后浏览器会请求麦克风权限；结束对话会立即关闭本地音频轨道。",
    imagePrompt: "图像提示词",
    imagePromptPlaceholder:
      "描述你想生成的图像，包括主体、构图、风格和需要呈现的文字。",
    imageReference: "参考图片",
    imageReferenceHelp:
      "可选：上传 PNG、JPEG、WEBP 或 GIF 作为构图、风格或主体参考；最多 4 张，合计不超过 8 MiB。",
    imageReferenceCount: "已添加",
    removeReferenceImage: "移除参考图片",
    imageReferenceInvalid: "请选择 PNG、JPEG、WEBP 或 GIF 图片。",
    imageReferenceTooLarge: "单张参考图片不能超过 4 MiB。",
    imageReferenceCountExceeded: "最多添加 4 张参考图片。",
    imageReferenceTotalExceeded: "参考图片合计不能超过 8 MiB。",
    imageReferenceReadError: "读取参考图片失败，请重试。",
    imageSize: "画面比例",
    imageQuality: "生成质量",
    imageSquare: "正方形",
    imageLandscape: "横向",
    imagePortrait: "纵向",
    image2kSquare: "2K 正方形",
    image2kLandscape: "2K 横向",
    image4kLandscape: "4K 横向",
    image4kPortrait: "4K 纵向",
    imageCustom: "自定义尺寸",
    imageWidth: "宽度（px）",
    imageHeight: "高度（px）",
    imageSizeHelp:
      "支持 gpt-image-2：宽高均为 16 的倍数、最大 3840px、比例不超过 3:1，总像素 655,360–8,294,400；超过 2560 × 1440 的输出为实验性。",
    imageSizeInvalid: "请输入符合上述限制的宽度和高度。",
    imageAuto: "自动",
    imageDraft: "草稿",
    imageStandard: "标准",
    imageHigh: "高",
    generateImage: "生成图片",
    generatingImage: "正在生成",
    generatedImage: "生成结果",
    imageEmpty: "填写图像提示词后，生成的图片会显示在这里。",
    downloadImage: "下载图片",
    noImagePrompt: "请先填写图像提示词。",
    imageGenerationFailed: "图片生成失败。",
    imageGenerationRetry: "请稍后重试；如果仍然失败，请调整提示词。",
    imageGenerationDetail: "上游详情",
  },
  en: {
    dashboard: "Overview",
    providers: "Providers",
    consumers: "Consumers",
    "codex-integration": "ChatGPT (CodeX)",
    "dsh-integration": "DSH (DeepSeek Harness)",
    "opencode-integration": "OpenCode",
    "direct-api-integration": "Direct API",
    transcriptions: "Speech to text",
    realtime: "Realtime voice",
    images: "Image Generation",
    usage: "Usage",
    audit: "Inference audit",
    topups: "Top up",
    "model-prices": "Model prices",
    "system-resources": "System resources",
    "admin-audit": "Admin audit",
    "provider-audit": "Provider audit",
    "model-downgrade-audit": "Model downgrade audit",
    users: "Users",
    settings: "Settings",
    signout: "Sign out",
    title: "OpenAI-LB",
    subtitle: "CodeX OAuth load balancer",
    console: "Console",
    navigationWorkspace: "Workspace",
    navigationIntegrations: "Downstream integrations",
    navigationTools: "Tools",
    navigationData: "Data",
    navigationAdministration: "Administration",
    english: "简体中文",
    roleLoading: "Loading",
    loginDescription: "Sign in to the operations console with Auth Mini",
    email: "Email",
    authRedirectTitle: "Verify your identity in Auth Mini",
    authRedirectHelp:
      "Email codes, passkeys, and ED25519 sign-in stay on the Auth Mini page. You will return to OpenAI-LB after signing in.",
    continueAuthMini: "Continue to Auth Mini",
    loading: "Loading OpenAI-LB…",
    pageDashboard: "Review the current account's 24-hour operating summary.",
    pageProviders:
      "Manage CodeX OAuth Providers you own, their runtime state, and user access.",
    pageConsumers:
      "Give each AI app its own downstream Consumer so usage, errors, and revocation stay isolated.",
    pageCodexIntegration:
      "Configure the local ChatGPT (CodeX) file manually or in the browser with a dedicated downstream Consumer.",
    pageDshIntegration:
      "Configure local DSH manually or authorize the browser to configure it with a dedicated downstream Consumer.",
    pageOpenCodeIntegration:
      "Configure the local OpenCode file manually or in the browser with a dedicated downstream Consumer.",
    pageDirectApiIntegration:
      "Call the OpenAI-compatible OpenAI-LB API with a dedicated downstream Consumer.",
    pageTranscriptions:
      "Record or upload audio, then transcribe it through a CodeX OAuth Provider available to the current user.",
    pageRealtime:
      "Talk to the realtime voice model over WebRTC while OpenAI-LB securely proxies session creation and control signaling to the OpenAI Realtime API.",
    pageImages:
      "Generate one image from a text prompt through a CodeX OAuth Provider available to the current user.",
    pageUsage:
      "Attribute requests, tokens, official cost, and actual cost to each Consumer.",
    pageAudit:
      "Trace each inference request, provider, pricing snapshot, and cumulative consumption; diagnostics follow the configured retention.",
    pageTopups:
      "Manage Midas funding, agreement authorization, and available credit.",
    pageModelPrices:
      "Review an official OpenAI standard token-pricing snapshot, shown per 1M tokens.",
    pageSystemResources:
      "Review current host load and OpenAI-LB data usage. Available only to root and administrators.",
    pageAdminAudit:
      "Review management operations performed by root and administrators.",
    pageProviderAudit:
      "Review successful and failed requests by upstream provider, model, and hour over the last 7 days.",
    pageModelDowngradeAudit:
      "Compare the requested downstream model with the model the upstream actually reported serving.",
    pageRequestDetail:
      "Review call context, message structure, and adjacent requests with the same Session ID.",
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
    officialProvidedValue: "Official cumulative provided value (USD)",
    actualProvidedValue: "Actual cumulative provided value (USD)",
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
    requestEnforced: "Requests are blocked when available credit is insufficient.",
    midasFundTitle: "Transfer to the OpenAI-LB public account",
    midasFundDescription: "Enter an amount to open Midas in a new window. It shows the recipient and amount read-only; sign in and confirm to complete the top-up.",
    topupAmount: "Top-up amount (USD)",
    topupAmountHint: "Up to 9 decimal places. Midas transfers this exact amount in USD nanodollars.",
    continueToMidas: "Continue to Midas",
    publicWalletUserId: "OpenAI-LB public-account user ID",
    copyMidasUserId: "Copy user ID",
    midasTransferRefresh: "Refresh this page after the transfer; cumulative top-ups are read from your current Midas inbound transfers.",
    midasUnavailable: "Midas is not configured",
    midasUnavailableDescription: "Ask root to configure the Midas public-account user ID and fund API key in Settings.",
    midasSettings: "Midas",
    midasSettingsDescription: "Midas is the only top-up ledger. The public-account user ID is safe to share with payers; the fund API key stays in server SQLite, and a blank key retains the current key.",
    midasApiBase: "API base URL",
    midasFundUserId: "Public-account user ID",
    midasFundApiKey: "Fund API key",
    midasConfigured: "Configured",
    midasNotConfigured: "Not configured",
    midasSettingsSaved: "Midas settings saved",
    modelPricesTitle: "Official model prices",
    modelPricesDescription:
      "Official OpenAI standard-pricing snapshot used for cost accounting. Only currently enabled models with standard token prices are shown.",
    pricingUnit: "Unit",
    pricingAsOf: "Pricing snapshot date",
    officialPricingSource: "Open official OpenAI pricing",
    officialModelPriceTable: "Standard token pricing",
    modelPricesTableDescription:
      "Short- and long-context rates are shown separately; “—” means OpenAI does not list a separate rate.",
    shortContext: "Short context",
    longContext: "Long context (≥272K)",
    cacheWrite: "Cache write",
    systemResources: "System resources",
    systemResourcesDescription:
      "Current host load and OpenAI-LB data footprint. Visible to root and administrators only.",
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
    openaiLbRss: "OpenAI-LB RSS",
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
    providerPool: "OAuth provider pool",
    providerDescription:
      "Manage OAuth providers you own. Root and administrators can manage the global pool. Token reads and changes are limited to administrators and audited.",
    addProvider: "Add provider",
    noProviders: "No providers",
    noProvidersDescription:
      "Sign in to OpenAI OAuth in a browser, or import an existing Access Key and Refresh Key.",
    name: "Name",
    account: "Account",
    owner: "Owner",
    status: "Status",
    circuitBreaker: "Circuit breaker",
    circuitOpen: "Circuit open",
    circuitRecovered: "Recovered",
    circuitNeverOpened: "No circuit history",
    circuitUntil: "Open until",
    circuitOpenedAt: "Opened",
    circuitClosedAt: "Recovered",
    circuitReason: "Trigger",
    circuitResolution: "Resolution",
    circuitRateLimitHeaders: "Rate-limit response headers",
    circuitHistory: "Circuit history",
    circuitHistoryTitle: "Provider circuit history",
    circuitHistoryDescription:
      "429 triggers, cooldown deadlines, and recovery results are persisted in SQLite.",
    actions: "Actions",
    refresh: "Refresh",
    providerUpdated: "Provider updated",
    providerAdded: "Provider added",
    oauthProviderAdded: "OAuth provider added",
    addProviderTitle: "Add CodeX OAuth provider",
    addProviderDescription:
      "Choose the path that matches the credentials you have. Tokens are stored as plaintext in SQLite and can only be read and edited by root and administrators.",
    addProviderOAuth: "Sign in to OpenAI OAuth in browser",
    addProviderOAuthDescription:
      "Complete OpenAI sign-in in a browser, then paste the full callback URL to add the provider.",
    addProviderCredentials: "I have an Access Key and Refresh Key",
    addProviderCredentialsDescription:
      "Import known credentials directly when they were securely obtained elsewhere.",
    addProviderOAuthTitle: "Sign in to OpenAI OAuth again",
    addProviderOAuthInProgressDescription:
      "Complete OpenAI sign-in in the new tab, then paste the full callback URL from the browser address bar. Only Cancel can exit while this authentication is in progress.",
    addProviderCredentialsTitle: "Import OpenAI OAuth credentials",
    addProviderCredentialsTitleDescription:
      "Enter an existing Access Key and Refresh Key.",
    nameOptional: "Name (optional)",
    providerNameHelp: "When empty, the upstream provider UUID is used.",
    providerOriginator: "Client identity (originator)",
    providerOriginatorCodex: "CodeX CLI (codex_cli_rs)",
    providerOriginatorPi: "Pi Agent (pi)",
    allowOtherOriginator: "Back up other originators",
    allowOtherOriginatorHelp: "Accept other or unidentified clients only when no matching provider is available. Upstream requests still use this provider’s originator.",
    originatorFallback: "Cross-originator fallback",
    downstreamOriginator: "Downstream originator",
    upstreamOriginator: "Upstream originator",
    originatorFallbackReason: "Cross-originator fallback reason",
    providerOriginatorOpencode: "OpenCode (opencode)",
    providerOriginatorHelp:
      "This immutable OAuth and upstream identity serves matching clients first. Cross-originator fallback can be enabled separately.",
    providerOriginatorLockedHelp:
      "This authorization already declared the identity; start a new OAuth flow to change it.",
    providerVisibility: "Visibility",
    providerVisibilityPrivate: "Private",
    providerVisibilityPublic: "Public",
    providerVisibilityHelp:
      "Private: only Consumers owned by this user may route to the provider. Public: every Consumer may route to it.",
    makeProviderPublic: "Publish to everyone",
    makeProviderPrivate: "Restrict to my Consumers",
    accessClaimHelp: "Must be a JWT containing the CodeX account_id claim.",
    callbackUrl: "Callback URL",
    callbackUrlHelp:
      "Paste the complete URL; code and state are parsed automatically.",
    oauthStateHelp:
      "OAuth state is stored once on the server and expires in 10 minutes.",
    startOauth: "Open OpenAI sign-in",
    completeOauth: "Finish adding provider",
    importCredentials: "Import credentials",
    editProvider: "Edit credentials",
    editProviderName: "Edit provider name",
    saveProviderName: "Save provider name",
    cancelProviderName: "Cancel provider name edit",
    tokenTitle: "Edit provider credentials",
    tokenDescription:
      "Read and update the provider's plaintext OAuth Tokens in SQLite. Edit the name directly in the name column.",
    loadingTokens: "Loading Tokens…",
    saveTokens: "Save",
    tokensSaved: "Provider saved",
    deleteProvider: "Delete",
    deleteProviderTitle: "Delete OAuth provider?",
    deleteProviderDescription:
      "This hides the Provider from management and scheduling. Its Tokens, affinities, and history are retained.",
    confirmDeleteProvider: "Hide provider",
    providerDeleted: "Provider hidden",
    testProvider: "Test",
    testingProvider: "Fetching provider Usage…",
    testTitle: "Provider Usage test",
    testDescription:
      "The server calls the Usage API with this provider's OAuth Token. The Token is not returned with the test result.",
    testSucceeded: "Provider test succeeded",
    httpProxy: "HTTP proxy",
    upstreamProxy: "Upstream proxy",
    proxyConfigured: "Configured",
    proxyNotConfigured: "Not configured",
    proxyConfigTitle: "Upstream proxy",
    proxyConfigDescription:
      "This proxy is used only by this provider. Saved addresses and credentials are never shown in the browser.",
    proxyUrl: "Proxy URL",
    proxyUrlHelp: "Only http:// is supported, for example http://username:password@host:port.",
    saveProxy: "Save proxy",
    removeProxy: "Remove proxy",
    proxyDiagnostics: "Connection diagnostics",
    proxyDiagnosticsTitle: "Provider proxy diagnostics",
    proxyDiagnosticsDescription:
      "The LB runs these checks. Proxy credentials are never returned to the browser.",
    testingProxy: "Checking proxy path…",
    testAgain: "Test again",
    proxyExitLocation: "Exit location",
    proxyNetworkQuality: "Network quality",
    lbToProxy: "LB → proxy",
    lbViaProxyToOpenAi: "LB via proxy → OpenAI",
    proxyLatencyHelp:
      "Shows time until a response is received; it is not one-way network latency.",
    proxyLocationUnavailable: "Exit location unavailable",
    proxyNotConfiguredDescription:
      "Configure an HTTP proxy before running diagnostics.",
    usageEmail: "Email",
    usagePlan: "Plan",
    quotaRemaining: "Quota remaining",
    resetsIn: "Resets in",
    quotaUnavailable: "Quota unavailable",
    credits: "Credits",
    rawUsage: "Raw Usage fields",
    usageUnavailable:
      "The Usage API returned no recognized quota fields. Review the raw fields below.",
    rateLimitResetsAvailable: "Resets available",
    viewRateLimitResets: "Reset credits",
    rateLimitResetsTitle: "Rate-limit reset credits",
    rateLimitResetsDescription:
      "Redeeming a credit consumes one earned entitlement and only resets an eligible ChatGPT / Codex rate-limit window.",
    rateLimitResetGrantedAt: "Granted",
    rateLimitResetExpiresAt: "Expires",
    rateLimitResetNextExpiresAt: "Credit expirations",
    rateLimitResetExpiresIn: "Left",
    rateLimitResetNoExpiry: "Expiry unavailable",
    rateLimitResetExpired: "Expired",
    rateLimitResetUnknownCredit:
      "The server did not return item details. Confirmation will use the next available credit.",
    rateLimitResetUse: "Use this credit",
    rateLimitResetUseNext: "Use next credit",
    rateLimitResetConfirmTitle: "Use a rate-limit reset credit?",
    rateLimitResetConfirmDescription:
      "This consumes one earned reset credit for the current Provider. A consumed credit cannot be restored.",
    confirmRateLimitReset: "Use reset credit",
    rateLimitResetting: "Resetting…",
    rateLimitResetSuccess: "Rate limit reset and current status refreshed.",
    rateLimitResetAlreadyRedeemed:
      "This reset was already completed and the current status refreshed.",
    rateLimitResetNothingToReset:
      "There is no eligible rate-limit window to reset right now.",
    rateLimitResetNoCredit:
      "This account has no earned reset credits available.",
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
    interceptDegradation: "Degradation interception",
    interceptDegradationHelp:
      "When enabled, successful upstream responses carrying an x-codex-turn-state header of 312 bytes (a possible model downgrade) are cancelled and returned as a 503 error.",
    interceptDegradationUpdated: "Degradation interception updated",
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
    averageLatency: "Average latency",
    last24Hours: "Last 24 hours",
    last7Days: "Last 7 days",
    allUsers: "All users",
    allConsumers: "All Consumers",
    allModels: "All models",
    userLabel: "User",
    model: "Model",
    downstreamModel: "Downstream model",
    upstreamModel: "Upstream model",
    downstreamUserAgent: "Downstream UA",
    modelDowngraded: "Model downgrade",
    modelMismatchHint:
      "The upstream model differs from the downstream requested model.",
    upstreamUserAgent: "Upstream UA",
    upstreamUserAgentHint:
      "Override User-Agent per originator for upstream requests immediately after saving. Leave empty to keep existing behavior; downstream UA still records the original request value.",
    experimentalTurnState312Filter: "Experimental 312 Turn State filter",
    experimentalTurnState312FilterHint:
      "When enabled, an x-codex-turn-state request header exactly 312 bytes long is removed before forwarding upstream. Disabled by default.",
    upstreamUserAgentCodex: "codex_cli_rs UA",
    upstreamUserAgentPi: "pi UA",
    upstreamUserAgentOpencode: "opencode UA",
    consumerLabel: "Consumer",
    date: "Date",
    rows: "Rows",
    columns: "Columns",
    data: "Data",
    hidden: "Hidden",
    inputTokens: "Input Tokens",
    cachedInputTokens: "Cached Input Tokens",
    outputTokens: "Output Tokens",
    cost: "Cost (USD)",
    requestCount: "Request count",
    usageRows: "aggregated rows",
    sortColumn: "Sort",
    total: "Total",
    clearFilters: "Clear filters",
    pivotFields: "Pivot fields",
    groupOrder: "Grouping order",
    dataOrder: "Data column order",
    sum: "Sum",
    auditTitle: "Inference audit",
    auditDescription:
      "Request/response diagnostic previews are stored in SQLite; Authorization and OAuth credentials are excluded.",
    noAudit: "No inference audit records",
    noAuditDescription:
      "A basic inference audit record is written when each proxy call terminates.",
    time: "Time",
    requestId: "Request ID",
    threadId: "Thread ID",
    copyThreadId: "Copy Thread ID",
    sessionId: "Session ID",
    copySessionId: "Copy Session ID",
    copyUserId: "Copy User ID",
    provider: "Provider",
    latency: "Latency",
    firstByteLatency: "First byte",
    totalLatency: "Total",
    requestSize: "Request size",
    responseSize: "Response size",
    codexTurnStateLength: "x-codex-turn-state string length",
    codexTurnStateLengthWarning: "x-codex-turn-state=312; possible model downgrade",
    requestTransportSize: "Request transport (compressed)",
    responseTransportSize: "Response transport (compressed)",
    compressionRatio: "Compression ratio",
    downstreamAcceptEncoding: "Downstream accepts",
    downstreamContentEncoding: "Downstream response encoding",
    upstreamAcceptEncoding: "Upstream request encoding",
    upstreamContentEncoding: "Upstream response encoding",
    networkTransport: "Network transport (compressed)",
    reasoningEffort: "Reasoning effort",
    fastMode: "Fast mode",
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
    auditDetailDescription:
      "Shows retained request and response diagnostic previews; sensitive credentials are excluded.",
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
    responseImages: "Response images",
    revisedPrompt: "Revised prompt",
    auditArchiveUnavailable:
      "The diagnostic record for this request has expired or is unavailable.",
    affinitySource: "Affinity source",
    affinityRequestId: "Affinity request ID",
    affinityHash: "Affinity hash",
    previousRequest: "Previous request",
    nextRequest: "Next request",
    messages: "Message structure",
    requestSettings: "Request settings",
    instructions: "Instructions",
    tools: "Tools",
    toolsDescription:
      "Review tool definitions as a list, or switch to raw JSON to inspect the complete schema.",
    toolsList: "Tool list",
    rawJson: "Raw JSON",
    toolParameters: "Parameters",
    toolRequired: "Required",
    toolOptional: "Optional",
    noTools: "No tools configured",
    unnamedTool: "Unnamed tool",
    rawRequest: "Raw request",
    identityPermissions: "Identity and permissions",
    identityDescription:
      "Auth Mini manages the browser session; the backend only verifies access JWTs.",
    proxyBoundary: "Proxy boundary",
    proxyDescription:
      "OpenAI / CodeX capabilities only; no other vendor protocol compatibility.",
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
    providerQueueRefresh:
      "Concurrency and queue counts update every 2 seconds.",
    accessKey: "Access key",
    refreshKey: "Refresh key",
    consumer: "Consumer",
    input: "Input",
    output: "Output",
    userId: "User ID",
    role: "Role",
    authIssuer: "Auth issuer",
    upstream: "Upstream",
    upstreamOpenaiBeta: "Upstream OpenAI-Beta",
    upstreamOpenaiBetaHint:
      "When empty, LB does not inject this header. A same-named client header still follows the transparent forwarding policy.",
    bodyLimit: "Body limit",
    affinityTtl: "Affinity TTL",
    statusActive: "Available",
    statusCooldown: "Cooling down",
    statusAuthError: "Authentication error",
    statusDisabled: "Disabled",
    statusUnknown: "Unknown",
    roleRoot: "Root",
    roleAdmin: "Administrator",
    roleUser: "Tenant user",
    loginUnknown: "Authentication failed. Try again.",
    adminAuditTitle: "Administrative operation audit",
    adminAuditDescription:
      "Provider, OAuth, Token, access, system, and audit management operations performed by root and administrators.",
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
    providerAuditProviders: "Providers",
    providerAuditModels: "Models",
    modelDowngradeTitle: "Model downgrade audit",
    modelDowngradeDescription:
      "An upstream can answer with a different model than the one requested; this page summarizes how requested models map to served models.",
    modelDowngradeFlowChart:
      "Downstream model → upstream provider → upstream model",
    modelDowngradeFlowDescription:
      "Traffic flows from the requested downstream model to the upstream provider that served it and then to the model it actually reported; red links carry requests whose served model differs from the requested one, with exact counts in the table below.",
    modelDowngradeRateChart: "Time — downgrade rate",
    modelDowngradeRate: "Downgrade rate",
    modelDowngradeRateDescription:
      "Downgrade rate = requests whose served upstream model differs from the requested model / requests with a recorded upstream model.",
    modelDowngradePeriod: "Data range",
    modelDowngradeTableDescription:
      "Each row is one combination of upstream provider, served upstream model, and requested downstream model.",
    modelDowngradeAuditedRequests: "Requests with a served model",
    modelDowngradeDowngradedRequests: "Downgraded requests",
    modelDowngradeConsistent: "Consistent",
    modelDowngradeDowngraded: "Downgraded",
    modelDowngradeUpstreamModel: "Served upstream model",
    modelDowngradeDownstreamModel: "Requested downstream model",
    modelDowngradeNoData: "No served model in the selected range",
    modelDowngradeNoDataDescription:
      "Once an upstream reports its model on a completion or failure event, mappings and the downgrade rate show up here.",
    administrator: "Actor",
    action: "Action",
    target: "Target",
    clientIp: "Client IP",
    setupTitle: "Initialize OpenAI-LB",
    setupDescription:
      "Connect the brand Auth Mini instance and bind the first verified user as the only root.",
    setupIssuer: "Auth Mini issuer",
    setupIssuerHelp:
      "Enter the Auth Mini HTTPS URL supplied by the brand. OpenAI-LB connects to it; it does not deploy or manage it.",
    setupAudience: "JWT audience",
    connectAuth: "Connect Auth Mini",
    changeAuth: "Change instance",
    setupLogin: "Verify the root identity",
    setupLoginHelp:
      "After sign-in, this Auth Mini user_id becomes the OpenAI-LB root.",
    finishSetup: "Bind root and finish setup",
    finishingSetup: "Finishing setup",
    setupStepConnect: "Connect identity",
    setupStepLogin: "Verify first user",
    setupStepFinish: "Bind root",
    setupConnected: "Connected",
    setupWaiting: "Pending",
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
    platformCapacityTitle: "Platform upstream capacity",
    platformCapacityDescription:
      "All upstream plans and primary quota windows are polled every 5 minutes. Plus counts as 1×, Pro Lite as 5×, and Pro as 20×. Only successfully sampled, comparable Plus, Pro Lite, and Pro accounts are included.",
    platformPlusCapacity: "Remaining Plus-equivalent quota",
    platformCapacitySampledAt: "Last sampled",
    platformCapacityPending: "Waiting for first sample",
    platformCapacityTrend: "Capacity trend, last 7 days",
    platformCapacityTrendDescription:
      "Shows the platform Plus-equivalent quota remaining at each sample.",
    platformCapacityTrendPending: "Waiting for historical samples.",
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
    imageHostModel: "Image host model",
    oauthAuthorizeUrl: "OAuth authorize URL",
    oauthTokenUrl: "OAuth token URL",
    oauthRedirectUri: "OAuth redirect URI",
    oauthClientId: "OAuth client ID",
    responseLimit: "Responses limit",
    imageLimit: "Image request limit",
    audioLimit: "Audio request limit",
    transcriptionInput: "Audio input",
    transcriptionInputHelp:
      "Upload an audio file or record directly with the browser microphone.",
    transcriptionModel: "Model ID",
    transcriptionModelId: "gpt-4o-transcribe",
    selectAudio: "Choose audio",
    startRecording: "Start recording",
    stopRecording: "Stop recording",
    recording: "Recording",
    languageHint: "Language hint",
    languageAuto: "Auto-detect",
    languageChinese: "Chinese",
    languageEnglish: "English",
    transcribe: "Transcribe",
    transcribing: "Transcribing",
    transcript: "Transcript",
    transcriptEmpty: "Choose or record audio to see the transcript here.",
    copyTranscript: "Copy transcript",
    microphoneUnavailable:
      "This browser cannot record audio. Upload an audio file instead.",
    microphoneDenied:
      "Microphone access failed. Check browser permissions and try again.",
    noAudioSelected: "Choose or record audio first.",
    realtimeStart: "Start realtime voice",
    realtimeStop: "End conversation",
    realtimeIdle: "Ready",
    realtimeConnecting: "Creating a secure voice connection…",
    realtimeLive: "Live conversation",
    realtimeConnectionFailed: "The realtime voice connection ended. Please try again.",
    realtimeInstruction: "Conversation instructions",
    realtimeInstructionHelp:
      "This is sent with the realtime session. The default is a concise, natural voice assistant.",
    realtimeTranscript: "Live transcript",
    realtimeTranscriptEmpty:
      "Realtime input and output transcripts appear here after you start speaking.",
    realtimeRouteTitle: "Realtime voice route",
    realtimeRouteDescription:
      "WebRTC carries browser media; the LB proxies only protected session creation and sideband control signaling.",
    realtimePublicEndpoint: "Public endpoint",
    realtimeUpstreamEndpoint: "OpenAI Realtime upstream",
    realtimeMicrophoneHelp:
      "Starting requests microphone permission. Ending the conversation immediately closes local audio tracks.",
    imagePrompt: "Image prompt",
    imagePromptPlaceholder:
      "Describe the subject, composition, style, and any text that should appear in the image.",
    imageReference: "Reference images",
    imageReferenceHelp:
      "Optional: add PNG, JPEG, WEBP, or GIF images for composition, style, or subject reference; up to 4 images and 8 MiB total.",
    imageReferenceCount: "Added",
    removeReferenceImage: "Remove reference image",
    imageReferenceInvalid: "Choose PNG, JPEG, WEBP, or GIF images.",
    imageReferenceTooLarge: "Each reference image must be 4 MiB or smaller.",
    imageReferenceCountExceeded: "You can add up to 4 reference images.",
    imageReferenceTotalExceeded:
      "Reference images must be 8 MiB or smaller in total.",
    imageReferenceReadError: "Could not read the reference image. Try again.",
    imageSize: "Aspect ratio",
    imageQuality: "Generation quality",
    imageSquare: "Square",
    imageLandscape: "Landscape",
    imagePortrait: "Portrait",
    image2kSquare: "2K square",
    image2kLandscape: "2K landscape",
    image4kLandscape: "4K landscape",
    image4kPortrait: "4K portrait",
    imageCustom: "Custom size",
    imageWidth: "Width (px)",
    imageHeight: "Height (px)",
    imageSizeHelp:
      "gpt-image-2 accepts dimensions in multiples of 16, up to 3840px, with an aspect ratio up to 3:1 and 655,360–8,294,400 total pixels. Outputs above 2560 × 1440 are experimental.",
    imageSizeInvalid: "Enter a width and height that meet these limits.",
    imageAuto: "Auto",
    imageDraft: "Draft",
    imageStandard: "Standard",
    imageHigh: "High",
    generateImage: "Generate image",
    generatingImage: "Generating image",
    generatedImage: "Generated image",
    imageEmpty:
      "The generated image will appear here after you submit a prompt.",
    downloadImage: "Download image",
    noImagePrompt: "Enter an image prompt first.",
    imageGenerationFailed: "Image generation failed.",
    imageGenerationRetry:
      "Try again later; if it keeps failing, adjust the prompt.",
    imageGenerationDetail: "Upstream detail",
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
    [
      "system-resources",
      "admin-audit",
      "provider-audit",
      "model-downgrade-audit",
    ].includes(requestedPage) && !isAdministrator
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
      label: t.navigationTools,
      items: [
        ["transcriptions", FileAudioIcon],
        ["realtime", RadioIcon],
        ["images", ImageIcon],
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
              ["model-downgrade-audit", WorkflowIcon],
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
              src="/openai.svg"
            />
            <div className="flex min-w-0 flex-col gap-0.5 group-data-[collapsible=icon]:hidden">
              <strong className="truncate text-sm">{t.title}</strong>
              <span className="truncate text-xs text-muted-foreground">{t.subtitle}</span>
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
                path="/transcriptions"
                element={<TranscriptionsPage sdk={sdk} locale={locale} />}
              />
              <Route
                path="/realtime"
                element={<RealtimeVoicePage sdk={sdk} locale={locale} />}
              />
              <Route
                path="/images"
                element={<ImageGenerationPage sdk={sdk} locale={locale} />}
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
                path="/model-downgrade-audit"
                element={
                  isAdministrator ? (
                    <ModelDowngradeAuditPage sdk={sdk} locale={locale} />
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
      integration: "ChatGPT（CodeX）",
      consumerName: "ChatGPT (CodeX)",
      description:
        "手动编辑本机 CodeX 配置，或授权浏览器读取和写入配置文件并自动创建专用 Consumer。",
      preparationTitle: "专用 Consumer",
      preparationDescription:
        "先在“下游消费者”中为 ChatGPT（CodeX）创建独立 Consumer 并保存只展示一次的密钥。手动配置需要把密钥粘贴到配置文件；自动配置只会把新密钥写入你已授权的本地文件。",
      manualTitle: "手动配置",
      manualDescription:
        "编辑用户级 ~/.codex/config.toml，将 OpenAI-LB 设为 CodeX 的模型提供方。",
      manualConfigInstruction:
        "将以下内容合并到 ~/.codex/config.toml；如果已有其他设置，只更新 model_provider 与 model_providers.ntnl-openai。",
      manualTokenInstruction:
        "将 <YOUR_CONSUMER_KEY> 替换为“下游消费者”页面创建的 Consumer 密钥。不要把真实密钥提交到 Git 或共享配置仓库。",
      manualVerifyTitle: "重启并验证",
      manualVerifyDescription:
        "保存配置后重启 CodeX。若请求失败，先用 /v1/models 验证该 Consumer 仍有效，再检查 config.toml 中的 base_url、wire_api 和密钥。",
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
      restartTitle: "重启 CodeX",
      restartDescription:
        "页面只会更新 model_provider 与 model_providers.ntnl-openai；其他 TOML 配置保持不变。写入完成后重启 CodeX。",
      configWritten: "已写入本机 CodeX 配置；请重启 CodeX。",
      configError: "配置未完成",
      configWriteFailed:
        "无法完成配置。请确认已授权文件访问，并选择用户目录下的 .codex/config.toml。",
      consumerCleanupFailed:
        "无法写入 config.toml，且自动撤销新建 Consumer 失败。请在“下游消费者”中撤销名称为 ChatGPT (CodeX) 的新记录。",
      browserRequired: "需要 Chrome 或 Edge",
      browserRequiredDescription:
        "此操作依赖浏览器的本地文件访问能力。请用最新版 Chrome 或 Edge 打开此页面后重试。",
      fileName: "config.toml",
      pickerDescription: "CodeX config.toml",
      accept: { "application/toml": [".toml"] },
    },
    dsh: {
      integration: "DSH（DeepSeek Harness）",
      consumerName: "DSH (DeepSeek Harness)",
      description:
        "手动添加 NTNL OpenAI Provider，或授权浏览器读取和更新 ~/.dsh 中的 YAML 配置。自动流程会创建一个专用 Consumer，并只把密钥写入本机凭据文件。",
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
        "写入 .credentials.yaml 失败，且无法恢复 settings.yaml。请检查 ~/.dsh 中的 NTNL OpenAI 配置后再重试。",
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
        "编辑用户级 ~/.config/opencode/opencode.jsonc，将 OpenAI-LB 添加为 OpenAI-compatible Provider。",
      manualConfigInstruction:
        "将以下 provider.openai-lb 片段合并到 ~/.config/opencode/opencode.jsonc，并把示例模型替换为 /v1/models 返回的可用 model id。",
      manualTokenInstruction:
        "将 <YOUR_CONSUMER_KEY> 替换为“下游消费者”页面创建的 Consumer 密钥。不要把真实密钥提交到 Git 或共享配置仓库。",
      manualVerifyTitle: "重启并验证",
      manualVerifyDescription:
        "保存配置后重启 OpenCode，并选择 openai-lb/gpt-5.4（或模型目录中的其他可用模型）。若请求失败，先用 /v1/models 验证 Consumer 和 baseURL。",
      copyLabel: "复制",
      copiedLabel: "已复制",
      automaticTitle: "浏览器自动配置",
      automaticDescription:
        "授权浏览器读取 opencode.jsonc 后，页面会创建专用 Consumer 并将 provider.openai-lb 写回该文件；其他 JSONC 配置会保留。",
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
        "页面只会更新 provider.openai-lb；其他 JSONC 配置保持不变。写入完成后重启 OpenCode。",
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
        "OpenAI-LB 代理 OpenAI /v1 兼容接口。直接用 Bearer Consumer 密钥调用所需端点，无需编辑任何本地配置文件。",
      firstStep: "先验证服务可访问的模型列表。",
      secondStep: "再调用 Responses API；也可调用图片或音频端点。",
      modelPlaceholder: "替换为可用模型 ID",
      sensitiveTitle: "密钥与排障边界",
      sensitiveDescription:
        "Consumer 密钥只应放在本机安全存储、密钥管理器或受控环境变量中。不要提交到 Git、粘贴到工单或放入浏览器。请求状态、Provider 选择、耗时与用量可在“审计”中追踪。",
    },
  },
  en: {
    codex: {
      integration: "ChatGPT (CodeX)",
      consumerName: "ChatGPT (CodeX)",
      description:
        "Edit the local CodeX configuration manually, or authorize the browser to write it and create a dedicated Consumer automatically.",
      preparationTitle: "Dedicated Consumer",
      preparationDescription:
        "First create a dedicated Consumer for ChatGPT (CodeX) on the Consumers page and save its one-time secret. Manual setup pastes it into the config file; automatic setup writes the new secret only to the local file you authorize.",
      manualTitle: "Manual configuration",
      manualDescription:
        "Edit the user-level ~/.codex/config.toml and point CodeX at OpenAI-LB.",
      manualConfigInstruction:
        "Merge this into ~/.codex/config.toml. If other settings already exist, update only model_provider and model_providers.ntnl-openai.",
      manualTokenInstruction:
        "Replace <YOUR_CONSUMER_KEY> with the Consumer secret created on the Consumers page. Never commit a real secret to Git or a shared config repository.",
      manualVerifyTitle: "Restart and verify",
      manualVerifyDescription:
        "Save the file and restart CodeX. If a request fails, verify the Consumer with /v1/models, then check base_url, wire_api, and the key in config.toml.",
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
      restartTitle: "Restart CodeX",
      restartDescription:
        "Only model_provider and model_providers.ntnl-openai are updated. All other TOML settings stay unchanged. Restart CodeX after the write completes.",
      configWritten:
        "Your local CodeX configuration is updated. Restart CodeX to use it.",
      configError: "Configuration was not completed",
      configWriteFailed:
        "Could not complete configuration. Confirm file access was allowed and select .codex/config.toml from your home directory.",
      consumerCleanupFailed:
        "config.toml could not be written and the new Consumer could not be deleted automatically. Delete the new Consumer named ChatGPT (CodeX) on the Consumers page.",
      browserRequired: "Chrome or Edge is required",
      browserRequiredDescription:
        "This action requires browser access to local files. Open this page in the latest Chrome or Edge and try again.",
      fileName: "config.toml",
      pickerDescription: "CodeX config.toml",
      accept: { "application/toml": [".toml"] },
    },
    dsh: {
      integration: "DSH (DeepSeek Harness)",
      consumerName: "DSH (DeepSeek Harness)",
      description:
        "Add an NTNL OpenAI Provider manually or authorize the browser to read and update the YAML configuration in ~/.dsh. The automatic flow creates a dedicated Consumer and writes its secret only to the local credentials file.",
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
        "Writing .credentials.yaml failed and settings.yaml could not be restored. Check the NTNL OpenAI configuration in ~/.dsh before retrying.",
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
        "Edit the user-level ~/.config/opencode/opencode.jsonc and add OpenAI-LB as an OpenAI-compatible provider.",
      manualConfigInstruction:
        "Merge this provider.openai-lb block into ~/.config/opencode/opencode.jsonc and replace the example with a model ID returned by /v1/models.",
      manualTokenInstruction:
        "Replace <YOUR_CONSUMER_KEY> with the Consumer secret created on the Consumers page. Never commit a real secret to Git or a shared config repository.",
      manualVerifyTitle: "Restart and verify",
      manualVerifyDescription:
        "Save the file, restart OpenCode, and choose openai-lb/gpt-5.4 (or another available model). If a request fails, verify the Consumer and baseURL with /v1/models.",
      copyLabel: "Copy",
      copiedLabel: "Copied",
      automaticTitle: "Configure in the browser",
      automaticDescription:
        "After you authorize opencode.jsonc, the page creates a dedicated Consumer and writes provider.openai-lb back while retaining other JSONC settings.",
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
        "Only provider.openai-lb is updated. All other JSONC settings stay unchanged. Restart OpenCode after the write completes.",
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
        "OpenAI-LB proxies OpenAI-compatible /v1 endpoints. Authenticate the endpoint you need with a Bearer Consumer secret; no local configuration file is edited.",
      firstStep: "First confirm the model list the service exposes.",
      secondStep:
        "Then call the Responses API; image and audio endpoints are also available.",
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

  return (
    <FileIntegrationPage
      sdk={sdk}
      content={content}
      config={{
        updateConfig: updateCodexConfig,
        manualConfig: (origin) => `model_provider = "ntnl-openai"

[model_providers.ntnl-openai]
name = "NTNL OpenAI"
base_url = "${origin}/v1"
experimental_bearer_token = "<YOUR_CONSUMER_KEY>"
wire_api = "responses"
`,
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

  return (
    <FileIntegrationPage
      sdk={sdk}
      content={content}
      config={{
        updateConfig: (config, token) =>
          updateOpenCodeConfig(config, token, origin),
        manualConfig: (configOrigin) => `{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "openai-lb": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "OpenAI-LB",
      "options": {
        "baseURL": "${configOrigin}/v1",
        "apiKey": "<YOUR_CONSUMER_KEY>"
      },
      "models": {
        "gpt-5.4": { "name": "gpt-5.4" }
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
            <TabsTrigger value="automatic">{content.automaticTitle}</TabsTrigger>
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
    ["Provider ID", "ntnl-openai"],
    ["API URL", "https://openai.ntnl.io/v1"],
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
            <TabsTrigger value="automatic">{content.automaticTitle}</TabsTrigger>
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
        updateDshSettings(settingsSource, "sk-pending", dshModels(modelIds))
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
        updateDshSettings(settingsSource, consumer.secret, dshModels(modelIds))
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
    '/v1/models \\\n  -H "Authorization: Bearer <YOUR_CONSUMER_KEY>" \\\n  -H "session-id: <STABLE_SESSION_ID>"'
  const responses =
    "curl " +
    origin +
    '/v1/responses \\\n  -H "Authorization: Bearer <YOUR_CONSUMER_KEY>" \\\n  -H "session-id: <STABLE_SESSION_ID>" \\\n  -H "Content-Type: application/json" \\\n  -d \'{\n    "model": "gpt-5.4",\n    "input": "Explain this Rust error"\n  }\''

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
            code={responses}
            copyLabel={copyLabel}
            copiedLabel={copiedLabel}
          />
          <p className="text-xs text-muted-foreground">
            {content.modelPlaceholder}: <code>gpt-5.4</code>
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
  const { data, error, loading } = useApiQuery<Record<string, number>>(
    sdk,
    "/api/dashboard"
  )
  if (loading) return <LoadingTable />
  if (error) return <ErrorState message={error} />
  const rows = [
    [t.activeConsumers, data?.active_consumers],
    [t.calls24h, data?.calls_24h],
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
      cacheHitRate(data?.cached_tokens_24h ?? 0, data?.input_tokens_24h ?? 0, locale),
    ],
    [
      t.inputOutputRatio,
      inputOutputRatio(data?.input_tokens_24h ?? 0, data?.output_tokens_24h ?? 0, locale),
    ],
  ]
  return (
    <div className="flex flex-col gap-5">
      <PlatformCapacity sdk={sdk} locale={locale} />
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
                <dd className="text-2xl font-semibold tabular-nums">
                  {value}
                </dd>
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
                  <TableHead className="text-center" colSpan={4}>
                    {t.shortContext}
                  </TableHead>
                  <TableHead className="text-center" colSpan={4}>
                    {t.longContext}
                  </TableHead>
                </TableRow>
                <TableRow>
                  {[t.input, t.cachedInput, t.cacheWrite, t.output].map(
                    (label) => (
                      <TableHead className="text-right" key={`short-${label}`}>
                        {label}
                      </TableHead>
                    )
                  )}
                  {[t.input, t.cachedInput, t.cacheWrite, t.output].map(
                    (label) => (
                      <TableHead className="text-right" key={`long-${label}`}>
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
                      {rate(price.short.input_usd_nanos)}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {rate(price.short.cached_input_usd_nanos)}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {rate(price.short.cache_write_usd_nanos)}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {rate(price.short.output_usd_nanos)}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {rate(price.long?.input_usd_nanos)}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {rate(price.long?.cached_input_usd_nanos)}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {rate(price.long?.cache_write_usd_nanos)}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {rate(price.long?.output_usd_nanos)}
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
  const canContinueToMidas = Boolean(summary.data.midas_configured && publicWalletUserId && amountUsdNanos !== null)

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
                <FieldLabel htmlFor="midas-topup-amount">{t.topupAmount}</FieldLabel>
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
                window.open(midasTransferUrl(publicWalletUserId, amountUsdNanos), "_blank", "noopener,noreferrer")
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
      detail: `${t.openaiLbRss}: ${formatStorageBytes(data.memory.process_used_bytes, locale)} · ${t.otherSystemMemory}: ${formatStorageBytes(data.memory.other_used_bytes, locale)} · ${t.systemAvailableMemory}: ${formatStorageBytes(data.memory.available_bytes, locale)}`,
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

function TranscriptionsPage({ sdk, locale }: { sdk: AuthSdk; locale: Locale }) {
  const t = copy[locale]
  const [audio, setAudio] = useState<File | null>(null)
  const [language, setLanguage] = useState("auto")
  const [transcript, setTranscript] = useState("")
  const [pending, setPending] = useState(false)
  const [recording, setRecording] = useState(false)
  const recorder = useRef<MediaRecorder | null>(null)
  const stream = useRef<MediaStream | null>(null)
  const chunks = useRef<Blob[]>([])

  useEffect(
    () => () => {
      if (recorder.current?.state !== "inactive") recorder.current?.stop()
      stream.current?.getTracks().forEach((track) => track.stop())
    },
    []
  )

  function selectAudio(file: File | null) {
    setAudio(file)
    setTranscript("")
  }

  async function startRecording() {
    if (!navigator.mediaDevices || !window.MediaRecorder) {
      toast.error(t.microphoneUnavailable)
      return
    }
    try {
      const mediaStream = await navigator.mediaDevices.getUserMedia({
        audio: true,
      })
      stream.current = mediaStream
      chunks.current = []
      const next = new MediaRecorder(mediaStream)
      next.ondataavailable = (event) => {
        if (event.data.size > 0) chunks.current.push(event.data)
      }
      next.onstop = () => {
        const type = next.mimeType || "audio/webm"
        selectAudio(new File(chunks.current, "recording.webm", { type }))
        mediaStream.getTracks().forEach((track) => track.stop())
        stream.current = null
        recorder.current = null
        setRecording(false)
      }
      recorder.current = next
      next.start()
      setRecording(true)
    } catch {
      toast.error(t.microphoneDenied)
    }
  }

  function stopRecording() {
    recorder.current?.stop()
  }

  async function transcribe() {
    if (!audio) {
      toast.error(t.noAudioSelected)
      return
    }
    setPending(true)
    try {
      const form = new FormData()
      form.set("file", audio)
      if (language !== "auto") form.set("language", language)
      const response = await apiForm<{ text: string }>(
        sdk,
        "/api/transcriptions",
        form
      )
      setTranscript(response.text)
    } catch (error) {
      toast.error(message(error, t))
    } finally {
      setPending(false)
    }
  }

  return (
    <div className="grid max-w-5xl gap-5">
      <Card>
        <CardHeader>
          <CardTitle>{t.transcriptionInput}</CardTitle>
          <CardDescription className="flex flex-wrap items-center gap-2">
            <span>{t.transcriptionInputHelp}</span>
            <Badge variant="secondary">
              {t.transcriptionModel}: <code>{t.transcriptionModelId}</code>
            </Badge>
          </CardDescription>
        </CardHeader>
        <CardContent className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_15rem]">
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="transcription-file">
                {t.selectAudio}
              </FieldLabel>
              <Input
                id="transcription-file"
                type="file"
                accept="audio/*,.m4a,.webm,.wav,.mp3,.ogg,.flac"
                onChange={(event) =>
                  selectAudio(event.target.files?.[0] ?? null)
                }
              />
              <FieldDescription>
                {audio
                  ? `${audio.name} · ${formatStorageBytes(audio.size, locale)}`
                  : t.transcriptEmpty}
              </FieldDescription>
            </Field>
            <div className="flex flex-wrap items-center gap-2">
              {recording ? (
                <Button variant="destructive" onClick={stopRecording}>
                  <SquareIcon data-icon="inline-start" />
                  {t.stopRecording}
                </Button>
              ) : (
                <Button
                  variant="secondary"
                  onClick={() => void startRecording()}
                >
                  <MicIcon data-icon="inline-start" />
                  {t.startRecording}
                </Button>
              )}
              {recording && <Badge variant="outline">{t.recording}</Badge>}
            </div>
          </FieldGroup>
          <Field>
            <FieldLabel htmlFor="transcription-language">
              {t.languageHint}
            </FieldLabel>
            <Select
              value={language}
              onValueChange={(value) => value && setLanguage(value)}
            >
              <SelectTrigger id="transcription-language">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem value="auto">{t.languageAuto}</SelectItem>
                  <SelectItem value="zh">{t.languageChinese}</SelectItem>
                  <SelectItem value="en">{t.languageEnglish}</SelectItem>
                </SelectGroup>
              </SelectContent>
            </Select>
          </Field>
        </CardContent>
        <CardContent className="border-t pt-5">
          <Button
            disabled={!audio || recording || pending}
            onClick={() => void transcribe()}
          >
            {pending ? (
              <Spinner data-icon="inline-start" />
            ) : (
              <UploadIcon data-icon="inline-start" />
            )}
            {pending ? t.transcribing : t.transcribe}
          </Button>
        </CardContent>
      </Card>

      <Card>
        <CardHeader className="flex-row items-center justify-between gap-4">
          <div className="grid gap-1">
            <CardTitle>{t.transcript}</CardTitle>
            <CardDescription>{t.transcriptEmpty}</CardDescription>
          </div>
          <Button
            variant="outline"
            size="sm"
            disabled={!transcript}
            onClick={() =>
              void navigator.clipboard
                .writeText(transcript)
                .then(() => toast.success(t.copied))
            }
          >
            <ClipboardIcon data-icon="inline-start" />
            {t.copyTranscript}
          </Button>
        </CardHeader>
        <CardContent>
          <textarea
            aria-label={t.transcript}
            className="min-h-48 w-full resize-y rounded-md border bg-background px-3 py-2 text-sm leading-6 outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
            placeholder={t.transcriptEmpty}
            readOnly
            value={transcript}
          />
        </CardContent>
      </Card>
    </div>
  )
}

type RealtimeSession = {
  stream: MediaStream
  peer: RTCPeerConnection
  socket: WebSocket
  output: HTMLAudioElement
}

type RealtimeCall = {
  sdp: string
  call_id: string
  sideband_token: string
}

function RealtimeVoicePage({ sdk, locale }: { sdk: AuthSdk; locale: Locale }) {
  const t = copy[locale]
  const [instructions, setInstructions] = useState(
    locale === "zh"
      ? "你是简洁、自然的中文语音助手。先倾听，再用清晰的短句回答。"
      : "You are a concise, natural voice assistant. Listen first, then answer in clear, short sentences."
  )
  const [status, setStatus] = useState<"idle" | "connecting" | "live" | "error">(
    "idle"
  )
  const [transcript, setTranscript] = useState("")
  const [error, setError] = useState("")
  const activeSession = useRef<RealtimeSession | null>(null)

  function stopSession(nextStatus: "idle" | "error" = "idle") {
    const session = activeSession.current
    activeSession.current = null
    session?.socket.close()
    session?.peer.close()
    session?.stream.getTracks().forEach((track) => track.stop())
    if (session?.output.srcObject) {
      session.output.pause()
      session.output.srcObject = null
    }
    setStatus(nextStatus)
  }

  useEffect(() => () => stopSession(), [])

  function appendTranscript(payload: unknown) {
    if (!payload || typeof payload !== "object") return
    const event = payload as { type?: unknown; delta?: unknown; text?: unknown }
    const type = typeof event.type === "string" ? event.type : ""
    const delta = typeof event.delta === "string" ? event.delta : event.text
    if (!type.includes("transcript") || typeof delta !== "string" || !delta) return
    setTranscript((current) => `${current}${delta}`)
  }

  async function startSession() {
    if (!navigator.mediaDevices || !window.RTCPeerConnection) {
      toast.error(t.microphoneUnavailable)
      return
    }
    setError("")
    setTranscript("")
    setStatus("connecting")
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true })
      const peer = new RTCPeerConnection()
      const output = new Audio()
      output.autoplay = true
      for (const track of stream.getTracks()) peer.addTrack(track, stream)
      peer.ontrack = ({ streams }) => {
        output.srcObject = streams[0] ?? null
        void output.play().catch(() => undefined)
      }
      peer.onconnectionstatechange = () => {
        if (peer.connectionState === "failed" && activeSession.current?.peer === peer) {
          stopSession("error")
          setError(t.realtimeConnectionFailed)
        }
      }
      const offer = await peer.createOffer()
      await peer.setLocalDescription(offer)
      if (!offer.sdp) throw new Error(t.realtimeConnectionFailed)
      const form = new FormData()
      form.set("sdp", offer.sdp)
      form.set(
        "session",
        new Blob(
          [
            JSON.stringify({
              type: "realtime",
              model: "gpt-realtime-1.5",
              instructions: instructions.trim(),
              output_modalities: ["audio"],
              audio: {
                input: {
                  format: { type: "audio/pcm", rate: 24000 },
                  noise_reduction: { type: "near_field" },
                  transcription: { model: "gpt-4o-mini-transcribe" },
                  turn_detection: {
                    type: "server_vad",
                    interrupt_response: true,
                    create_response: true,
                    silence_duration_ms: 500,
                  },
                },
                output: {
                  format: { type: "audio/pcm", rate: 24000 },
                  voice: "marin",
                },
              },
            }),
          ],
          { type: "application/json" }
        )
      )
      const call = await apiForm<RealtimeCall>(sdk, "/api/realtime/calls", form, {
        headers: { "x-session-id": crypto.randomUUID() },
      })
      await peer.setRemoteDescription({ type: "answer", sdp: call.sdp })
      const protocol = window.location.protocol === "https:" ? "wss:" : "ws:"
      const socket = new WebSocket(
        `${protocol}//${window.location.host}/api/realtime?token=${encodeURIComponent(call.sideband_token)}`
      )
      activeSession.current = { stream, peer, socket, output }
      socket.onopen = () => {
        if (activeSession.current?.socket === socket) setStatus("live")
      }
      socket.onmessage = (event) => {
        if (typeof event.data !== "string") return
        try {
          appendTranscript(JSON.parse(event.data))
        } catch {
          // The sideband can send non-JSON control frames; WebRTC media remains active.
        }
      }
      socket.onerror = () => {
        if (activeSession.current?.socket === socket) {
          stopSession("error")
          setError(t.realtimeConnectionFailed)
        }
      }
      socket.onclose = () => {
        if (activeSession.current?.socket === socket) {
          stopSession("error")
          setError(t.realtimeConnectionFailed)
        }
      }
    } catch (cause) {
      stopSession("error")
      setError(message(cause, t))
    }
  }

  const statusLabel = {
    idle: t.realtimeIdle,
    connecting: t.realtimeConnecting,
    live: t.realtimeLive,
    error: t.realtimeConnectionFailed,
  }[status]
  const active = status === "connecting" || status === "live"

  return (
    <div className="grid max-w-6xl gap-5">
      <Card>
        <CardHeader className="flex-row items-start justify-between gap-4">
          <div className="grid gap-1">
            <CardTitle>gpt-realtime-1.5</CardTitle>
            <CardDescription>{t.realtimeMicrophoneHelp}</CardDescription>
          </div>
          <Badge variant={status === "error" ? "destructive" : "outline"}>
            <RadioIcon data-icon="inline-start" />
            {statusLabel}
          </Badge>
        </CardHeader>
        <CardContent className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_minmax(18rem,0.72fr)]">
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="realtime-instructions">
                {t.realtimeInstruction}
              </FieldLabel>
              <Textarea
                id="realtime-instructions"
                rows={4}
                disabled={active}
                value={instructions}
                onChange={(event) => setInstructions(event.target.value)}
              />
              <FieldDescription>{t.realtimeInstructionHelp}</FieldDescription>
            </Field>
            <div className="flex flex-wrap items-center gap-2">
              {active ? (
                <Button variant="destructive" onClick={() => stopSession()}>
                  <SquareIcon data-icon="inline-start" />
                  {t.realtimeStop}
                </Button>
              ) : (
                <Button onClick={() => void startSession()}>
                  <MicIcon data-icon="inline-start" />
                  {t.realtimeStart}
                </Button>
              )}
              {status === "connecting" && <Spinner />}
            </div>
          </FieldGroup>
          <Field>
            <FieldLabel htmlFor="realtime-transcript">
              {t.realtimeTranscript}
            </FieldLabel>
            <Textarea
              id="realtime-transcript"
              className="min-h-44 resize-y leading-6"
              placeholder={t.realtimeTranscriptEmpty}
              readOnly
              value={transcript}
            />
          </Field>
        </CardContent>
      </Card>

      {error && (
        <Alert variant="destructive">
          <ShieldAlertIcon />
          <AlertTitle>{t.realtimeConnectionFailed}</AlertTitle>
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      <Card>
        <CardHeader>
          <CardTitle>{t.realtimeRouteTitle}</CardTitle>
          <CardDescription>{t.realtimeRouteDescription}</CardDescription>
        </CardHeader>
        <CardContent>
          <dl className="grid gap-px overflow-hidden rounded-lg border bg-border md:grid-cols-2">
            <div className="grid gap-1 bg-background p-4">
              <dt>{t.realtimePublicEndpoint}</dt>
              <dd>
                <code>POST /v1/realtime/calls</code>
              </dd>
            </div>
            <div className="grid gap-1 bg-background p-4">
              <dt>{t.realtimeUpstreamEndpoint}</dt>
              <dd>
                <code>POST https://api.openai.com/v1/realtime/calls</code>
              </dd>
            </div>
          </dl>
        </CardContent>
      </Card>
    </div>
  )
}

const MAX_REFERENCE_IMAGES = 4
const MAX_REFERENCE_IMAGE_BYTES = 4 * 1024 * 1024
const MAX_REFERENCE_IMAGE_TOTAL_BYTES = 8 * 1024 * 1024
const SUPPORTED_REFERENCE_IMAGE_TYPES = [
  "image/png",
  "image/jpeg",
  "image/webp",
  "image/gif",
]

function readFileAsDataUrl(file: File) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader()
    reader.addEventListener("load", () => {
      if (typeof reader.result === "string") resolve(reader.result)
      else reject(new Error("reference image data is unavailable"))
    })
    reader.addEventListener("error", () =>
      reject(reader.error ?? new Error("reference image read failed"))
    )
    reader.readAsDataURL(file)
  })
}

function isSupportedImageSize(width: string, height: string) {
  const parsedWidth = Number(width)
  const parsedHeight = Number(height)
  if (
    !Number.isInteger(parsedWidth) ||
    !Number.isInteger(parsedHeight) ||
    parsedWidth > 3840 ||
    parsedHeight > 3840 ||
    parsedWidth % 16 !== 0 ||
    parsedHeight % 16 !== 0
  ) {
    return false
  }
  const pixels = parsedWidth * parsedHeight
  return (
    pixels >= 655360 &&
    pixels <= 8294400 &&
    Math.max(parsedWidth, parsedHeight) <=
      3 * Math.min(parsedWidth, parsedHeight)
  )
}

function ImageGenerationPage({
  sdk,
  locale,
}: {
  sdk: AuthSdk
  locale: Locale
}) {
  const t = copy[locale]
  const [prompt, setPrompt] = useState("")
  const [sizePreset, setSizePreset] = useState("1024x1024")
  const [width, setWidth] = useState("1024")
  const [height, setHeight] = useState("1024")
  const [quality, setQuality] = useState("auto")
  const [image, setImage] = useState("")
  const [referenceImages, setReferenceImages] = useState<ReferenceImage[]>([])
  const [pending, setPending] = useState(false)
  const isCustomSize = sizePreset === "custom"
  const customSizeValid = isSupportedImageSize(width, height)
  const size = isCustomSize ? `${width}x${height}` : sizePreset

  async function selectReferenceImages(fileList: FileList | null) {
    const files = Array.from(fileList ?? [])
    if (!files.length) return
    if (referenceImages.length + files.length > MAX_REFERENCE_IMAGES) {
      toast.error(t.imageReferenceCountExceeded)
      return
    }
    if (
      files.some((file) => !SUPPORTED_REFERENCE_IMAGE_TYPES.includes(file.type))
    ) {
      toast.error(t.imageReferenceInvalid)
      return
    }
    if (files.some((file) => file.size > MAX_REFERENCE_IMAGE_BYTES)) {
      toast.error(t.imageReferenceTooLarge)
      return
    }
    const currentBytes = referenceImages.reduce(
      (total, reference) => total + reference.size,
      0
    )
    const selectedBytes = files.reduce((total, file) => total + file.size, 0)
    if (currentBytes + selectedBytes > MAX_REFERENCE_IMAGE_TOTAL_BYTES) {
      toast.error(t.imageReferenceTotalExceeded)
      return
    }
    try {
      const next = await Promise.all(
        files.map(async (file) => ({
          id: crypto.randomUUID(),
          name: file.name,
          size: file.size,
          dataUrl: await readFileAsDataUrl(file),
        }))
      )
      setReferenceImages((current) => [...current, ...next])
    } catch {
      toast.error(t.imageReferenceReadError)
    }
  }

  function removeReferenceImage(id: string) {
    setReferenceImages((current) =>
      current.filter((reference) => reference.id !== id)
    )
  }

  async function generate() {
    if (!prompt.trim()) {
      toast.error(t.noImagePrompt)
      return
    }
    setPending(true)
    try {
      const response = await api<{ data: Array<{ b64_json: string }> }>(
        sdk,
        "/api/images/generations",
        {
          method: "POST",
          body: JSON.stringify({
            model: "gpt-image-2",
            prompt: prompt.trim(),
            n: 1,
            size,
            quality,
            output_format: "png",
            ...(referenceImages.length > 0
              ? {
                  reference_images: referenceImages.map(
                    (reference) => reference.dataUrl
                  ),
                }
              : {}),
          }),
        }
      )
      const result = response.data[0]?.b64_json
      if (!result)
        throw new Error("image generation response is missing image data")
      setImage(`data:image/png;base64,${result}`)
    } catch (error) {
      toast.error(message(error, t))
    } finally {
      setPending(false)
    }
  }

  function download() {
    const link = document.createElement("a")
    link.download = "generated-image.png"
    link.href = image
    link.click()
  }

  return (
    <div className="grid max-w-5xl gap-5">
      <Card>
        <CardHeader>
          <CardTitle>{t.imagePrompt}</CardTitle>
          <CardDescription>{t.pageImages}</CardDescription>
        </CardHeader>
        <CardContent className="grid gap-5">
          <Field>
            <FieldLabel htmlFor="image-prompt">{t.imagePrompt}</FieldLabel>
            <Textarea
              id="image-prompt"
              className="min-h-36 resize-y leading-6"
              onChange={(event) => setPrompt(event.target.value)}
              placeholder={t.imagePromptPlaceholder}
              value={prompt}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="image-reference">
              {t.imageReference}
            </FieldLabel>
            <Input
              id="image-reference"
              type="file"
              accept="image/png,image/jpeg,image/webp,image/gif"
              multiple
              onChange={(event) => {
                void selectReferenceImages(event.target.files)
                event.currentTarget.value = ""
              }}
            />
            <FieldDescription>
              {t.imageReferenceHelp} {t.imageReferenceCount}{" "}
              {referenceImages.length}/{MAX_REFERENCE_IMAGES}
            </FieldDescription>
            {referenceImages.length > 0 && (
              <div
                aria-label={t.imageReference}
                className="grid grid-cols-2 gap-3 sm:grid-cols-4"
                role="list"
              >
                {referenceImages.map((reference) => (
                  <div className="min-w-0" key={reference.id} role="listitem">
                    <div className="relative overflow-hidden rounded-md border bg-muted">
                      <img
                        alt={reference.name}
                        className="aspect-square w-full object-cover"
                        src={reference.dataUrl}
                      />
                      <Button
                        aria-label={`${t.removeReferenceImage}: ${reference.name}`}
                        className="absolute top-1 right-1 bg-background/90"
                        onClick={() => removeReferenceImage(reference.id)}
                        size="icon-sm"
                        type="button"
                        variant="ghost"
                      >
                        <Trash2Icon />
                      </Button>
                    </div>
                    <p
                      className="truncate pt-1 text-xs text-muted-foreground"
                      title={reference.name}
                    >
                      {reference.name}
                    </p>
                  </div>
                ))}
              </div>
            )}
          </Field>
          <div className="grid gap-5 sm:grid-cols-2">
            <Field>
              <FieldLabel htmlFor="image-size">{t.imageSize}</FieldLabel>
              <Select
                value={sizePreset}
                onValueChange={(value) => value && setSizePreset(value)}
              >
                <SelectTrigger id="image-size">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    <SelectItem value="auto">{t.imageAuto}</SelectItem>
                    <SelectItem value="1024x1024">
                      {t.imageSquare} · 1024 × 1024
                    </SelectItem>
                    <SelectItem value="1536x1024">
                      {t.imageLandscape} · 1536 × 1024
                    </SelectItem>
                    <SelectItem value="1024x1536">
                      {t.imagePortrait} · 1024 × 1536
                    </SelectItem>
                    <SelectItem value="2048x2048">
                      {t.image2kSquare} · 2048 × 2048
                    </SelectItem>
                    <SelectItem value="2048x1152">
                      {t.image2kLandscape} · 2048 × 1152
                    </SelectItem>
                    <SelectItem value="3840x2160">
                      {t.image4kLandscape} · 3840 × 2160
                    </SelectItem>
                    <SelectItem value="2160x3840">
                      {t.image4kPortrait} · 2160 × 3840
                    </SelectItem>
                    <SelectItem value="custom">{t.imageCustom}</SelectItem>
                  </SelectGroup>
                </SelectContent>
              </Select>
              <FieldDescription>{t.imageSizeHelp}</FieldDescription>
            </Field>
            <Field>
              <FieldLabel htmlFor="image-quality">{t.imageQuality}</FieldLabel>
              <Select
                value={quality}
                onValueChange={(value) => value && setQuality(value)}
              >
                <SelectTrigger id="image-quality">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    <SelectItem value="auto">{t.imageAuto}</SelectItem>
                    <SelectItem value="low">{t.imageDraft}</SelectItem>
                    <SelectItem value="medium">{t.imageStandard}</SelectItem>
                    <SelectItem value="high">{t.imageHigh}</SelectItem>
                  </SelectGroup>
                </SelectContent>
              </Select>
            </Field>
          </div>
          {isCustomSize && (
            <Field data-invalid={!customSizeValid}>
              <FieldLabel>{t.imageCustom}</FieldLabel>
              <div className="grid gap-3 sm:grid-cols-2">
                <div className="grid gap-2">
                  <FieldLabel htmlFor="image-width">{t.imageWidth}</FieldLabel>
                  <Input
                    aria-invalid={!customSizeValid}
                    id="image-width"
                    inputMode="numeric"
                    max={3840}
                    min={16}
                    onChange={(event) => setWidth(event.target.value)}
                    step={16}
                    type="number"
                    value={width}
                  />
                </div>
                <div className="grid gap-2">
                  <FieldLabel htmlFor="image-height">
                    {t.imageHeight}
                  </FieldLabel>
                  <Input
                    aria-invalid={!customSizeValid}
                    id="image-height"
                    inputMode="numeric"
                    max={3840}
                    min={16}
                    onChange={(event) => setHeight(event.target.value)}
                    step={16}
                    type="number"
                    value={height}
                  />
                </div>
              </div>
              {!customSizeValid && (
                <FieldError>{t.imageSizeInvalid}</FieldError>
              )}
            </Field>
          )}
        </CardContent>
        <CardContent className="border-t pt-5">
          <Button
            disabled={
              !prompt.trim() || pending || (isCustomSize && !customSizeValid)
            }
            onClick={() => void generate()}
          >
            {pending ? (
              <Spinner data-icon="inline-start" />
            ) : (
              <ImageIcon data-icon="inline-start" />
            )}
            {pending ? t.generatingImage : t.generateImage}
          </Button>
        </CardContent>
      </Card>

      <Card>
        <CardHeader className="flex-row items-center justify-between gap-4">
          <div className="grid gap-1">
            <CardTitle>{t.generatedImage}</CardTitle>
            <CardDescription>{t.imageEmpty}</CardDescription>
          </div>
          <Button
            disabled={!image}
            onClick={download}
            size="sm"
            variant="outline"
          >
            <DownloadIcon data-icon="inline-start" />
            {t.downloadImage}
          </Button>
        </CardHeader>
        <CardContent>
          {image ? (
            <div className="flex min-h-72 items-center justify-center overflow-hidden rounded-lg border bg-muted/30 p-3">
              <img
                alt={prompt}
                className="max-h-[42rem] max-w-full object-contain"
                src={image}
              />
            </div>
          ) : (
            <div className="flex min-h-72 items-center justify-center rounded-lg border border-dashed px-6 text-center text-sm text-muted-foreground">
              {t.imageEmpty}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  )
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
  const [open, setOpen] = useState(false)
  const [tokenDialog, setTokenDialog] =
    useState<ProviderTokenDialogState | null>(null)
  const [editingProviderId, setEditingProviderId] = useState<string | null>(
    null
  )
  const [editingProviderName, setEditingProviderName] = useState("")
  const [editingProviderPending, setEditingProviderPending] = useState(false)
  const [testState, setTestState] = useState<ProviderTestState | null>(null)
  const [resetProvider, setResetProvider] = useState<Provider | null>(null)
  const [resetTarget, setResetTarget] =
    useState<ProviderRateLimitResetTarget | null>(null)
  const [resetPending, setResetPending] = useState(false)
  const [circuitProvider, setCircuitProvider] = useState<Provider | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<Provider | null>(null)
  const [deletePending, setDeletePending] = useState(false)
  const [proxyDialog, setProxyDialog] =
    useState<ProviderProxyDialogState | null>(null)
  const tokenRequest = useRef<AbortController | null>(null)
  const testRequest = useRef<AbortController | null>(null)
  const { data, error, loading } = useApiQuery<Provider[]>(
    sdk,
    "/api/providers",
    2_000
  )
  const { data: usageData, error: usageError } =
    useApiQuery<ProviderUsageResponse>(sdk, "/api/providers/usage")
  const { data: resetData, error: resetError } =
    useApiQuery<ProviderRateLimitResetsResponse>(
      sdk,
      "/api/providers/rate-limit-resets"
    )
  const { data: circuitData } = useApiQuery<ProviderCircuitSummaryResponse>(
    sdk,
    "/api/providers/circuit-events"
  )
  useEffect(
    () => () => {
      tokenRequest.current?.abort()
      testRequest.current?.abort()
    },
    []
  )
  function refreshProviders() {
    void queryClient.invalidateQueries({ queryKey: ["/api/providers"] })
    void queryClient.invalidateQueries({ queryKey: ["/api/providers/usage"] })
    void queryClient.invalidateQueries({
      queryKey: ["/api/providers/circuit-events"],
    })
    void queryClient.invalidateQueries({
      queryKey: ["/api/providers/rate-limit-resets"],
    })
  }
  async function mutate(id: string, body: object) {
    try {
      await api(sdk, `/api/providers/${id}`, {
        method: "PATCH",
        body: JSON.stringify(body),
      })
      refreshProviders()
      toast.success(t.providerUpdated)
    } catch (cause) {
      toast.error(message(cause, t))
    }
  }
  function beginProviderNameEdit(provider: Provider) {
    setEditingProviderId(provider.id)
    setEditingProviderName(provider.name)
  }
  function cancelProviderNameEdit() {
    if (editingProviderPending) return
    setEditingProviderId(null)
    setEditingProviderName("")
  }
  async function saveProviderName(provider: Provider) {
    if (editingProviderPending || editingProviderId !== provider.id) return
    setEditingProviderPending(true)
    try {
      await api(sdk, `/api/providers/${provider.id}`, {
        method: "PATCH",
        body: JSON.stringify({ name: editingProviderName }),
      })
      setEditingProviderId(null)
      setEditingProviderName("")
      refreshProviders()
      toast.success(t.providerUpdated)
    } catch (cause) {
      toast.error(message(cause, t))
    } finally {
      setEditingProviderPending(false)
    }
  }
  async function openTokens(provider: Provider) {
    tokenRequest.current?.abort()
    const request = new AbortController()
    tokenRequest.current = request
    setTokenDialog({ provider, loading: true })
    try {
      const tokens = await api<ProviderTokens>(
        sdk,
        `/api/providers/${provider.id}`,
        { signal: request.signal }
      )
      if (tokenRequest.current === request)
        setTokenDialog({ provider, loading: false, tokens })
    } catch (cause) {
      if (!isAbortError(cause) && tokenRequest.current === request)
        setTokenDialog({ provider, loading: false, error: message(cause, t) })
    } finally {
      if (tokenRequest.current === request) tokenRequest.current = null
    }
  }
  async function test(provider: Provider) {
    testRequest.current?.abort()
    const request = new AbortController()
    testRequest.current = request
    setTestState({ provider, status: "loading" })
    try {
      const result = await api<{ usage: ProviderUsage }>(
        sdk,
        `/api/providers/${provider.id}/test`,
        { method: "POST", signal: request.signal }
      )
      if (testRequest.current === request) {
        setTestState({ provider, status: "success", usage: result.usage })
        toast.success(t.testSucceeded)
      }
    } catch (cause) {
      if (!isAbortError(cause) && testRequest.current === request)
        setTestState({ provider, status: "error", error: message(cause, t) })
    } finally {
      if (testRequest.current === request) testRequest.current = null
    }
  }
  async function remove() {
    if (!deleteTarget || deletePending) return
    setDeletePending(true)
    try {
      await api(sdk, `/api/providers/${deleteTarget.id}`, { method: "DELETE" })
      setDeleteTarget(null)
      refreshProviders()
      toast.success(t.providerDeleted)
    } catch (cause) {
      toast.error(message(cause, t))
    } finally {
      setDeletePending(false)
    }
  }
  function closeTokens() {
    tokenRequest.current?.abort()
    tokenRequest.current = null
    setTokenDialog(null)
  }
  function closeTest() {
    testRequest.current?.abort()
    testRequest.current = null
    setTestState(null)
  }
  function beginRateLimitReset(
    provider: Provider,
    credit?: ProviderRateLimitResetCredit
  ) {
    setResetProvider(null)
    setResetTarget({
      provider,
      credit,
      redeemRequestId: crypto.randomUUID(),
    })
  }
  async function consumeRateLimitReset() {
    if (!resetTarget || resetPending) return
    setResetPending(true)
    try {
      const result = await api<ProviderRateLimitResetResult>(
        sdk,
        `/api/providers/${resetTarget.provider.id}/rate-limit-resets/consume`,
        {
          method: "POST",
          body: JSON.stringify({
            credit_id: resetTarget.credit?.id,
            redeem_request_id: resetTarget.redeemRequestId,
          }),
        }
      )
      const code = result.code
      if (code === "nothing_to_reset" || code === "nothingToReset") {
        toast.warning(t.rateLimitResetNothingToReset)
      } else if (code === "no_credit" || code === "noCredit") {
        toast.warning(t.rateLimitResetNoCredit)
      } else if (code === "already_redeemed" || code === "alreadyRedeemed") {
        toast.success(t.rateLimitResetAlreadyRedeemed)
      } else {
        toast.success(t.rateLimitResetSuccess)
      }
      setResetTarget(null)
      refreshProviders()
    } catch (cause) {
      toast.error(message(cause, t))
    } finally {
      setResetPending(false)
    }
  }
  if (loading) return <LoadingTable />
  if (error) return <ErrorState message={error} />
  return (
    <>
      <Card>
        <CardHeader className="flex-row items-start justify-between">
          <div className="min-w-0">
            <CardTitle>{t.providerPool}</CardTitle>
            <CardDescription>{t.providerDescription}</CardDescription>
            <p className="mt-1 text-xs text-muted-foreground">
              {t.providerQueueRefresh}
            </p>
          </div>
          <Button onClick={() => setOpen(true)}>
            <PlusIcon data-icon="inline-start" />
            {t.addProvider}
          </Button>
        </CardHeader>
        <CardContent>
          {!data?.length ? (
            <EmptyState
              icon={<BoxesIcon />}
              title={t.noProviders}
              description={t.noProvidersDescription}
              action={
                <Button onClick={() => setOpen(true)}>{t.addProvider}</Button>
              }
            />
          ) : (
            <div className="grid gap-4 md:grid-cols-2 2xl:grid-cols-3">
              {data.map((provider) => {
                const entry = usageData?.providers[provider.id]
                const usage = entry?.usage
                const quota =
                  usage?.rate_limit?.primary_window ??
                  usage?.rate_limit?.secondary_window
                const resetEntry = resetData?.providers[provider.id]
                const resets = resetEntry?.resets
                const resetCredits = sortRateLimitResetCreditsByExpiry(
                  resets?.credits ?? []
                )
                const availableResetCount = Math.max(
                  0,
                  resets?.available_count ?? 0
                )
                const email = usageEmail(usage)
                const resetUnavailable = resetError || resetEntry?.error

                return (
                  <article
                    key={provider.id}
                    className="flex min-w-0 flex-col gap-4 rounded-lg border bg-background p-4"
                  >
                    <header className="flex items-start justify-between gap-3">
                      <div className="min-w-0 flex-1 space-y-2">
                        <div className="flex min-w-0 flex-wrap items-center gap-2">
                          {editingProviderId === provider.id ? (
                            <div className="flex min-w-0 flex-1 items-center gap-1">
                              <Input
                                aria-label={
                                  t.editProviderName + ": " + provider.name
                                }
                                autoFocus
                                className="h-8 min-w-0"
                                value={editingProviderName}
                                onChange={(event) =>
                                  setEditingProviderName(event.target.value)
                                }
                                onKeyDown={(event) => {
                                  if (event.key === "Enter") {
                                    event.preventDefault()
                                    void saveProviderName(provider)
                                  }
                                  if (event.key === "Escape") {
                                    event.preventDefault()
                                    cancelProviderNameEdit()
                                  }
                                }}
                              />
                              <Button
                                size="icon-xs"
                                variant="ghost"
                                aria-label={
                                  t.saveProviderName + ": " + provider.name
                                }
                                disabled={editingProviderPending}
                                onClick={() => void saveProviderName(provider)}
                              >
                                {editingProviderPending ? (
                                  <Spinner />
                                ) : (
                                  <CheckIcon />
                                )}
                              </Button>
                              <Button
                                size="icon-xs"
                                variant="ghost"
                                aria-label={
                                  t.cancelProviderName + ": " + provider.name
                                }
                                disabled={editingProviderPending}
                                onClick={cancelProviderNameEdit}
                              >
                                <XIcon />
                              </Button>
                            </div>
                          ) : (
                            <div className="flex min-w-0 items-center gap-1">
                              <h3
                                className="min-w-0 truncate font-medium"
                                title={provider.name}
                              >
                                {provider.name}
                              </h3>
                              <Button
                                size="icon-xs"
                                variant="ghost"
                                aria-label={
                                  t.editProviderName + ": " + provider.name
                                }
                                disabled={editingProviderPending}
                                onClick={() => beginProviderNameEdit(provider)}
                              >
                                <PencilIcon />
                              </Button>
                            </div>
                          )}
                          <StatusBadge
                            status={provider.status}
                            locale={locale}
                          />
                        </div>
                        {user.role !== "user" && (
                          <div className="flex min-w-0 items-center gap-1 text-xs text-muted-foreground">
                            <span>{t.owner}:</span>
                            {provider.owner_id ? (
                              <LinkitUserInfo userId={provider.owner_id} />
                            ) : (
                              "—"
                            )}
                          </div>
                        )}
                      </div>
                      <Switch
                        aria-label={t.status + ": " + provider.name}
                        checked={!provider.manual_disabled}
                        onCheckedChange={(checked) =>
                          void mutate(provider.id, { enabled: checked })
                        }
                      />
                    </header>

                    <div className="grid gap-3 rounded-md bg-muted/40 p-3 sm:grid-cols-2">
                      <div className="min-w-0">
                        <p className="text-xs font-medium text-muted-foreground">
                          {t.httpProxy}
                        </p>
                        <Badge variant={provider.http_proxy_configured ? "secondary" : "outline"}>
                          {provider.http_proxy_configured
                            ? t.proxyConfigured
                            : t.proxyNotConfigured}
                        </Badge>
                      </div>
                      <div className="min-w-0">
                        <p className="text-xs font-medium text-muted-foreground">
                          {t.providerOriginator}
                        </p>
                        <Badge variant="secondary">{provider.originator}</Badge>
                        {provider.allow_other_originator && (
                          <Badge variant="outline" className="ml-1">{t.originatorFallback}</Badge>
                        )}
                      </div>
                      <div className="sm:col-span-2">
                        <OriginatorFallbackField
                          id={`provider-fallback-${provider.id}`}
                          checked={provider.allow_other_originator}
                          onChange={(allowed) => void mutate(provider.id, { allow_other_originator: allowed })}
                          locale={locale}
                        />
                      </div>
                      <div className="min-w-0">
                        <p className="text-xs font-medium text-muted-foreground">
                          {t.providerVisibility}
                        </p>
                        <Badge
                          variant={
                            provider.visibility === "public"
                              ? "secondary"
                              : "outline"
                          }
                        >
                          {provider.visibility === "public"
                            ? t.providerVisibilityPublic
                            : t.providerVisibilityPrivate}
                        </Badge>
                      </div>
                      <div className="min-w-0">
                        <p className="text-xs font-medium text-muted-foreground">
                          {t.usageEmail}
                        </p>
                        <p
                          className="truncate text-sm"
                          title={email || undefined}
                        >
                          {email || "—"}
                        </p>
                      </div>
                      <div className="min-w-0">
                        <p className="text-xs font-medium text-muted-foreground">
                          {t.usagePlan}
                        </p>
                        <p className="truncate text-sm">
                          {usage?.plan_type || "—"}
                        </p>
                      </div>
                      <div>
                        <p className="text-xs font-medium text-muted-foreground">
                          {t.providerLoad}
                        </p>
                        <p className="text-sm tabular-nums">
                          {provider.inflight} / {provider.concurrency_limit}
                        </p>
                      </div>
                      <div>
                        <p className="text-xs font-medium text-muted-foreground">
                          {t.providerQueued}
                        </p>
                        <p className="text-sm tabular-nums">
                          {provider.queued}
                        </p>
                      </div>
                      <div>
                        <p className="text-xs font-medium text-muted-foreground">
                          {t.actualProvidedValue}
                        </p>
                        <p className="text-sm tabular-nums">
                          {formatUsd(
                            provider.actual_provided_usd_nanos,
                            locale
                          )}
                        </p>
                      </div>
                      <div>
                        <p className="text-xs font-medium text-muted-foreground">
                          {t.officialProvidedValue}
                        </p>
                        <p className="text-sm tabular-nums">
                          {formatUsd(
                            provider.official_provided_usd_nanos,
                            locale
                          )}
                        </p>
                      </div>
                    </div>

                    <div className="grid gap-4 sm:grid-cols-2">
                      <div className="min-w-0 space-y-2">
                        <div className="flex items-center justify-between gap-2">
                          <p className="text-xs font-medium text-muted-foreground">
                            {t.quotaRemaining}
                          </p>
                          <span className="text-xs tabular-nums">
                            {remainingPercent(quota)}
                          </span>
                        </div>
                        <QuotaProgress
                          window={quota}
                          label={t.quotaRemaining + ": " + provider.name}
                          unavailable={entry?.error || usageError}
                        />
                        <p className="text-xs text-muted-foreground tabular-nums">
                          {t.resetsIn}: {quotaReset(quota, locale)}
                        </p>
                      </div>
                      <div className="min-w-0 space-y-2">
                        <div className="flex items-center justify-between gap-2">
                          <p className="text-xs font-medium text-muted-foreground">
                            {t.rateLimitResetsAvailable}
                          </p>
                          {resetUnavailable ? (
                            <span
                              className="text-xs text-muted-foreground"
                              title={resetUnavailable}
                            >
                              —
                            </span>
                          ) : (
                            <Badge variant="secondary">
                              {availableResetCount}
                            </Badge>
                          )}
                        </div>
                        {resetUnavailable ? (
                          <p
                            className="text-xs text-muted-foreground"
                            title={resetUnavailable}
                          >
                            {t.quotaUnavailable}
                          </p>
                        ) : (
                          <RateLimitResetExpiries
                            locale={locale}
                            credits={resetCredits}
                          />
                        )}
                      </div>
                    </div>

                    <div className="flex items-start justify-between gap-3 border-t pt-3">
                      <div className="min-w-0">
                        <p className="mb-2 text-xs font-medium text-muted-foreground">
                          {t.circuitBreaker}
                        </p>
                        <ProviderCircuitStatus
                          event={circuitData?.providers[provider.id]}
                          locale={locale}
                          provider={provider}
                          onOpenHistory={() => setCircuitProvider(provider)}
                        />
                      </div>
                    </div>

                    <div className="flex flex-wrap gap-2 border-t pt-3">
                      <Button size="sm" variant="outline" onClick={() => setProxyDialog({ provider })}>
                        {provider.http_proxy_configured && <CheckIcon data-icon="inline-start" />}
                        {t.upstreamProxy}
                      </Button>
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={() =>
                          void mutate(provider.id, {
                            visibility:
                              provider.visibility === "public"
                                ? "private"
                                : "public",
                          })
                        }
                      >
                        {provider.visibility === "public" ? (
                          <LockIcon data-icon="inline-start" />
                        ) : (
                          <GlobeIcon data-icon="inline-start" />
                        )}
                        {provider.visibility === "public"
                          ? t.makeProviderPrivate
                          : t.makeProviderPublic}
                      </Button>
                      {availableResetCount > 0 && (
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() => setResetProvider(provider)}
                        >
                          <RefreshCwIcon data-icon="inline-start" />
                          {t.viewRateLimitResets}
                        </Button>
                      )}
                      {isAdministrator && (
                        <Button
                          size="sm"
                          variant="outline"
                          disabled={
                            tokenDialog?.provider.id === provider.id &&
                            tokenDialog.loading
                          }
                          onClick={() => void openTokens(provider)}
                        >
                          {tokenDialog?.provider.id === provider.id &&
                          tokenDialog.loading ? (
                            <Spinner data-icon="inline-start" />
                          ) : (
                            <KeyRoundIcon data-icon="inline-start" />
                          )}
                          {t.editProvider}
                        </Button>
                      )}
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={
                          testState?.provider.id === provider.id &&
                          testState.status === "loading"
                        }
                        onClick={() => void test(provider)}
                      >
                        {testState?.provider.id === provider.id &&
                        testState.status === "loading" ? (
                          <Spinner data-icon="inline-start" />
                        ) : (
                          <ActivityIcon data-icon="inline-start" />
                        )}
                        {t.testProvider}
                      </Button>
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={() =>
                          void mutate(provider.id, { refresh: true })
                        }
                      >
                        <RefreshCwIcon data-icon="inline-start" />
                        {t.refresh}
                      </Button>
                      <Button
                        size="icon-sm"
                        variant="ghost"
                        aria-label={t.deleteProvider + ": " + provider.name}
                        onClick={() => setDeleteTarget(provider)}
                      >
                        <Trash2Icon />
                      </Button>
                    </div>
                  </article>
                )
              })}
            </div>
          )}
        </CardContent>
      </Card>
      <ProviderDialog
        sdk={sdk}
        locale={locale}
        open={open}
        onOpenChange={setOpen}
        onDone={() => {
          setOpen(false)
          refreshProviders()
        }}
      />
      <ProviderTokensDialog
        sdk={sdk}
        locale={locale}
        state={tokenDialog}
        onClose={closeTokens}
        onSaved={() => {
          closeTokens()
          refreshProviders()
        }}
      />
      <ProviderTestDialog
        locale={locale}
        state={testState}
        onClose={closeTest}
      />
      <ProviderProxyDialog
        sdk={sdk}
        locale={locale}
        state={proxyDialog}
        onClose={() => setProxyDialog(null)}
        onSaved={() => {
          refreshProviders()
        }}
      />
      {resetProvider && (
        <ProviderRateLimitResetsDialog
          locale={locale}
          provider={resetProvider}
          resets={resetData?.providers[resetProvider.id]?.resets}
          error={resetData?.providers[resetProvider.id]?.error || resetError}
          onClose={() => setResetProvider(null)}
          onConsume={beginRateLimitReset}
        />
      )}
      {circuitProvider && (
        <ProviderCircuitHistorySheet
          sdk={sdk}
          locale={locale}
          provider={circuitProvider}
          onClose={() => setCircuitProvider(null)}
        />
      )}
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
              variant="destructive"
              disabled={deletePending}
              onClick={() => void remove()}
            >
              {deletePending && <Spinner data-icon="inline-start" />}
              {t.confirmDeleteProvider}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
      <AlertDialog
        open={Boolean(resetTarget)}
        onOpenChange={(next) => !next && !resetPending && setResetTarget(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t.rateLimitResetConfirmTitle}</AlertDialogTitle>
            <AlertDialogDescription>
              {t.rateLimitResetConfirmDescription}
            </AlertDialogDescription>
            {resetTarget?.credit && (
              <RateLimitResetExpiry
                locale={locale}
                credit={resetTarget.credit}
              />
            )}
          </AlertDialogHeader>
          <Definition
            rows={[
              [t.provider, resetTarget?.provider.name || "—"],
              [
                t.viewRateLimitResets,
                resetTarget?.credit?.title || t.rateLimitResetUseNext,
              ],
            ]}
          />
          <AlertDialogFooter>
            <AlertDialogCancel disabled={resetPending}>
              {t.cancel}
            </AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              disabled={resetPending}
              onClick={(event) => {
                event.preventDefault()
                void consumeRateLimitReset()
              }}
            >
              {resetPending && <Spinner data-icon="inline-start" />}
              {resetPending ? t.rateLimitResetting : t.confirmRateLimitReset}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}

function ProviderProxyDialog({
  sdk,
  locale,
  state,
  onClose,
  onSaved,
}: {
  sdk: AuthSdk
  locale: Locale
  state: ProviderProxyDialogState | null
  onClose: () => void
  onSaved: () => void
}) {
  const t = copy[locale]
  const [url, setUrl] = useState("")
  const [pending, setPending] = useState(false)
  const [configured, setConfigured] = useState(false)
  const [health, setHealth] = useState<ProviderProxyHealth | "loading">()
  useEffect(() => {
    setUrl("")
    setPending(false)
    setConfigured(Boolean(state?.provider.http_proxy_configured))
    setHealth(undefined)
  }, [state])
  async function save(remove = false) {
    if (!state || pending) return
    setPending(true)
    try {
      await api(sdk, `/api/providers/${state.provider.id}`, {
        method: "PATCH",
        body: JSON.stringify({ http_proxy_url: remove ? "" : url.trim() }),
      })
      toast.success(t.providerUpdated)
      setConfigured(!remove)
      setHealth(undefined)
      onSaved()
    } catch (cause) {
      toast.error(message(cause, t))
    } finally {
      setPending(false)
    }
  }
  async function test() {
    if (!state || pending || !configured) return
    setHealth("loading")
    try {
      setHealth(await api<ProviderProxyHealth>(sdk, `/api/providers/${state.provider.id}/proxy-health`, { method: "POST" }))
    } catch (cause) {
      setHealth({ proxy_configured: true, error: message(cause, t) })
    }
  }
  const testing = health === "loading"
  const result = health === "loading" ? undefined : health
  const location = result?.location
  const place = [location?.city, location?.region, location?.country]
    .filter(Boolean)
    .join(", ")
  return (
    <Dialog open={Boolean(state)} onOpenChange={(next) => !next && !pending && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t.proxyConfigTitle}: {state?.provider.name}</DialogTitle>
          <DialogDescription>{t.proxyConfigDescription}</DialogDescription>
        </DialogHeader>
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor="provider-http-proxy">{t.proxyUrl}</FieldLabel>
            <Input
              id="provider-http-proxy"
              autoComplete="off"
              type="password"
              placeholder="http://username:password@host:port"
              value={url}
              onChange={(event) => setUrl(event.target.value)}
            />
            <FieldDescription>{t.proxyUrlHelp}</FieldDescription>
          </Field>
          <DialogFooter className="gap-2 sm:gap-0">
            {configured && (
              <Button type="button" variant="destructive" disabled={pending} onClick={() => void save(true)}>
                {pending && <Spinner data-icon="inline-start" />}
                {t.removeProxy}
              </Button>
            )}
            <Button type="button" variant="outline" disabled={pending} onClick={onClose}>
              {t.cancel}
            </Button>
            <Button type="button" disabled={pending || !url.trim()} onClick={() => void save()}>
              {pending && <Spinner data-icon="inline-start" />}
              {t.saveProxy}
            </Button>
          </DialogFooter>
        </FieldGroup>
        <section className="space-y-3 border-t pt-5">
          <div>
            <h3 className="text-sm font-medium">{t.proxyDiagnostics}</h3>
            <p className="mt-1 text-xs text-muted-foreground">{t.proxyDiagnosticsDescription}</p>
          </div>
          {!configured ? (
            <p className="text-sm text-muted-foreground">{t.proxyNotConfiguredDescription}</p>
          ) : testing ? (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <Spinner />
              {t.testingProxy}
            </div>
          ) : result?.error ? (
            <Alert variant="destructive">
              <AlertTitle>{t.proxyDiagnostics}</AlertTitle>
              <AlertDescription>{result.error}</AlertDescription>
            </Alert>
          ) : result ? (
            <div className="flex flex-col gap-5">
              <section className="space-y-2">
                <h4 className="text-sm font-medium">{t.proxyExitLocation}</h4>
                {location?.ip ? (
                  <Definition rows={[
                    ["IP", location.ip],
                    [t.proxyExitLocation, place || "—"],
                    ["ASN / Org", location.org || "—"],
                  ]} />
                ) : (
                  <p className="text-sm text-muted-foreground">{t.proxyLocationUnavailable}</p>
                )}
              </section>
              <section className="space-y-2">
                <h4 className="text-sm font-medium">{t.proxyNetworkQuality}</h4>
                <Definition rows={[
                  [t.lbToProxy, result.lb_to_proxy_ms === undefined ? "—" : `${result.lb_to_proxy_ms} ms`],
                  [t.lbViaProxyToOpenAi, result.proxy_to_openai_ms === undefined ? "—" : `${result.proxy_to_openai_ms} ms`],
                ]} />
                <p className="text-xs text-muted-foreground">{t.proxyLatencyHelp}</p>
              </section>
            </div>
          ) : null}
          {configured && (
            <Button type="button" variant="outline" disabled={testing} onClick={() => void test()}>
              {testing && <Spinner data-icon="inline-start" />}
              {result ? t.testAgain : t.proxyDiagnostics}
            </Button>
          )}
        </section>
      </DialogContent>
    </Dialog>
  )
}

function ProviderTokensDialog({
  sdk,
  locale,
  state,
  onClose,
  onSaved,
}: {
  sdk: AuthSdk
  locale: Locale
  state: ProviderTokenDialogState | null
  onClose: () => void
  onSaved: () => void
}) {
  const t = copy[locale]
  const [access, setAccess] = useState("")
  const [refresh, setRefresh] = useState("")
  const [pending, setPending] = useState(false)
  const saveRequest = useRef<AbortController | null>(null)
  useEffect(() => {
    saveRequest.current?.abort()
    saveRequest.current = null
    setPending(false)
    setAccess(state?.tokens?.access_key ?? "")
    setRefresh(state?.tokens?.refresh_key ?? "")
  }, [state])
  async function save(event: FormEvent) {
    event.preventDefault()
    if (!state || pending) return
    const request = new AbortController()
    saveRequest.current = request
    setPending(true)
    try {
      await api(sdk, `/api/providers/${state.provider.id}`, {
        method: "PUT",
        body: JSON.stringify({
          access_key: access,
          refresh_key: refresh,
        }),
        signal: request.signal,
      })
      if (saveRequest.current === request) {
        toast.success(t.tokensSaved)
        onSaved()
      }
    } catch (cause) {
      if (!isAbortError(cause) && saveRequest.current === request)
        toast.error(message(cause, t))
    } finally {
      if (saveRequest.current === request) {
        saveRequest.current = null
        setPending(false)
      }
    }
  }
  function close() {
    saveRequest.current?.abort()
    saveRequest.current = null
    setPending(false)
    onClose()
  }
  return (
    <Dialog open={Boolean(state)} onOpenChange={(next) => !next && close()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {t.tokenTitle}: {state?.provider.name}
          </DialogTitle>
          <DialogDescription>{t.tokenDescription}</DialogDescription>
        </DialogHeader>
        {state?.loading ? (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Spinner />
            {t.loadingTokens}
          </div>
        ) : state?.error ? (
          <ErrorState message={state.error} />
        ) : (
          <form onSubmit={save}>
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="edit-access-key">{t.accessKey}</FieldLabel>
                <Input
                  id="edit-access-key"
                  autoComplete="off"
                  value={access}
                  onChange={(event) => setAccess(event.target.value)}
                  required
                />
                <FieldDescription>{t.accessClaimHelp}</FieldDescription>
              </Field>
              <Field>
                <FieldLabel htmlFor="edit-refresh-key">
                  {t.refreshKey}
                </FieldLabel>
                <Input
                  id="edit-refresh-key"
                  autoComplete="off"
                  value={refresh}
                  onChange={(event) => setRefresh(event.target.value)}
                  required
                />
              </Field>
              <Field>
                <FieldLabel>{t.providerOriginator}</FieldLabel>
                <div>
                  <Badge variant="secondary">
                    {state?.provider.originator ?? ""}
                  </Badge>
                </div>
                <FieldDescription>
                  {t.providerOriginatorLockedHelp}
                </FieldDescription>
              </Field>
              <DialogFooter>
                <Button type="button" variant="outline" onClick={close}>
                  {t.cancel}
                </Button>
                <Button
                  type="submit"
                  disabled={pending || !access.trim() || !refresh.trim()}
                >
                  {pending && <Spinner data-icon="inline-start" />}
                  {t.saveTokens}
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
  const usage = state?.usage
  const primary = usage?.rate_limit?.primary_window
  const secondary = usage?.rate_limit?.secondary_window
  const hasSummary = Boolean(
    usageEmail(usage) ||
    usage?.plan_type ||
    primary ||
    secondary ||
    usage?.credits
  )
  return (
    <Dialog open={Boolean(state)} onOpenChange={(next) => !next && onClose()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>
            {t.testTitle}: {state?.provider.name}
          </DialogTitle>
          <DialogDescription>{t.testDescription}</DialogDescription>
        </DialogHeader>
        {state?.status === "loading" ? (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Spinner />
            {t.testingProvider}
          </div>
        ) : state?.status === "error" ? (
          <ErrorState message={state.error || t.unknownError} />
        ) : (
          usage && (
            <div className="flex flex-col gap-4">
              {hasSummary ? (
                <Definition
                  rows={[
                    [t.usageEmail, usageEmail(usage) || "—"],
                    [t.usagePlan, usage.plan_type || "—"],
                    [t.quotaRemaining, remainingPercent(primary ?? secondary)],
                    [t.resetsIn, quotaReset(primary ?? secondary, locale)],
                    [
                      t.credits,
                      usage.credits?.unlimited
                        ? "∞"
                        : (usage.credits?.balance ?? "—"),
                    ],
                  ]}
                />
              ) : (
                <Alert>
                  <AlertTitle>{t.usageUnavailable}</AlertTitle>
                </Alert>
              )}
              <div className="flex flex-col gap-2">
                <h3 className="text-sm font-medium">{t.rawUsage}</h3>
                <ScrollArea className="max-h-72 rounded-lg border bg-muted p-3">
                  <pre className="text-xs break-all whitespace-pre-wrap">
                    {JSON.stringify(usage, null, 2)}
                  </pre>
                </ScrollArea>
              </div>
            </div>
          )
        )}
        <DialogFooter>
          <Button onClick={onClose}>{t.close}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function ProviderRateLimitResetsDialog({
  locale,
  provider,
  resets,
  error,
  onClose,
  onConsume,
}: {
  locale: Locale
  provider: Provider
  resets?: ProviderRateLimitResets
  error?: string
  onClose: () => void
  onConsume: (provider: Provider, credit?: ProviderRateLimitResetCredit) => void
}) {
  const t = copy[locale]
  const credits = sortRateLimitResetCreditsByExpiry(resets?.credits ?? [])
  const availableCount = Math.max(0, resets?.available_count ?? 0)
  return (
    <Dialog open onOpenChange={(next) => !next && onClose()}>
      <DialogContent className="sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>
            {t.rateLimitResetsTitle}: {provider.name}
          </DialogTitle>
          <DialogDescription>{t.rateLimitResetsDescription}</DialogDescription>
        </DialogHeader>
        {error ? (
          <ErrorState message={error} />
        ) : (
          <div className="flex flex-col gap-4">
            <Definition rows={[[t.rateLimitResetsAvailable, availableCount]]} />
            {credits.length > 0 ? (
              <DataTable>
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>{t.viewRateLimitResets}</TableHead>
                      <TableHead>{t.rateLimitResetExpiresAt}</TableHead>
                      <TableHead className="text-right">{t.actions}</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {credits.map((credit) => (
                      <TableRow key={credit.id}>
                        <TableCell>
                          <div className="flex flex-col gap-1">
                            <span className="font-medium">
                              {credit.title || credit.reset_type || credit.id}
                            </span>
                            {credit.description && (
                              <span className="text-xs text-muted-foreground">
                                {credit.description}
                              </span>
                            )}
                            <span className="text-xs text-muted-foreground">
                              {t.rateLimitResetGrantedAt}:{" "}
                              {formatTime(credit.granted_at, locale)}
                            </span>
                          </div>
                        </TableCell>
                        <TableCell>
                          <RateLimitResetExpiry
                            locale={locale}
                            credit={credit}
                          />
                        </TableCell>
                        <TableCell className="text-right">
                          <Button
                            size="sm"
                            variant="outline"
                            onClick={() => onConsume(provider, credit)}
                          >
                            <RefreshCwIcon data-icon="inline-start" />
                            {t.rateLimitResetUse}
                          </Button>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </DataTable>
            ) : availableCount > 0 ? (
              <Alert>
                <RefreshCwIcon />
                <AlertTitle>{t.rateLimitResetsTitle}</AlertTitle>
                <AlertDescription className="flex flex-wrap items-center justify-between gap-3">
                  <span>{t.rateLimitResetUnknownCredit}</span>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => onConsume(provider)}
                  >
                    <RefreshCwIcon data-icon="inline-start" />
                    {t.rateLimitResetUseNext}
                  </Button>
                </AlertDescription>
              </Alert>
            ) : (
              <EmptyState
                icon={<RefreshCwIcon />}
                title={t.rateLimitResetsAvailable}
                description={t.rateLimitResetNoCredit}
              />
            )}
          </div>
        )}
        <DialogFooter>
          <Button onClick={onClose}>{t.close}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function RateLimitResetExpiry({
  locale,
  credit,
}: {
  locale: Locale
  credit?: ProviderRateLimitResetCredit
}) {
  const t = copy[locale]
  if (!credit) return <span className="text-xs text-muted-foreground">—</span>

  const expiresAt = rateLimitResetTimestampSeconds(credit.expires_at)
  if (expiresAt === undefined)
    return (
      <span className="text-xs text-muted-foreground">
        {t.rateLimitResetNoExpiry}
      </span>
    )
  const status = rateLimitResetExpiryStatus(expiresAt)
  const exactTime = formatRateLimitResetExpiryTime(expiresAt, locale)

  return (
    <Tooltip>
      <TooltipTrigger
        aria-label={`${t.rateLimitResetExpiresAt}: ${exactTime}`}
        className="inline-flex cursor-help rounded-sm bg-transparent p-0 text-inherit outline-none focus-visible:ring-3 focus-visible:ring-ring"
      >
        <Badge
          variant={
            status === "expires-soon" || status === "expired"
              ? "destructive"
              : "secondary"
          }
        >
          {status === "expired"
            ? t.rateLimitResetExpired
            : `${t.rateLimitResetExpiresIn} ${formatRateLimitResetTimeRemaining(expiresAt, locale)}`}
        </Badge>
      </TooltipTrigger>
      <TooltipContent>
        {t.rateLimitResetExpiresAt}: {exactTime}
      </TooltipContent>
    </Tooltip>
  )
}

function RateLimitResetExpiries({
  locale,
  credits,
}: {
  locale: Locale
  credits: ProviderRateLimitResetCredit[]
}) {
  if (!credits.length)
    return <span className="text-xs text-muted-foreground">—</span>

  return (
    <div className="flex min-w-28 flex-col items-start gap-1">
      {credits.map((credit) => (
        <RateLimitResetExpiry key={credit.id} locale={locale} credit={credit} />
      ))}
    </div>
  )
}

function ProviderCircuitHistorySheet({
  sdk,
  locale,
  provider,
  onClose,
}: {
  sdk: AuthSdk
  locale: Locale
  provider: Provider
  onClose: () => void
}) {
  const t = copy[locale]
  const { data, error, loading } = useApiQuery<ProviderCircuitEvent[]>(
    sdk,
    `/api/providers/${provider.id}/circuit-events`
  )
  return (
    <Sheet open onOpenChange={(next) => !next && onClose()}>
      <SheetContent className="w-full p-0 sm:max-w-xl">
        <SheetHeader>
          <SheetTitle>
            {t.circuitHistoryTitle}: {provider.name}
          </SheetTitle>
          <SheetDescription>{t.circuitHistoryDescription}</SheetDescription>
        </SheetHeader>
        <ScrollArea className="min-h-0 flex-1 px-4 pb-4">
          {loading ? (
            <div className="flex flex-col gap-3">
              {Array.from({ length: 3 }, (_, index) => (
                <Skeleton key={index} className="h-28 w-full" />
              ))}
            </div>
          ) : error ? (
            <ErrorState message={error} />
          ) : !data?.length ? (
            <EmptyState
              icon={<ShieldCheckIcon />}
              title={t.circuitNeverOpened}
              description={t.circuitHistoryDescription}
            />
          ) : (
            <div className="flex flex-col gap-4">
              {data.map((event, index) => (
                <div key={event.id} className="flex flex-col gap-3">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <Badge
                      variant={event.closed_at ? "secondary" : "destructive"}
                    >
                      {event.closed_at ? (
                        <CheckCircle2Icon />
                      ) : (
                        <ShieldAlertIcon />
                      )}
                      {event.closed_at ? t.circuitRecovered : t.circuitOpen}
                    </Badge>
                    <span className="text-xs text-muted-foreground tabular-nums">
                      {t.circuitOpenedAt}: {formatTime(event.opened_at, locale)}
                    </span>
                  </div>
                  <Definition
                    rows={[
                      [t.circuitReason, event.cause],
                      [
                        t.circuitUntil,
                        formatTime(event.cooldown_until, locale),
                      ],
                      [
                        t.circuitClosedAt,
                        event.closed_at
                          ? formatTime(event.closed_at, locale)
                          : "—",
                      ],
                      [t.circuitResolution, event.resolution || "—"],
                    ]}
                  />
                  <div className="flex flex-col gap-2">
                    <span className="text-xs font-medium text-muted-foreground">
                      {t.circuitRateLimitHeaders}
                    </span>
                    <ScrollArea className="max-h-40 rounded-lg border bg-muted p-3">
                      <pre className="text-xs break-all whitespace-pre-wrap">
                        {formatCircuitRateLimitHeaders(event.rate_limit_json)}
                      </pre>
                    </ScrollArea>
                  </div>
                  {index < data.length - 1 && <Separator />}
                </div>
              ))}
            </div>
          )}
        </ScrollArea>
      </SheetContent>
    </Sheet>
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
  const [path, setPath] = useState<"choose" | "oauth" | "credentials">("choose")

  function close() {
    setPath("choose")
    onOpenChange(false)
  }

  if (path === "credentials") {
    return (
      <ProviderCredentialsDialog
        sdk={sdk}
        locale={locale}
        open={open}
        onClose={close}
        onDone={onDone}
      />
    )
  }

  if (path === "oauth") {
    return (
      <ProviderOAuthDialog
        sdk={sdk}
        locale={locale}
        open={open}
        onClose={close}
        onDone={onDone}
      />
    )
  }

  return (
    <Dialog open={open} onOpenChange={(next) => !next && close()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t.addProviderTitle}</DialogTitle>
          <DialogDescription>{t.addProviderDescription}</DialogDescription>
        </DialogHeader>
        <div className="grid gap-3">
          <Button
            type="button"
            variant="outline"
            className="grid h-auto w-full min-w-0 gap-1 rounded-lg border p-4 text-left whitespace-normal transition-colors hover:bg-muted focus-visible:ring-3 focus-visible:ring-ring"
            onClick={() => setPath("oauth")}
          >
            <span className="flex min-w-0 items-center gap-2 text-sm font-medium">
              <LogInIcon className="size-4" />
              <span className="min-w-0">{t.addProviderOAuth}</span>
            </span>
            <span className="min-w-0 text-sm text-pretty text-muted-foreground">
              {t.addProviderOAuthDescription}
            </span>
          </Button>
          <Button
            type="button"
            variant="outline"
            className="grid h-auto w-full min-w-0 gap-1 rounded-lg border p-4 text-left whitespace-normal transition-colors hover:bg-muted focus-visible:ring-3 focus-visible:ring-ring"
            onClick={() => setPath("credentials")}
          >
            <span className="flex min-w-0 items-center gap-2 text-sm font-medium">
              <KeyRoundIcon className="size-4" />
              <span className="min-w-0">{t.addProviderCredentials}</span>
            </span>
            <span className="min-w-0 text-sm text-pretty text-muted-foreground">
              {t.addProviderCredentialsDescription}
            </span>
          </Button>
        </div>
        <DialogFooter>
          <Button type="button" variant="outline" onClick={close}>
            {t.cancel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function ProviderCredentialsDialog({
  sdk,
  locale,
  open,
  onClose,
  onDone,
}: {
  sdk: AuthSdk
  locale: Locale
  open: boolean
  onClose: () => void
  onDone: () => void
}) {
  const t = copy[locale]
  const [name, setName] = useState("")
  const [access, setAccess] = useState("")
  const [refresh, setRefresh] = useState("")
  const [originator, setOriginator] = useState<ProviderOriginator>("codex_cli_rs")
  const [allowOtherOriginator, setAllowOtherOriginator] = useState(false)
  const [visibility, setVisibility] = useState<ProviderVisibility>("private")
  const [pending, setPending] = useState(false)

  async function direct(event: FormEvent) {
    event.preventDefault()
    if (pending) return
    setPending(true)
    try {
      await api(sdk, "/api/providers", {
        method: "POST",
        body: JSON.stringify({
          name,
          access_key: access,
          refresh_key: refresh,
          originator,
          allow_other_originator: allowOtherOriginator,
          visibility,
        }),
      })
      toast.success(t.providerAdded)
      onClose()
      onDone()
    } catch (cause) {
      toast.error(message(cause, t))
    } finally {
      setPending(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={(next) => !next && !pending && onClose()}>
      <DialogContent>
        <form onSubmit={direct}>
          <DialogHeader>
            <DialogTitle>{t.addProviderCredentialsTitle}</DialogTitle>
            <DialogDescription>
              {t.addProviderCredentialsTitleDescription}
            </DialogDescription>
          </DialogHeader>
          <FieldGroup className="pt-4">
            <ProviderNameField
              id="provider-name"
              value={name}
              onChange={setName}
              locale={locale}
            />
            <Field>
              <FieldLabel htmlFor="access-key">{t.accessKey}</FieldLabel>
              <Input
                id="access-key"
                type="password"
                autoComplete="off"
                value={access}
                onChange={(event) => setAccess(event.target.value)}
                required
              />
              <FieldDescription>{t.accessClaimHelp}</FieldDescription>
            </Field>
            <Field>
              <FieldLabel htmlFor="refresh-key">{t.refreshKey}</FieldLabel>
              <Input
                id="refresh-key"
                type="password"
                autoComplete="off"
                value={refresh}
                onChange={(event) => setRefresh(event.target.value)}
                required
              />
            </Field>
            <OriginatorField
              id="provider-originator"
              value={originator}
              onChange={setOriginator}
              locale={locale}
            />
            <OriginatorFallbackField
              id="provider-originator-fallback"
              checked={allowOtherOriginator}
              onChange={setAllowOtherOriginator}
              disabled={pending}
              locale={locale}
            />
            <ProviderVisibilityField
              id="provider-visibility"
              value={visibility}
              onChange={setVisibility}
              locale={locale}
            />
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                disabled={pending}
                onClick={onClose}
              >
                {t.cancel}
              </Button>
              <Button
                type="submit"
                disabled={pending || !access.trim() || !refresh.trim()}
              >
                {pending && <Spinner data-icon="inline-start" />}
                {t.importCredentials}
              </Button>
            </DialogFooter>
          </FieldGroup>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function ProviderOAuthDialog({
  sdk,
  locale,
  open,
  onClose,
  onDone,
}: {
  sdk: AuthSdk
  locale: Locale
  open: boolean
  onClose: () => void
  onDone: () => void
}) {
  const t = copy[locale]
  const [oauth, setOauth] = useState<OAuthFlow | null>(null)
  const [name, setName] = useState("")
  const [originator, setOriginator] = useState<ProviderOriginator>("codex_cli_rs")
  const [allowOtherOriginator, setAllowOtherOriginator] = useState(false)
  const [visibility, setVisibility] = useState<ProviderVisibility>("private")
  const [callbackUrl, setCallbackUrl] = useState("")
  const [pending, setPending] = useState(false)

  async function startOAuth() {
    setPending(true)
    try {
      const value = await api<OAuthFlow>(sdk, "/api/oauth/start", {
        method: "POST",
        body: JSON.stringify({ originator }),
      })
      setOauth(value)
      window.open(value.authorize_url, "_blank", "noopener,noreferrer")
    } catch (cause) {
      toast.error(message(cause, t))
    } finally {
      setPending(false)
    }
  }

  async function completeOAuth() {
    if (!oauth) return
    setPending(true)
    try {
      await api(sdk, "/api/oauth/complete", {
        method: "POST",
        body: JSON.stringify({
          callback_url: callbackUrl,
          name,
          allow_other_originator: allowOtherOriginator,
          visibility,
        }),
      })
      toast.success(t.oauthProviderAdded)
      setOauth(null)
      onClose()
      onDone()
    } catch (cause) {
      toast.error(message(cause, t))
    } finally {
      setPending(false)
    }
  }

  function cancel() {
    if (pending) return
    setOauth(null)
    onClose()
  }

  return (
    <Dialog
      open={open}
      disablePointerDismissal={Boolean(oauth)}
      onOpenChange={(next) => !next && !oauth && !pending && onClose()}
    >
      <DialogContent showCloseButton={!oauth}>
        <form
          onSubmit={(event) => {
            event.preventDefault()
            if (oauth) void completeOAuth()
            else void startOAuth()
          }}
        >
          <DialogHeader>
            <DialogTitle>{t.addProviderOAuthTitle}</DialogTitle>
            <DialogDescription>
              {t.addProviderOAuthInProgressDescription}
            </DialogDescription>
          </DialogHeader>
          <FieldGroup className="pt-4">
            <ProviderNameField
              id="oauth-provider-name"
              value={name}
              onChange={setName}
              locale={locale}
            />
            <OriginatorField
              id="oauth-provider-originator"
              value={originator}
              onChange={setOriginator}
              disabled={oauth !== null}
              locale={locale}
            />
            <OriginatorFallbackField
              id="oauth-provider-originator-fallback"
              checked={allowOtherOriginator}
              onChange={setAllowOtherOriginator}
              disabled={pending}
              locale={locale}
            />
            <ProviderVisibilityField
              id="oauth-provider-visibility"
              value={visibility}
              onChange={setVisibility}
              locale={locale}
            />
            {oauth && (
              <Field>
                <FieldLabel htmlFor="oauth-callback-url">
                  {t.callbackUrl}
                </FieldLabel>
                <Textarea
                  id="oauth-callback-url"
                  autoComplete="off"
                  spellCheck={false}
                  value={callbackUrl}
                  onChange={(event) => setCallbackUrl(event.target.value)}
                  required
                />
                <FieldDescription>
                  {t.callbackUrlHelp} {t.oauthStateHelp}
                </FieldDescription>
              </Field>
            )}
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                disabled={pending}
                onClick={cancel}
              >
                {t.cancel}
              </Button>
              <Button
                type="submit"
                disabled={pending || (oauth !== null && !callbackUrl.trim())}
              >
                {pending && <Spinner data-icon="inline-start" />}
                {oauth ? t.completeOauth : t.startOauth}
              </Button>
            </DialogFooter>
          </FieldGroup>
        </form>
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

function OriginatorFallbackField({
  id, checked, onChange, disabled, locale,
}: {
  id: string
  checked: boolean
  onChange: (checked: boolean) => void
  disabled?: boolean
  locale: Locale
}) {
  const t = copy[locale]
  return (
    <Field>
      <div className="flex items-center justify-between gap-3">
        <FieldLabel htmlFor={id}>{t.allowOtherOriginator}</FieldLabel>
        <Switch id={id} checked={checked} disabled={disabled} onCheckedChange={onChange} aria-describedby={`${id}-help`} />
      </div>
      <FieldDescription id={`${id}-help`}>{t.allowOtherOriginatorHelp}</FieldDescription>
    </Field>
  )
}

function OriginatorField({
  id,
  value,
  onChange,
  disabled,
  locale,
}: {
  id: string
  value: ProviderOriginator
  onChange: (value: ProviderOriginator) => void
  disabled?: boolean
  locale: Locale
}) {
  const t = copy[locale]
  return (
    <Field>
      <FieldLabel htmlFor={id}>{t.providerOriginator}</FieldLabel>
      <Select
        value={value}
        disabled={disabled}
        onValueChange={(next) => onChange(next as ProviderOriginator)}
      >
        <SelectTrigger id={id}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            {providerOriginators.map((option) => (
              <SelectItem key={option} value={option}>
                {providerOriginatorLabel(option, t)}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
      <FieldDescription>
        {disabled ? t.providerOriginatorLockedHelp : t.providerOriginatorHelp}
      </FieldDescription>
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
  const [interceptDegradation, setInterceptDegradation] = useState(false)
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
          intercept_degradation: interceptDegradation,
        }),
      })
      setSecret(value.secret)
      setName("")
      setRequestArchive(false)
      setInterceptDegradation(false)
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
  async function updateDegradationInterception(id: string, checked: boolean) {
    try {
      await api(sdk, `/api/consumers/${id}`, {
        method: "PATCH",
        body: JSON.stringify({ intercept_degradation: checked }),
      })
      refreshConsumers()
      toast.success(t.interceptDegradationUpdated)
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
      const value = await api<{ secret: string }>(sdk, `/api/consumers/${id}/rotate`, {
        method: "POST",
      })
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
                    <TableHead>{t.interceptDegradation}</TableHead>
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
                          aria-label={`${t.interceptDegradation}: ${consumer.name}`}
                          checked={consumer.intercept_degradation}
                          onCheckedChange={(checked) =>
                            void updateDegradationInterception(
                              consumer.id,
                              checked
                            )
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
              <Field orientation="horizontal">
                <FieldContent>
                  <FieldLabel htmlFor="consumer-intercept-degradation">
                    {t.interceptDegradation}
                  </FieldLabel>
                  <FieldDescription>{t.interceptDegradationHelp}</FieldDescription>
                </FieldContent>
                <Switch
                  id="consumer-intercept-degradation"
                  checked={interceptDegradation}
                  onCheckedChange={setInterceptDegradation}
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
        onOpenChange={(next) =>
          !next && !rotateId && setRotateTarget(null)
        }
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
          <UsageDimensionCell
            dimension={dimension}
            row={row.original.values}
          />
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
  const { data, error, loading, refreshing, reload } = useApiQuery<AuditPageResponse>(
    sdk,
    query
  )
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
                      <TableHead>{t.downstreamUserAgent}</TableHead>
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
                      <TableHead>{t.sessionId}</TableHead>
                      <TableHead>{t.status}</TableHead>
                      <TableHead>{t.usage}</TableHead>
                      <TableHead className="text-right">{t.actions}</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {rows.map((row) => (
                      <TableRow key={row.id}>
                        <TableCell>
                          <LinkitUserInfo
                            userId={row.user_id}
                          />
                        </TableCell>
                        <TableCell className="font-medium">
                          {row.consumer_name}
                        </TableCell>
                        <TableCell className="whitespace-nowrap">
                          {formatTime(row.created_at, locale)}
                        </TableCell>
                        <TableCell
                          className={
                            auditTurnStateMayIndicateDowngrade(row)
                              ? "bg-amber-500/10"
                              : undefined
                          }
                        >
                          <div className="flex min-w-52 flex-col gap-1">
                            <div className="flex flex-wrap items-center gap-1">
                              <code>{row.model || row.upstream_model || "—"}</code>
                              {auditModelDiffers(row) && (
                                <>
                                  <span
                                    className="text-muted-foreground"
                                    aria-hidden="true"
                                  >
                                    →
                                  </span>
                                  <code
                                    className="rounded bg-destructive/10 px-1.5 py-0.5 font-medium text-destructive"
                                    title={t.upstreamModel}
                                  >
                                    {row.upstream_model}
                                  </code>
                                </>
                              )}
                            </div>
                            {(row.reasoning_effort ||
                              row.fast_mode ||
                              auditModelDiffers(row) ||
                              auditTurnStateMayIndicateDowngrade(row)) && (
                              <div className="flex flex-wrap gap-1">
                                {auditModelDiffers(row) && (
                                  <Badge
                                    variant="destructive"
                                    title={t.modelMismatchHint}
                                  >
                                    {t.modelDowngraded}
                                  </Badge>
                                )}
                                {auditTurnStateMayIndicateDowngrade(row) && (
                                  <Badge
                                    variant="outline"
                                    className="border-amber-500/40 bg-amber-500/10 text-amber-700 dark:text-amber-300"
                                    title={t.codexTurnStateLengthWarning}
                                  >
                                    {t.codexTurnStateLengthWarning}
                                  </Badge>
                                )}
                                {row.reasoning_effort && (
                                  <Badge variant="secondary">
                                    {t.reasoningEffort} · {row.reasoning_effort}
                                  </Badge>
                                )}
                                {row.fast_mode && (
                                  <Badge variant="secondary">Fast</Badge>
                                )}
                              </div>
                            )}
                          </div>
                        </TableCell>
                        <TableCell>
                          <span
                            className="block max-w-64 truncate"
                            title={row.downstream_user_agent || undefined}
                          >
                            {row.downstream_user_agent || "—"}
                          </span>
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
                          {row.session_id ? (
                            <CopyableIdentifier
                              value={row.session_id}
                              label={t.sessionId}
                              copyLabel={t.copySessionId}
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

function auditModelDiffers(data: Pick<Audit, "model" | "upstream_model">) {
  return Boolean(
    data.model && data.upstream_model && data.model !== data.upstream_model
  )
}

function auditTurnStateMayIndicateDowngrade(
  data: Pick<Audit, "codex_turn_state_length">
) {
  return data.codex_turn_state_length === 312
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
        {auditModelDiffers(data) && (
          <Alert variant="destructive">
            <ShieldAlertIcon />
            <AlertTitle>{labels.modelDowngraded}</AlertTitle>
            <AlertDescription>
              {labels.modelMismatchHint} {data.model} → {data.upstream_model}
            </AlertDescription>
          </Alert>
        )}
        <Definition
          rows={[
            [labels.requestId, data.request_id],
            [labels.sessionId, data.session_id || "—"],
            [labels.threadId, data.thread_id || "—"],
            [labels.consumer, data.consumer_name],
            [labels.userId, data.user_id],
            [labels.provider, data.provider_name || data.provider_id || "—"],
            [labels.downstreamModel, data.model || "—"],
            [labels.upstreamModel, data.upstream_model || "—"],
            [labels.downstreamUserAgent, data.downstream_user_agent || "—"],
            [labels.downstreamOriginator, data.downstream_originator || "—"],
            [labels.upstreamOriginator, data.upstream_originator || "—"],
            [labels.originatorFallbackReason, data.originator_fallback_reason || "—"],
            [labels.reasoningEffort, data.reasoning_effort || "—"],
            [labels.fastMode, data.fast_mode ? "Fast" : "—"],
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
              labels.codexTurnStateLength,
              data.codex_turn_state_length == null
                ? "—"
                : data.codex_turn_state_length.toLocaleString(locale),
            ],
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
          requestBody={data.request_body}
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
                  <Badge variant="outline">{labels.headerSnapshotMissing}</Badge>
                )}
              </div>
            </TableHead>
            <TableHead>
              <div className="flex items-center gap-2">
                {rightLabel}
                {!rightAvailable && (
                  <Badge variant="outline">{labels.headerSnapshotMissing}</Badge>
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
  requestBody,
  truncated = false,
}: {
  title: string
  value?: string
  requestBody?: string
  truncated?: boolean
}) {
  const t = currentMessages()
  const images = auditImageResponses(value, requestBody)
  return (
    <section className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <h3 className="text-sm font-medium">{title}</h3>
        {truncated && <Badge variant="secondary">{t.previewTruncated}</Badge>}
      </div>
      {images.length > 0 && (
        <div className="grid gap-4 sm:grid-cols-2">
          {images.map((image, index) => (
            <figure key={image.src} className="min-w-0 rounded-md border p-3">
              <img
                className="max-h-96 max-w-full rounded-md border bg-muted object-contain"
                src={image.src}
                alt={`${t.responseImages} ${index + 1}`}
              />
              {image.revisedPrompt && (
                <figcaption className="mt-3 flex flex-col gap-1">
                  <span className="text-xs font-medium text-muted-foreground">
                    {t.revisedPrompt}
                  </span>
                  <p className="text-sm leading-6 break-words whitespace-pre-wrap">
                    {image.revisedPrompt}
                  </p>
                </figcaption>
              )}
            </figure>
          ))}
        </div>
      )}
      <pre className="max-h-80 overflow-auto rounded-md border bg-muted p-3 font-mono text-xs break-all whitespace-pre-wrap">
        {value || "—"}
      </pre>
    </section>
  )
}

function PlatformCapacity({ sdk, locale }: { sdk: AuthSdk; locale: Locale }) {
  const t = copy[locale]
  const query = useQuery({
    queryKey: ["/api/provider-capacity"],
    queryFn: ({ signal }) =>
      api<ProviderCapacity>(sdk, "/api/provider-capacity", { signal }),
    refetchInterval: 60_000,
  })
  const capacity = query.data
  const hasCapacity = Boolean(capacity?.included_provider_count)
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t.platformCapacityTitle}</CardTitle>
        <CardDescription>
          {query.error ? message(query.error, t) : t.platformCapacityDescription}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <dl className="grid gap-px overflow-hidden rounded-lg border bg-border sm:grid-cols-2">
          <div className="flex flex-col gap-1 bg-background p-4">
            <dt>{t.platformPlusCapacity}</dt>
            <dd className="text-2xl font-semibold tabular-nums">
              {hasCapacity
                ? formatPlusEquivalentCapacity(
                    capacity?.plus_equivalent_remaining_basis_points ?? 0,
                    locale
                  )
                : "—"}
            </dd>
          </div>
          <div className="flex flex-col gap-1 bg-background p-4">
            <dt>{t.platformCapacitySampledAt}</dt>
            <dd className="text-sm font-medium tabular-nums">
              {capacity?.last_sampled_at
                ? formatTime(capacity.last_sampled_at, locale)
                : t.platformCapacityPending}
            </dd>
          </div>
        </dl>
        <PlatformCapacityTrend history={capacity?.history ?? []} locale={locale} />
      </CardContent>
    </Card>
  )
}

function PlatformCapacityTrend({
  history,
  locale,
}: {
  history: ProviderCapacityHistoryPoint[]
  locale: Locale
}) {
  const t = copy[locale]
  if (!history.length) {
    return (
      <>
        <Separator className="mt-4" />
        <section className="pt-4" aria-label={t.platformCapacityTrend}>
          <p className="text-sm font-medium">{t.platformCapacityTrend}</p>
          <p className="mt-1 text-sm text-muted-foreground">
            {t.platformCapacityTrendPending}
          </p>
        </section>
      </>
    )
  }
  const chartConfig = {
    remaining: {
      label: t.platformPlusCapacity,
      color: "var(--chart-2)",
    },
  } satisfies ChartConfig
  const chartData = history.map((point) => ({
    sampled_at: point.sampled_at,
    remaining: point.plus_equivalent_remaining_basis_points / 100,
  }))
  return (
    <>
      <Separator className="mt-4" />
      <figure className="pt-4">
        <figcaption className="flex items-baseline justify-between gap-3">
          <div>
            <p className="text-sm font-medium">{t.platformCapacityTrend}</p>
            <p className="mt-1 text-xs text-muted-foreground">
              {t.platformCapacityTrendDescription}
            </p>
          </div>
          <span className="shrink-0 text-xs text-muted-foreground">
            {t.last7Days}
          </span>
        </figcaption>
        <ChartContainer
          config={chartConfig}
          className="mt-3 aspect-auto h-40 w-full"
        >
          <LineChart accessibilityLayer data={chartData}>
            <CartesianGrid vertical={false} />
            <ChartTooltip
              labelFormatter={(value) => formatTime(Number(value), locale)}
              formatter={(value) => [
                `${Number(value).toLocaleString(locale, { maximumFractionDigits: 2 })}%`,
                t.platformPlusCapacity,
              ]}
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
              dataKey="sampled_at"
              minTickGap={36}
              tickFormatter={(value) =>
                formatCapacityTrendTime(Number(value), locale)
              }
              tickLine={false}
              tickMargin={8}
            />
            <YAxis
              axisLine={false}
              tickFormatter={(value) => `${Number(value).toLocaleString(locale)}%`}
              tickLine={false}
              width={72}
            />
            <Line
              dataKey="remaining"
              dot={false}
              stroke="var(--color-remaining)"
              strokeWidth={2}
              type="monotone"
            />
          </LineChart>
        </ChartContainer>
      </figure>
    </>
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
              <SelectTrigger
                aria-label={`${t.role}: ${item.id}`}
              >
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
              [t.upstreamOpenaiBeta, data.upstream_openai_beta || "—"],
              [
                t.bodyLimit,
                `${data.response_body_limit} / ${data.image_body_limit} / ${data.audio_body_limit} bytes`,
              ],
              [t.affinityTtl, `${data.affinity_ttl_seconds} s`],
            ]}
          />
        </CardContent>
      </Card>
      {(user.role === "root" || user.role === "admin") && (
        <>
          <UpstreamUserAgentSettings
            sdk={sdk}
            locale={locale}
            initial={
              data.upstream_user_agents || {
                codex_cli_rs: data.upstream_user_agent,
                pi: null,
                opencode: null,
              }
            }
          />
          <ExperimentalTurnState312FilterSettings
            sdk={sdk}
            locale={locale}
            initial={Boolean(data.experimental_filter_codex_turn_state_312)}
          />
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

function UpstreamUserAgentSettings({
  sdk,
  locale,
  initial,
}: {
  sdk: AuthSdk
  locale: Locale
  initial: {
    codex_cli_rs?: string | null
    pi?: string | null
    opencode?: string | null
  }
}) {
  const t = copy[locale]
  const queryClient = useQueryClient()
  const [values, setValues] = useState({
    codex_cli_rs: initial.codex_cli_rs || "",
    pi: initial.pi || "",
    opencode: initial.opencode || "",
  })
  const [pending, setPending] = useState(false)
  useEffect(
    () =>
      setValues({
        codex_cli_rs: initial.codex_cli_rs || "",
        pi: initial.pi || "",
        opencode: initial.opencode || "",
      }),
    [initial]
  )

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (pending) return
    setPending(true)
    try {
      for (const originator of ["codex_cli_rs", "pi", "opencode"] as const) {
        await api(sdk, "/api/settings/upstream-user-agent", {
          method: "PATCH",
          body: JSON.stringify({
            originator,
            user_agent: values[originator],
          }),
        })
      }
      await queryClient.invalidateQueries({ queryKey: ["/api/settings"] })
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
        <CardTitle>{t.upstreamUserAgent}</CardTitle>
        <CardDescription>{t.upstreamUserAgentHint}</CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={save}>
          <FieldGroup>
            {(
              [
                ["codex_cli_rs", t.upstreamUserAgentCodex],
                ["pi", t.upstreamUserAgentPi],
                ["opencode", t.upstreamUserAgentOpencode],
              ] as const
            ).map(([originator, label]) => (
              <Field data-disabled={pending} key={originator}>
                <FieldLabel htmlFor={`settings-upstream-user-agent-${originator}`}>
                  {label}
                </FieldLabel>
                <Input
                  id={`settings-upstream-user-agent-${originator}`}
                  value={values[originator]}
                  onChange={(event) =>
                    setValues((current) => ({
                      ...current,
                      [originator]: event.target.value,
                    }))
                  }
                  maxLength={1024}
                  disabled={pending}
                />
              </Field>
            ))}
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

function ExperimentalTurnState312FilterSettings({
  sdk,
  locale,
  initial,
}: {
  sdk: AuthSdk
  locale: Locale
  initial: boolean
}) {
  const t = copy[locale]
  const queryClient = useQueryClient()
  const [enabled, setEnabled] = useState(initial)
  const [pending, setPending] = useState(false)
  useEffect(() => setEnabled(initial), [initial])

  async function update(enabled: boolean) {
    if (pending) return
    setPending(true)
    try {
      await api(sdk, "/api/settings/experimental-turn-state-312-filter", {
        method: "PATCH",
        body: JSON.stringify({ enabled }),
      })
      setEnabled(enabled)
      await queryClient.invalidateQueries({ queryKey: ["/api/settings"] })
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
        <CardTitle>{t.experimentalTurnState312Filter}</CardTitle>
        <CardDescription>
          {t.experimentalTurnState312FilterHint}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <Field orientation="horizontal" data-disabled={pending}>
          <FieldContent>
            <FieldLabel htmlFor="settings-experimental-turn-state-312-filter">
              {t.experimentalTurnState312Filter}
            </FieldLabel>
            <FieldDescription>
              {t.experimentalTurnState312FilterHint}
            </FieldDescription>
          </FieldContent>
          <Switch
            id="settings-experimental-turn-state-312-filter"
            checked={enabled}
            disabled={pending}
            onCheckedChange={update}
          />
        </Field>
      </CardContent>
    </Card>
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
  const query = useApiQuery<MidasSettingsData>(
    sdk,
    "/api/payments/settings"
  )
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
              <FieldLabel htmlFor="midas-api-base">
                {t.midasApiBase}
              </FieldLabel>
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
  const [settings, setSettings] = useState(initial)
  const [pending, setPending] = useState(false)
  function update<K extends keyof SettingsData>(
    key: K,
    value: SettingsData[K]
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
            </Field>
            <Field>
              <FieldLabel htmlFor="settings-upstream-openai-beta">
                {t.upstreamOpenaiBeta}
              </FieldLabel>
              <Input
                id="settings-upstream-openai-beta"
                value={settings.upstream_openai_beta || ""}
                onChange={(event) =>
                  update("upstream_openai_beta", event.target.value)
                }
                placeholder="responses=experimental"
                aria-describedby="settings-upstream-openai-beta-hint"
              />
              <p
                id="settings-upstream-openai-beta-hint"
                className="text-sm text-muted-foreground"
              >
                {t.upstreamOpenaiBetaHint}
              </p>
            </Field>
            <Field>
              <FieldLabel htmlFor="settings-image-model">
                {t.imageHostModel}
              </FieldLabel>
              <Input
                id="settings-image-model"
                value={settings.image_host_model}
                onChange={(event) =>
                  update("image_host_model", event.target.value)
                }
                required
              />
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
            <Field>
              <FieldLabel htmlFor="settings-authorize-url">
                {t.oauthAuthorizeUrl}
              </FieldLabel>
              <Input
                id="settings-authorize-url"
                type="url"
                value={settings.oauth_authorize_url}
                onChange={(event) =>
                  update("oauth_authorize_url", event.target.value)
                }
                required
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="settings-token-url">
                {t.oauthTokenUrl}
              </FieldLabel>
              <Input
                id="settings-token-url"
                type="url"
                value={settings.oauth_token_url}
                onChange={(event) =>
                  update("oauth_token_url", event.target.value)
                }
                required
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="settings-redirect-uri">
                {t.oauthRedirectUri}
              </FieldLabel>
              <Input
                id="settings-redirect-uri"
                type="url"
                value={settings.oauth_redirect_uri}
                onChange={(event) =>
                  update("oauth_redirect_uri", event.target.value)
                }
                required
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="settings-client-id">
                {t.oauthClientId}
              </FieldLabel>
              <Input
                id="settings-client-id"
                value={settings.oauth_client_id}
                onChange={(event) =>
                  update("oauth_client_id", event.target.value)
                }
                required
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
                <FieldLabel htmlFor="settings-image-limit">
                  {t.imageLimit}
                </FieldLabel>
                <Input
                  id="settings-image-limit"
                  type="number"
                  min={1024}
                  max={16777216}
                  value={settings.image_body_limit}
                  onChange={(event) =>
                    update("image_body_limit", event.target.valueAsNumber)
                  }
                  required
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="settings-audio-limit">
                  {t.audioLimit}
                </FieldLabel>
                <Input
                  id="settings-audio-limit"
                  type="number"
                  min={1048576}
                  max={2000000000}
                  value={settings.audio_body_limit}
                  onChange={(event) =>
                    update("audio_body_limit", event.target.valueAsNumber)
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
                  {locale === "zh"
                    ? "请求/响应诊断记录保留天数"
                    : "Request/response diagnostic retention (days)"}
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
                      <LinkitUserInfo
                        userId={row.admin_user_id}
                      />
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

type ProviderAuditMetricKey =
  | "failure_rate"
  | "requests"
  | "input_tokens"

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
            labelFormatter={(value) =>
              formatAuditHour(Number(value), locale)
            }
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
            tickFormatter={(value) =>
              formatAuditHour(Number(value), locale)
            }
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
                      key={[row.hour_start, row.provider_id ?? "unknown", row.model].join("-")}
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

type ModelDowngradeAuditResponse = {
  period: UsagePeriod
  since: number
  until: number
  rows: ModelDowngradeRow[]
}

function ModelDowngradeAuditPage({
  sdk,
  locale,
}: {
  sdk: AuthSdk
  locale: Locale
}) {
  const t = copy[locale]
  const [period, setPeriod] = useState<UsagePeriod>("7d")
  const [providerFilter, setProviderFilter] = useState("all")
  const { data, error, loading, refreshing, reload } =
    useApiQuery<ModelDowngradeAuditResponse>(
      sdk,
      "/api/model-downgrade-audit?period=" + period
    )
  const rows = useMemo(() => data?.rows ?? [], [data])
  const providerOptions = useMemo(
    () =>
      [
        ...new Map(
          rows.map((row) => [
            row.provider_id ?? unknownProviderId,
            row.provider_name ?? t.providerAuditUnknownProvider,
          ])
        ),
      ].sort((left, right) => left[1].localeCompare(right[1], locale)),
    [locale, rows, t.providerAuditUnknownProvider]
  )
  const flows = useMemo(
    () => modelDowngradeFlows(rows, providerFilter),
    [providerFilter, rows]
  )
  const sankey = useMemo(
    () => modelDowngradeSankey(flows, t.providerAuditUnknownProvider),
    [flows, t.providerAuditUnknownProvider]
  )
  const ratePoints = useMemo(
    () =>
      data
        ? modelDowngradeRatePoints(rows, providerFilter, data.since, data.until)
        : [],
    [data, providerFilter, rows]
  )
  const totals = useMemo(
    () =>
      flows.reduce(
        (summary, flow) => ({
          requests: summary.requests + flow.requests,
          downgraded:
            summary.downgraded + (isModelDowngrade(flow) ? flow.requests : 0),
        }),
        { requests: 0, downgraded: 0 }
      ),
    [flows]
  )
  const downgradeRate = totals.requests
    ? totals.downgraded / totals.requests
    : 0
  const chartConfig = {
    downgrade_rate: {
      label: t.modelDowngradeRate,
      color: "var(--destructive)",
    },
  } satisfies ChartConfig

  if (loading) return <LoadingTable />
  if (error) return <ErrorState message={error} />
  return (
    <div className="flex flex-col gap-5">
      <Card>
        <CardHeader className="flex-row items-start justify-between gap-3">
          <div>
            <CardTitle>{t.modelDowngradeTitle}</CardTitle>
            <CardDescription>{t.modelDowngradeDescription}</CardDescription>
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
          <div className="grid gap-px overflow-hidden rounded-lg border bg-border sm:grid-cols-3">
            <div className="bg-background p-4">
              <dt>{t.modelDowngradeAuditedRequests}</dt>
              <dd className="mt-1 text-2xl font-semibold tabular-nums">
                {totals.requests.toLocaleString(locale)}
              </dd>
            </div>
            <div className="bg-background p-4">
              <dt>{t.modelDowngradeDowngradedRequests}</dt>
              <dd className="mt-1 text-2xl font-semibold tabular-nums">
                {totals.downgraded.toLocaleString(locale)}
              </dd>
            </div>
            <div className="bg-background p-4">
              <dt>{t.modelDowngradeRate}</dt>
              <dd className="mt-1 text-2xl font-semibold tabular-nums">
                {formatPercent(downgradeRate * 100, locale)}
              </dd>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-3 border-t pt-4">
            <Tabs
              value={period}
              onValueChange={(value) => value && setPeriod(value as UsagePeriod)}
            >
              <TabsList aria-label={t.modelDowngradePeriod}>
                <TabsTrigger value="7d">{t.last7Days}</TabsTrigger>
                <TabsTrigger value="24h">{t.last24Hours}</TabsTrigger>
              </TabsList>
            </Tabs>
            <UsageSelect
              id="model-downgrade-audit-provider-filter"
              label={t.provider}
              value={providerFilter}
              onValueChange={setProviderFilter}
              allLabel={t.providerAuditAllProviders}
              options={providerOptions}
            />
          </div>
          {!flows.length ? (
            <EmptyState
              icon={<WorkflowIcon />}
              title={t.modelDowngradeNoData}
              description={t.modelDowngradeNoDataDescription}
            />
          ) : (
            <div className="flex flex-col gap-8">
              <figure>
                <figcaption className="mb-3 text-sm font-medium">
                  {t.modelDowngradeFlowChart}
                </figcaption>
                <ModelDowngradeSankey
                  config={chartConfig}
                  downgradedLabel={t.modelDowngradeDowngraded}
                  links={sankey.links}
                  locale={locale}
                  nodes={sankey.nodes}
                  requestsLabel={t.requests}
                />
                <p className="mt-3 text-xs text-muted-foreground">
                  {t.modelDowngradeFlowDescription}
                </p>
              </figure>
              <figure>
                <figcaption className="mb-3 text-sm font-medium">
                  {t.modelDowngradeRateChart}
                </figcaption>
                <ModelDowngradeRateChart
                  config={chartConfig}
                  locale={locale}
                  points={ratePoints}
                  rateLabel={t.modelDowngradeRate}
                />
                <p className="mt-3 text-xs text-muted-foreground">
                  {t.modelDowngradeRateDescription}
                </p>
              </figure>
            </div>
          )}
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>{t.modelDowngradeTitle}</CardTitle>
          <CardDescription>{t.modelDowngradeTableDescription}</CardDescription>
        </CardHeader>
        <CardContent>
          {!flows.length ? (
            <EmptyState
              icon={<WorkflowIcon />}
              title={t.modelDowngradeNoData}
              description={t.modelDowngradeNoDataDescription}
            />
          ) : (
            <DataTable>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>{t.provider}</TableHead>
                    <TableHead>{t.modelDowngradeUpstreamModel}</TableHead>
                    <TableHead>{t.modelDowngradeDownstreamModel}</TableHead>
                    <TableHead className="text-right">{t.requests}</TableHead>
                    <TableHead>{t.status}</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {flows.map((flow) => (
                    <TableRow
                      key={[
                        flow.provider_id ?? unknownProviderId,
                        flow.upstream_model,
                        flow.downstream_model,
                      ].join("-")}
                    >
                      <TableCell>
                        <code>
                          {flow.provider_name ?? t.providerAuditUnknownProvider}
                        </code>
                      </TableCell>
                      <TableCell>
                        <code>{flow.upstream_model}</code>
                      </TableCell>
                      <TableCell>
                        <code>{flow.downstream_model}</code>
                      </TableCell>
                      <TableCell className="text-right tabular-nums">
                        {flow.requests.toLocaleString(locale)}
                      </TableCell>
                      <TableCell>
                        {isModelDowngrade(flow) ? (
                          <Badge variant="destructive">
                            {t.modelDowngradeDowngraded}
                          </Badge>
                        ) : (
                          <Badge variant="outline">
                            {t.modelDowngradeConsistent}
                          </Badge>
                        )}
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
function ProviderCircuitStatus({
  event,
  locale,
  provider,
  onOpenHistory,
}: {
  event?: ProviderCircuitEvent
  locale: Locale
  provider: Provider
  onOpenHistory: () => void
}) {
  const t = copy[locale]
  const open = provider.status === "cooldown"
  const deadline = provider.cooldown_until ?? event?.cooldown_until
  return (
    <div className="flex min-w-0 flex-col items-start gap-1">
      <Badge variant={open ? "destructive" : "secondary"}>
        {open ? <ShieldAlertIcon /> : <CheckCircle2Icon />}
        {open
          ? t.circuitOpen
          : event?.closed_at
            ? t.circuitRecovered
            : t.circuitNeverOpened}
      </Badge>
      {open ? (
        <span className="text-xs text-muted-foreground tabular-nums">
          {t.circuitUntil}: {formatTime(deadline, locale)}
        </span>
      ) : event?.closed_at ? (
        <span className="text-xs text-muted-foreground tabular-nums">
          {t.circuitClosedAt}: {formatTime(event.closed_at, locale)}
        </span>
      ) : null}
      {open && provider.last_error && (
        <span className="text-xs break-words text-muted-foreground">
          {provider.last_error}
        </span>
      )}
      <Button size="sm" variant="link" onClick={onOpenHistory}>
        <ScrollTextIcon data-icon="inline-start" />
        {t.circuitHistory}
      </Button>
    </div>
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
    const reason = (cause as Error & { reason?: string }).reason
    if (reason === "image_data_missing") {
      const detail = cause.message
        .replace(
          /^Image generation failed: the upstream provider did not return image data\.\s*/i,
          ""
        )
        .trim()
      return `${t.imageGenerationFailed} ${t.imageGenerationRetry}${detail ? ` ${t.imageGenerationDetail}: ${detail}` : ""}`
    }
    return cause.message
  }
  return t.unknownError
}
function isAbortError(cause: unknown) {
  return cause instanceof DOMException && cause.name === "AbortError"
}
function formatTime(timestamp: RateLimitResetTimestamp, locale: Locale) {
  const seconds = rateLimitResetTimestampSeconds(timestamp)
  return seconds === undefined
    ? "—"
    : new Intl.DateTimeFormat(locale === "zh" ? "zh-CN" : "en-US", {
        dateStyle: "short",
        timeStyle: "medium",
      }).format(seconds * 1000)
}
function formatCapacityTrendTime(timestamp: number, locale: Locale) {
  return new Intl.DateTimeFormat(locale === "zh" ? "zh-CN" : "en-US", {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(timestamp * 1000)
}
function formatCircuitRateLimitHeaders(value: string) {
  try {
    return JSON.stringify(JSON.parse(value), null, 2)
  } catch {
    return value
  }
}
function formatRateLimitResetTimeRemaining(timestamp: number, locale: Locale) {
  const seconds = Math.max(0, timestamp - Math.floor(Date.now() / 1000))
  const days = Math.floor(seconds / 86400)
  const hours = Math.floor((seconds % 86400) / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  return locale === "zh"
    ? `${days ? `${days}天` : ""}${hours}小时${minutes}分`
    : `${days ? `${days}d ` : ""}${hours}h ${minutes}m`
}
function formatRateLimitResetExpiryTime(timestamp: number, locale: Locale) {
  return new Intl.DateTimeFormat(locale === "zh" ? "zh-CN" : "en-US", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    timeZone: "Asia/Shanghai",
    timeZoneName: "short",
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

function inputOutputRatio(inputTokens: number, outputTokens: number, locale: Locale) {
  if (inputTokens <= 0 && outputTokens <= 0) return "—"
  if (outputTokens <= 0) return "∞:1"
  return `${(inputTokens / outputTokens).toLocaleString(locale, {
    maximumFractionDigits: 2,
  })}:1`
}

function usageEmail(usage: ProviderUsage | undefined) {
  return usage?.email ?? usage?.account_email ?? usage?.account?.email
}
function quotaPercent(window: UsageWindow | undefined) {
  return typeof window?.used_percent === "number"
    ? Math.max(0, Math.min(100, 100 - window.used_percent))
    : undefined
}
function formatPlusEquivalentCapacity(
  remainingBasisPoints: number,
  locale: Locale
) {
  return `${(remainingBasisPoints / 100).toLocaleString(locale, {
    maximumFractionDigits: 2,
  })}%`
}
function remainingPercent(window: UsageWindow | undefined) {
  const percent = quotaPercent(window)
  return percent === undefined ? "—" : `${percent.toFixed(1)}%`
}
function quotaReset(window: UsageWindow | undefined, locale: Locale) {
  const seconds = window?.reset_at
    ? Math.max(0, window.reset_at - Math.floor(Date.now() / 1000))
    : window?.reset_after_seconds
  if (typeof seconds !== "number") return "—"
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  return locale === "zh" ? `${hours}小时${minutes}分` : `${hours}h ${minutes}m`
}
function QuotaProgress({
  window,
  label,
  unavailable,
}: {
  window: UsageWindow | undefined
  label: string
  unavailable?: string
}) {
  const percent = quotaPercent(window)
  if (percent === undefined)
    return (
      <span className="text-xs text-muted-foreground" title={unavailable}>
        —
      </span>
    )
  return (
    <div className="flex min-w-28 items-center gap-2">
      <progress
        aria-label={label}
        className="h-2 w-20 accent-primary"
        value={percent}
        max={100}
      >
        {percent.toFixed(1)}%
      </progress>
      <span className="text-xs tabular-nums">{percent.toFixed(1)}%</span>
    </div>
  )
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
        "/transcriptions": "transcriptions",
        "/realtime": "realtime",
        "/images": "images",
        "/usage": "usage",
        "/audit": "audit",
        "/topups": "topups",
        "/model-prices": "model-prices",
        "/system-resources": "system-resources",
        "/admin-audit": "admin-audit",
        "/provider-audit": "provider-audit",
        "/model-downgrade-audit": "model-downgrade-audit",
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
    transcriptions: t.pageTranscriptions,
    realtime: t.pageRealtime,
    images: t.pageImages,
    usage: t.pageUsage,
    audit: t.pageAudit,
    topups: t.pageTopups,
    "model-prices": t.pageModelPrices,
    "system-resources": t.pageSystemResources,
    "admin-audit": t.pageAdminAudit,
    "provider-audit": t.pageProviderAudit,
    "model-downgrade-audit": t.pageModelDowngradeAudit,
    "request-detail": t.pageRequestDetail,
    users: t.pageUsers,
    settings: t.pageSettings,
  }[page]
}

export default App
