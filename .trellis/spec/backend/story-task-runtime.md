# Story / Task 运行时建模（Story-as-durable-session）

> 定义 Story / Task / Story session / LifecycleRun / child session 五件套的精确关系与职责边界。
> 与 [repository-pattern.md](./repository-pattern.md) / [database-guidelines.md](./database-guidelines.md) / [workflow/lifecycle-edge.md](./workflow/lifecycle-edge.md) 一起参考。
> 本 spec 是主线任务 `04-27-slim-runtime-layer-session-owner` M1-M7 重构的架构基线；所有 story/task 相关的代码修改必须先对齐本文。

---

## 1. Overview (TL;DR)

后端的 Story / Task 模块按 **Model C** 定位：

- **Story 是一个 aggregate root**，表达"一条持久化的业务工作单元"。它不是一个执行器、不是一条运行队列、也不是一堆自管的子实体；它是 **某个 durable session 的业务视图**。
- 每个 Story 在运行时对应 **一个 Story session**（`SessionBinding(owner_type=Story)` 绑定），这个 session 的 event stream 是 story 内一切状态变更的唯一审计源。
- **Task 是 Story aggregate 下的 child entity**，描述"story 内某一段执行单元的用户可见状态视图与声明"（标题、描述、agent binding、workspace 约束、状态、产物等），不是独立 aggregate、没有独立 repository、没有独立表（合入 `stories.tasks` JSONB 列）。
- **Task 不承载 runtime 业务**：不持有 AgentDash 内部 `session_id`、不持有执行器原生 `executor_session_id`、不持有 one-shot / auto-retry 等执行策略。Task 只通过 `SessionBinding(owner_type=Task, label="execution")` 指向可查看的 execution child session；执行器 resume id 只归属 `SessionMeta.executor_session_id`。
- **LifecycleRun 是独立 domain entity**，1:1 挂在 Story session 上；它只记录 step 运行态，不持有具体 Task 实例 id。Task 通过 `lifecycle_step_key` 指向对应 step。**Task.status / Task.artifacts 是 step state 的只读投影**，不是独立真相。
- **child session**（对话、companion、step 的远程执行、多次问答）都是 Story session 的子 session，通过 companion 或 step binding 与 Story session 挂钩。Task 最多提供“查看对应 child session”的入口，不拥有独立 runtime lifecycle。
- 对外 API 的 `start_task` / `continue_task` / `cancel_task` 等入口保留名字，内部统一委托给 `activate_story_step(story_id, step_key, user_input)` 服务命令——**Task 启动路径 = 标准 workflow 装配**，不存在 `compose_task_runtime` 这种特例分支。
- `state_changes` 表不再是业务事件的独立写入源，降级为"跨 session 全局游标索引"（D3-β），由 Story session event stream 的变更通过投影自动维护；`/events/since` API 保持兼容。

形象地说：**"Story 就是 durable session 的业务名片"**。运行时真相活在 session event stream 与 LifecycleRun/step state 里；Story aggregate 只承担业务视图（启动参数 + 面向用户审计）。

---

## 2. 核心抽象

### 2.1 Story（aggregate root, durable）

Story 是持久化的业务 aggregate，承载两类数据：

1. **启动 session 的模板参数**：title / priority / tags / context / workspace / agent_binding / default_workspace_id。
2. **面向用户的业务审计**：`Story.status`（业务审计字段，非 runtime projection；见 §9 待澄清）、created_at / updated_at。

Story **持有 `Vec<Task>`**，作为 aggregate 内的 child entity 集合。

**职责边界**：

- Story aggregate **不** 持有 runtime 真相；runtime 真相在 Story session event stream + LifecycleRun/step state。
- `Story.task_count` 作为 UI 冗余列保留（前端依赖），但可以通过 `jsonb_array_length(tasks)` 校准。
- 所有 task mutation（add / update / remove）必须通过 Story aggregate 的内部方法完成（如 `Story::add_task` / `Story::update_task` / `Story::remove_task`），禁止从外部直接操作 `Story.tasks`。

