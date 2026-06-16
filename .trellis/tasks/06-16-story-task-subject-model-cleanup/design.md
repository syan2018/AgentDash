# Story/Task subject 模型清理设计

## 背景判断

当前项目主线已经收敛到 AgentRun / LifecycleRun / SubjectRef / RuntimeSession trace。Story / Task 早期承载过 PM 看板、AI 拆解、Task 执行和状态投影等多种设想，后续重构让这些能力以适配层方式挂接到新主线，形成了重复事实源和职责漂移。

本设计的目标是让 Story / Task 回到当前主线中的清晰位置：

- Story 是业务主题和上下文容器。
- Story 入口启动的是普通 AgentRun / LifecycleRun，只是 subject 为 Story。
- Task 是 AgentRun 可创建和管理的通用 Todo / work item，不是运行时执行实体。
- Story 下看到的 Task 是 Story 视角的 subject projection，不是 Task 的唯一归属。
- 运行时事实由 Lifecycle / AgentRun 链路表达，Story / Task 只通过 association 或 projection 读取。

## 目标模型

```text
Project
  └─ Story
       ├─ 人工流程状态
       ├─ Story context sources / containers

Task
  ├─ 通用计划项 / Todo
  ├─ 创建者 / 管理者可来自任意 AgentRun
  ├─ 可关联 Story / Project / Routine / External subject
  ├─ 可由 AgentRun 自执行、assign 给 Companion subagent、或由 orchestration 批量生成
  └─ 可关联 0..N 个 AgentRun / LifecycleRun 执行事实

AgentRun / LifecycleRun
  ├─ subject_ref = project | story | task
  ├─ LifecycleAgent / AgentFrame
  ├─ RuntimeNodeState
  └─ RuntimeSession trace

LifecycleSubjectAssociation
  └─ 连接 Story / Task / 其它 subject 与 run / agent / frame 事实
```

## Story 边界

Story 保留为 Project 下的工作主题，承担人工可操作的流程状态。Story 状态表达产品流程和用户判断，例如创建、上下文准备、拆解、执行中、验收、完成等。

Story 状态可以被系统建议推进，但事实来源仍是用户或明确的业务命令。LifecycleRun 的失败、取消、完成可以成为 UI 提示或投影信号，不直接替代 Story 的人工状态语义。

Story 继续承担 context container：

- title / description / priority / type / tags
- default workspace
- context source refs
- context containers / disabled inherited containers
- session composition
- Story 视角的 Task projection

## Story-subject AgentRun

`StoryAgent` 只作为产品口语表达，代码模型中使用普通 AgentRun / LifecycleRun：

```json
{
  "subject_ref": {
    "kind": "story",
    "id": "<story_id>"
  }
}
```

Story 入口启动时的差异来自三层：

1. `SubjectContextAssignmentResolver` 根据 Story subject 解析 Project、Story、Workspace 和 context contributions。
2. frame construction 注入 Story 专属 blocks，例如 Story brief、context summary、Todo list、linked runs summary。
3. capability resolver 使用 `CapabilityScopeCtx::Story` 派生 Story scope 默认 capability。

这条路径复用 ProjectAgent / AgentRun / LifecycleRun 主链，不引入独立 StoryAgent entity、repository、runtime 或 session 类型。

## Task 边界

Task 的命名继续保留，目标语义收敛为通用 Todo / work item。Task 不归属 Story domain；任意 AgentRun 都可以在自身工作过程中创建、更新、关闭和派发 Task。Story 页面中的 Task 列表只是按 Story subject、context、origin 或 association 规则投影出的视图。

- 人工或 AgentRun 拆解出的计划项。
- 可排序、可编辑、可关闭。
- 可绑定上下文 refs。
- 可关联 Project、Story、Routine、LifecycleRun 或 External subject。
- 可作为 subagent / AgentRun 的 subject 或 projection target。
- 可被 AgentRun 自执行，也可被快速 assign 给 Companion subagent。
- 可由 dynamic orchestration / workflow planning 一次性生成系列 Task，再通过显式 fanout 指令分配给工作流 subagents。
- 可展示 linked runs 和 latest execution summary。

