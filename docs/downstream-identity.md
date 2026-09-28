# 下游客户端身份与上游 originator 路由

提供商的真实上游身份与跨来源路由权限分别配置：

- `originator`：创建时声明的 OAuth / 上游请求身份，支持 `codex_cli_rs`、`pi`、`opencode`，创建后不可修改。
- `allow_other_originator`：是否为其他或未知来源兜底，默认 `false`；提供商所有者或管理员可以随时开关。

开启兜底不改变凭据、OAuth 授权身份或发给上游的 `originator`。`any` 和 `*` 不是合法的提供商身份。

## 客户端识别

下游 `originator` 优先于 `User-Agent`。未携带 originator 时按 UA 识别；两个信号都能识别且属于不同客户端族时拒绝请求。显式携带未知 originator 时，不再用 UA 将其归入某个已知族，而只允许兜底。

| 客户端族 | 下游 `User-Agent` 前缀 | 下游 `originator` | 精确匹配的提供商 |
| --- | --- | --- | --- |
| `codex` | `codex_cli_rs/`、`codex-tui/`、`codex_vscode/`、`codex_exec/`、`Codex Desktop/` | 以 `codex` 开头，如 `codex_vscode`、`Codex Desktop` | `codex_cli_rs` |
| `pi` | `pi (`、`pi/`，或恰好 `pi` | `pi` | `pi` |
| `opencode` | `opencode/`、`opencode `，或恰好 `opencode` | `opencode` | `opencode` |
| `console` | 不校验 | 不校验 | `codex_cli_rs` |
| 未识别 | 无法识别或未携带 | 无法识别或未携带 | 无，仅允许显式开启的兜底提供商 |

`console` 是使用 Auth Mini 会话鉴权的第一方浏览器控制台。其他来源仍须通过 Consumer 鉴权；未知身份不代表匿名访问。

## 两级路由

1. 排除禁用、删除、冷却中、认证异常及本次重试排除的提供商。
2. 优先选择与下游客户端族精确匹配的可用提供商。
3. 精确池为空时，才选择 `allow_other_originator=true` 的可用提供商。
4. 两层都为空，返回 `503 provider_pool_empty`。

亲和性和最小负载选择只在当前层内执行。普通请求的专属池恢复后，新的选择回到专属池，不被旧备用亲和性锁住。单纯并发占满仍按现有规则排队，不视为故障；取得并发名额后重新检查可用性、兜底权限和路由层级。

重试保留现有错误分类和次数上限，每次重新选择都使用相同的两级规则。已经建立的 Realtime 会话继续使用绑定的提供商，不因为专属池恢复而迁移；后续连接仍检查该提供商的身份或兜底许可。开关关闭不打断已经发出的请求，但阻止后续跨来源选择和排队请求使用它。

## Pi 兜底配置

创建或导入提供商时选择 Pi 身份，再打开「允许为其他 originator 兜底」：

```json
{
  "originator": "pi",
  "allow_other_originator": true
}
```

也可以对已有 Pi 提供商调用 `PATCH /api/providers/{id}`：

```json
{"allow_other_originator": true}
```

Pi 请求正常使用它；Codex、OpenCode 或未知来源仅在没有可用精确提供商时使用它。它不是所有来源的纯备用。每次上游请求仍发送 `originator: pi`，不会把下游身份或通配值发给上游。

OAuth 创建支持在 `/api/oauth/complete` 中提交这个开关。它只控制本地路由，不修改 OAuth state 记录的真实身份；凭据导入支持在 `/api/providers` 的创建请求中提交。

## 上游请求头与能力边界

- `originator` 始终使用选中提供商的真实身份，覆盖写入且只有一个值。
- 精确匹配时保持既有 UA 行为。跨来源兜底时使用该提供商身份对应的全局 UA 配置；未配置则使用无版本号默认值：`pi`、`opencode` 或 `codex_cli_rs (DeepSeek-LB)`，不会透传其他客户端族的 UA。
- Realtime 建连及后续 WebSocket 使用相同的身份规则。
- 音频转写仍只接受 Codex 来源和 console；开启兜底不解除该能力限制。该端点保留原有桌面 UA 默认值（显式配置的身份 UA 仍优先），即使使用 Pi 提供商兜底，originator 也仍为 `pi`。上游是否接受特定凭据和端点组合，仍由上游决定。
- 图片生成（`/v1/images/generations` 与控制台 `/api/images/generations`）是一次性请求：不要求调用方提供 `session-id`，上游请求仍会携带该头——调用方提供时原样转发，没有提供时由 LB 生成一个新的 UUIDv7（内嵌时间戳即请求时间）。
- 平台用量、重置额度等查询继续使用真实提供商身份。

## Pi 族请求头规整

`originator=pi` 的提供商意味着上游必须看到一条 Pi Agent 形状的请求。因此 DeepSeek-LB 对该族
（`/v1/responses`、`/v1/responses/compact`、`/backend-api/codex/responses`、
`/backend-api/codex/responses/compact`）实施闭集白名单：白名单之外的下游请求头一律剥离，
不做透传；缺失的必填头按下面的规则补齐或拒绝。

### 必填与校验

| 请求头 | 规则 | 违规 |
| --- | --- | --- |
| `authorization` | `Bearer <...>`，由 Consumer 鉴权使用，随后替换为提供商令牌 | `401` |
| `originator` | 必须为 `pi` | `503 downstream_identity_mismatch` |
| `user-agent` | 必须属于 pi 族形状（`pi (...)` / `pi/... (...)`） | `503 downstream_client_unknown` |
| `session-id` | **必须是它自己的 UUIDv7**（小写、版本号 7、RFC 9562 变体位，且内嵌毫秒时间戳在 `2020-01-01` 至当前时间 +1 天之间）。缺失返回 `400 downstream_session_id_missing`，形状不符返回 `400 downstream_session_id_invalid`；LB **不会替调用方合成或改写**这个值 | `400` |
| `content-encoding` | 仅接受 `zstd`（或 `identity`/缺省） | `400 downstream_content_encoding_invalid` |