**持久化**：

- 物理表 `stories`，新增 `tasks JSONB` 列承载 `Vec<Task>`。
- `StoryRepository::update(&Story)` 负责 aggregate 整体（含 tasks）的原子写回；无独立 `TaskRepository`。
- 详见 §5。

### 2.2 Story session（kind=Story, durable）

Story session 是 Story 在运行层的"外壳"。

**性质**：

- `SessionBinding(owner_type=Story, owner_id=story_id, label=?)` 1:1 绑定到 Story（见 §4 label 规范）。
- 本身是一条完整 session：有 `sessions` / `session_bindings` / `session_events` 行，承载 ACP notification 与领域事件。
- **event stream 是 story 内一切状态变更的唯一审计源**。Task.status / Task.artifacts 的变化、LifecycleRun 的步骤推进、companion 消息……全部以 session event 落地。

**职责**：

- 承载本 story 的 owner 级对话（companion 对话）。
- 通过 `lifecycle_runs.session_id` 字段承担 1 个 LifecycleRun 的运行载体。
- 作为 N 个 child session 的父（通过 companion_context / step binding）。

**运行时生命周期**：

- Story session 的生命周期 ≈ Story 的生命周期；Story 被归档/删除时对应 session 也进入终态。
- Story session 的 event stream 允许重放，Task view / Story.status / LifecycleRun state 都可以从 event stream 重建（见 §6）。

### 2.3 LifecycleRun（独立 entity）

LifecycleRun 是 workflow 推进的运行态，独立 domain entity。

**关系**：

- `LifecycleRun.session_id` 指向 Story session（1:1）。
- 一个 Story session 同时至多挂 1 个活跃 LifecycleRun。
- `LifecycleRun.steps: Vec<LifecycleStepState>` 只表达 workflow step 的运行态；step state 不携带 Task id。
- Step state 是 Task 投影的真相源：Task.status / Task.artifacts 从对应 step 的 state 推导而来。

**约束**：

- `LifecycleRun` 不直接 owner 任何 Task，也不通过 definition/run state 反向引用 Task。Task → step 的绑定在 Story aggregate 内的 `Task.lifecycle_step_key` 上表达。
- step 推进 → step state 变化 → 投影到 Task.status / Task.artifacts（M2 收口）。
- Run 的详细边/推进规则见 [workflow/lifecycle-edge.md](./workflow/lifecycle-edge.md)。

### 2.4 Task（Story aggregate 下的 child entity）

Task 是 Story aggregate 内某段执行单元的 durable spec。

**字段分层**：

- **Durable spec**（来自用户/编排层；可变更需走 Story aggregate mutation）：
  - `id: Uuid`
  - `story_id: Uuid`
  - `workspace_id: Option<Uuid>`
  - `lifecycle_step_key: Option<String>`：Task 指向 Story lifecycle 中的 step key；这是外层 Task 到内层 step 模板的绑定，不允许反向写入 `LifecycleStepDefinition`。
  - `title / description / tags`
  - `agent_binding: AgentBinding`
  - `created_at / updated_at`
- **投影字段**（由 LifecycleRun/step state 反投射；**外部不可直接写**）：
  - `status: TaskStatus`
  - `artifacts: Vec<Artifact>`

**类型级约束**：

- Task.status / Task.artifacts 必须通过类型机制保证"外部不可写"，例如：
  - `status` / `artifacts` 字段私有化 + 仅暴露 `apply_projection(&StepState)` 内部方法；或
  - Task 拆分为 `TaskSpec`（外部可变）+ `TaskView`（只读投影视图）两种表达，API 层返回合并视图。
- 具体机制在 M2 实现阶段敲定；本 spec 强制的契约是：**`grep "task.status\s*="` 在生产代码中不能出现业务直接写入**。
- Task entity 禁止新增 runtime 字段；`executor_session_id`、`session_id`、`execution_mode`、retry policy 等属于 session / lifecycle step 策略层，不属于 Task。

**持久化**：

