# Live Canvas 后端分片实施简报

## 拆解批次（2~4 批）

1. **Canvas 领域 + API 最小闭环**
   - 目标：定义 Canvas/Bindings 实体、仓储、CRUD 接口和 runtime snapshot 路由；提供最小 `create/present` 执行链（已在 PRD/plan 要求）。
   - 关键文件：`crates/agentdash-domain/`、`crates/agentdash-application/`、`crates/agentdash-infrastructure/src/persistence/sqlite/`、`crates/agentdash-api/src/routes/`（参考 `acp_sessions.rs` 如何构造 `FlowCapabilities`）。
   - 建议顺序：首先完成实体+repo+service，再写入 REST API 并暴露 `GET /api/canvases/{id}/runtime-snapshot?session_id=...`。
   - 风险：若 snapshot 结果未与新 entity 绑定，会导致后续 provider 无法定位文件；需要和 runtime contract 团队同步字段。
   - 验收：能够按 `project_id` 创建 Canvas，保存 `entry_file`/`sandbox_config`/`bindings`，API 返回 runtime 快照 payload（文件列表 + 数据绑定元）。
   - 最小闭环：域模型 + runtime snapshot API + `present_canvas` 事件可让 Session 端收到 `canvas_presented` 并拉取文件，此批先落地意义最大。

2. **Canvas Mount Provider + Flow Capability**
   - 目标：新增 `PROVIDER_CANVAS_FS`、`canvas_fs` provider，并让 Canvas mount 追加进 session address space；扩展 `FlowCapabilities` 和 `session_plan` 工具摘要。
   - 关键文件：`crates/agentdash-application/src/address_space/mount.rs`（参考 `build_lifecycle_mount` 增加）、`.../provider.rs`、`.../relay_service.rs`、`.../tools/provider.rs`、`task/session_runtime_inputs.rs`（附加 mount）、`session_plan.rs`、`crates/agentdash-spi/src/connector.rs`、`crates/agentdash-api/src/routes/acp_sessions.rs`（现有 owner-specific flow capabilities）。
   - 顺序：在领域实体出炉后立即追加该 provider，并顺带更新 `FlowCapabilities`（增加 `canvas_preview`/`canvas_present` 类字段）以及 `acp_sessions` 的 request builder/`session_plan` 的工具列表。
   - 风险：provider 映射和 `fs_apply_patch` 需要正确声明 `MountEditCapabilities`，否则 `fs_apply_patch` 会 fallback 报错；`FlowCapabilities` 未同步会导致前端 prompt 提示错误。
   - 验收：`canvas_fs` 能读写 Canvas 目录，`session_runtime_inputs` 追加 Canvas mount，`FlowCapabilities` 反映新增工具，`session_plan` 里出现 Canvas 工具，`acp_sessions` 请求构造正确。

3. **Canvas Runtime Snapshot + Tools契约检查**
   - 目标：确认 runtime snapshot payload 与 iframe 预期一致, 并保证 `FlowCapabilities` 对 canvas 工具的可见性（`session_plan` 只显示已注入工具）。
   - 关键文件：`prd.md` 中定义的 payload，`frontend` 的 runtime expectation 需向 `backend` 明示；`agentdash-acp-meta/lib.rs`（meta 格式），`session_plan.rs`、`acp_sessions.rs`（提示文案）、`connector.rs`（flow cap struct）。
   - 顺序：可以与 provider 批并行，但要在 Panel 开发前完成 so runtime knows payload shape。
   - 风险：PRD 里强调 snapshot 不能塞入 `/sessions/{id}/context`，后端要开独立路由；若仍塞入会话上下文，系统事件会变得臃肿。
   - 验收：`GET /api/canvases/{id}/runtime-snapshot` 返回文件列表 + 字符串 alias，`FlowCapabilities` 中 Canvas 工具出现在 `session_plan` 输出，`agentdash-acp-meta` event 元数据可包含 `canvas_presented`。

## 最小推荐闭环

优先落地**批次 1（Canvas 领域+API）**：只要有 Canvas 实体、runtime snapshot API 以及 `present_canvas` event，就能通过现有 `fs_*` 和 `SessionPage` 事件框架完成“Create → Inject Data → Present → 拉 Snapshot → iframe 启动”闭环，其他批次功能可逐步叠加。

## PRD 与已有代码的冲突点

- PRD 建议 Canvas 不复用 `context_containers`；当前 Address Space 构建天然通过 `effective_context_containers` 只读 `project.config.context_containers`，因此必须在 `build_address_space` 之后追加 Canvas mount，否则会把 Canvas 错误归类为 context container。
- PRD 说明 runtime snapshot 应独立 API；但 `get_session_context()` 目前已经返回 address space + context snapshot（`crates/agentdash-api/src/routes/acp_sessions.rs` 的 `get_session_context`），后端不能直接把 Canvas 文件塞入此接口，否则会违反 “避免大量数据塞进 Session Notification” 的要求；唯一可行做法是开 `/api/canvases/.../runtime-snapshot`。

