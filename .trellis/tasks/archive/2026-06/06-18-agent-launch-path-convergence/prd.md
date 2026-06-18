# Agent 启动路径主轴收束

## Goal

将项目中分散的 Agent 启动路径收束到同一条主轴：先 materialize AgentRun / Lifecycle / RuntimeSession 控制面事实，再通过统一的 `LaunchCommand -> FrameLaunchEnvelope -> LaunchPlan -> PreparedTurn -> ConnectorAcceptedTurn` 启动 turn。

本任务的价值不是在既有分叉上继续包兼容层，而是识别并删除重复 materialization、重复 frame construction 和 route-specific launch 分支，让不同来源只表达必要 modifier。

## Background

当前分支已经在收束 companion agent identity 与 authority 边界。只读 review 发现：

- `session/launch` 已经形成统一 turn launch 主线。
- `FrameConstructionService` 已经成为 launch envelope 的事实入口，但 classification 仍是互斥 route：`companion_hint -> project_agent -> lifecycle node -> existing surface`。
- `ProjectAgentRunStartService` 的两阶段 start + mailbox 投递方向合理，但职责偏厚。
- `AgentRun mailbox` 已经是用户输入和 resume / steer / launch 的 durable 投递层，应作为继续保留的投递事实源。
- `AgentNodeLauncher` 仍自行创建 `LifecycleAgent`、`RuntimeSession`、`AgentFrame` 和 anchor，绕开 `LifecycleDispatchService`。
- `CompanionRequestTool::execute_sub_request` 同时承担 payload 解析、hook、dispatch、gate、task assignment、ProjectAgent binding 和 child session launch，边界过重。
- `LaunchCommand` 存在 `companion_hint`、`routine_hint`、`local_relay_*` 等 source-specific 字段，说明差异还没有被建模成可组合 modifier。

## Requirements

- 建立 Agent 启动的单一权威边界：materialization 和 turn launch 必须分层，不能由各入口自行拼装运行事实。
- 删除或合并重复 materialization 路径，尤其是 workflow AgentCall 节点不应自行创建 agent/session/frame/anchor。
- Companion child agent 启动应收束为专属服务或 dispatch adapter，tool 层只保留工具参数解析、权限/roster 校验和调用入口，不继续内联完整启动编排。
- `FrameConstructionService` 应从互斥 route 收束为 owner composer + launch modifier pipeline；`companion`、`routine`、`local relay`、`hook auto-resume` 等来源差异应作为 modifier，而不是顶层分叉。
- 保留正确的 durable 投递事实源：AgentRun workspace 输入仍通过 mailbox / command receipt / scheduler 决定 launch、queue、steer 或 resume。
- 不做兼容性回退，不保留旧入口并行逻辑。当前项目未上线，允许直接迁移到正确模型。
- 重构中如涉及数据库事实结构，需要同步 migration；不做字段别名和旧值兼容。
- 文档更新只记录新的架构原因和事实，不记录旧实现的反模式清单。

## Acceptance Criteria

- [ ] 所有生产 Agent turn launch 最终都通过 `SessionLaunchService::launch_command*` 或其明确的统一替代入口进入 `SessionLaunchOrchestrator`。
- [ ] Agent materialization 只有一个权威服务负责创建或复用 `LifecycleRun`、`LifecycleAgent`、`RuntimeSession`、`RuntimeSessionExecutionAnchor`、initial/current `AgentFrame`、optional gate 和 lineage。
- [ ] Workflow AgentCall 节点不再拥有独立 agent/session/frame/anchor 创建流程。
- [ ] Companion sub-agent dispatch 从 `CompanionRequestTool` 内联流程中抽离；companion 差异以 typed dispatch intent / launch modifier 表达。
- [ ] `LaunchCommand` 不再横向堆 source-specific optional 字段；来源差异通过 typed source payload 或 modifier 建模。
- [ ] `FrameConstructionService` 的路径选择不再把 companion 作为最高优先级互斥 route；companion 成为套在 child owner surface 上的 modifier。
- [ ] ProjectAgent draft start 仍保持两阶段语义：外层 start receipt materialize workspace，首条输入进入 mailbox，由 scheduler 产生 delivery outcome。
- [ ] 关键路径有 focused backend tests 覆盖：ProjectAgent start、AgentRun composer submit、companion child dispatch、workflow AgentCall node、local relay prompt。
- [ ] 相关 spec 更新到新的权威模型，描述为什么 materialization 和 turn launch 分层。

## Out Of Scope

- 不重做前端 AgentRun workspace 交互视觉。
- 不扩展新的 companion product capability。
- 不引入旧 API 兼容层或旧数据兼容投影。
- 不把 mailbox durable 投递层替换成 route-local launch 分支。

## Open Questions

- 首轮实现是否允许直接删除 `AgentNodeLauncher` 的专用 materialization，并用统一 materialization service 替代；推荐答案：允许，这是本任务最应该优先斩掉的重复路径。
- Companion modifier pipeline 的最终 API 形态需要在 design 阶段落到具体 Rust 类型；推荐答案：先设计 `AgentMaterializationService` 与 `LaunchModifier`，再做最小闭环迁移。
