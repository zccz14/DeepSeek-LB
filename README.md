# OpenAI-LB

OpenAI-LB 是一个面向 OpenAI / CodeX OAuth 上游提供商的反向代理和负载均衡器。它以单个 Rust 可执行文件交付，内嵌 React + shadcn 管理界面，并使用 SQLite 保存配置、权限、上游提供商、Consumer、用量和逐调用审计。

项目只处理 OpenAI / CodeX 能力，包括 Responses、Compact、图像生成、音频转写和模型列表。它不提供其他 AI 厂商的协议兼容层。

## 直接运行

从 [GitHub Releases](https://github.com/zccz14/OpenAI-LB/releases) 下载当前平台的压缩包并校验同名 `.sha256` 文件，然后运行：

预构建发布仅覆盖 Linux x86_64；GitHub Release 会自动部署到 https://openai.ntnl.io。

```bash
./openai-lb
```

服务监听 `0.0.0.0:8080`。首次启动会自动完成以下本地准备：

- 创建 `~/.openai-lb/`。
- 创建 `~/.openai-lb/openai-lb.sqlite3` 并执行版本化迁移。
- 提供 Setup API 和 Setup GUI。

打开 `http://localhost:8080`，填写品牌提供的 Auth Mini issuer，然后前往该 Auth Mini 实例的托管登录页面。登录成功后浏览器会返回 OpenAI-LB，再将当前用户绑定为唯一 `root`。Setup 完成后初始化入口立即关闭。

OpenAI-LB 连接现有的品牌 Auth Mini 实例。用户不需要为 OpenAI-LB 部署 Auth Mini。前端使用 `auth-mini` SDK，后端使用 `auth-mini-axum` 的预热 JWKS verifier 验证 Ed25519 access JWT，并以精确的服务 hostname audience 和 `user_id` 关联本地 `root / admin / user` 权限。

## 产品能力

- 注册多组 CodeX OAuth `access_key` / `refresh_key`，支持 PKCE OAuth 和自动刷新。
- 每个上游提供商声明自己的客户端身份 `originator`（`codex_cli_rs` / `pi` / `opencode`），创建后不可修改；OAuth 授权、导入凭据与所有上游请求（含平台自身的用量查询）都使用该身份。
- 下游请求优先按 `originator`、其次按 `User-Agent` 识别客户端族；两个头都能识别时必须一致，优先路由到同一族的提供商。新增 `allow_other_originator`（默认关闭），仅在精确池为空时为其他或未知来源兜底；上游身份始终使用提供商自己的 originator，详见 [`docs/downstream-identity.md`](docs/downstream-identity.md)。
- root/admin 可在“设置 → 上游 UA”分别配置 `codex_cli_rs`、`pi`、`opencode` 的全局上游 User-Agent；发送请求时按选中的 Provider 身份覆盖下游 UA。
- 每个上游 Provider 记录 `owner_id`；普通用户可管理自己的 Provider，Provider 创建者保留凭据管理和收益归属。
- 每个上游 Provider 声明 `visibility`：`private`（默认）只允许该 Provider 拥有者名下的 Consumer 使用，`public` 允许所有 Consumer 使用。Providers 页可随时切换，无需重启；本次升级前已存在的 Provider 保留原有全局可用性，因此回填为 `public`。
- 以 Consumer 为租户调用凭据，密钥只显示一次，数据库只保存 SHA-256 哈希。
- 按 Consumer 汇总请求、Token、缓存 Token、错误和延迟。
- root 与管理员可在左侧“管理员”分组的“系统资源”页面查看宿主机 CPU、内存、网络、磁盘和 SQLite 文件占用，并按需执行 SQLite VACUUM。
- 每次代理调用保留 Session ID、Thread ID、请求 ID、用户、Consumer、上游提供商、接口、模型、状态、耗时和用量；请求/响应诊断记录仅对开启 `request_archive` 的 Consumer 保存，失败和客户端取消同样遵循该开关。
- 每个 Consumer 可选开启 `intercept_degradation`（默认关闭）。开启后，上游响应头 `x-codex-turn-state` 恰为 312 字节的成功响应（控制台标注为可能发生模型降级）不再转发，代理取消该次调用并直接以 HTTP 503 与 `degradation_intercepted` 返回。
- 使用 `session-id` 请求头进行渠道亲和；亲和键在持久化前进行 SHA-256 哈希。
- 每个上游提供商默认最多同时处理 3 个请求，超出后按先后顺序排队。root 和管理员可在“设置 → 上游并发控制”统一修改上限，保存后立即生效；降低上限时，正在处理的请求继续完成。名额从路由开始占用，到响应结束或取消时释放，覆盖流式响应和实时连接。上游提供商页面每 2 秒刷新处理中数量、上限和等待队列长度。队列属于当前服务进程，配置保存在 SQLite。
- 跟踪 `Retry-After` 与 `x-ratelimit-*`，对 429 上游提供商自动冷却并在到期后恢复。
- 对 401/403 上游提供商标记认证错误；手工禁用上游提供商不会自动恢复。
- Responses、SSE、音频上传和二进制响应保持流式传输。
- 充值只使用 Midas 公共 fund account：root 在后台保存该账户的 `user_id` 和 fund API key；用户在 OpenAI-LB 输入 USD 金额后，页面会打开带有该公开 ID 和精确纳美元金额的 Midas 确认页。用户登录并确认后，Midas 发起内部转账；OpenAI-LB 只按“该用户 → 公共账户”的当前累计转入额计算充值。旧 Midas 自动扣款与历史基线不再保留。

权限边界：

| 角色 | 权限 |
| --- | --- |
| `root` | 系统配置、用户角色、全部 Provider、全局审计与个人 Consumer；可管理全部 Provider，路由时可使用全部 `public` Provider 与自有 `private` Provider |
| `admin` | 全部 Provider、全局审计与个人 Consumer；可管理全部 Provider，路由时可使用全部 `public` Provider 与自有 `private` Provider |
| `user` | 自有 Provider、个人 Consumer、个人用量与个人审计；路由时可使用全部 `public` Provider 与自有 `private` Provider |

## 调用示例

在管理界面为每个 AI App 创建一个独立 Consumer 后调用代理。这样可以按 App 隔离用量、错误记录和吊销范围：

```bash
curl http://localhost:8080/v1/responses \
  -H 'Authorization: Bearer sk-REPLACE_ME' \
  -H 'Content-Type: application/json' \
  -H 'User-Agent: codex_cli_rs/0.51.0 (macos 15.0; arm64)' \
  -H 'originator: codex_cli_rs' \
  -H 'session-id: deployment-a' \
  -d '{"model":"gpt-5.4","input":"Explain this Rust error","stream":true}'
```

`originator` 优先；如果携带则 `User-Agent` 可以省略，但两个头都携带时必须属于同一个客户端族。两者都无法识别时只使用显式开启 `allow_other_originator` 的可用提供商；没有兜底则返回 `503 provider_pool_empty`。调用 Pi Agent 或 OpenCode 时分别使用 `pi (darwin 24.5.0; arm64)` / `pi`、`opencode/1.18.31 (linux; x86_64)` / `opencode`。

Session ID 是渠道亲和和审计会话导航的唯一输入，客户端应通过 `session-id` 请求头提供稳定值。所有 `/v1/*` 与 `/backend-api/codex/*` 代理端点都要求该请求头；缺少或留空时直接返回 400，图片生成除外：它是一次性请求，调用方不需要提供，代理会生成一个 UUIDv7 发给上游。Pi 族请求更严：`session-id` 必须是调用方自己的 UUIDv7，缺失或形状不符都返回 400，代理不会替调用方合成或改写它（UUIDv7 的时间戳上游可读，伪造时间戳是风险信号）。Thread ID 仍是逐调用审计上下文：CodeX 请求使用 `x-codex-conversation-id`，其他 App 可使用 `thread-id`；Pi 族不发送也不转发该头。两者都属于基础审计字段，无论 Consumer 是否开启 `request_archive` 都会落库；代理不会向响应添加额外字段或请求头。

```bash
curl http://localhost:8080/v1/audio/transcriptions \
  -H 'Authorization: Bearer sk-REPLACE_ME' \
  -H 'User-Agent: codex_cli_rs/0.51.0 (macos 15.0; arm64)' \
  -H 'originator: codex_cli_rs' \
  -H 'session-id: deployment-a' \
  -F 'file=@meeting.wav' \
  -F 'model=gpt-4o-transcribe'
```

转写只对 `codex` 客户端族开放：上游是 ChatGPT 的桌面专属端点，OpenAI-LB 会固定以 `Codex Desktop` 身份调用它。

```bash
curl http://localhost:8080/v1/images/generations \
  -H 'Authorization: Bearer sk-REPLACE_ME' \
  -H 'Content-Type: application/json' \
  -H 'User-Agent: codex_cli_rs/0.51.0 (macos 15.0; arm64)' \
  -H 'originator: codex_cli_rs' \
  -d '{"model":"gpt-image-2","prompt":"A precise exploded diagram","size":"2048x1152"}'
```

`gpt-image-2` accepts custom `WIDTHxHEIGHT` image sizes. Both dimensions must be multiples of 16 and no larger than 3840; the aspect ratio cannot exceed 3:1, and the total pixel count must be 655,360–8,294,400. Set `size` to `auto` to let the upstream choose.

## 数据与并发模型

SQLite 为单实例数据层。连接池中的每条连接统一启用 WAL、foreign keys、5 秒 busy timeout 和 `synchronous=NORMAL`，连接池上限为 4。

代理热路径使用内存上游提供商快照和亲和映射。每次选择先按调用方的客户端身份与可见性过滤：`originator` 必须与请求所属客户端族一致，Provider 必须是 `public`，或 `private` 且 `owner_id` 等于请求所认证 Consumer 的 `user_id`；随后在该集合内执行可用性、亲和和负载选择。管理员身份不扩大路由范围，因此 `private` Provider 始终只服务拥有者自己的 Consumer。可见性变更提交后立即重新加载快照，新请求不再选到已收紧的 Provider；已固定亲和或已建立的 Realtime 连接在重连时会重新校验可见性。上游提供商 Rate Limit 观测、亲和持久化、过期清理和 cooldown 恢复由后台任务批量提交。逐调用审计与请求/响应诊断记录进入容量为 4096 的有界队列，由单 writer 以最多 128 条的事务写入；审计队列不会阻塞代理，SQLite 暂时不可写时已入队事件会退避重试，队列耗尽时会丢弃新审计并限频记录。客户端取消仍结算为 HTTP 499。Consumer 的 `request_archive` 开关默认关闭，开启后才保存最多 1 MiB 的请求/响应正文预览并标记是否截断；图像生成响应预览上限为 4 MiB，以容纳常见的 base64 图像结果。Consumer 的 `intercept_degradation` 开关默认关闭，开启后带 312 字节 `x-codex-turn-state` 响应头的成功响应会被取消，以 503 和 `degradation_intercepted` 结算，不再向该 Consumer 传输响应正文。审计详情可按单条记录永久删除请求和响应正文，同时保留调用状态、用量和请求/响应头诊断；普通用户仅能删除自己的记录，root 与管理员可删除任意记录，并写入管理员操作审计。GUI 图片生成使用每个用户隐藏的系统 Consumer，始终写入完整的基础审计与诊断归档。诊断归档另有 64 MiB 内存预算，耗尽时只跳过正文诊断，基础 `api_calls` 审计仍会写入。认证、Cookie、Token、Secret 和 Consumer 凭据类请求头不会写入。诊断记录默认保留 24 小时，可由 root 在“设置”中配置为 1–365 天；后台每小时自动清理过期诊断记录，长期用量审计不受影响。

音频 multipart 不会整体读入内存。请求体从 Axum `Body` 直接流入 Reqwest；因为流式请求体无法安全重放，音频上传开始后不执行跨上游提供商重试。Responses 和图像使用独立的小型 JSON 请求限制。

## 本地开发

要求 Rust 1.93、Node.js 24 和 npm。

```bash
cd web
npm ci
npm run build
cd ..
cargo run
```

开发前端时运行 `npm run dev`；Vite 会把 API 请求代理到 `http://localhost:8080`。

生产构建先生成一次 `web/dist`，Rust 只负责嵌入已有静态资源：

```bash
cd web && npm ci && npm run check && cd ..
cargo build --release --locked
```

`build.rs` 不启动 npm，因此 Rust 构建环境不需要 Node。GitHub Release 工作流先构建一次前端，再复用相同产物生成各平台二进制。

## 生产部署

生产环境运行在 AWS Tokyo 的 `openai-lb-tokyo` EC2，并通过 <https://openai.ntnl.io> 提供 HTTPS 服务。Nginx 只负责 TLS 和流式反向代理；SQLite 与应用配置保存在实例的 `/var/lib/openai-lb/.openai-lb/`。

推送 `v*` tag 后，Release workflow 会先发布三个官方平台资产，再使用 GitHub OIDC 和 AWS Systems Manager 将 Linux x86_64 资产部署到 EC2。部署过程校验 Release 的 SHA-256，使用 systemd 重启服务并执行本机健康检查；健康检查失败时恢复上一版本。仓库不保存 AWS 长期密钥，也不开放 SSH 入站端口。

部署 Action 使用以下 Repository variables：

- `AWS_DEPLOY_ROLE_ARN`
- `AWS_REGION`
- `EC2_INSTANCE_ID`

## Consumer 凭据校验

已登录的用户可以调用 `POST /api/consumers/{id}/verify`，使用 Auth Mini
Bearer 鉴权，JSON 请求体为 `{"secret":"待校验的 Consumer Token"}`。
接口按完整 Token 的 SHA-256 校验该用户指定的 Consumer，返回
`id`、`credential_matches`、`is_disabled`、`request_archive`。
校验不修改消费者，不返回 Token 或哈希，也不调用模型上游。
被删除、不属于当前用户、系统专用或不存在的 Consumer 均返回 404。
Token 匹配与消费者启用是两个独立状态；仅二者同时满足时凭据才可用。
应用可在校验后调用现有创建、轮换或更新接口修复自己的集成，再次校验后报告成功。
这只是校验时的状态快照，之后的删除、禁用或轮换仍会使凭据失效。

## 验证

```bash
cd web && npm run check && cd ..
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features --locked
```

Pull Request 工作流还会运行 RustSec 依赖审计。

## 安全说明

- OAuth PKCE verifier 只保存在单实例进程内，十分钟后过期；服务重启会使未完成的 OAuth 授权失效。
- 升级前版本遗留的 `master.key` 已不再使用，可在新版本部署后删除。
- 入站 `Authorization`、Cookie、hop-by-hop headers 和代理专用亲和头不会转发到上游；下游 `originator` 头也不会转发，上游身份始终由选中的提供商决定。
- `originator=pi` 的上游请求按 Pi Agent 的真实请求头闭集规整：白名单之外的下游头（`thread-id`、`x-codex-*`、`x-openai-*` 等）一律剥离，`session-id` 必须是调用方自己的 UUIDv7（否则 400，且该拒绝会写入审计），`x-client-request-id` 由 LB 按 `session-id` 派生；Pi 的 zstd 请求正文由 LB 解压校验后再压缩转发。详见 [`docs/downstream-identity.md`](docs/downstream-identity.md)。
- 请求始终需要鉴权。可识别来源优先使用同身份提供商；其他或未知来源只能在无精确候选时使用 `allow_other_originator=true` 的兜底提供商。两个已识别身份信号冲突时仍拒绝。跨来源兜底的上游 originator 和 UA 始终按实际提供商身份选取。
- `private` Provider 只允许其拥有者名下的 Consumer 消耗，与请求来源是否精确识别无关；跨用户使用只能由拥有者显式切换为 `public`。
- 请求/响应诊断记录会保存最多 1 MiB 的正文预览，图像生成响应为最多 4 MiB；SQLite 文件因此可能包含 prompt、输出、图像或音频片段，应按敏感业务数据保护并使用较短保留期。
- 诊断记录不会保存 Authorization、Cookie、Token、Secret、Consumer 类请求头或 OAuth 凭据。
- SQLite 文件应位于本机磁盘；不要让多个实例通过网络文件系统同时写入同一数据库。
- 生产部署应在 OpenAI-LB 前提供 TLS，并限制数据目录的系统账户访问权限。

## License

[MIT](./LICENSE)
