# PRD：统一执行器 discovery 与 session prompt 路由

## 背景

当前系统中存在**两条完全独立的 prompt dispatch 路径**，导致 discovery 接口暴露的执行器集合与 session prompt 实际可路由的执行器集合不一致。

### 现象

- 前端 discovery 展示在线本机 backend 上报的远程 executors（CODEX / CLAUDE_CODE / GEMINI / OPENCODE 等）。
- 用户从 session 页选中这些 executor 后发送 prompt，云端主路由报错：`未知执行器 'CODEX'，无法路由到任何连接器`。
- 即使是 **Task-bound 的 relay 会话**，第一轮通过 `TurnDispatcher` 正确走 relay，后续用户在 chat view 发第二条消息时也会因走 `session_hub.start_prompt()` 而失败。
- Cancel 路径同理：session API 的 `cancel_session` 直接走 `session_hub.cancel()` → `CompositeConnector`，relay session 的 cancel 无法触达远程后端。

### 根因

系统存在两条分裂的 prompt dispatch 路径：

| 路径 | 入口 | 执行方式 | 覆盖执行器 |
|------|------|---------|-----------|
| Task 生命周期 | `TaskLifecycleService` → `TurnDispatcher` | cloud-native / relay 分流 | 全部 |
| Session prompt API | `acp_sessions::prompt_session` → `session_hub.start_prompt()` → `CompositeConnector` | 仅 cloud-native | 仅 PiAgent + 插件 |

`CompositeConnector` 的 routing table 只包含静态注册的子连接器（PiAgent + 插件），不包含通过 BackendRegistry 动态上报的远程执行器。

而 `TurnDispatcher` 拥有 relay 分发能力，但这个能力没有下沉到 connector 层，只在 API 层的 `dispatch_relay` 中实现——导致 session prompt API 无法复用。

## 决策：统一执行路径（方案 A 深化）

### 设计目标

> 业务层（SessionHub / prompt_pipeline）只看到 `connector.prompt() → ExecutionStream`，不区分执行器是本地进程还是远程后端。relay 与 cloud-native 的差异被封装在 connector 内部。

### 目标架构

```
ALL paths ──→ SessionHub.start_prompt()
                │
          prompt_pipeline.rs
            connector.prompt() → ExecutionStream
            → SessionTurnProcessor (统一事件处理)
            → persist / hooks / terminal / auto-resume
                │
          CompositeConnector
            ├─ PiAgent         → in-process stream
            ├─ RelayConnector  → WebSocket 桥接 stream   ← 新增
            └─ Plugin          → 插件提供的 stream
```

### 关键设计决策

#### 1. RelayAgentConnector 放在 `agentdash-application`

- Application 层已有 `BackendTransport` 端口 trait，`BackendRegistry`（API 层）实现它
- `RelayAgentConnector` 实现 `AgentConnector`（来自 `agentdash-spi`），通过 `RelayPromptTransport` trait 与远程后端交互
- 保持 `agentdash-executor` 只关注本地执行器

#### 2. relay 通知桥接为标准 ExecutionStream

- `RelayAgentConnector.prompt()` 内部创建 `mpsc` channel
- 在共享的 `RelaySessionSinkMap` 中注册 `session_id → tx`
- WebSocket handler 将 relay notification 投递到对应 tx
- receiver 包装为 `ExecutionStream`，被 `prompt_pipeline.rs` 统一消费
- **淘汰** `SessionHub.feed_turn_notification` 和 `signal_relay_terminal`

#### 3. `ExecutionContext` 扩展 `target_backend_id`

- `target_backend_id: Option<String>` — 由 workspace 解析得出
- cloud-native 执行器忽略此字段
- relay connector 据此决定发送到哪个后端

#### 4. `BackendTransport` 扩展为 `RelayPromptTransport`

在 `agentdash-application` 新增 sub-trait：

- `relay_prompt()` — 发送 prompt 命令
- `relay_cancel()` — 取消远程会话
- `list_online_executors()` — 列出在线后端执行器
- `register/unregister_session_sink()` — 注册/注销 per-session 通知接收端

