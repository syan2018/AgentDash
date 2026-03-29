# 分层架构收尾重构 — 执行方案

> 基于 03-28 重构成果，完成剩余架构改进。所有 Phase 均在本轮落地。
> 创建时间：2026-03-29

---

## 执行状态总览

| Phase | 目标 | 状态 | 完成时间 |
|-------|------|------|---------|
| 1 | executor/connector.rs re-export 清理 | ✅ done | 2026-03-29 |
| 2 | 消除 Executor → Application 反向依赖 | ✅ done | 2026-03-29 |
| 3 | 简化 DeferredTurnDispatcher | ✅ done | 2026-03-29 |
| 4 | build_task_session_context_response 迁入 Application | ✅ done | 2026-03-29 |
| 5 | SessionHub 模块拆分 | ✅ done | 2026-03-29 |
| 6 | 预存 clippy lint 修复 | ✅ done | 2026-03-29 |
| 7 | 全量验证 | ✅ done | 2026-03-29 |

---

## Phase 1: executor/connector.rs re-export 清理

**目标**：删除 `executor/connector.rs` 这个纯转发中间层，在 `lib.rs` 直接 re-export。

**实际变更**：
- 删除 `crates/agentdash-executor/src/connector.rs`
- `lib.rs` 直接从 `agentdash-connector-contract` 和 `adapters` re-export
- 5 个内部消费者 (`pi_agent.rs`, `remote_acp.rs`, `composite.rs`, `pi_agent_mcp.rs`, `vibe_kanban.rs`) 更新为直接引用源 crate

---

## Phase 2: 消除 Executor → Application 反向依赖（方案 B — 重构 trace 发射）

**目标**：移除 `agentdash-executor` 对 `agentdash-application` 的依赖。

**实际变更**：
1. `HookSessionRuntime` 新增 `broadcast::Sender<HookTraceEntry>` 字段，`append_trace()` 同时向 broadcast 发送
2. `HookSessionRuntimeAccess` trait 新增 `subscribe_traces()` 方法（默认返回 None）
3. `ExecutionContext` 新增 `runtime_delegate: Option<DynAgentRuntimeDelegate>` 字段
4. `SessionHub` 在构建 `ExecutionContext` 时通过 `HookRuntimeDelegate::new()` 填充 `runtime_delegate`
5. `PiAgentConnector` 改用 `context.runtime_delegate` 和 `context.hook_session.subscribe_traces()`
6. `HookRuntimeDelegate` 移除 `trace_event_tx` 字段和 `new_with_trace_events()` 构造器
7. `executor/Cargo.toml` 移除 `agentdash-application` 依赖
8. `connector-contract/Cargo.toml` 新增 `tokio = { workspace = true, features = ["sync"] }`

**验证**：executor 代码中零 `agentdash_application` 引用。

---

## Phase 3: 简化 DeferredTurnDispatcher

**目标**：消除 `DeferredTurnDispatcher` wrapper，`AppStateTurnDispatcher` 直接持有独立依赖。

**实际变更**：
- `AppStateTurnDispatcher` 改为持有 `SessionHub`, `BackendRegistry`, `RepositorySet`, `RestartTracker`, `remote_sessions`
- 仅 auto-retry 路径通过 `tokio::sync::OnceCell<Arc<TaskLifecycleService>>` 延迟注入，通过 `set_retry_service()` 绑定
- 删除 `DeferredTurnDispatcher` 类型
- 所有自由函数（`dispatch_cloud_native`, `dispatch_relay`, `relay_start_prompt`, `relay_cancel`, `schedule_auto_retry`）不再接受 `&Arc<AppState>`，改为接受具体依赖
- `app_state.rs` 构建顺序调整：repos → dispatcher → task_lifecycle_service → state → set_retry_service

---

## Phase 4: build_task_session_context_response 迁入 Application

**目标**：将业务编排逻辑从 API 路由层迁入 Application 层。

**实际变更**：
- 新增 `application/task/context_builder.rs`：包含 `BuiltTaskSessionContext` 和 `build_task_session_context()` + `resolve_task_executor_source()`
- `api/routes/task_execution.rs` 和 `api/routes/acp_sessions.rs` 改为调用 application 层函数
- 从 `task_execution.rs` 删除旧函数和 `BuiltTaskSessionContextResponse` 类型

---

## Phase 5: SessionHub 模块拆分

**目标**：将 `hub.rs`（约 1986 行）拆分为多个子模块。

**实际变更**：

| 新模块 | 职责 | 行数 |
|--------|------|------|
| `hub_support.rs` | 通知构建器、事件解析器、SessionRuntime/TurnTerminalKind 类型、meta_to_execution_state | ~240 |
| `session_store.rs` | SessionStore — session meta/history 持久化读写 | ~110 |
| `hub.rs` | SessionHub 核心 impl + 测试 | ~1630 |

hub.rs 从 1986 行降至 1632 行（含 ~700 行测试），核心逻辑约 930 行。

---

## Phase 6: clippy lint 修复 — 参数结构体重构

**目标**：以正确方式消除所有 clippy 警告，不使用 crate 级 allow 绕过。

**实际变更**：

自动修复：
- `cargo clippy --fix`：`collapsible_if`, `redundant_closure`, `default_constructed_unit_structs`
- `pi_agent_provider_registry.rs`：提取 `BridgeFactory` type alias（消除 `type_complexity`）
- `hook_delegate.rs`：添加 `#[allow(clippy::new_ret_no_self)]`

`too_many_arguments` 全部通过提取参数结构体解决（14 个函数，0 个 crate 级 allow）：

| 结构体 | 所在模块 | 消除的函数 |
|--------|---------|-----------|
| `TextSearchParams` | `application::address_space::relay_service` | `search_text_extended`, `search_inline` |
| `SearchParams` | `local::tool_executor` | `search`, `run_ripgrep` |
| `FallbackCollector` | `local::tool_executor` | `fallback_walk`（重构为 struct 方法） |
| `TurnEventContext` | `application::task::gateway::turn_monitor` | `handle_turn_notification`, `resolve_failure_outcome` |
| `HookTriggerInput` | `application::session::hub` | `emit_session_hook_trigger` |
| `ToolCallArtifactInput` | `application::task::gateway::repo_ops` | `persist_tool_call_artifact` |
| `TaskTurnServices` | `application::task::gateway::turn_context` | `prepare_task_turn_context` |
| `CompanionDispatchConfig` | `application::task::tools::companion` | `build_companion_dispatch_plan` |
| `SubagentResult` | `application::hooks` | `build_subagent_result_context` |
| `EventDescription` | `executor::connectors::pi_agent` | `make_event_notification` |
| `PiAgentConnectorDeps` | `api::app_state` | `build_pi_agent_connector` |

---

## Phase 7: 全量验证

| 检查项 | 结果 |
|--------|------|
| `cargo check --workspace` | ✅ 通过 |
| `cargo clippy --workspace` | ✅ 仅 third_party 1 warning |
| `cargo test --workspace` | ✅ 202 passed, 0 failed, 1 ignored |
| executor 无 application 依赖 | ✅ Cargo.toml 和源码均无引用 |

---

## 重构后依赖关系图

```
domain ← connector-contract ← application ← api
                             ↗
              executor ------
```

**关键改进**：
- executor → application 反向依赖已消除
- DeferredTurnDispatcher 间接层已删除
- 业务编排逻辑已下沉到 application 层
- SessionHub 按职责拆分为 3 个模块
- clippy 警告已全部处理
