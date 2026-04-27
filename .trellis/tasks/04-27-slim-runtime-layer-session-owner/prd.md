# Story/Task 模块重构：Story ≡ durable session 业务视图 · Task 合入 Story 聚合 · Session event 为真相唯一源

## Goal

`story/task` 是项目最早开发的模块，长期未系统性跟进，代码里累积的不是简单冗余——**架构上它和 session/workflow 两层的关系从未被显式对齐**，结果是状态真相分散、启动路径分袍、代码可读性差。

本任务在 Model C（Story ≡ durable session 的业务视图）定位下，把这三个痛点一次性解决：

1. **状态真相模糊 / 排障困难**：Session event stream 成为唯一审计源，Task/Story 业务视图从 event 投影而来
2. **代码杂乱 / 可读性差**：Task 合入 Story aggregate，删除独立仓储 + 字符串协议 + 万能 gateway + 多源 status 写入
3. **Task 启动路径与 workflow 不一致**：`compose_task_runtime` 特例分支取消，task 激活 = Story session 下的 step 激活，走标准 workflow 装配

## Pain Points（按痛点权重排序）

| # | 痛点 | 根因 | 解决入口 |
|---|---|---|---|
| P1 | 状态真相模糊 / 排障困难 | Task.status 9+ 写入源 · projection 三路分散 · StateChange 与 session event 两套真相 | M2 |
| P2 | 代码杂乱 / 可读性差 | Task 独立仓储 + 跨域耦合 + 万能 gateway + 字符串 TaskStatus 协议 + 空壳/半迁移模块 | M1 / M6 / M7 |
| P3 | Task 启动路径与 workflow 分袍时常不一致 | `compose_task_runtime` 特例分支不走标准 workflow 装配 · 缺 service/API 级 activate step 入口 | M5 |

## 终局建模（Model C 精确化）

```
Story (aggregate root, durable)
  ├─ durable_metadata          # 启动 session 的模板参数 + 面向用户业务审计
  │   ├─ title / priority / tags
  │   ├─ context / workspace / agent_binding
  │   └─ status                # 业务审计状态（面向用户，不是 runtime 真相）
  ├─ tasks: Vec<Task>          # ← 合入 Story aggregate（无独立 repo, 无独立表）
  └─ (conceptually: 关联一个 Story session, 通过 SessionBinding)

Story session (kind=Story, durable)
  ├─ event_stream              # 唯一审计源（承载 StateChange 类事件）
  ├─ lifecycle_run: 1 个       # 独立 entity, 挂在此 session
  │   └─ steps (each refers to task_id)
  └─ child sessions: N 个      # 实际对话 / companion / step 远程执行

Task (Story aggregate 下的 child entity, 不是独立 aggregate)
  ├─ durable spec              # title / description / agent_binding / workspace / tags
  ├─ status, artifacts         # ← 只读投影 from lifecycle step state
  └─ 物理持久化 = stories.tasks JSON 列 (由 StoryRepository::update 整体写回)
```

### 三层关系

- **Story ↔ Story session**：1:1 绑定（经 SessionBinding）。Story 是 durable aggregate，Story session 是这个 aggregate 对应的"运行外壳"。stories 表保留独立业务语义（启动参数 + 审计）。
- **Story session ↔ LifecycleRun**：1:1。Run 独立 entity，挂在 Story session 上。Step 里 `task_id` 引用 Story aggregate 内的 Task。
- **Story session ↔ child session**：1:N。实际对话、companion、step 远程执行都是 Story session 的 child。命名统一为 **Story session（root）/ child session**。

## Key Decisions（已对齐）

| 决策 | 选择 | 影响 |
|---|---|---|
| D1 Task 形态 | 归 Story 下，无独立 repository（DDD child entity） | M1 |
| D2 LifecycleRun 独立性 | 独立 entity，挂 Story session | M3 |
| D3 StateChange 未来 | β: 降级为全局投影索引；真相 = session event stream + step state；`state_changes` 表保留作"跨 session 全局游标索引"，由上游 event 自动投影维护；`/events/since` API 保持兼容 | M2 |
| D4 Task runtime API | 保留 facade（`start_task` 等不改名） | M5 内部实现改写 |
| D5 DB 迁移 | stories 表保留；tasks 合进 stories（JSON 列，migration） | M1 |

