# Companion 协作模型升级：灵活调度 + 上下文记忆继承 + 跨层开放

## 背景

当前 `companion_dispatch` / `companion_complete` 是一个**严格异步、仅 Story owner 可用**的子 Agent 派发机制。虽然基础设施已经较为完善，但存在以下局限：

1. **调度模型单一**：只有纯异步（fire-and-forget + 回调注入），`wait_for_completion=true` 被显式禁用。`AdoptionMode` 虽然有 `blocking_review`，但实际不阻塞父 Agent 执行流
2. **发起方限制过严**：仅 Story owner session 可 dispatch，Project Agent 完全无此能力
3. **上下文继承有限**：`CompanionSliceMode` 仅支持从父会话的 Hook context 中切片继承，不支持携带前 session 的历史上下文记忆
4. **定位偏向 subagent**：当前架构更像"上下级指派"，而非"会话间对等协作"

### 核心重定位

> **Companion 不是 subagent，而是一种通用的会话间协作方式。**

调度模式（同步/异步/回调）应该是调用参数，不是能力限制。发起方应该由业务场景决定，而不是硬编码到 session 类型上。

---

## Goal

将 companion 从"Story Agent 的专属子任务分发机制"升级为**"会话间通用协作原语"**，具体包括：

1. 支持灵活的调度模式（非阻塞回调 / 显式 await / 同步等待）
2. 开放 Project Agent 的 companion 能力
3. 支持 companion session 携带前 session 的上下文记忆
4. 保持 Task execution session 暂不开放 dispatch（当前只保留 complete 能力）

---

## Requirements

### R1：灵活调度模式

当前只有一种：异步派发 + `inject_notification` 回调。需要扩展为三种模式：

| 模式 | 行为 | 适用场景 |
|------|------|---------|
| **async-callback**（现有） | dispatch 后立即返回，companion 完成时注入通知到父会话流 | 并行协作：主 Agent 继续做别的事，companion 结果到了再消费 |
| **async-await**（新增） | dispatch 后立即返回，但父 Agent 可在后续任意 turn 调用 `await_companion(dispatch_id)` 阻塞等待结果 | 分阶段协作：先 dispatch 多个 companion 并行跑，后续汇总时逐个 await |
| **sync**（新增，打开 `wait_for_completion`） | dispatch 后 tool call 不返回，直到 companion 完成才带着结果返回——对 Agent 来说就是一次"慢的 tool call" | 简单委托：Project Agent 派 companion 调研一个问题，同步拿结果写进知识库 |

#### 设计要点

- `wait_for_completion` 参数从报错改为实际实现
- 新增 `await_companion(dispatch_id)` 工具，用于 async-await 模式
- 同步模式需要合理的超时策略（对 agent_loop 的 tool call timeout 有影响）
- 需要考虑 companion 执行失败/超时时父会话的降级处理

### R2：Project Agent 开放 companion 能力

- `FlowCapabilities` 中 Project Agent 的 `companion_dispatch` 和 `companion_complete` 改为 `true`
- `conditional_flow_tools()` 中 `ProjectAgent` owner kind 增加对应工具注入
- Project Agent 的 companion 使用场景：
  - 派发 companion 调研技术方案后汇总
  - 派发 companion 审查多个 Story 一致性
  - 拆解大需求为多个 Story 草案（子任务编排）
- **Task execution session 暂不开放 dispatch**，保持现状（只能 complete）

#### 设计要点

- 保留 `FlowCapabilities` 机制按需配置，但 Project Agent 默认开放
- Project Agent dispatch 的 companion session 绑定到 Project owner 下（需确认 SessionBinding owner 路由兼容性）
- Project 层的 context 结构与 Story 不同，`CompanionSliceMode` 的 slice 逻辑需要适配

### R3：Companion 上下文记忆继承

当前 `CompanionSliceMode` 仅从父会话的 HookSessionRuntime snapshot 中切片继承上下文。新增支持：

- **前 session 历史记忆注入**：companion session 可以携带指定 session 的对话历史 / 摘要作为初始上下文
- **跨 session 上下文传递**：不限于父子关系，允许从任意关联 session 继承上下文片段

