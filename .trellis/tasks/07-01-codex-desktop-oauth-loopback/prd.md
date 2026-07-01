# 修复桌面包 Codex OAuth 本机回调

## Goal

修复 external 桌面包中 ChatGPT / Codex OAuth 授权完成后浏览器访问 `http://localhost:1455/auth/callback` 被拒绝连接的问题，让桌面端在用户本机接收 loopback callback，云端 API 继续负责 Provider 权限校验、token exchange、凭据加密保存和状态投影。

## Background

- 当前 Codex OAuth redirect URI 固定为 `http://localhost:1455/auth/callback`，而回调 listener 由 `agentdash-api` 的 `start_local_pkce_oauth_flow` 启动，证据见 `crates/agentdash-api/src/routes/llm_providers.rs:43`、`crates/agentdash-api/src/routes/llm_providers.rs:45`、`crates/agentdash-api/src/routes/llm_providers.rs:372`、`crates/agentdash-api/src/oauth_flow.rs:74`。
- Desktop release bundle 默认使用 `external` API mode 并指向远端 server，业务数据、登录、项目和 runner enrollment 权威事实属于 cloud server，证据见 `.trellis/spec/cross-layer/desktop-local-runtime.md:20`。
- 桌面前端的业务 API origin 来自构建时 `VITE_API_ORIGIN`，证据见 `packages/app-tauri/vite.config.ts:6`、`packages/app-tauri/vite.config.ts:17`、`packages/app-web/src/api/origin.ts:1`。
- Tauri 壳已有打开系统浏览器的本机能力，`open_external_url` 只允许 http/https，证据见 `crates/agentdash-local-tauri/src/main.rs:308`、`packages/app-tauri/src/App.tsx:32`、`.trellis/spec/cross-layer/desktop-local-runtime.md:827`。
- LLM Provider DTO 属于 `agentdash-contracts` 并生成到 `packages/app-web/src/generated/llm-provider-contracts.ts`，前端 service 消费 generated DTO，证据见 `.trellis/spec/cross-layer/frontend-backend-contracts.md:89`、`.trellis/spec/cross-layer/frontend-backend-contracts.md:100`、`packages/app-web/src/api/llmProviders.ts:3`。

## Requirements

- R1. Codex OAuth 的 loopback listener 必须运行在桌面用户本机，而不是 external 云端 API 进程中；浏览器回到 `localhost:1455` 时应由当前桌面应用接收 `code`。
- R2. 云端 API 必须继续作为 Codex credential 的权威保存边界，负责 provider/user target 权限校验、OpenAI token exchange、`chatgpt_account_id` 提取、凭据加密保存和 flow status 投影。
- R3. 桌面端 OAuth flow 必须使用 PKCE：本机生成并持有 `code_verifier`，授权 URL 携带 `code_challenge`，callback 后把 `code + verifier` 提交到云端 complete 端点。
- R4. Flow 必须短时、一次性并绑定当前用户、provider、credential target；过期、取消、重复 complete、权限变化都必须返回稳定错误状态。
- R5. 桌面端应处理 `localhost` 的 IPv4/IPv6 行为差异：在 redirect URI 仍为 `http://localhost:1455/auth/callback` 时，本机 listener 覆盖 `127.0.0.1` 与 `::1` 的可达性。
- R6. 前端设置页应在 Tauri 桌面环境中调用桌面 bridge 发起 Codex OAuth，并继续通过现有状态轮询体验展示 pending/completed/failed。
- R7. 纯 Web / 无桌面 bridge 环境以 unavailable 状态呈现 Codex OAuth；原因是 `localhost` callback 语义属于用户设备，云端页面无法接收用户机器上的 loopback 回调。
- R8. 新增或调整的 OAuth request/response DTO 必须进入 `agentdash-contracts::integration::llm_provider` 并生成 TypeScript，不在前端手写跨层 wire type。
- R9. 日志、错误消息、诊断和任务状态保持 OAuth secret 脱敏；`code`、`code_verifier`、access token、refresh token 和 bearer token 只在必要的内存流程中短暂存在。

## Acceptance Criteria

- [ ] AC1. external 桌面包中点击 ChatGPT / Codex OAuth 后，系统浏览器授权完成可以成功回到 `localhost:1455/auth/callback`，UI 状态变为 completed，目标 global provider 或 user BYOK credential 被保存。
- [ ] AC2. 云端 API 端不再为 external 桌面 OAuth flow 绑定 `localhost:1455`；`localhost` callback 的 bind/accept 生命周期由 Tauri 本机宿主负责。
- [ ] AC3. Flow complete 端点校验当前用户、provider、target、TTL 和一次性状态后才执行 token exchange；失败状态可通过现有 status API 查询。
- [ ] AC4. OpenAI token exchange、account id 提取和 credential 保存复用或迁移现有后端能力，保存后 provider preview 仍显示 `ChatGPT OAuth`。
- [ ] AC5. 在 `localhost` 解析到 IPv6 的浏览器环境下，callback 仍可被本机 listener 接收，或测试覆盖双栈监听行为。
- [ ] AC6. 前端服务层消费 generated `llm-provider-contracts.ts` 类型；新增 DTO 后 `pnpm run contracts:check` 通过。
- [ ] AC7. Tauri / Rust 后端 / 前端聚焦测试覆盖成功、取消、过期、state mismatch、端口占用、重复 complete、无桌面 bridge 的 UI 行为。
- [ ] AC8. 验证命令至少覆盖 `cargo test` 的相关 Rust crates、`pnpm run contracts:check`、`pnpm run frontend:check` 和 `pnpm run desktop:frontend:check`。

## Scope Boundary

- OpenAI / Codex OAuth client id 与 redirect URI 保持现状，原因是本任务修复的是本机 loopback ownership，不改变 OpenAI 已登记的 native OAuth surface。
- 云端 flow 状态默认沿用内存态短 flow，原因是该流程是交互式、短 TTL、一次性授权，持久化事实只从 token exchange 成功后的 LLM Provider credential 开始。
- 纯 Web 云端页面中的 Codex OAuth 授权能力不进入本任务范围，原因是该能力缺少桌面端本机 loopback 接收者。

## Open Questions

无阻塞问题。默认按“本机接 code，云端换 token 并保存凭据”的方案推进。