- 无独立表。Task 以 `Vec<Task>` 形式保存在 `stories.tasks JSONB` 列内。
- 任何 task CRUD 走 `StoryRepository::update(&Story)`（整体写回）。

### 2.5 child session

child session 是 Story session 下的子 session，覆盖以下场景：

- **Companion 对话**：PiAgent companion / 其他 companion 类型，通过 `CompanionContext.parent_session_id = <story_session_id>` 挂钩。
- **Lifecycle step 的远程执行**：某个 step 激活时创建一条独立 session（workflow 里叫 agent node session / lifecycle node session），其 `parent_session_id = <story_session_id>`。
- **多轮对话拆分**：历史上一个 story 可能挂多条 session；在 Model C 下这些都统一为"Story session 的 child"。

**命名统一**：本 spec 将 Story 的根 session 称为 **"Story session"**（root），其下挂的所有子 session 统称 **"child session"**。Task 不再拥有"自己的 session"——task 对应的 lifecycle step 若有独立 session，属于 child session 范畴。

---

## 3. 关系拓扑

```
+-----------------------------------------------------------------+
|                         Story (aggregate)                       |
|  durable spec: title / priority / context / agent_binding ...   |
|  Story.status (业务审计)                                         |
|  Vec<Task> (child entities; 物理 = stories.tasks JSONB)         |
+-----------------------------------------------------------------+
             |                             |
             | 1:1 (via SessionBinding)    | Task.lifecycle_step_key -> step_key
             v                             v
+-----------------------------------+    +-------------------------+
|    Story session (kind=Story)     |<---|  LifecycleStepState     |
|  sessions / session_bindings /    |    |   step_key              |
|  session_events                   |    +-------------------------+
|                                   |             ^
|  event stream = 真相源             |             | belongs to
|                                   |             |
|  + companion 对话消息              |    +-------------------------+
|                                   |    |     LifecycleRun        |
|  (root 节点)                       |<---|  session_id -> Story    |
+-----------------------------------+    |  steps: Vec<Step>       |
     |         |            |            +-------------------------+
     | 1:N child session                  (1:1 with Story session)
     v         v            v
 +--------+  +--------+  +------------------+
 |companion| |step     | |其他 child session|
 |session  | |node     | |(review / ...)    |
 |         | |session  | |                   |
 +--------+  +--------+  +------------------+
   ^          ^
   |          |
   |          +- CompanionContext.parent_session_id = Story session id
   +- parent_session_id = Story session id
```

**关系摘要**：

| 关系 | cardinality | 绑定方式 |
|------|-------------|----------|
| Story ↔ Story session | 1:1 | `SessionBinding(owner_type=Story, owner_id=story_id)` |
| Story session ↔ LifecycleRun | 1:1（活跃） | `LifecycleRun.session_id = story_session_id` |
| Story ↔ Task | 1:N | Story aggregate 持有 `Vec<Task>` |
| LifecycleStep ↔ Task | 0..1:1 | `Task.lifecycle_step_key -> LifecycleStepDefinition.key` |
| Story session ↔ child session | 1:N | `CompanionContext.parent_session_id` 或 step-owned session 的 parent 关系 |

---

## 4. SessionBinding.label 规范

`SessionBinding.label` 是 **free-form string**（DB 无 CHECK 约束；历史字段定义注释："`execution / companion / planning / review`"）。本 spec 对 Story owner / Task owner 的 label 值集固化如下。

### 4.1 值集

| owner_type | label 值 | 语义 | 备注 |
|-----------|----------|------|------|
| `Story` | `"companion"` | Story session 的 root 绑定（story-level owner session） | 现状实际值唯一为 `"companion"`（见 research） |
| `Task` | `"execution"` | Task 状态视图对应的 execution child session（用于会话查看） | 现状硬编码 |
| `Project` | `"execution"` | Project session（本 spec 不处理 Project 详细语义） | — |

### 4.2 实施约束

