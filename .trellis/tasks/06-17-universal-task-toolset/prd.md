# 通用 Task 工具集规划

## Goal

规划一套 AgentDash 通用 Task 工具集，让 agent 在单个 run、subagent 扇出、跨 session continuation 中都能维护可审计的任务清单，并与现有 `LifecycleRun.tasks` / Story Task projection 模型自然衔接。

本项目只有 `Task` 这一个业务概念。业务代码、API、DTO、DB、MCP tool name、event kind、store、组件命名都必须使用 Task；`Todo` 只允许出现在参考实现调研或少量面向模型的工具说明中，用来表达“Task 工具集可以作为自己的待办清单使用”。

目标不是恢复旧 Story-owned Task CRUD，而是定义一组 agent-facing Task 工具和 UI/API 边界，使模型能高效维护 `LifecycleRun.tasks`。

## Requirements

- 参考 `references/codex`、`references/claude-code` 和 `references/pi-mono` 中的内嵌清单 / plan / task 工具设计，提炼可复用的交互模式，但落地为 AgentDash Task 工具集。
- 明确 AgentDash 中 Task 的统一边界：
  - run-scoped Task plan：持久计划项，事实源是 `LifecycleRun.tasks`。
  - agent-facing task list：同一事实源的工具化读写视图，允许清单式批量更新体验。
  - story/task projection：从 Story-bound run、linked run、`story_ref` 推导出的只读视图。
- 规划 agent-facing 工具集合，第一版只保留一对完整且设计良好的读写工具：`task_read` 与 `task_write`。状态推进、内容编辑、references 维护、排序、归档等都归入写工具的 Task mutation 语义。
- 规划厚 Task 读回模型：读工具不能只返回标题和状态，还要能按读取模式返回 body、context references、Story/run linkage、owner/assignment、来源关系、审计版本和必要的执行投影摘要。
- 规划厚 Task 写入模型：写工具需要支持 patch 与 snapshot 两种写入姿势，并用同一套 Task schema 覆盖 create/update/status/reorder/drop/reference 变更。
- 参考 CLI 工具设计时优先吸收单入口、多 mode/flag、默认 compact、显式 detail、机器可读输出和写后完整读回这些模式，而不是扩大工具数量。
- 规划与既有 assignment/fanout 能力的衔接方式，不把通用 Task 工具集扩成重复的派发工具集合。
- 规划工具如何直接操作 `LifecycleRun.tasks`，避免出现第二套清单事实源。
- 规划 UI/事件投影：如何展示当前 run 的 Task 清单、subagent 关联 Task 汇总、Story projection 和执行证据关系。
- 明确权限和审计要求：工具调用应能落到 run/session/activity 语义事件，避免隐式修改 Story 事实。
- 输出足够清晰的 `design.md` 和 `implement.md`，供后续进入标准 Trellis 实现流程。

## Acceptance Criteria

- [ ] `research/` 中包含 Codex、Claude Code、pi-mono 参考实现调研结果，并标注哪些交互模式适合映射为 AgentDash Task 工具。
- [ ] `prd.md` 记录用户价值、功能边界、验收标准和未决问题。
- [ ] `design.md` 定义 Task 工具 API、数据模型、投影、事件、权限和 `LifecycleRun.tasks` 写入规则。
- [ ] `implement.md` 给出可执行 DAG、验证命令和可能的并行切分点。
- [ ] 规划明确说明 AgentDash 只有 Task 一个业务概念，且不引入旧 `dispatch_preference` / Task artifacts 事实字段。
- [ ] 规划明确给出工具数量最小化原则，第一版只设计 `task_read` / `task_write`，并解释为什么不需要独立 status 工具。
- [ ] `task_read` 至少定义 overview/list/detail/context/execution/projection 等读取模式，以及每种模式的默认字段和扩展字段。

## Notes

- 当前任务是规划任务，不直接进入实现。
- 参考实现调研完成后，再决定工具说明是否需要写“可作为自己的待办清单使用”；正式命名必须统一为 Task。