## Main Line Items (M1–M8)

### M1 · Task 合入 Story aggregate（P2 根治 + D1/D5）

- Task 变为 Story aggregate 下的 child entity；`Story` 持有 `Vec<Task>`
- 删除 `TaskRepository` / `TaskAggregateCommandRepository`
- 顺手删除 `TaskRepository::list_by_project` / `list_by_workspace`（research 证实**无生产调用者**）
- 删除 `story/management.rs` 里的 `TaskMutationInput` / `apply_task_mutation` / `build_task`；task mutation 改为 Story aggregate 的内部方法（如 `Story::add_task` / `Story::update_task` / `Story::remove_task`）
- `StoryRepository::update` 负责 Story 整体（含 tasks）的持久化
- `task_count` 冗余列**保留**（前端/UI 依赖；合入后可用 `jsonb_array_length(tasks)` 校准）
- `project/management.rs:104-106` 的 N+1 双循环（stories × tasks）**自动消除**（JSONB 直取）
- DB migration：
  - stories 表新增 `tasks` JSONB 列
  - 数据迁移：读 tasks 表 → 按 story_id 聚合 → 写入 stories.tasks
  - 旧 tasks 表保留一段时间做只读回滚，后续 cleanup
- API 层 `/stories/{id}/tasks/...` 路由内部改走 StoryRepository（API schema 不变）

### M2 · 状态真相单一化：真相 = Session event stream + step state；state_changes 降级为全局投影索引（P1 根治 + D3-β）

- **Task.status / Task.artifacts 降为 step state 的只读投影**：删除所有直接写入 `Task.status` 的路径
- **state_changes 表降级为全局投影索引**（D3-β）：
  - 真相源 = Story session event stream + LifecycleRun/step state
  - `state_changes` 表继续存在，但**不再是业务事件独立写入源**，改为由上游 event 变更时自动投影写入（投影器在 event pipeline 里注册）
  - `/events/since/{since_id}` API 保留；`state_changes.id`（全局 BIGSERIAL）继续承担跨 session 全局游标
  - 现有 `ChangeKind::TaskStatusChanged` 等在 projection 层自动追加，不再由业务代码直接写
- **projection 三路收口**：
  - 拉式：[task/state_reconciler.rs](crates/agentdash-application/src/task/state_reconciler.rs) `reconcile_running_tasks_on_boot` → 改为 "replay session events → rebuild Task view"
  - 指令式：[reconcile/runtime.rs](crates/agentdash-application/src/reconcile/runtime.rs) `on_task_status_changed/on_story_status_changed` → 保留作为"业务决策 → cancel session"的**指令通道**，但不再写 Task.status
  - 查询式：[task/session_runtime_inputs.rs](crates/agentdash-application/src/task/session_runtime_inputs.rs) `resolve_workflow_via_task_sessions` → 随 M5 删除
- **字符串 TaskStatus 协议消失**：[effect_executor.rs](crates/agentdash-application/src/task/gateway/effect_executor.rs) 里 `"completed" => TaskStatus::Completed` 这套手写映射删除（Task.status 不再可直接写）
- **跨域耦合自然消失**：story/management.rs 里不再有 task.status 写入（M1 已删）

### M3 · Story session / child session 语义统一（P1 辅助 + D2）

- `LifecycleRun.session_id` / `LifecycleStepState.session_id` / `CompanionContext.parent_session_id` 使用同一抽象类型（type alias 或 newtype）
- 命名统一为 "Story session（root）/ child session"
- 补 `.trellis/spec/backend/story-task-runtime.md`：画出 Story session / LifecycleRun / child session / Task view 四件套关系图
- 内部类型命名对齐；前端 API 字段命名不改（`parent_session_id` 保留）

### M4 · WorkflowBindingKind 收敛（P3 辅助）