- Story session 创建路径（`story_sessions.rs::create`）默认 `label = "companion"`，不再允许客户端任意透传字符串。
- 引入一层输入校验：API 层将 `req.label: Option<String>` 限制到枚举白名单；非白名单值返回 400。
- DB migration：若存量有 Story owner 且 label ≠ `"companion"` 的行，需要专项评估（research 显示实际值集只有 `"companion"`，迁移风险低）。
- **不为了本轮重构引入 `label` 枚举化**（跨 owner_type 的 label 值集不一致；Story 端枚举化 + Task 端硬编码并存即可）。

### 4.3 Label 不是 owner 类型的替代

`(owner_type, owner_id, label)` 三元组定位 binding；`owner_type` 决定语义边界，`label` 是辅助分类。**不要通过 label 来表达 owner_type 差异**（例如用 `label="story"` 代替 `owner_type=Story`）。

---

## 5. 持久化规则

### 5.1 Story aggregate 整体写回

- `StoryRepository::create / update / delete` 负责 Story aggregate（含 `Vec<Task>`）的原子持久化。
- 删除 `TaskRepository` / `TaskAggregateCommandRepository` 两个 port（无替代 port）。
- 事务边界：若 Story mutation 同时需要写 `state_changes`（投影索引），在同一事务内完成；细节见 §6。

### 5.2 DB schema

**stories 表**（M1 变更）：

- 保留原有业务列（id / project_id / title / status / context / ...）。
- 新增 `tasks JSONB NOT NULL DEFAULT '[]'::jsonb`。
- `task_count INT` 冗余列保留（前端 UI 依赖）；可以在 aggregate 写回时校准为 `tasks.len()`。

**tasks 表**（deprecated）：

- 保留一段时间作只读回滚参照，**不再接受写入**。
- 数据迁移脚本：
  - 读 `tasks` 按 `story_id` 分组 → 构造 JSON 数组 → `UPDATE stories SET tasks = ...`。
  - 迁移脚本必须幂等，允许重跑（参考 [database-guidelines.md](./database-guidelines.md) 中 PL/pgSQL 规范）。
- 后续清理时机见 M1 后续阶段（不在本 spec 范围）。

**lifecycle_definitions / workflow_definitions 表**：

- `binding_kind='task'` 的行需要 migration 迁移到 `binding_kind='story'`（或按业务决定删除），配合 `WorkflowBindingKind` 定义侧收敛。
- 详见 [workflow/lifecycle-edge.md](./workflow/lifecycle-edge.md) 与 [domain-payload-typing.md](./domain-payload-typing.md)。

### 5.3 回退策略

- 迁移期间保留旧 `tasks` 表，便于紧急回退。
- 代码读路径切到 `stories.tasks` JSONB 后，**写路径不再回写旧表**；若需要双写过渡期，应由迁移 PR 显式设计，不是 spec 强制。

### 5.4 SessionBinding 持久化

- Story session 的 binding 在 Story 创建时生成（owner_type=Story, label="companion"）。
- Task 的 step 若创建独立 child session，通过 `SessionBinding(owner_type=Task, label="execution")` 绑定；这一绑定不会替代 Story-owner binding，两者并存。

---

## 6. Event sourcing 与投影（D3-β）

### 6.1 真相源

story 内一切状态变更的真相源是：

1. **Story session event stream**（`session_events` 表，按 `session_id + event_seq` per-session 单调）——承载 ACP notification + 领域事件扩展（LifecycleRun/step state 变更通过 session event 发射）。
2. **LifecycleRun / step state**（`lifecycle_runs` 表）——workflow 推进的专用状态。

Task.status / Task.artifacts 作为**只读投影**由 step state 反投射。

### 6.2 state_changes 表降级

- `state_changes` 表（BIGSERIAL PRIMARY KEY `id`）**不再是业务事件的独立写入源**；业务代码禁止直接 `append_state_change` 来表达"task status 变了""story status 变了"。
- 它的职责变为 **"跨 session 全局游标索引"**：
  - 由 Story session event stream / LifecycleRun state 的变更通过 **投影器**（projector）自动写入。
  - 投影器在事件管道中注册；原则上投影器与触发事件同事务提交（事务内 append），保证游标与事件一致。
  - `/events/since/{since_id}` API 保留；`state_changes.id` 继续承担跨 project 的全局游标职责。
