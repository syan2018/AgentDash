# Routine mount 与跨轮次上下文设计

## Goal

将 Routine 从“触发后派发一次 prompt”的自动化入口，升级为可在多次触发、多轮 Session、per-entity 执行之间传递上下文的长期自动化系统。

本任务先完成设计文档与实施计划，再推进 Routine mount、Routine memory、per-entity context、execution projection 与内嵌信息管理 skill 的 MVP 实现。

## User Value

- Routine 能长期记住它负责的自动化目标、事实、决策、待办与处理水位。
- 定时、Webhook、插件触发之间可以共享同一套 Routine 级上下文。
- `per_entity` Routine 能围绕 PR、Issue、告警、客户或其它外部实体维护独立上下文。
- Agent 在 Routine 触发的 Session 中能通过稳定 VFS 路径读写上下文，而不是依赖越来越长的 prompt template 或隐式 session history。
- Routine 的信息管理方式有明确 Agent-facing skill 指引，降低上下文污染和无结构堆积。

## Confirmed Facts

- 当前 Routine 领域模型包含 `Routine`、`RoutineTriggerConfig`、`SessionStrategy` 和 `RoutineExecution`，位于 `crates/agentdash-domain/src/routine/`。
- 当前 `RoutineExecutor` 负责 scheduled、webhook、plugin 三种入口，并最终调用 `SessionLaunchService` 派发 prompt。
- 当前 `RoutineExecution.completed` 表示 prompt 已成功派发到 Session，不表示 Agent 执行真正完成。
- 当前 `SessionStrategy::Reuse` 与 `SessionStrategy::PerEntity` 提供 session 复用能力，但没有提供 Routine 级长期 memory。
- 当前 VFS 架构已经支持 runtime mount provider，`lifecycle_vfs` 是 workflow lifecycle run 的投影先例。
- 当前 embedded skill bundle 规范已经存在，skill 内容应复用统一 bundle/materialization 机制，避免每个业务域手写同步逻辑。
- 现有架构文档要求 `LaunchCommand` 只表达来源意图，最终 VFS 应在 Construction 阶段形成 `SessionConstructionPlan`。
- Routine 信息管理 skill 应默认注入所有 Routine Session；是否实际使用由 prompt、skill 指引和 Agent 当轮判断决定。
- Routine memory 的 MVP 写入权限应有限开放，只允许 Agent 写入受控 memory/entity 路径，不开放当前触发事实或任意 execution 投影写入。
- Routine memory 浏览/编辑可以复用通用 VFS Browser；本任务只要求协议与 mount 支持，Routine 页面入口作为后续前端备忘，不纳入本任务实现范围。

## Requirements

- Routine 触发的 Session 应自动获得一个 Routine 级 VFS mount，用于读取当前触发上下文与长期 Routine memory。
- Routine mount 必须支持 Routine 级 memory，例如 brief、facts、decisions、open-items、changelog。
- Routine mount 必须支持当前 execution 的只读触发投影，例如 trigger payload、trigger source、execution metadata、resolved prompt。
- `per_entity` 模式下，Routine mount 必须支持 entity-scoped memory，并通过 `entity_key` 定位稳定路径。
- Routine mount 必须能与现有 Project / Workspace / Lifecycle / Skill Asset VFS 共存，并遵守 mount id 唯一、provider/root_ref 合法、capability 与 provider 范围一致等 VFS 不变量。
- Routine memory 的持久化来源必须明确，优先复用已有 owner-based inline/context file 能力；如无法复用，应设计最小专用存储模型。
- Routine 触发链路应通过 Session construction / capability projection 注入 mount，而不是在 `RoutineExecutor` 中直接拼接最终 VFS。
- Routine 信息管理 skill 应说明 Agent 如何读写 Routine memory、entity memory、执行摘要与失败恢复点。
- Routine 信息管理 skill 应通过 embedded skill bundle 或项目 SkillAsset projection 管理内容，保持 skill 内容与 mount provider 职责分离。
- Routine 信息管理 skill 应作为 Routine Session 的默认能力基线注入，但不强制 Agent 每轮必须读写 Routine memory。
- Routine mount 写入权限必须路径级受控：`current/*` 只读，Routine 级 memory 与当前 entity memory 可写，非当前 entity 与 execution 投影默认只读或不暴露写入。
- Routine mount 协议应能被通用 VFS Browser 消费；Routine 页面可后续增加“打开 Routine Memory”的入口，但本任务不实现该前端入口。
- 文档必须明确 MVP 范围、后续扩展点、数据流、权限/写入策略、migration 影响与验证方式。

## Non-Goals

- 本任务不改变现有 Routine API 行为。
- 本任务不把 RoutineExecution 的 `completed` 语义扩展为 Agent 完成态；该能力作为后续 session turn completion 关联扩展讨论。
- 本任务不设计完整插件 trigger provider lifecycle；只保证 Routine mount 设计能兼容 plugin 触发来源。
- 本任务不把 Routine memory 设计为复杂知识图谱或结构化任务系统；MVP 采用 Agent-facing 文件化 memory。

## Acceptance Criteria

- [x] Routine Session 自动获得 `routine` VFS mount。
- [x] `routine_vfs` 暴露 `current/*` 只读触发投影、Routine 级 memory、当前 entity memory。
- [x] Routine memory 写入权限有限开放：只允许受控 `memory/*` 与当前 `entities/{entity_key}/*`。
- [x] Routine launch source metadata 进入 Session construction，RoutineExecutor 不直接组装最终 VFS。
- [x] `routine-memory` skill 默认注入 Routine Session，普通非 Routine Session 不受影响。
- [x] 通用 VFS Browser 可通过现有 VFS 协议消费 Routine mount；Routine 页面入口只作为后续前端备忘。
- [x] 后端聚焦编译与相关测试完成，或明确记录非本任务阻塞。

## Open Questions

- 是否需要在后续前端任务中为 Routine 页面增加一个跳转通用 VFS Browser 的入口，用于打开当前 Routine 的 `routine://` mount。