Task 状态表达计划协作状态，而不是 executor 运行状态。候选状态：

```text
open -> active -> review -> done
         |
       blocked
         |
      dropped
```

运行态信息通过 linked AgentRun / Lifecycle projection 展示，例如 current agent、runtime node status、artifacts、trace refs。

## Task 与 AgentRun / Companion Subagent

Task 与 AgentRun / subagent 的连接优先复用 subject association 或后续专用 execution link。AgentRun 可以是 Task 的创建者、管理者、执行尝试来源或 review 来源，但 Task 的 durable storage 不应放入 AgentRun runtime 事实表。

```text
Task subject_ref -> LifecycleSubjectAssociation -> LifecycleRun / LifecycleAgent / AgentFrame
```

短期沿用 `SubjectRef(kind=task)` 和 Task 命名。关联关系用于展示执行尝试、review run、follow-up run，不把 runtime fields 写回 Task 本体作为事实源。

Task 的 assignment / fanout 是计划层动作，不直接表示 runtime running：

- 自执行：当前 AgentRun 创建或领取 Task，并把 Task 作为自己的 Todo 推进；Task 可记录 manager / latest linked run，但运行状态来自当前 AgentRun / Lifecycle projection。
- 快速 assign：父 AgentRun 通过 Companion `sub` 或等价 dispatch 能力创建 child AgentRun，并将 Task 作为 child run 的 subject 或启动上下文。
- 编排扇出：dynamic orchestration 先产生一组 Task plan，再由用户或平台确认 fanout，批量创建 child AgentRun / workflow node association。

这三种入口都应该产出可追踪的 TaskExecutionLink / LifecycleSubjectAssociation，而不是把 child runtime refs 直接写进 Task 本体作为唯一事实。

## Dynamic Orchestration Fanout

dynamic orchestration 与 Task 的关系应是“计划生成 + 显式派发”：

```text
dynamic orchestration proposal
  -> Task batch plan
  -> optional review / approve
  -> create Tasks
  -> fanout dispatch to workflow / companion subagents
  -> write subject associations / execution links
```

动态编排可以生成 Task 集合、依赖、推荐 companion / agent profile 和分配策略，但 Task 本体仍是通用计划项。workflow runtime 负责调度和执行，Companion/subagent 负责具体处理，Task 页面只展示计划状态和 linked run projection。

一键扇出需要保留明确命令边界：

- `create_tasks_from_plan`：持久化 Task 集合。
- `assign_tasks`：为单个或多个 Task 创建 assignment intent。
- `fanout_tasks`：基于 assignment intent 创建 child AgentRun / workflow node dispatch。

后续实现可以合并 UI 操作，但应用层命令应保持可审计，避免一次按钮同时混淆计划创建、审批和 runtime dispatch。

审批门作为可配置能力保留，但默认不阻塞 fanout。默认策略应支持 Task batch plan 确认后直接 fanout；当 Project policy、workflow rule 或 permission grant 后续配置要求审批时，才进入显式 review / approve。这样 Task fanout 清理可以先收敛命令边界，不被当前较久未维护的 permission / approval 系统阻塞。

## Task 仓储边界

当前代码把 Task 合入 Story aggregate 的 `stories.tasks JSONB`，这是早期 Story 拆解模型的残留。目标模型中 Task 应从 Story aggregate 中独立出来，形成通用 Task repository / table：

- Task 持有自己的 durable identity、title、description、plan status、ordering / grouping metadata。
- Task 可以记录 creator / source，例如 human、AgentRun、Story subject run、imported。
- Task 可以记录 manager / assignment intent，例如 self-managed、assigned-to-companion、orchestration-fanout-pending。
- Task 通过 link / association 连接 Story、Project、Routine、External source 或 AgentRun。
- Story 不删除 Task 的事实；Story 删除只影响 Story projection / link，Task 是否归档由 Task 自己的生命周期决定。
- AgentRun 不直接持久化 Task 本体；AgentRun 创建 / 管理 Task 是 capability，执行关联通过 subject association / link 表表达。