- `WorkflowBindingKind` 定义侧收敛为 `Project / Story`
- `SessionOwnerType` 保留 `Project / Story / Task` 三档（session binding 的 owner 坐标系不动）
- DB migration：检查现有 `binding_kind='task'` 的 lifecycle definition 记录，迁移到 `binding_kind='story'` 或删除
- 该字段 enum 值收敛的向后兼容策略：TBD（取决于 DB 实际数据）

### M5 · Task 启动路径并入标准 workflow 装配（P3 根治 + D4）

**Compose 层**：
- 删除 `session/assembler.rs::compose_task_runtime` / `TaskRuntimeSpec` / `TaskRuntimeOutput`（210 行特例分支）
- Task 启动走 `compose_lifecycle_node`
- `LifecycleNodeSpec` 补齐 user prompt 注入（`override_prompt` / `additional_prompt`）、explicit executor config
- 删除 `task/session_runtime_inputs.rs` 整个文件
- 删除 `resolve_workflow_via_task_sessions`（task→binding→session→lifecycle 反查消失）
- `build_task_agent_context` / `TaskAgentBuildInput` / task-specific contributor pipeline 合并入 lifecycle node context pipeline

**Service/API 层（新增）**：
- 新增统一 `activate_story_step(story_id, step_key, user_input)` service 命令入口
  - 内部调 `LifecycleRunService::activate_step` + `compose_lifecycle_node`
- `start_task(task_id, ...)` / `continue_task` / `cancel_task` facade 保留原名，内部委托给 `activate_story_step` + 定位 task 对应的 step_key
- 未来 UI 手动推 step 或外部触发也复用这个统一入口

### M6 · Reconciler 命名纠正（P2）

当前两个 reconciler 命名混淆，职责方向相反：
- `state_reconciler`：启动期**读** session 真相**写** Task view（projection 路径）
- `RuntimeReconciler`：运行期 Task/Story 业务决策**写** session cancel（command 路径）

**目标**：
- 要么重命名（`TaskViewProjector` / `StoryCancelCommander` 或类似反映真实职责的名字）
- 要么合并到统一的 Story session coordination 层
- 最终决定取决于 M2 实现后剩下的职责形态

### M7 · 万能 gateway 拆分（P2）

[task/gateway/repo_ops.rs](crates/agentdash-application/src/task/gateway/repo_ops.rs) 554 行拆分，按职责：
- repo ops（`get_task` / `update_task_status`）
- session 桥接（`create_task_session` / `sync_task_executor_session_binding_from_hub`）
- resolve 族（`resolve_effective_task_workspace` / `resolve_task_backend_id` / `resolve_project_scope_for_owner`）
- artifact 持久化（随 M2 重审——artifact 也应该是 step state 投影）
- meta 桥接（随 M2/收尾 task R7 下沉）

### M8 · Story-as-durable-session spec 文档化

`.trellis/spec/backend/story-task-runtime.md` 新建，内容：
- Story ↔ Story session 1:1 绑定的 domain primitive 定义
- `SessionBinding(owner_type=Story, label=?)` 中 label 的值集规范
- Story aggregate（含 Vec<Task>）的持久化规则
- Story session event stream 作为 StateChange 继任者的映射关系
- Story session / LifecycleRun / child session / Task view 四件套关系图
- 对外 API（start_task 等 facade）的内部委托链路

## Acceptance Criteria

### 痛点 1 · 状态真相

- [ ] `grep "task.status\s*=" crates/agentdash-application/src` 无独立写入（仅 projection 入口与测试 fixture）
- [ ] 无 `ChangeKind::Task*` 独立写入路径；Task 相关业务事件统一走 Story session event
- [ ] `resolve_workflow_via_task_sessions` 被删除
- [ ] `Task.status` 字段如果保留，必须用类型机制保证不可从外部写（如只读 view struct 或 crate-private setter）

### 痛点 2 · 代码整洁

