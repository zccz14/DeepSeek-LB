# DeepSeek-LB

DeepSeek-LB 是 DeepSeek API 的反向代理与负载均衡器。它以单个 Rust 可执行文件交付，内嵌 React + shadcn 管理界面，并使用 SQLite 保存配置、权限、上游提供商、Consumer、用量和逐调用审计。

服务只代理 DeepSeek，不兼容其他 AI 厂商协议：下游可使用 OpenAI 兼容的 `/v1/chat/completions`、`/v1/responses` 与 `/v1/models`（同时提供不带 `/v1` 前缀的别名），以及把 DeepSeek 原生联网搜索封装成独立接口的 `/v1/web-search`。

## 功能

- DeepSeek API Key 组成上游提供商池；每个 Provider 就是一条 API Key 加一个名称。
- 每个 Provider 有 `public` / `private` 可见性，默认 `private`：`private` 只服务拥有者名下的 Consumer，`public` 进入全站共享池。
- Consumer 是下游 AI App 使用的凭据（`sk-*`，只在创建或轮换时展示一次）。
- 逐调用审计：模型、Provider、Token 用量、缓存命中、峰谷时段、费用、首字节与总延迟、错误码，以及可选的请求/响应诊断正文。
- 联网搜索：`/v1/web-search` 用一次 DeepSeek 原生 `web_search` 检索网页来源，返回按 URL 去重的来源与本次用量；宿主模型由 root/管理员在“设置 → 联网搜索”配置，控制台也有同名工具页可直接试用。
- 请求费用查询：下游记录响应头 `x-deepseek-lb-request-id` 后，可按单条或批量查询每次调用的实收金额与用量；支持上游网关（NormAI）通过 `x-normai-request-id` 传入统一请求 ID，跨上游的定价差异由各负载均衡器内部处理。
- 费用核算使用 DeepSeek 官方美元价格，区分高峰（peak）与非高峰（off-peak）两档；中国法定节假日全天按非高峰计费。
- Midas 预付费：用户向 fund 账户充值后即可调用；未充值且未开启欠费的请求返回 HTTP 402。
- Auth Mini 统一登录（Ed25519 JWT / JWKS），首个登录用户绑定为 root。
- root/admin 视图包含仪表盘、上游、消费者、用量、审计、提供商审计、模型价格、系统资源、用户与设置；普通用户只看到自己的工作区。

## 快速开始

要求 Rust 1.93、Node.js 24 和 npm。

```bash
cd web
npm ci
npm run build
cd ..
cargo run
```

开发前端时运行 `npm run dev`；Vite 会把 API 请求代理到 `http://localhost:8080`。

首次启动后打开 `http://localhost:8080`，用 Auth Mini 登录完成 root 绑定；随后在“上游提供商”里添加 DeepSeek API Key，在“下游消费者”里创建 Consumer，即可把 AI App 指向本服务。

生产构建先生成一次 `web/dist`，Rust 只负责嵌入已有静态资源：

```bash
cd web && npm ci && npm run check && cd ..
cargo build --release --locked
```

`build.rs` 不启动 npm，因此 Rust 构建环境不需要 Node。GitHub Release 工作流先构建一次前端，再复用相同产物生成二进制。

## 使用方式

对话补全：

```bash
curl https://deepseek.ntnl.io/v1/chat/completions \
  -H 'Authorization: Bearer <YOUR_CONSUMER_KEY>' \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "deepseek-flash",
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

Responses API（Codex 等客户端使用）：

```bash
curl https://deepseek.ntnl.io/v1/responses \
  -H 'Authorization: Bearer <YOUR_CONSUMER_KEY>' \
  -H 'Content-Type: application/json' \
  -d '{"model": "deepseek-v4-pro", "input": "Explain this Rust error"}'
```

模型目录：

```bash
curl https://deepseek.ntnl.io/v1/models -H 'Authorization: Bearer <YOUR_CONSUMER_KEY>'
```

可调用的模型由 root/admin 在“设置 → 可用模型”中维护，默认 `deepseek-flash` 与 `deepseek-v4-pro`；不在白名单中的模型会在到达上游之前以 HTTP 400 拒绝。

### 联网搜索

```bash
curl https://deepseek.ntnl.io/v1/web-search \
  -H 'Authorization: Bearer <YOUR_CONSUMER_KEY>' \
  -H 'Content-Type: application/json' \
  -d '{"query":"latest DeepSeek release","max_results":8,"max_uses":1,"allowed_domains":["api-docs.deepseek.com"]}'
```

`/v1/web-search` 把 DeepSeek 原生 `web_search` 封装为独立的搜索 API，供不使用对话协议的下游调用：用 Consumer 密钥发起，宿主模型由“设置 → 联网搜索”中的 `web_search_model` 配置（默认 `deepseek-flash`），响应包含本次实际执行的搜索词、按 URL 去重的来源与本次用量。DeepSeek 没有专用搜索端点，因此每次搜索实际是一次携带 `web_search_20250305` 服务端工具的 Messages 轮次，发往上游 Anthropic 兼容端点 `{upstream_base}/anthropic/v1/messages`，按宿主模型的 Token 价格计费并计入该 Consumer 的用量与审计。搜索与推理共用同一健康上游池与冷却规则：上游返回 401/402/403/429 后，本次搜索会在另一个可用 Provider 上重试一次。

请求体除必填的 `query` 外，还可选地带四个搜索参数，非法值一律返回 400，不会被静默忽略：

- `max_results`：返回来源数量上限（1–50，默认 8）。DeepSeek 没有结果数量参数，因此该上限只在上游返回后按 URL 去重并截断，不改变本次搜索的成本。
- `max_uses`：本轮允许的服务端搜索次数（1–5，默认 5）；次数越多，推理 Token 与费用越高。
- `allowed_domains` 或 `blocked_domains`：裸域名数组，子域名自动包含；两者只能用其一。
- `user_location`：`country`、`region`、`city`、`timezone` 组成的近似位置对象。

结果是 DeepSeek 原生搜索返回的结构化来源（`url`、可选的 `title`、`snippet`、`published_at`），不包含模型生成的回答；没有可引用来源时以 HTTP 502 与 `web_search_data_missing` 失败，而不是返回空结果。控制台的“联网搜索”工具页调用同一能力（`POST /api/web-search`），使用登录用户的隐藏控制台 Consumer，同样写入审计并按该用户的余额计费。

### 请求费用查询

下游（如上层网关或 AI App）在收到推理响应时记录响应头 `x-deepseek-lb-request-id`，即可单条或批量查询该次调用的实收金额与用量：

```bash
curl https://deepseek.ntnl.io/v1/requests/<REQUEST_ID> \
  -H 'Authorization: Bearer <YOUR_CONSUMER_KEY>'

