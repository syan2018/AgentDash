# 修复 Workflow 推进后的 Agent 工具与 Hook 注入热更新

## 背景

当前 Agent 在执行 `complete_lifecycle_node` 推进 Workflow/Lifecycle node 时，后端会计算新的 phase capability surface，并调用运行时热更新路径：

```text
complete_lifecycle_node
→ LifecycleOrchestrator::advance_current_node
→ apply_activated_phase_nodes
→ apply_to_running_session
→ SessionHub::replace_current_capability_surface
→ AgentConnector::update_session_tools
```

但 review 发现这条链路没有真正让当前 Agent loop 获得新工具与 hook 注入：

- `Agent::run_loop()` 在 turn 开始时把 `state.tools` 复制为本轮局部 `tool_instances`，并把 tool schema 复制进 `AgentContext.tools`；后续 provider request 和 tool lookup 都继续读旧快照。
- `CapabilityChanged` hook 的 `resolution.injections` 目前只进入 trace envelope，没有进入 Agent 输入、pending action、Bundle turn delta 或 connector context update。
- `HookRuntimeDelegate::emit_hook_injection_fragments()` 只发 `ContextAuditBus`，没有回灌当前 `TurnFrame.context_bundle.turn_delta`。
- `replace_current_capability_surface()` 只更新 active turn，没有同步 `runtime.session_profile`，下一轮 prompt 可能回退旧工具面。

这会导致 UI/日志上看似工具热更新完成，但模型实际请求仍使用旧工具 schema，新 phase 的工具、约束或能力提示没有稳定进入 Agent。

## 目标

1. Workflow/Lifecycle node 推进后，当前 running turn 的下一次 LLM request 必须使用最新工具 schema。
2. 当前 running turn 的工具执行查找必须使用最新工具实例，不能继续从旧 `tool_instances` 查找。
3. `CapabilityChanged` hook 产出的 injection 必须有明确数据面，并被 Agent 实际消费或明确转为 pending/Bundle 更新。
4. live capability surface 更新后，session profile 必须同步，后续 prompt 不回退旧 MCP/VFS/FlowCapabilities。
5. 增加测试覆盖，避免“runtime 状态更新了，但 Agent loop 没更新”的假阳性。

## 设计决策

### D1. System prompt 不是运行期动态注入面

正常运行中不应因为 workflow phase / hook injection 变化去刷新 PiAgent system prompt。
system prompt 是缓存友好的稳定前缀，只在 prompt 边界由 pipeline 构建；如果出现
“需要动态修改 system prompt 才能让 Agent 看到”的内容，说明该内容的归属错了：

- 工具能力变化：进入 live tool registry，并在下一次 provider request 前刷新 tool schema。
- 当前 step / hook / phase 的运行期补充：进入动态 Agent 输入面（steering / follow-up / pending action），不进入 system prompt。
- 跨 turn 稳定业务上下文：下一轮 prompt 重新装配 Bundle / session profile 后再进入 system prompt。

因此本任务的当前 turn 热更新修复不实现 `update_session_context_bundle()` 驱动的
live system prompt 刷新；如未来接入 Bundle `turn_delta`，也必须明确它的消费端是
动态上下文消息或下一轮 prompt 装配，而不是运行中重设 system prompt。

## 非目标

- 不做旧 API/数据库兼容方案；当前项目未上线，直接保持最正确状态。
- 不重构整个 workflow engine。
- 不把所有 connector 一次性切到结构化 Bundle 消费；本任务优先保障 PiAgent live turn 行为正确。
- 不在 running turn 中动态重设 system prompt；动态内容必须走 Agent 动态输入面。

## 问题拆解

### 1. Agent loop 工具快照不可热更新

涉及文件：

- `crates/agentdash-agent/src/agent.rs`
- `crates/agentdash-agent/src/agent_loop.rs`
- `crates/agentdash-executor/src/connectors/pi_agent/connector.rs`

当前 `Agent::set_tools()` 只写 `AgentState.tools`。但 `run_loop()` 启动时已经复制出：

- `tool_instances`
- `AgentContext.tools`

后续 provider request 使用 `context.tools`，工具执行使用 `tool_instances`。因此 active turn 内的 `set_tools()` 对本轮无效。

建议方向：

- 引入可共享的 live tool registry，例如 `Arc<RwLock<Vec<DynAgentTool>>>` 或专用 `ToolRuntimeState`。
- provider request 前重新生成 `ToolDefinition` 列表。
- `prepare_tool_call()` 前读取最新工具实例。
- 保留当前静态快照能力作为 fallback 或测试便利，但 active loop 必须支持 live source。

### 2. CapabilityChanged hook injection 没有消费通道

涉及文件：

- `crates/agentdash-application/src/session/hub/hook_dispatch.rs`
- `crates/agentdash-application/src/session/hook_delegate.rs`
- `crates/agentdash-application/src/hooks/provider.rs`

