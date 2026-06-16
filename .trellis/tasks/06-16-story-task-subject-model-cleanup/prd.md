# Story/Task subject 模型清理规划

## Goal

收敛 Story / Task 在当前 AgentRun / LifecycleRun / SubjectRef 主线下的业务定位，形成一轮可执行清理方案：

- Story 保留为 Project 下的工作主题、人工流程状态和上下文容器。
- Story 入口启动的 AgentRun / LifecycleRun 使用 `SubjectRef(kind=story)` 表达，差异来自 Story 注入块和 Story scope capability。
- Task 从执行状态机收敛为 AgentRun 可创建和管理的通用 Todo / work item 语义，作为任意 AgentRun 都可维护的计划项，并通过 subject association / execution link 投影关联的 AgentRun / subagent 事实。
- Story 视角下的 Task 列表是对通用 Task 体系的 subject projection，不是 Task 的唯一归属域。
- Task 支持三类业务入口：AgentRun 自己作为 Todo 执行、快速 assign 给 Companion subagent、由 dynamic orchestration 生成系列 Task 后一键扇出给工作流 subagents。
- 运行时事实继续归属于 LifecycleRun / LifecycleAgent / AgentFrame / RuntimeNodeState / RuntimeSession trace，避免 Story / Task 承担第二套执行控制面。

## Requirements

- 明确 Story 的保留职责：title、description、人工状态流转、priority/type/tags、context sources、默认 workspace 和当前 Todo 列表。
- 明确 `StoryAgent` 只作为口语化产品形态：本质是普通 AgentRun / LifecycleRun 携带 `SubjectRef(kind=story)` 后得到的 story-bound run。
- 明确 Story 入口初始化时的特殊能力来源：Story context injection blocks、`CapabilityScopeCtx::Story` 默认 capability、ProjectAgent launch config。
- 明确 Task 的命名继续保留，不迁移为 Todo / WorkItem；Task 的目标语义调整为通用 Todo / work item。
- 明确 Task / Todo 的目标语义：任意 AgentRun 可创建、维护、关闭的计划项、检查项、subagent 派发目标和 linked runs 投影入口。
- 明确 Task 的执行入口形态：
  - AgentRun 自执行：当前 AgentRun 把 Task 当作自己的 Todo / checklist 项推进。
  - Companion 快速 assign：父 AgentRun 将 Task 作为明确工作单元派发给 Companion subagent。
  - Dynamic orchestration 扇出：动态编排生成一组 Task，并通过显式指令批量分配给 workflow / companion subagents。
- 明确 Task / Todo 状态只表达计划协作状态，例如 open / active / review / done / blocked / dropped，运行状态从 linked AgentRun / Lifecycle projection 派生。
- 明确 Task 不归属 Story domain；Task 应拥有独立仓储 / 表或通用 task domain。Story 只通过 subject links、origin refs、labels 或 projection rules 聚合相关 Tasks。
- 明确 Task 与 AgentRun / subagent 的连接方式：通过 `SubjectRef(kind=task)`、`LifecycleSubjectAssociation`、run / agent / frame refs 建立关联；AgentRun 可作为 Task 创建者、管理者或执行关联方，但不把 Task 存进 AgentRun runtime 事实表。
- 明确 Task assignment 是计划层关系，不等价于 runtime running；assign / fanout 只创建执行意图、子 AgentRun / Lifecycle association 和可观察 linked run。
- 明确 Task fanout 的审批策略：保留 review / approval 门，但默认处于开放状态；批量 fanout 默认可以直接执行，后续可由 workflow、project policy 或 permission grant 配置切换为需要审批。
- 记录权限审批系统需要后续单独研究收束，本任务不把 Task / fanout 清理阻塞在 permission 系统重构上。
- 明确现有重复事实源的清理方向：Task execution DTO、Task runtime status、Task artifacts、Task dispatch preference、Story / Task MCP 独立状态推进工具、前端 Task 执行面板。
- 保留 SubjectContextAssignmentResolver 作为 subject context 解析核心，并让 Story / Task / Project subject 共用该解析边界。
- 输出可拆分的后续实现步骤，支持先做低风险 read model / UI / spec 收口，再做领域字段迁移。

## Acceptance Criteria

- [ ] `prd.md` 记录 Story、Story-subject AgentRun、通用 Task、AgentRun-created Task、subagent association 的目标业务语义。
- [ ] `design.md` 记录目标架构边界、数据流、状态归属和 API / UI 收口方向。
- [ ] `implement.md` 给出后续清理顺序、验证命令和风险文件。
- [ ] 规划明确区分 Story 人工流程状态与 Task/Todo 计划状态、Lifecycle runtime 状态。
- [ ] 规划明确 `StoryAgent` 不新增独立实体、repository、runtime 或 session 类型，而是 Story subject run 的产品称呼。
- [ ] 规划明确 Task 可以发展为完整通用 Todo 体系，由任意 AgentRun 创建/管理，但执行事实通过 linked AgentRun / Lifecycle projection 展示。
- [ ] 规划明确 Story 视角的 Task 列表是 projection，不是 Task 的仓储归属。
- [ ] 规划明确 Task 支持自执行、Companion subagent assign、dynamic orchestration 批量生成和扇出分配三类入口。
- [ ] 规划明确 fanout 审批门默认开放、可配置收紧，并把权限审批系统收束列为后续研究方向。
- [ ] 后续实现前可以基于本任务拆分为字段迁移、API 收口、前端瘦身、MCP / capability 收口等子任务。

## Notes

- 本任务先沉淀目标模型和清理路线，不直接修改业务代码。
- 现有 `06-14-module-overdesign-review` 已覆盖 Task projection / execution read model 的事实链问题；本任务承接更上层的产品建模收束。
- Permission / approval 系统已有较久未维护的迹象，Task fanout 只保留接入点；权限系统本身应另起研究或清理任务处理。
