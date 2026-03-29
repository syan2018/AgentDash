# 重构执行方案 — Application / Connector-Contract 分层重构

> 基于 2026-03-28 代码实际状态编写，对照 PRD 逐项审计后精确定义 delta。

---

## 执行状态总览

| Phase | 状态 | 完成时间 |
|-------|------|---------|
| A1: BackendTransport 端口 trait | ✅ completed | 2026-03-28 |
| A2: relay 类型抽象隔离 | ✅ completed | 2026-03-28 |
| B1: RelayFsMountProvider 审计 | ✅ completed | 2026-03-28 |
| B2: workspace_resolution 迁移 | ✅ completed | 2026-03-28 |
| B3: HookRuntimeDelegate 迁移 | ✅ completed | 2026-03-28 |
| C1: Gateway → TaskLifecycleService | ✅ completed | 2026-03-28 |
| D1: SessionHub 模块化 | ✅ completed | 2026-03-28 |
| E1: connector-contract 审计 | ✅ completed | 2026-03-28 |
| E2: Application relay 依赖移除 | ✅ completed | 2026-03-28 |
| E3: API re-export shim 清理 | ✅ completed | 2026-03-28 |
| E4: executor 层清理 | ✅ completed | 2026-03-28 |
| F: 全量验证 | ✅ completed | 2026-03-28 |

### 验证结果
- `cargo check --workspace` ✅ 通过
- `cargo clippy --workspace` ✅ 通过（排除预存 lint）
- `cargo test --workspace` ✅ 202 passed, 0 failed, 1 ignored

---

## 0. 现状审计结论

### 已完成项（PRD 中标记但代码已迁移）

| PRD 项 | 当前状态 | 说明 |
|--------|---------|------|
| R2: Agent 生命周期 SPI 抽取 | ✅ 已完成 | `lifecycle.rs` 已在 `connector-contract`，agent 已 re-export |
| R4: SessionHub 迁入 Application | ✅ 已完成 | `application/session/hub.rs` (2148行) |
| R4: HookSessionRuntime 迁入 Application | ✅ 已完成 | `application/session/hook_runtime.rs` |
| R6: session_plan → Application | ✅ 已完成 | API 层 shim 已删除 |
| R6: session_context → Application | ✅ 已完成 | API 层 shim 已删除 |
| R6: execution_hooks → Application | ✅ 已完成 | API 层 shim 已删除 |
| R6: runtime_bridge → Application | ✅ 已完成 | API 层保留 relay-specific adapter |
| R6: address_space_access 核心 → Application | ✅ 已完成 | relay_service, tools, inline_persistence 均在 Application |
| R6: task_agent_context 核心 → Application | ✅ 已完成 | API 层仅剩 thin adapter |
| R5: TaskLifecycleService 创建 | ✅ 已完成 | `application/task/service.rs`，含 `TurnDispatcher` 端口 |

---

## 1. 分阶段执行记录

### Phase A: 端口层定义

#### A1: BackendTransport 端口 trait ✅

**变更摘要**：
- 新增 `application/backend_transport.rs` — 定义 `BackendTransport` trait（3 方法）、`GitRepoInfo`、`TransportError`
- `application/workspace/resolution.rs` — `BackendAvailability` 改为 `BackendTransport` 的 blanket impl
- `application/lib.rs` — 注册新模块

**设计决策**：
- 采用细粒度方法（非 JSON 泛化）
- `browse_directory` 不纳入 trait（MountProvider 直接用 BackendRegistry）
- `BackendAvailability` 保留为 blanket impl 而非替换

#### A2: relay 类型抽象隔离 ✅

**变更摘要**：
- `application/runtime_bridge.rs` — 移除 relay FileEntry 转换函数，仅保留 ACP/MCP 转换
- `api/runtime_bridge.rs` — 从纯 re-export 升级为 adapter，承接 relay FileEntry 转换逻辑

---

### Phase B: 代码迁移

#### B1: RelayFsMountProvider 确认 ✅

审计确认 `api/mount_providers/relay_fs.rs` 位置正确，无需移动。

#### B2: workspace_resolution 迁移 ✅

**变更摘要**：
- 新增 `application/workspace/detection.rs` — 核心检测逻辑
- `api/workspace_resolution.rs` — 精简为薄 adapter（错误映射）
- `api/relay/registry.rs` — 新增 `list_online_ids()`
- `api/workspace_resolution.rs` — 实现 `BackendTransport for BackendRegistry`
- 删除 `AppStateBackendAvailability` adapter