Application 层定义自己的 payload 类型，API 层负责翻译为 relay 协议。

## 实施阶段

### Phase 0: 准备（不影响现有行为）

- `ExecutionContext` 加 `target_backend_id: Option<String>`，所有调用点设 `None`
- 定义 `RelayPromptTransport` trait（空实现）
- **验证**：编译通过，行为不变

### Phase 1: 实现 RelayAgentConnector

- `agentdash-application/src/relay_connector.rs` — 实现 `AgentConnector` trait
- `BackendRegistry` 实现 `RelayPromptTransport`
- 在 `AppState::new()` 中注册为 CompositeConnector 子连接器
- **验证**：`list_executors()` 返回含远程执行器，discovery 端点自然正确

### Phase 2: 切换 relay 通知路径

- WebSocket handler 改为通过 sink map 投递（有注册 → 新路径，无注册 → 旧路径 fallback）
- context 准备层填充 `target_backend_id`
- **验证**：relay session 通知正确流入 connector stream

### Phase 3: 删除旧 relay 分发路径

- `TurnDispatcher` 简化：删除 `dispatch_relay`，统一走 `session_hub.start_prompt()`
- 删除 `remote_sessions` map
- 删除 `SessionHub` 上的 `feed_turn_notification` / `signal_relay_terminal` / `set_session_processor_tx` / `set_session_hook_runtime`
- discovery 端点简化：不再手动合并 BackendRegistry executor
- cancel 路径自然修复
- **验证**：所有 prompt 路径统一，旧代码完全清除

## 验收标准

- [ ] 前端可见的 executor 集合与真实 prompt 能力一致
- [ ] 用户不会再遇到"下拉可选但发送 400"的行为
- [ ] 所有执行器（PI_AGENT / CODEX / CLAUDE_CODE / 插件）在 session prompt API 和 task lifecycle 中走同一条业务路径
- [ ] `SessionHub.start_prompt()` 是唯一的 prompt 入口
- [ ] relay session 的后续 turn / cancel / approve 正确路由
- [ ] `AGENTS.md` 中关于此问题的说明可以移除

## 相关文件

| 文件 | 变更类型 |
|------|---------|
| `crates/agentdash-spi/src/connector.rs` | `ExecutionContext` 加字段 |
| `crates/agentdash-application/src/backend_transport.rs` | 新增 `RelayPromptTransport` trait |
| `crates/agentdash-application/src/relay_connector.rs` | **新增** `RelayAgentConnector` |
| `crates/agentdash-api/src/app_state.rs` | 注册 relay connector |
| `crates/agentdash-api/src/workspace_resolution.rs` | `BackendRegistry` 实现新 trait |
| `crates/agentdash-api/src/relay/ws_handler.rs` | 通知路由改为 sink map |
| `crates/agentdash-api/src/relay/registry.rs` | 新增 sink map 管理 |
| `crates/agentdash-api/src/bootstrap/turn_dispatcher.rs` | 简化为统一路径 |
| `crates/agentdash-api/src/routes/discovery.rs` | 删除手动 BackendRegistry 合并 |
| `crates/agentdash-api/src/routes/acp_sessions.rs` | context 填充 backend_id |
| `crates/agentdash-application/src/session/hub.rs` | 删除 relay 专用方法 |
| `crates/agentdash-application/src/session/prompt_pipeline.rs` | 无变更（已是统一处理） |
| `crates/agentdash-application/src/session/turn_processor.rs` | 无变更（已是统一处理器） |

## 风险

| 风险 | 缓解 |
|------|------|
| relay stream 生命周期（后端断连时 sender 泄露） | `unregister_session_sink` + backend disconnect 事件清理 |
| CompositeConnector 动态路由过期 | `resolve_connector` miss 时 refresh 重试 |
| 非 Task-bound session 缺少 workspace context | relay executor 要求至少绑定 workspace（现有约束） |
| 并行过渡期 double-feed | sink map 有注册 → 新路径，无 → 旧路径（Phase 2 互斥） |