#### 核心挑战：身份 shift 时的上下文组织

当 companion session 携带了来自另一个 session（甚至另一个 Agent 身份）的上下文时：

1. **Persona 冲突**：companion session 有自己的 Persona/角色定义，继承来的上下文中可能包含不同的角色指令
2. **上下文边界**：需要明确区分"继承的参考上下文"和"当前 session 的执行上下文"
3. **隐私/权限**：某些 session 的上下文可能不适合被其他 session 看到

#### 可能的设计方向

- 引入 `ContextMemorySlice` 概念：从源 session 中提取结构化的记忆片段（不是原始消息）
- 注入方式：作为 system prompt 的一个独立 section（例如 `## 参考上下文（来自 session X）`），而非混入 conversation history
- 由 dispatch 调用方指定要继承哪些 session 的记忆、以什么粒度（摘要 / 完整 / 仅结论）
- Persona 指令严格隔离：继承的记忆不包含源 session 的 persona/system prompt

### R4：并发管理

支持一个 session 同时 dispatch 多个 companion 后，需要：

- `dispatch_id` 的跟踪机制（当前已有）
- 多结果汇聚：`await_companion` 支持按 dispatch_id 单独等待，也支持 `await_all_companions` 批量等待
- 并发限制：可配置单个 session 的最大并发 companion 数量

---

## Acceptance Criteria

- [ ] `companion_dispatch` 支持三种调度模式：async-callback / async-await / sync
- [ ] Project Agent session 能成功 dispatch 和 complete companion
- [ ] Task execution session 仍**不能** dispatch（只能 complete）
- [ ] sync 模式下 companion 执行完成后结果直接作为 tool call 返回值
- [ ] async-await 模式下 `await_companion(dispatch_id)` 能正确阻塞并返回结果
- [ ] companion session 能携带指定 session 的上下文记忆作为初始参考
- [ ] 继承的上下文与当前 session 的 Persona 不产生冲突（结构化隔离）
- [ ] 并发 dispatch 多个 companion 后能正确追踪和汇聚结果

---

## Technical Notes

### 与现有任务的关系

- **`03-23-agent-workflow-architecture-convergence`**：该任务提到"story owner / project agent 的 workflow 也由 assignment 自动解析"，本任务的 R2（Project Agent 开放 companion）是其自然延伸
- **`03-23-agent-tooling-redundancy-closure`**：该任务关注工具暴露的条件裁剪，本任务调整 FlowCapabilities 后需要与其对齐

### 实现优先级建议

1. **P0**：R2（Project Agent 开放）— 改动最小、价值最直接
2. **P1**：R1 sync 模式（打开 `wait_for_completion`）— 最常用的新能力
3. **P1**：R3 上下文记忆继承（核心设计挑战，需要先定义 ContextMemorySlice 结构）
4. **P2**：R1 async-await 模式（`await_companion` 工具）— 高级用法
5. **P2**：R4 并发管理 — 可以随需要逐步完善

### 关键文件

**后端**：
- `crates/agentdash-api/src/address_space_access.rs` — companion_dispatch / companion_complete 工具实现
- `crates/agentdash-api/src/routes/acp_sessions.rs` — FlowCapabilities 设置（Project Agent 能力开放）
- `crates/agentdash-application/src/session_plan.rs` — `conditional_flow_tools()` 工具注入逻辑
- `crates/agentdash-executor/src/connector.rs` — FlowCapabilities 结构定义
- `crates/agentdash-executor/src/hub.rs` — `start_prompt_with_follow_up()` 异步执行逻辑
- `crates/agentdash-agent/src/agent_loop.rs` — tool call 执行与超时逻辑

**前端**：
- `frontend/src/features/acp-session/ui/AcpSystemEventGuard.ts` — companion 事件白名单
- `frontend/src/features/acp-session/ui/AcpSystemEventCard.tsx` — companion 事件渲染

**规范**：
- `.trellis/spec/backend/execution-hook-runtime.md` §3.6 — companion 契约定义（需同步更新）