## Read Model 收口

执行视图统一以 `SubjectExecutionView` 为主：

- Story 页面展示 `SubjectRef(kind=story)` 的 linked runs。
- Task 页面或抽屉展示 `SubjectRef(kind=task)` 的 linked runs。
- `/tasks/{id}/execution` 这类 Task 专属轻量 DTO 后续应复用或让位给 subject execution view。

Task 的 latest execution summary 可以作为 read projection 生成，但其来源必须能追溯到 association / run / agent / frame / runtime node。

Story 视角下的 Task projection 可以按以下来源组合：

- Task 显式 link 到 Story subject。
- Task 由 Story subject AgentRun 创建。
- Task linked AgentRun 关联到同一 Story subject。
- Task context refs 或 labels 明确指向 Story。

## MCP / Capability 收口

Story / Task 的工具能力应该从 subject scope 派生。任意 AgentRun 都可以通过 capability 获得 Task 创建和管理工具；Story subject frame 可以额外获得 Story 投影相关的 Task 查询 / 建议能力，是因为 capability scope 为 Story；Task subject frame 可以获得 Task 上下文和 linked run 能力，是因为 capability scope 为 Task。

Companion subagent assign 和 dynamic orchestration fanout 应作为 collaboration / workflow capability 暴露：

- collaboration capability 提供单个 Task 快速 assign 给 Companion subagent。
- workflow / orchestration capability 提供批量 Task plan、审批和 fanout。
- Task management capability 只负责计划项 CRUD / 状态 / link，不直接执行 runtime dispatch。
- permission / approval 系统只作为 fanout policy 的可选约束来源；授权事实后续应回到统一 PermissionGrant / policy projection，不在 Task 模型内另建审批事实。

后续工具面应优先表达为 subject-scoped capabilities，而不是扩大 StoryMcpServer / TaskMcpServer 的实体感。状态推进、artifact 上报、dispatch 入口都应对应明确的事实源：

- Story 人工流程状态走 Story command。
- Task 计划状态走 Task command。
- Runtime artifacts / status 走 Lifecycle / AgentRun projection。
- subagent dispatch 走 AgentRun / Lifecycle launch command，并写 subject association。
- dynamic orchestration 扇出走 workflow command，并在 dispatch 后写 Task execution links / subject associations。

## UI 收口方向

Story 页面保留轻量但闭环的工作面：

- Story brief 和人工状态。
- Story context。
- Story 视角 Task projection。
- linked AgentRuns / Lifecycle runs。

Task 交互降级为计划项编辑和 linked runs 查看。用户需要进入执行过程时，从 linked run 跳到 AgentRun workspace。

Story 看板、批量操作、复杂 Task execution panel 可以在后续实现中按产品价值保留、隐藏或重做，但目标是让 UI 语言围绕 Story subject run 和 Todo list，而不是第二套 execution dashboard。

## 数据迁移考虑

预研期可以直接做正确迁移：

- Task status enum 从执行状态迁移为 Todo 状态。
- Task artifacts 迁出实体事实源，改为 execution projection 或 linked artifacts。
- Task dispatch preference 迁为 launch hint 或 dispatch command 参数。
- 从 `stories.tasks JSONB` 迁出为通用 Task repository / table，Story 只保留 projection links 或通过 association 查询相关 Tasks。

迁移时需要处理现有 JSONB 中的旧状态值和 artifacts 字段，优先选择确定性映射，不引入兼容双写。

## 关键取舍

本设计保留 Story 的人工流程状态，因为它服务于用户组织工作和确认阶段；收敛 Task 的执行状态机，因为执行事实已经由 Lifecycle / AgentRun 承担。

本设计保留 Task 的未来扩展空间，因为 AgentRun 运行中生成、管理和派发 Todo 是有效产品能力；收敛它的 runtime ownership，因为 ownership 会让 Task 与 Lifecycle 重复表达同一执行事实。
