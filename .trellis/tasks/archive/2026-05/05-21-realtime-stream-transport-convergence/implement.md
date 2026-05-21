# 实时流通路收敛实施计划

## Checklist

- [x] 加载开发前规范：frontend、backend、cross-layer、shared，以及流式协议相关 spec。
- [x] 前端项目事件流：
  - [x] 将 `connectEventStream` 改为 NDJSON fetch transport，或直接在 `eventStore` 内实现项目流 NDJSON 消费。
  - [x] 支持 `AbortController`、`x-stream-since-id`、逐行解析、连接状态、指数退避重连。
  - [x] 删除未使用的 `fetchEventsSince()` 和项目级 `EventSource` 类型状态。
- [x] 前端 session 流：
  - [x] 删除 `EventSourceTransport`、`preferSseOnly()`、`VITE_ACP_STREAM_TRANSPORT` 分支。
  - [x] 保留并简化 `FetchNdjsonTransport`。
- [x] 前端 UI / proxy：
  - [x] 去掉 Backend 面板中的 `SSE` 文案，必要时弱化为项目同步状态。
  - [x] 删除 Vite `/api/events/stream` 专用代理规则，保留 `/api` 长连接代理配置。
- [x] 后端项目流：
  - [x] 删除 `event_stream` SSE handler、SSE imports 与 router 中 `/events/stream`。
  - [x] 让 `event_stream_ndjson` 读取 `x-stream-since-id`，保留 query/header 合理解析。
  - [x] 移除不用的 `/events/since/{since_id}`，除非实现中仍有明确内部消费者。
- [x] 后端 session 流：
  - [x] 删除 `acp_session_stream_sse` handler、SSE imports 与 router 中 `/acp/sessions/{id}/stream`。
  - [x] 保持 `/stream/ndjson` 行格式不变。
- [x] 文档/spec：
  - [x] 更新 `.trellis/spec/backend/session/streaming-protocol.md` 为 NDJSON-only 浏览器流契约。
  - [x] 更新 README / app-web README / vite 注释中仍称浏览器 REST + SSE 的描述。
- [x] 验证：
  - [x] `pnpm --filter app-web typecheck` 或项目现有前端类型检查命令。
  - [x] 前端相关测试。
  - [x] Rust fmt/check/test 覆盖 API stream 与 ACP session route。
  - [x] `rg` 确认 SSE 浏览器路径已清理。

## Risky Files

- `packages/app-web/src/stores/eventStore.ts`
- `packages/app-web/src/api/eventStream.ts`
- `packages/app-web/src/features/session/model/streamTransport.ts`
- `packages/app-web/src/components/layout/workspace-layout.tsx`
- `packages/app-web/vite.config.ts`
- `crates/agentdash-api/src/stream.rs`
- `crates/agentdash-api/src/routes/acp_sessions.rs`
- `crates/agentdash-api/src/routes.rs`
- `.trellis/spec/backend/session/streaming-protocol.md`

## Validation Commands

```powershell
pnpm --filter app-web typecheck
pnpm --filter app-web test
cargo fmt --check
cargo check
rg "new EventSource|EventSourceTransport|VITE_ACP_STREAM_TRANSPORT|/events/stream`\"|/acp/sessions/\\{id\\}/stream`\"" packages crates .trellis README.md docs
```

## Review Gate Before Start

- PRD、design、implement 已创建。
- 用户确认按 NDJSON-only 浏览器流方向实现。