- `ChangeKind::TaskStatusChanged / TaskArtifactAdded / StoryStatusChanged / ...` 等 kind 仍然存在于数据流，但是由 projector 产出而非业务代码直接写。

### 6.3 投影器（projector）

- **Task projector**：监听 Story session event 中"LifecycleStepState 变更"类事件 → 通过 `run.session_id` 找 Story binding → 在 `Story.tasks` 中按 `Task.lifecycle_step_key == step_key` 定位目标 Task → 计算目标 Task 的 status / artifacts → 更新 Story aggregate（或直接 patch JSONB 中的 task 子元素，取决于实现策略）→ 同事务 append 一条 `state_changes` 记录。
- **Story projector**：监听"业务级 story 状态"事件（如人工归档、验证通过）→ 更新 `Story.status` + append `state_changes`。
- **启动期 rebuild**：应用启动时，`task::view_projector::project_task_views_on_boot` 从 LifecycleRun/step state 反投影到 Task view（未来可演进为 replay Story session events → 重建 Task view）；不再依赖旧的多路写入合并策略。
- **业务终态 → session cancel 指令通道**（与 projection 方向相反）：`reconcile::terminal_cancel::TerminalCancelCoordinator` 在 Task/Story 进入终态时，对关联 running session 发起 cancel，保证业务状态与 session 生命周期一致。

### 6.4 跨 session 全局游标

- `session_events` 是 per-session 游标，不适合做全局 resume。保留 `state_changes.id` 作为全局游标，使前端 `/events/since` / project-level backlog 仍能按单调游标拉取。
- **替代方案评估**：后续若要彻底废弃 `state_changes` 表，需要新增 `session_events.global_seq BIGSERIAL` 或专门的全局 outbox 表；本 spec 范围不做此设计。

---

## 7. 对外 API 入口规范

### 7.1 Facade 保留

下列对外入口名字保留（API schema 不变、前端消费契约不变）：

- `start_task(task_id, user_input?)`
- `continue_task(task_id, user_input?)`
- `cancel_task(task_id)`
- 其他 task-level HTTP / MCP 路由同理

### 7.2 内部委托链路

所有 Task 启动/推进操作的内部路径统一为：

```
start_task(task_id, user_input)
  └─ 定位 task → 读取/补齐 Task.lifecycle_step_key → 找到 story_id + 对应 step_key
      └─ activate_story_step(story_id, step_key, user_input)
          └─ LifecycleRunService::activate_step(run_id, step_key)
              + session 装配: compose_lifecycle_node(...) (产出 PreparedSessionInputs)
```

**关键原则**：

- `compose_task_runtime` / `TaskRuntimeSpec` / `TaskRuntimeOutput` 三者**彻底删除**（M5），不再存在 Task-specific 的装配分支。
- `PreparedTurnContext` 过渡壳已删除；task owner prompt augmentation 也必须走 `SessionRequestAssembler::compose_story_step`。
- `LifecycleNodeSpec` 补齐 user prompt 注入（`override_prompt` / `additional_prompt`）、explicit executor config；Task 侧所需的上下文组装全部并入 lifecycle node context pipeline（吸纳 `build_task_agent_context` / `TaskAgentBuildInput` 的逻辑）。
- `task/session_runtime_inputs.rs` 整个文件删除；`resolve_workflow_via_task_sessions` 反查消失。
- Story session / LifecycleRun 的定位通过 `find_active_run_for_story(story_id)` 完成（repo 层新增查询方法，或通过 SessionBinding → session_id → lifecycle_run 两跳）。

### 7.3 activate_story_step 签名要点

```rust
async fn activate_story_step(
    story_id: Uuid,
    step_key: String,
    user_input: Option<UserPromptInput>,  // 承载 prompt / attachments 等
) -> Result<ActivateStoryStepOutput, ApplicationError>;
```

