# Unified Context Composer And Session Plan

## Goal

设计一套统一的 `Context Composer / Session Planner`，把当前分散在 Address Space、prompt 拼接、session 启动和 agent 绑定中的上下文编排逻辑收拢到一个明确模型中，统一决定：

- 给当前 session 暴露哪些 mounts
- 给当前 agent 暴露哪些 tools
- 使用什么 agent 身份 / persona 模板
- 强制携带哪些工作流信息、系统上下文、声明式 source refs
- 哪些容器或信息对不同 owner / agent 类型可见

## Why This Exists

- 现在 `Project / Story` 容器派生已经进入主链路，但“agent 最终看到什么”仍不只由 mount 决定。
- prompt、workflow、工具白名单、身份模板、source refs、系统上下文都还散在不同模块里，容易出现：
  - mount 已经挂上了，但 agent 不稳定发现
  - session 能读文件，但 prompt 没把上下文结构讲清楚
  - owner 层策略和实际 tool visibility 不一致
  - Story / Task 不同入口拼接规则继续分叉
- 这个任务要定义更高一层的会话编排器，让未来的 Task、Story、Owner Session 都通过统一 planner 生成“完整执行计划”。

## Requirements

- 定义统一的 `SessionPlan` / `ContextComposition` 概念模型，至少包含：
  - mounts
  - tools
  - agent persona / identity
  - workflow instructions
  - required context blocks
  - declared sources / context summaries
- 该模型必须支持 `Project -> Story -> Task/Session` 的分层派生与覆盖。
- 必须支持显式注入 mount 摘要，让 agent 更稳定知道自己可访问哪些容器、每个容器是什么用途、有哪些权限。
- 必须支持未来把用户自定义 agent 身份、团队工作流模板、强制系统提示拼进统一编排结果。
- 设计上要兼容未来更多 provider、更多 agent 类型和不同 session 入口，不允许把逻辑继续散落回路由层。

## Acceptance Criteria

- [ ] 明确统一 `Context Composer / Session Planner` 的职责边界。
- [ ] 明确 session plan 中至少应有哪些结构化字段。
- [ ] 明确 `Project / Story / Task` 三层策略如何合成最终 plan。
- [ ] 明确 mount 摘要、tool visibility、persona、workflow info 如何注入给 agent。
- [ ] 明确与现有 Address Space、Prompt Builder、Session Gateway 的关系与迁移方向。
- [ ] 明确首轮实现建议切片，便于后续拆任务落地。

## Core Idea

建议把“上下文编排”从“只生成 mount table”升级为“生成完整 session execution plan”。

这个 plan 不只是文件系统，还应至少包含六类信息：

1. `mounts`
- 派生后的最终虚拟/物理容器列表
- mount id、权限、用途说明、metadata

2. `tools`
- 当前 agent 可见工具集合
- 是否允许 shell / write / search / MCP
- 针对不同 owner 或 agent 类型做裁剪

3. `persona`
- agent 身份模板
- 默认行为约束
- 项目或故事级附加角色说明

4. `workflow`
- 当前任务阶段
- 强制携带的流程说明
- 必须遵循的团队规范 / spec 提示

5. `declared_context`
- Story / Task 显式附带的 source refs
- 系统自动摘要出的关键上下文块
- mount 摘要、容器用途说明、风险提示

6. `runtime_policies`
- 是否允许联网
- 是否允许写文件
- 是否允许执行命令
- 是否允许访问本地 workspace

## Proposed Layering

### Project Layer

定义项目级默认编排策略：

- 默认 mounts
- 默认工具集合
- 默认 persona / workflow 模板
- 项目级必须附带的上下文块

### Story Layer

在项目默认值上做细化：

- 追加或禁用容器
- 收窄或放宽工具可见性
- 补充故事级背景、目标、约束
- 指定对 owner session / task session 的额外 prompt 片段

### Task / Session Layer

绑定实际执行上下文：

- 当前 workspace
- 当前 agent binding
- 当前任务目标
- 当前会话入口类型

最终由 planner 输出一份稳定的 `SessionPlan`，再交给具体 gateway / connector 消费。

## First Follow-up Slice

这个规划任务完成后，第一批可拆出的实现任务建议是：

1. 显式 mount 摘要注入
- 在 Task / Story prompt 中结构化列出可用 mounts、权限和用途说明

2. 统一 session plan 数据结构
- 从现有 `AddressSpaceService` 往上提炼成更完整的 plan object

3. tool visibility 收口
- 把工具授权从分散逻辑迁移到 planner 输出

4. persona / workflow 模板编排
- 允许 Project / Story 定义默认 agent 身份与必须附带的流程上下文

## Relationship To Existing Work

- `03-18-project-virtual-workspace-provider-service` 解决的是“上下文容器从哪里来，以及如何派生 mount”
- 当前任务解决的是“agent 最终到底拿到什么完整执行上下文”
- 二者关系是前者提供素材与策略输入，后者输出最终 session 计划

## Out of Scope

- 这一轮不直接重写全部 session 启动链路
- 不立刻实现可视化配置 UI
- 不先解决所有 provider 协议细节
- 不承诺一次统一所有历史 prompt 模板

## Delivery Notes

- 该任务当前先保持 `planning`，作为后续 prompt / planner / tool visibility 收口的设计母任务。
- 它会直接承接你提到的“把容器摘要显式注入到 Task/Story prompt 里”的后续实现，但不会把问题局限成只补一段 prompt 文案。

## Related Files

- `.trellis/spec/backend/address-space-access.md`
- `.trellis/tasks/03-18-project-virtual-workspace-provider-service/prd.md`
- `crates/agentdash-api/src/address_space_access.rs`
- `crates/agentdash-api/src/bootstrap/task_execution_gateway.rs`
- `crates/agentdash-api/src/routes/acp_sessions.rs`
- `crates/agentdash-executor/src/connectors/pi_agent.rs`