当前 `emit_capability_changed_hook()` 只触发 evaluate，并把 injections 写入 trace。运行期通用 injections 又不再自动桥接为 inline user message，因此模型不会看到这些内容。

需要明确语义并实现其中一条闭环：

- 方案 A：将 `CapabilityChanged` injections 转成 pending hook action，由下一次 `transform_context()` 消费为 steering。
- 方案 B：将 injections 转成 `TurnFrame.context_bundle.turn_delta`，并通过 connector context update 刷新当前 Agent context。
- 方案 C：保持 injections 只进 trace，但把 capability update markdown 作为唯一正式 steering 通道，并删除/弱化误导性的 injection 期待。

推荐优先采用 A 或 B，避免 Hook 面板显示“已注入”但 Agent 实际不可见。

### 3. 运行期 hook fragment 没有回灌 Bundle

涉及文件：

- `crates/agentdash-application/src/session/hook_delegate.rs`
- `crates/agentdash-application/src/session/hub/tool_builder.rs`
- `crates/agentdash-spi/src/connector/mod.rs`

当前 `emit_hook_injection_fragments()` 只发 audit。根据现有 spec，目标态应该同时进入 `turn_delta` 或触发 `update_session_context_bundle()`。

建议方向：

- 在 Application 层提供一个安全的 current turn bundle mutation 入口。
- hook delegate 在 evaluate 后将可见 fragment 追加到当前 turn 的 `context_bundle.turn_delta`。
- 对 PiAgent 调用 `update_session_context_bundle()` 或在下一次 provider request 前重新读取/render bundle。

### 4. live capability surface 没有写回 session_profile

涉及文件：

- `crates/agentdash-application/src/session/hub/tool_builder.rs`
- `crates/agentdash-application/src/session/prompt_pipeline.rs`

当前 active turn 更新后，`session_profile` 仍保留旧 surface。下一轮 prompt 若请求没有显式携带 `mcp_servers/flow_capabilities/vfs`，会从旧 profile 恢复，导致能力回退。

修复要求：

- `replace_current_capability_surface()` 成功后同步更新 `runtime.session_profile`。
- 测试下一轮 prompt 未携带显式能力配置时仍继承最新 surface。

## 验收标准

- Agent 在同一 running turn 内完成 lifecycle node 后，后续 LLM provider request 的 tool schema 包含新 phase 的工具集。
- Agent 在同一 running turn 内能调用新 phase 新增的工具；旧 phase 被移除的工具不再可调用。
- `CapabilityChanged` hook 产出的有效 injection 不再只停留在 trace；要么进入 Agent steering，要么进入 Bundle turn delta 并触发 PiAgent 上下文刷新。
- live capability surface 更新后，`session_profile` 与 active turn 保持一致。
- 新增测试覆盖：
  - `agentdash-agent`：运行中 `set_tools/update tools` 后，下一次 provider request 使用新 tool definitions。
  - `agentdash-agent`：运行中工具执行查找使用新工具实例。
  - `agentdash-application`：`replace_current_capability_surface()` 同步 active turn 与 `session_profile`。
  - `agentdash-application` 或集成测试：`CapabilityChanged` injection/steering 数据面被真实消费。

## 建议实施顺序

1. 先修 Agent live tool source，让热更新具备真实底座。
2. 再修 `replace_current_capability_surface()` 的 `session_profile` 同步，防止跨 turn 回退。
3. 收口 `CapabilityChanged` injection 语义，选定 pending action 或 Bundle turn delta 方案。
4. 补测试，特别避免只断言外层 runtime 状态的假阳性。

## Review Finding 追踪

- P1：`Agent::run_loop()` 工具快照导致当前 loop 无法获得热更新。
- P1：`CapabilityChanged` hook injections 只进 trace，没有进入 Agent 输入。
- P2：运行期 hook fragment 只发审计，没有回灌当前 Bundle。
- P2：live capability surface 未同步 `session_profile`，下一轮可能回退旧工具面。

## 完成记录

- 2026-05-07：Agent loop 工具热更新已改为 live tool source，provider request schema 与 tool lookup 不再只读 turn 开始时的旧快照。
- 2026-05-07：`CapabilityChanged` hook injection 进入统一 `SessionRuntimeHookInjectionSink`，结构化回灌 `TurnFrame.context_bundle.turn_delta` 并同步进入 audit；需要当前 running turn 立即消费的内容通过 live session notification/steering 投递，不重设 system prompt。
- 2026-05-07：`replace_current_capability_surface()` 同步 active turn 与 `session_profile`，下一轮 prompt 不再回退旧工具面。
- 2026-05-07：进一步由 `workflow-runtime-context-path-convergence` 任务收束为统一 runtime context transition applier，避免后续 live apply、pending 入队、next-turn apply 各自实现半条链。