- [ ] `TaskRepository` / `TaskAggregateCommandRepository` 删除；Story aggregate 持有 `Vec<Task>`
- [ ] `src/story/management.rs` 不再出现 `TaskMutation` / `apply_task_mutation` / `build_task`
- [ ] `src/task/gateway/repo_ops.rs` 拆成 ≤ 250 行的专职文件
- [ ] `TaskStatus` 字符串协议删除；任何"字符串 → TaskStatus"的显式映射消失
- [ ] `src/task/session_runtime_inputs.rs` 整个文件删除

### 痛点 3 · Task 启动与 workflow 一致

- [ ] `compose_task_runtime` / `TaskRuntimeSpec` / `TaskRuntimeOutput` 从 `session/assembler.rs` 删除
- [ ] 新增 `activate_story_step` service 命令入口；`start_task` 等 facade 内部委托它
- [ ] Task 启动路径与 `compose_lifecycle_node` / `compose_companion_with_workflow` 走同一 `PreparedSessionInputs` 装配

### 架构定位

- [ ] `.trellis/spec/backend/story-task-runtime.md` 新建，明确 Model C（Story ≡ durable session）定位
- [ ] `WorkflowBindingKind` 定义侧收敛为 `Project / Story`；Task-scope lifecycle definition 迁移完成或文档化退场
- [ ] `LifecycleRun.session_id` / `LifecycleStepState.session_id` / `CompanionContext.parent_session_id` 使用同一抽象类型
- [ ] stories 表新增 tasks JSONB 列；数据迁移完成；tasks 旧表标记 deprecated

### 质量门

- [ ] CI（lint / typecheck / 已有单测 + 新增 event projection 层单测）全绿
- [ ] 不破坏既有 API schema（前端消费契约保持，`start_task` 等入口名保留）
- [ ] 新增测试覆盖 event → Task view 的投影正确性（金本位 fixture）

## Out of Scope

- **不** 新增 runtime 层实体（`StoryExecutionPlan` / `StoryRuntime` 等）
- **不** 把 runtime 真相迁到 Story aggregate（真相在 Story session event stream + LifecycleRun state）
- **不** 合并 stories 与 sessions 表（stories 表保留独立业务语义）
- **不** 改前端 API schema / DTO 字段命名
- **不** 重构 `SessionBinding` 架构
- **不** 引入事件总线 / 外部 CQRS 基础设施（用现有 session event stream）

## Deferred to cleanup tail · [04-27-story-task-cleanup-tail](../04-27-story-task-cleanup-tail/prd.md)

| R# | 主题 | Model C 下的新含义 |
|---|---|---|
| R4 | `task/tools/` 空壳删除 | 纯清理 |
| R7 | `task/meta.rs` ACP meta 下沉 | 下沉到 session 消息层 |
| R8 | TaskLock 与 session hub 锁重叠 | 需核查 session hub 并发语义 |
| R9 | RestartTracker 归属（task vs step vs session） | 独立讨论，可能影响 `execution_mode` |
| R10 | `Task.executor_session_id` 尾巴字段 | follow-up/resume 路径评估 |
| R11 | `execution.rs` DTO 语义 | M5 后重新评估去留 |
| R14 | 四条 session 相关 API 路由重叠盘点 | 独立路由核查 |
| R15 | Story.status 定位 | Model C 下 Story.status 是业务审计字段（非投影），是否保留 / 是否引入 suggested transition 独立讨论 |

### 原 R 清单在主线的映射

| 原 R# | 去向 |
|---|---|
| R1 Task.status 多源写入 | M2 |
| R2 projection 三路分散 | M2 |
| R3 Task/Story 跨域耦合 | M1（合入后自然消失） |
| R5 万能 gateway 拆分 | M7 |
| R6 字符串 TaskStatus 协议 | M2（字段不可直接写后，协议自然消失） |
| R12 reconciler 命名 | M6 |
| R13 bootstrap 投影入口 | M2 |
| R16 WorkflowBindingKind 收敛 | M4 |
| R17 Task aggregate boundary（a 激进版） | M1（完整落地 Task 合入 Story） |
| R18 父子 session 语义合并 | M3 |
| R19 `compose_task_runtime` 并入 lifecycle node | M5（扩：+ activate_story_step 入口） |
| R20 Story 多 session 建模 | Model C 下消解（不再是多平级 session） |

