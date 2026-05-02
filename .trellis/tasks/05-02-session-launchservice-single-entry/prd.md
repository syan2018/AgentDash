# PRD：会话启动单入口 LaunchService 收敛

## 1. 背景与动机

当前会话拉起存在多来源并行：

- HTTP 用户发送（`/sessions/{id}/prompt`）
- Hook auto-resume
- Companion 回流续跑
- Routine 调度触发
- Task start/continue
- Workflow Orchestrator node kickoff
- Local relay command prompt

虽然生命周期判定函数已集中（`resolve_session_prompt_lifecycle`），但“判定结果的消费与请求组装”仍分散在多个模块，导致：

1. 同一 lifecycle 在不同来源路径行为不一致；
2. 调试成本高，出现问题时无法快速定位到统一主线；
3. 容易出现“看似在续跑，实际走了初始化重路径”的语义漂移。

本 PRD 目标是建立**单一会话启动主线**，保证“入口可多，主线唯一”。

---

## 2. 问题陈述

### 2.1 现状问题

1. **入口分散，缺少统一启动服务**
   - 多个来源直接构造 `PromptSessionRequest` 并调用 `start_prompt`。
   - 无法保证所有来源遵循同一 lifecycle 消费规则。

2. **Plain 续跑语义未全域硬约束**
   - 已在 API owner 路径收敛为 continuation-first；
   - 但其它来源仍可能在 Plain 语义下进入 compose 重路径。

3. **owner 语义与 bootstrap 状态契约不够显式**
   - owner 绑定与 `bootstrap_state` 的一致性缺少统一入口约束。

### 2.2 目标用户痛点

- 团队需要一个“最直观、最清晰”的会话启动主线，避免再次在生命周期与上下文注入路径上迷路。

---

## 3. 产品目标

### 3.1 核心目标

建立 `SessionLaunchService` 单入口，统一所有会话启动来源，并采用**二层语义模型**：

1. **公开语义仅两类：Init / Continue**
2. **Continue-first：除首轮初始化外默认都走 Continue**
3. **恢复策略是 Continue 的内部实现细节，不再作为并列主线概念**
4. **所有来源最终经同一 launch pipeline**

### 3.2 成功标准

1. 启动链路可归纳为固定 6 步（见第 6 节）。
2. 在 Phase 1 范围内（HTTP + hook auto-resume）不再出现 Continue 误入 owner 初始化 compose。
3. 引入约束测试，防止未来回归。

---

## 4. 非目标（本轮不做）

1. 不改动 API/协议外观（不新增前端协议字段）。
2. 不做数据库 schema 变更。
3. 不在本 PRD 内重构会话 UI 展示逻辑。
4. 不在 Phase 1 处理 Routine/Task/Workflow/Local 全量接线（按阶段推进）。

---

## 5. 范围与分期

### Phase 1（本次优先）

- 收敛来源：
  - HTTP prompt 主通道
  - hook auto-resume
- 新增 `SessionLaunchService` 基础结构与 launch 主线；
- 由这两条来源切换到统一 launch 入口；
- 增加 lifecycle 约束回归测试骨架。

### Phase 2

- 收敛来源：Routine + Companion parent resume

### Phase 3

- 收敛来源：Task + Workflow Orchestrator + Local relay

### Phase 4

- 全量一致性回归矩阵 + 清理旧分散路径。

---

## 6. 目标架构（统一主线）

引入 `SessionLaunchService::launch(intent)`，固定执行 6 步：

1. `load_session_meta_and_bindings`
2. `resolve_lifecycle`（统一调用 `resolve_session_prompt_lifecycle`）
3. `build_base_request`（来源归一）
4. `assemble_if_needed`
   - `Init`：走初始化特化 compose path
   - `Continue`：默认直通 continuation path；仅在 runtime 不在线时启用恢复策略
5. `finalize_request`
6. `start_prompt_with_follow_up`

内部生命周期到公开语义的映射：

- `SessionPromptLifecycle::OwnerBootstrap` -> `Init`
- `SessionPromptLifecycle::Plain` -> `Continue`
- `SessionPromptLifecycle::RepositoryRehydrate(*)` -> `Continue`（恢复策略分支）

---

## 7. 功能需求（FR）

### FR-1：统一入口