curl https://deepseek.ntnl.io/v1/requests/query \
  -H 'Authorization: Bearer <YOUR_CONSUMER_KEY>' \
  -H 'Content-Type: application/json' \
  -d '{"ids":["<REQUEST_ID_1>","<REQUEST_ID_2>"]}'
```

金额为实收费用（USD 纳美元，1e-9 USD），恒等于审计账目；查询以 request-id 为凭据，任意有效 Consumer 均可调用，更适合下游异步批量回填。若调用方（如 NormAI）通过请求头 `x-normai-request-id` 提供统一请求 ID，本服务会优先采用它——响应头、费用查询与控制台“请求 ID”三处同值；缺失或非法时生成本服务自己的 UUID。字段、重试与回填示例见 [docs/request-cost-query.md](./docs/request-cost-query.md)。

### 亲和与线程

- `x-lb-affinity-key`、`thread-id`、`session-id`、`x-deepseek-session-id` 中的第一个非空值决定角色亲和：同一 key 的连续请求优先复用同一 Provider，Provider 不可用时自动重新选择。
- `thread-id` / `x-deepseek-session-id` 同时写入审计，用于请求详情页在同一条会话内前后翻页。

## 调度与计费

- 每次请求先按可见性过滤 Provider，再按亲和与在途负载选择；Provider 的并发上限由 root 在“设置 → 并发”中配置。
- 401/403 记为 `auth_error`，402（余额不足）记为 `balance_error`，429 触发 30 秒冷却；触发失败的上游会在同一次请求内被另一个可用 Provider 重试一次，全部失败时返回上游错误。
- 计费按请求开始时刻的时段（peak / off-peak）与 DeepSeek 官方价格计算，写入 `api_calls` 的官方价、实收价与倍率；root 可通过 `model_price_multiplier` 设置加价倍率（默认 1.0，即按官方价格收费）。
- 未配置 Midas 时，所有用户默认必须使用充值或转账余额；`allow_all_users_debt` 或单个用户的 `allow_debt` 可放开限制。

## 生产部署

生产环境运行在 AWS Tokyo 的 EC2 上，并通过 <https://deepseek.ntnl.io> 提供 HTTPS 服务。Nginx 只负责 TLS 和流式反向代理；SQLite 与应用配置保存在实例的 `/var/lib/deepseek-lb/.deepseek-lb/`。

推送 `v*` tag 后，Release workflow 会发布 `deepseek-lb-x86_64-unknown-linux-gnu.tar.gz` 及其 SHA-256，再使用 GitHub OIDC 与 AWS Systems Manager 把资产部署到 EC2。部署过程校验 SHA-256、更新 `/opt/deepseek-lb/current` 符号链接、重启 systemd 服务并执行本机健康检查；健康检查失败时回滚到上一版本。仓库不保存 AWS 长期密钥，也不开放 SSH 入站端口。

部署 Action 使用以下 Repository variables：

- `AWS_DEPLOY_ROLE_ARN`
- `AWS_REGION`
- `EC2_INSTANCE_ID`

## Consumer 凭据校验

已登录用户可以调用 `POST /api/consumers/{id}/verify`，使用 Auth Mini Bearer 鉴权，JSON 请求体为 `{"secret":"待校验的 Consumer Token"}`。接口按完整 Token 的 SHA-256 校验该用户指定的 Consumer，返回 `id`、`credential_matches`、`is_disabled`、`request_archive`；不修改数据，也不调用模型上游。

## 验证

```bash
cd web && npm run check && cd ..
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features --locked
```

Pull Request 工作流还会运行 RustSec 依赖审计。

## 安全说明

- 上游 API Key 以明文保存在 SQLite 中：读取接口只对 root/admin 开放、响应带 `no-store`，并写入管理员操作审计。
- 入站 `Authorization`、Cookie 与 hop-by-hop headers 不会转发到上游；上游始终使用所选 Provider 的 API Key。
- 请求/响应诊断记录会保存最多 2 MiB 的正文预览；SQLite 文件因此可能包含 prompt 或输出片段，应按敏感业务数据保护并使用较短保留期。
- 诊断记录不会保存 Authorization、Cookie、Token、Secret 或 Consumer 类请求头。
- 费用查询以 request-id 为凭据：持有该 ID 的任意有效 Consumer 均可读取对应记录的金额与用量（ID 由本服务生成或上游网关提供，请视为凭据保管）；入站 `x-normai-request-id` 只作为请求标识采用，不参与权限判断；控制台审计仍按租户隔离。
- SQLite 文件应位于本机磁盘；不要让多个实例通过网络文件系统同时写入同一数据库。
- 生产部署应在 DeepSeek-LB 前提供 TLS，并限制数据目录的系统账户访问权限。

## License

[MIT](./LICENSE)
