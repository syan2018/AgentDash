# SSE 事件流稳定化与 Fetch Streaming 改造

## 背景

当前前端在开发模式（Vite + proxy）下会出现 `ws/http proxy error: read ECONNRESET`，典型触发点包括：
- 页面刷新 / Fast Refresh / HMR 时旧连接被关闭、代理侧打印错误
- SSE（`/api/events/stream`、`/api/acp/sessions/{id}/stream`）属于长连接，容易被中间层超时/重置

同时，我们后续明确需要一条 **完整的 fetch streaming 流程**（可自定义 header、可 POST、可统一分帧/重连逻辑），以满足：
- 鉴权（Authorization header）
- 统一的流式传输抽象（SSE/EventSource 与 fetch/ReadableStream 可切换）
- 更可控的重连、续传、错误处理与观测

> 时间：2026-02-26

## Goal（目标）

1. **稳定**：开发期 SSE/流式请求在 proxy、HMR 场景下稳定，不产生连接泄漏；必要的报错降噪但不吞真实错误。
2. **契约清晰**：为“事件流 / 会话流”定义清晰的一致性契约（id、resume、错误、心跳）。
3. **可演进**：引入 fetch streaming（NDJSON 或等价协议）作为 P1，最终形成统一 transport 抽象，并允许逐步迁移。

## Non-goals（非目标）

- 不在本任务中做全量权限体系/登录体系（但要为未来 header 鉴权预留路径）。
- 不在本任务中重写业务状态模型（Story/Task 的语义以后端为准）。

## 范围与里程碑

### P0：SSE 稳定化（近期必须完成）

#### P0.1 全局事件流支持 resume（断线续传）

现状：
- `/api/acp/sessions/{id}/stream` 已支持 `Last-Event-ID` + 历史回放 + `Event.id(...)`。
- `/api/events/stream` 目前只发送 `data(json)`，缺少 `id`，浏览器重连无法可靠续传，只能“重新连接后从最新开始”。

改造：
- 服务端为全局事件流每条消息附带 **稳定递增的 `id`**（基于 store event_id 或 StateChange id）。
- 服务端读取 `Last-Event-ID`（header），在连接建立时先补发缺失事件，再进入实时订阅。
- 明确事件类型：`Connected / StateChanged / Heartbeat` 等保持向后兼容。

#### P0.2 Dev 代理与 HMR 清理策略统一

目标：
- SSE 长连接不再因 proxy 默认超时被中断。
- Fast Refresh 下旧 EventSource 必须被关闭，避免“连接累积 → 频繁 reset”。

计划：
- 对所有 SSE 端点（至少 `/api/events/stream`、`/api/acp/sessions/*/stream`）在 `frontend/vite.config.ts` 中配置：
  - `timeout: 0`
  - `proxyTimeout: 0`
  - 仅对 `ECONNRESET/EPIPE` 进行降噪（其他错误保留并返回 502）
- 在前端实现统一的“流连接注册表”，在 `import.meta.hot.dispose` 时统一 close。

#### P0.3 观测与错误矩阵（最小化即可）

- 后端：为流式端点增加关键日志（连接建立、携带的 last-event-id、补发数量、lagged 次数、关闭原因）。
- 前端：对“连接断开/重连中/恢复”做可见状态（不把瞬时断线当致命错误）。

---

### P1：Fetch Streaming（后续明确要做，纳入本任务规划）

#### P1.1 新增 NDJSON 流式端点（fetch 友好、可带 header）

新增端点（建议）：
- `GET /api/events/stream/ndjson`
- `GET /api/acp/sessions/{id}/stream/ndjson`

响应规范（NDJSON）：
- `Content-Type: application/x-ndjson; charset=utf-8`
- 每行一个 JSON object（与 SSE data payload 等价），以 `\\n` 分隔
- 支持心跳：周期性输出 `{"type":"heartbeat", ...}`（或与现有 `Heartbeat` 保持一致）

续传（resume）：
- 客户端通过 header `x-stream-since-id: <number>` 或 query `?since_id=<number>`（二选一，最终以契约为准）
- 服务端先补发 since_id 之后的历史，再进入实时订阅

备注：
- SSE 仍保留，用于浏览器原生 EventSource 以及最简单的默认路径；NDJSON 用于需要 header/更可控 transport 的场景。

#### P1.2 前端引入统一 transport 抽象

目标：
- 业务层（stores/hooks）不再直接依赖 `EventSource`。
- 支持两种 transport：
  - `EventSourceTransport`（SSE）
  - `FetchNdjsonTransport`（fetch + ReadableStream + NDJSON 分帧）

策略：
- dev/默认：优先 fetch NDJSON（可带 header、可统一重连），fallback 到 SSE
- 支持环境变量：
  - `VITE_API_ORIGIN`（绕过 Vite proxy，直接连后端；可选）

#### P1.3 兼容性与迁移策略

- 先改全局事件流，再改 ACP 会话流；确保 UI 行为不变。
- 增量上线：新增端点与 transport，不一次性替换所有调用点。

## 契约（Contracts）

### 全局事件流（SSE）

- URL：`GET /api/events/stream`
- Content-Type：`text/event-stream`
- Resume：
  - 客户端自动重连携带 `Last-Event-ID`
  - 服务端保证 `Event.id` 单调递增且可用于补发

### ACP 会话流（SSE）

- URL：`GET /api/acp/sessions/{id}/stream`
- 现状：已支持 `Last-Event-ID` + 历史回放（作为全局事件流改造的参考实现）

### NDJSON 流（fetch）

- URL：
  - `GET /api/events/stream/ndjson`
  - `GET /api/acp/sessions/{id}/stream/ndjson`
- Resume：`x-stream-since-id` 或 `since_id`（最终落地时确定唯一方案）

## 验收标准（Acceptance Criteria）

P0：
- [ ] Dev 环境下频繁修改触发 HMR 时，不出现“连接累积导致的持续 ECONNRESET 噪音”
- [ ] `/api/events/stream` 支持基于 id 的 resume：断网/刷新后不会丢事件或重复回放到不可控程度
- [ ] 前端能够准确反映“连接中/断开/重连中”状态（不把短暂断线当致命错误）

P1（规划验收）：
- [ ] 定义并记录 NDJSON 端点与 resume 契约（字段、header/query、错误矩阵）
- [ ] 前端 transport 抽象设计确定（接口 + fallback 策略 + env 约定）

## 风险与注意事项

- 事件 id 的“稳定性来源”必须明确：优先使用后端 store 中的 event_id / change_id，避免“进程重启后从 1 重新计数”导致 resume 混乱。
- 流式端点对代理/网关敏感：需要明确 `Cache-Control`、buffering 行为（后续如上 Nginx 需追加配置项）。
- 不要在前端自行推断 Story/Task 状态；一切以事件流/后端 API 为准。