## Technical Notes

### 主线前 TBD 核查结论（已 research 核对，详见 [research/tbd-verifications.md](./research/tbd-verifications.md)）

| TBD | 结论 | 对主线的意义 |
|---|---|---|
| 1 · `binding_kind='task'` 迁移 | 生产仅 2 处（MCP 解析 + `trellis_dag_task.json` builtin）；UPDATE migration + 改 json 即可；DB 存量数量需跑 `SELECT COUNT(*)` | M4 可直接实施；存量 COUNT 留 migration 阶段核对 |
| 2 · StateChange 游标 | `state_changes.id` 全局 BIGSERIAL vs `session_events.event_seq` per-session，schema 互不兼容 | **已采 D3-β**：state_changes 降级为全局投影索引（M2 已更新） |
| 3 · Story label 值集 | 实际仅 `"companion"`（单一值），API 默认值，DB 无 CHECK 约束 | M3/M8 枚举化零风险 |
| 4 · `activate_step` facade | 签名简单、已被 API 消费、非事务；需补 `find_active_run_for_story` 查询 + `user_input` 参数 | M5 改造成本中等，无阻塞 |
| 5 · Story aggregate 性能 | `list_by_project`/`list_by_workspace` 无生产调用（直接删）；N+1 存在于 `project/management.rs:104-106`（M1 合入自动消除）；单 story 1000 task ≈ 1MB JSONB 无容量顾虑；`task_count` 列建议保留 | M1 完全可实施，无需 pagination |

### 当前代码规模

```
crates/agentdash-application/src/task/:     ~3155 行 （M1+M5+M7 后预期缩减到 ≤1800 行）
crates/agentdash-application/src/story/:     ~430 行 （M1 后：取消 task 逻辑，但增加 aggregate 方法）
crates/agentdash-application/src/workflow/: ~3323 行 （M5 后：+activate_story_step，微增）
crates/agentdash-application/src/session/assembler.rs: 1409 行（M5 后 ≤1200 行）
```

### 相关 spec / 历史任务

- **[.trellis/spec/backend/story-task-runtime.md](../../spec/backend/story-task-runtime.md)** — 本任务 M8 产出；M1-M7 所有重构的架构基线
- `04-01-agentdash-gsd-workflow-alignment`：外层 orchestration / 内层 lifecycle 的判断（兼容）
- `04-22-complete-lifecycle-node-refactor`：lifecycle node 收束方向
- `.trellis/spec/backend/workflow/lifecycle-edge.md`
- `.trellis/spec/guides/cross-layer-thinking-guide.md`

### M8 spec 的 6 项待澄清（不阻塞 M1）

1. Story.status 最终定位（业务审计 vs 投影）→ M2/cleanup R15
2. Task 投影字段类型机制（TaskSpec+TaskView 拆分 vs 私有化 setter）→ M2 选
3. `state_changes` 表最终去留 → 本次降级为索引，未来可进一步废弃
4. Task facade 是否长期保留 → 本次保留名字，产品决策
5. 多 Project 场景 aggregate 合并 → 独立任务
6. 全局游标设计 → 本次保留 `state_changes.id` 作全局游标，未来若废弃 `state_changes` 需独立评估 `session_events.global_seq` 或 outbox

### 与已删除任务 `04-27-story-task-lifecycle-step-isomorphism-assessment` 的关系

那份评估的观察（Story/Task 与 Lifecycle/Step 同构现象）在 Model C 下得到更精确的解释：两者同构不是命名巧合，也不是两个独立生命周期，而是**Story 本就是 durable session 的业务视图**。Task 是这个 session 里 lifecycle step 的业务 spec。草案里的 "Story owns runtime" 一句按字面理解不对（runtime 真相在 session event + LifecycleRun，不是 Story aggregate），但精神方向（task 不自持 runtime、task 编译为 step spec）与本任务一致。