### LB 拥有或派生

| 请求头 | 上游取值 |
| --- | --- |
| `authorization` / `chatgpt-account-id` / `originator` | 选中提供商的令牌、账号与身份 |
| `user-agent` | 全局上游 UA 配置；未配置时沿用下游 UA（精确匹配族已校验为 pi 形状），跨来源兜底时用默认值 `pi` |
| `session-id` | 下游自己送来的那个 UUIDv7，原样转发（跨来源兜底时也原样转发调用方的 id） |
| `x-client-request-id` | 恒等于生效的 `session-id`，下游填写的值不生效 |
| `accept` | `text/event-stream` |
| `accept-language` | `*` |
| `sec-fetch-mode` | `cors` |
| `openai-beta` | `responses=experimental`；配置了上游 `openai-beta` 时以配置为准 |
| `content-encoding` | `zstd`：LB 解压入站正文、完成解析与改写后重新压缩 |
| `content-type` | `application/json` |

### 放行

| 请求头 | 说明 |
| --- | --- |
| `accept-encoding` | 原样转发（Pi 送 `gzip, deflate`）；未提供时由 HTTP 客户端补 `gzip` |

### 剥离

`thread-id`、全部 `x-codex-*`（`turn-state`、`turn-metadata` 等）、全部 `x-openai-*`、
`openai-*`（`openai-beta` 除外，且由 LB 写入）、`x-session-id`、`session_id`、
`x-session-affinity`、`x-api-key`、`x-request-id`，以及任何未列入上面的请求头。

效果是上游对 Pi 族只会收到这一个集合：

```
accept, accept-encoding, accept-language, authorization, chatgpt-account-id,
content-encoding, content-type, openai-beta, originator, sec-fetch-mode,
session-id, user-agent, x-client-request-id
```

### 正文与边界情况

- Pi 在 SSE 路径上用 zstd 压缩正文，LB 先解压再解析（模型、额度、审计都依赖正文），随后按
  相同级别重新压缩并带上 `content-encoding: zstd`，因此上游看到的编码与真 Pi 一致。解压后的
  正文上限 64 MiB，超出返回 `400 downstream_body_too_large`。
- Pi 的一次性摘要调用（compaction、分支摘要）为关闭提示缓存而不带 `session-id`，与缺少该头的
  任何其他调用一样返回 `400 downstream_session_id_missing`。
- 严格 UUIDv7 校验意味着 `pi --session-id <自定义值>` 不能经 LB 使用；默认会话 ID 不受影响。
- 跨来源兜底时（Codex/OpenCode 客户端使用 Pi 提供商）仍按上表剥离异族请求头，但 `session-id`
  原样转发调用方自己的值：LB 不伪造 id。
- **为什么从不合成 id**：UUIDv7 的前 48 bit 是调用方可读的毫秒时间戳。真 Pi 的 id 时间戳等于
  会话开始时间；LB 造出来的 id 只能带一个假时间戳（或与请求时间明显不符），而上游解析这个字段
  的成本几乎为零。因此宁可拒绝也不伪造：值一定来自调用方，`session-id` 与 body 的
  `prompt_cache_key` 也始终一致（真 Pi 两者恒等，替换其中之一就会造出真实客户端不可能产生的
  矛盾）。

## 错误与审计

| 情况 | HTTP / `reason` |
| --- | --- |
| 两个已识别身份信号冲突 | `503 downstream_identity_mismatch` |
| 精确池和兜底池都为空，包括未知来源没有可用兜底 | `503 provider_pool_empty` |
| Realtime 绑定提供商不再匹配且未允许跨来源 | `503 provider_identity_mismatch` |
| 非 Codex 来源调用转写端点 | `503 codex_client_required` |
| Pi 族请求的 `session-id` 不是 UUIDv7 | `400 downstream_session_id_invalid` |
| Pi 族请求缺少 `session-id`（含 Pi 自身的摘要调用） | `400 downstream_session_id_missing` |
| 请求正文的 `content-encoding` 不是 zstd，或声明 zstd 但正文无法解压 | `400 downstream_content_encoding_invalid` |
| 解压后的请求正文超过 64 MiB | `400 downstream_body_too_large` |
| 补齐阶段仍缺少 `session-id`（其他客户端族；图片生成除外，由 LB 生成 UUIDv7） | `400 session-id request header is required` |

基础调用审计始终记录下游原始 `downstream_originator`、下游 UA、实际 `provider_id`、`upstream_originator` 和 `originator_fallback_reason`，不依赖正文归档开关。兜底原因：

- `provider_pool_empty`：已识别来源没有可用精确候选（含重试排除后为空）。
- `downstream_client_unknown`：无法识别客户端族，只能使用兜底候选。
- `null`：未发生跨来源兜底；历史记录的新字段也为 `null`。

审计列表和详情 API 返回这些字段，详情界面展示身份和兜底原因。成功兜底不会被记为失败。

## 迁移与复杂度

数据库为全部历史提供商补充 `allow_other_originator=false`，不自动开启跨来源流量。提供商的真实身份保持不变。

新增业务路径集中在候选池分层、未知客户端仅走兜底、排队后的重新校验，以及跨来源 UA 默认值。复用现有负载、亲和、权限、冷却和有限重试机制，不引入旧版双路由或额外重试循环。单元和集成测试覆盖池优先级、恢复、禁用/删除/冷却、排队撤权、OAuth、鉴权、身份冲突、出站头与无正文归档的审计。