- 必须提供 `SessionLaunchIntent` 统一描述来源与输入。
- 所有接入来源最终调用 `SessionLaunchService::launch`。

### FR-2：二层模型硬约束

- 对外只暴露两类启动语义：`Init` 与 `Continue`。
- `Continue` 不得进入 owner 初始化 compose。
- `Init` 仅在 `bootstrap_state=Pending` 且满足首轮注入条件时可触发。
- 恢复策略（内部）仅作为 Continue 的执行细分：
  - runtime 在线：直接续跑；
  - runtime 不在线 + connector 支持 restore：executor-state rehydrate；
  - runtime 不在线 + connector 不支持 restore：system-context rehydrate。

### FR-3：可观测性

- launch 结果返回 `lifecycle_kind` 与 `path_kind`（仅内部/日志使用即可）。
- 出错时能明确定位处于哪一步（1~6）。

### FR-4：兼容现有行为

- 保持现有 API 入参与返回结构不变；
- 对外行为保持等价或更清晰（不引入新交互成本）。

---

## 8. 状态约束（Invariants）

1. **P0：Continue 不得触发 owner bootstrap compose**（等价于当前 `SessionPromptLifecycle::Plain` 禁止 init compose）。
2. **P1：HTTP 与 Hook 必须统一走同一 launch 主线**，不得各自旁路组装。
3. **P1：启动审计触发与生命周期一一对应**，禁止“有效 bundle 不变但重复 bootstrap 审计”。
4. **P1：生产内建入口（HTTP / hook auto-resume）默认 strict launch**，augmenter 缺失时 fail-fast，不允许裸请求静默降级。
5. `bootstrap_state=Pending` 只能在首轮 bootstrap 成功后转 `Bootstrapped`。
6. 同一来源不得绕过 launch service 直接构造“最终请求 + start_prompt”。

---

## 9. 影响文件（初版）

- `crates/agentdash-application/src/session/*`
  - `types.rs`
  - `augmenter.rs`
  - `prompt_pipeline.rs`
  - 新增 `launch_service.rs`（拟）
- `crates/agentdash-api/src/routes/acp_sessions.rs`
- `crates/agentdash-application/src/session/hub/hook_dispatch.rs`

后续阶段扩展：

- `crates/agentdash-application/src/routine/executor.rs`
- `crates/agentdash-application/src/task/service.rs`
- `crates/agentdash-application/src/workflow/orchestrator.rs`
- `crates/agentdash-application/src/companion/tools.rs`
- `crates/agentdash-local/src/command_handler.rs`

---

## 10. 验收标准（Phase 1）

### AC-1 行为验收

1. HTTP 主通道在 Continue 语义下不进入 owner 初始化 compose（含 Plain 与 Rehydrate）。
2. hook auto-resume 与 HTTP 主通道使用同一 launch 主线。
3. HTTP 与 hook auto-resume 都使用 strict launch（augmenter 缺失时不会触发 connector prompt）。

### AC-2 回归验收

1. 二层语义约束测试通过（Init/Continue）；内部恢复策略覆盖 runtime 在线与离线两类。
2. `cargo check -p agentdash-api` 与核心目标测试通过。

### AC-3 可维护性验收

1. 新增单入口模块后，启动流程文档可描述为一条主线；
2. 代码注释明确“为何 Plain 不能走 init compose”。

---

## 11. 风险与缓解

1. **风险：旧路径与新入口并存导致双逻辑**
   - 缓解：Phase 1 仅切两条关键来源，并标记旧路径为待迁移。

2. **风险：来源差异导致 launch intent 过度复杂**
   - 缓解：先收敛最小公共字段，source-specific 字段通过可选 overrides 处理。

3. **风险：重构影响上下文注入完整性**
   - 缓解：保留现有 assemble/finalize 能力，仅改变入口组织，不先改数据结构。

---

## 12. 开放问题

1. owner 绑定到“已有 session”时是否强制重置为 Pending（建议在 Phase 2 一并收敛）。
2. `path_kind` 是否需要持久化到审计事件（当前建议先日志，不落库）。

---

## 13. 执行备注

本 PRD 对齐任务：

- `.trellis/tasks/05-02-session-launchservice-single-entry/task.json`
- 子阶段：Phase 1 -> Phase 4

本轮先执行：**Phase 1（HTTP + hook auto-resume）**。