#### B3: HookRuntimeDelegate 迁移 ✅

**变更摘要**：
- 新增 `application/session/hook_delegate.rs` — 权威实现
- `executor/runtime_delegate.rs` → 删除（Phase E4）
- `pi_agent.rs` → 直接引用 `agentdash_application::session::HookRuntimeDelegate`

---

### Phase C: Gateway → Service 切换

#### C1: TaskLifecycleService 集成 ✅

**变更摘要**：
- 新增 `api/bootstrap/turn_dispatcher.rs` — `AppStateTurnDispatcher` + `DeferredTurnDispatcher`
- `app_state.rs` — 构建 `TaskLifecycleService`，使用 `DeferredTurnDispatcher` 解决循环依赖
- `routes/task_execution.rs` — 路由直接调用 `TaskLifecycleService`
- 删除 `api/bootstrap/task_execution_gateway.rs`
- 删除 `application/task/execution.rs` 中的 `TaskExecutionGateway` trait 及泛型函数

---

### Phase D: SessionHub 模块化

#### D1: 类型提取 ✅

**变更摘要**：
- 新增 `application/session/types.rs` — `UserPromptInput`、`PromptSessionRequest`、`ResolvedPromptPayload`、`CompanionSessionContext`、`SessionMeta`、`SessionExecutionState`
- `hub.rs` 从 2148 行降至 1787 行

**备注**：进一步的 store/prompt/event_pipeline/hook_bridge 拆分属于下一轮优化范围。本次重构的核心目标（分层架构修正）已通过类型独立化达成。

---

### Phase E: 依赖清理

#### E1: connector-contract 审计 ✅

审计确认无多余依赖。`json-patch` 用于 discovery 流 patch 格式，保留合理。

#### E2: Application relay 依赖移除 ✅

从 `application/Cargo.toml` 移除 `agentdash-relay` 依赖。Application 代码中零 relay 引用。

#### E3: API re-export shim 清理 ✅

**删除的 shim 文件**：
- `api/execution_hooks/mod.rs`
- `api/session_plan.rs`
- `api/session_context.rs`

**消费者更新**：
- `app_state.rs` → 直接引用 `agentdash_application::hooks::AppExecutionHookProvider`
- `routes/acp_sessions.rs` → 直接引用 `agentdash_application::session_context::*`
- `routes/workflows.rs` → 直接引用 `agentdash_application::session_context::*`
- `routes/backends.rs` → 直接引用 `agentdash_application::session_context::*`

#### E4: executor 层清理 ✅

**变更摘要**：
- 删除 `executor/runtime_delegate.rs` (31KB)
- `pi_agent.rs` → 直接引用 `agentdash_application::session::HookRuntimeDelegate`
- `executor/Cargo.toml` → `agentdash-application` 升级为 `pi-agent` feature 正式依赖
- `executor/lib.rs` → 移除 deprecated 模块声明和 re-export

---

### Phase F: 全量验证 ✅

| 检查项 | 结果 |
|-------|------|
| `cargo check --workspace` | ✅ 通过 |
| `cargo clippy --workspace` (排除预存 lint) | ✅ 通过 |
| `cargo test --workspace` | ✅ 202 passed, 0 failed |
| 无新增 deprecated 注解 | ✅ |

---

## 2. 架构验收检查清单

- [x] `connector-contract` 不依赖 `executors`（vibe-kanban）
- [x] `connector-contract` 包含完整 Agent lifecycle SPI
- [x] Application 层不依赖 `agentdash-relay`
- [x] `BackendRegistry` 通过 `BackendTransport` trait 间接访问
- [x] 无 `TaskExecutionGateway` trait
- [x] SessionHub 公共类型已提取到 `types.rs`
- [x] API 层无纯 re-export shim 文件（`execution_hooks`、`session_plan`、`session_context` 已删除）
- [x] executor 层无 deprecated wrapper（`runtime_delegate.rs` 已删除）
- [x] `cargo check --workspace` 通过
- [x] `cargo test --workspace` 全部通过

### 遗留建议（下一轮优化）

1. ~~**SessionHub 进一步拆分**~~：✅ 已在 03-29 完成（拆出 `hub_support.rs`、`session_store.rs`）
2. **address_space_access shim 清理**：`api/address_space_access/` 下仍有多个 `pub use application::...` 的 shim 文件
3. ~~**预存 clippy lint 修复**~~：✅ 已在 03-29 完成（参数结构体重构，零 crate 级 allow）