- 内部职责：(1) 找到 story → story session → 活跃 LifecycleRun；(2) 调 `LifecycleRunService::activate_step`；(3) 走 `compose_lifecycle_node` 完成 session 装配；(4) 触发后续执行。
- 错误语义：`ApplicationError`（承接 `WorkflowApplicationError` + `DomainError`）。API 层映射到标准 HTTP 语义，参考 [error-handling.md](./error-handling.md)。
- 事务边界：`activate_step` 当前非事务（两步 IO 各自提交）。本 spec 不强制在主线引入事务，但建议 M2 阶段至少把 `run.update + state_change projector append` 纳入同事务（并发裂缝修复）。

### 7.4 未来演化路径

- UI 手动推 step、外部 trigger、MCP 工具触发 step 激活，**全部**走 `activate_story_step`；不允许为新场景再开特例入口。
- Task facade 的签名 / 路由 / DTO 在 Model C 后仍然是对外契约；未来若评估彻底下线 Task facade，需独立任务单独处理。

---

## 8. 跨 M 项影响索引

| spec 小节 | 影响的 M 项 |
|-----------|-----------|
| §2.1 Story aggregate / §2.4 Task / §5 持久化 | M1（合入 Story aggregate / stories.tasks JSONB 列） |
| §2.4 Task 投影字段 / §6 event projection | M2（状态真相单一化 / D3-β） |
| §2.2 Story session / §2.5 child session / §3 拓扑 / §4 label | M3（Story session / child session 命名统一） |
| §5.2 binding_kind migration / §2.3 LifecycleRun | M4（WorkflowBindingKind 收敛） |
| §7 对外 API / §2.3 LifecycleRun | M5（Task 启动路径并入标准 workflow 装配 + activate_story_step） |
| §6.3 projector 边界 | M6（reconciler 命名纠正） |
| §5 持久化 / 职责拆分 | M7（gateway 拆分；ops / session 桥 / resolve / artifact / meta 分组） |

---

## 9. 待澄清 / 未来演化

以下问题本 spec 不定论，留给主线后续阶段或 cleanup tail：

1. **Story.status 的最终定位**：保持"业务审计字段"（面向用户的 story 粒度状态），不是 runtime projection 真相。LifecycleRun / step state / session event 是运行时事实；未来可以由 runtime 给出 suggested transition，但本 spec 不引入新 projector。
2. **Task 投影字段的类型机制**：是采用 `TaskSpec` + `TaskView` 拆分，还是让 Task 私有化 setter 仅 projector 可调，由 M2 实现阶段选择。本 spec 强制的契约仅是"外部不可直接写"。
3. **`state_changes` 表最终去留**：D3-β 选择降级为索引；若未来引入 `session_events.global_seq` 或独立 outbox，可以进一步废弃 `state_changes` 表。
4. **Task facade 彻底下线**：`start_task` 等入口是否长期保留由产品决策；spec 约束的是"内部链路统一"，不是"对外 API 永久保留"。当前代码中 `StoryStepActivationService` 承载 activation 编排，task route 只是用户入口 facade。
5. **多 Project 场景下的 aggregate 激进合并**：若未来要把 Project 也纳入同类 owner session 模型，Story ↔ Project 的关系需要独立讨论。
6. **PRD 与 research 对 `state_changes` 的位置表达**：PRD 主文多处描述 state_changes "降级为全局投影索引"，research TBD-2 指出 "schema 互不兼容，不能简单映射"。本 spec 采纳 PRD D3-β 的最终解释（降级为索引，由 projector 维护，schema 保留），若后续实现层遇到跨 session 游标需求，需回到 research 的评估重新设计全局游标方案，不应当成"已解决"。
7. **Task retry / execution mode**：`Task.execution_mode`、`RestartTracker`、`task:retry` effect 已从 Model C task 层删除。若未来需要自动重试，应作为 session 或 lifecycle step execution policy 独立建模，不能回写到 Task durable spec。

---

*创建：2026-04-27 — 主线 M8 任务产出*
