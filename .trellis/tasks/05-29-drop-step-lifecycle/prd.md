# 删除 Step lifecycle 死轨，统一为 Activity 模型

> 病灶 2。事实源：`docs/reviews/2026-05-29-slop-cleanup-review/00-synthesis.md`。
> **决策：Activity lifecycle 是唯一目标轨，Step 轨整体删除。** 预研期，无兼容包袱。

## Scope
跨 domain / application(workflow) / infrastructure 删除旧 Step-based lifecycle，保留并收敛 Activity-based 模型。

## 证据
- domain `crates/agentdash-domain/src/workflow/entity.rs:299` `step_states`（注释"不再上线"）+ `activate_step/complete_step/fail_step/bind_step_session/record_gate_collision/advance_dag_successors` 约 300 行；`LifecycleStepState`/`LifecycleStepExecutionStatus`。
- application `workflow/catalog.rs:143` `WorkflowCatalogService::upsert_workflow_definition` 与 `:178` `ActivityLifecycleCatalogService` 重复 upsert；`workflow/run.rs` step 路径；`session/` provider_lifecycle 对 step_states 的投影。
- infra `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs` 两实体共用 `lifecycle_definitions` 表靠 `entry_activity_key <> ''` 区分（L197/213/229/272/373）。

## Approach
1. domain：删 `step_states` 及全部读写方法、`LifecycleStepState`/`LifecycleStepExecutionStatus`、旧 `LifecycleDefinition`（若已被 `ActivityLifecycleDefinition` 完全取代）。保留 `activity_state`/`ActivityLifecycleRunState`。
2. application：删旧轨 catalog（或合并为单一 service）；删 `run.rs` step 推进路径；`provider_lifecycle` 改投影 Activity run。
3. infra：`lifecycle_definitions` 加 `kind` 列（migration），消除 `entry_activity_key <> ''`；或旧 `LifecycleDefinition` 整体删除则相应删其持久化路径。

## Acceptance
- [ ] `grep -rn "step_states\|LifecycleStepState\|LifecycleStepExecutionStatus" crates/` 归零
- [ ] `cargo check --workspace` 通过
- [ ] 改表则新增 migration
- [ ] 无残留旧 `LifecycleDefinition` 引用（若需保留须在本文件记录理由）

## Constraints
- 仅改 `crates/` + `migrations/`，不动 `packages/`。
- **不要 git commit**，orchestrator 统一 gate 后提交。
- 若发现 Step 与 Activity 非纯新旧关系（某功能仅 Step 有），停下并在本文件记录，不强删。

## 冲突记录（2026-05-29，执行中止）

**结论：Step 轨与 Activity 轨当前不是纯新旧关系，而是服务于不同入口的两套并行活跃 runtime；多个功能仅 Step 有、Activity 无对应。按硬约束停止强删，未改动任何代码（除本记录）。**

### 调查事实（file:line）

1. **Activity 轨确实是 advance/terminal 侧的唯一活跃推进器**，且数据模型完全自洽（`ActivityDefinition`/`ActivityTransition`/`ActivityAttemptState`），不依赖任何 Step 类型：
   - `workflow/orchestrator.rs:126/173` — `on_activity_session_terminal` + `advance_current_activity`（`complete_lifecycle_node` 工具 / session 终态回调）只走 `ActivityLifecycleRunService`。
   - `workflow/engine.rs`、`workflow/activity_run.rs`、`workflow/scheduler.rs` 全 Activity。

2. **但「任务启动 / 续跑」入口仍硬绑 Step 轨，且无 Activity 等价实现**：
   - `task/service.rs:104 activate_story_step` 是 Story-as-durable-session 模型下 task start/continue 的唯一领域入口（`start_task_inner:431` / `continue_task_inner:451` → 公开 API `start_task`/`continue_task`）。
   - 它依赖 `LifecycleRunService::bind_session_and_activate_step`（`task/service.rs:160`）、`fail_step`（`:230`）、`run.current_step_key()`（`:352`）、`lifecycle.steps` 遍历（`:127/338/356`）、`Task.lifecycle_step_key`（`:379`）。
   - Activity 轨没有对应的「按 task 启动并绑定 step session」入口——orchestrator 只处理 advance/terminal，不处理 task-start。

3. **Companion workflow overlay 仅 Step 有**：
   - `companion/tools.rs:868 setup_companion_workflow` → `LifecycleRunService::start_run`（`:910`），无 Activity 版本。

4. **`step_states` 在多个 live 运行时路径被读取（非纯死字段）**：
   - `workflow/tools/advance_node.rs:143/194`（advance_node 工具）
   - `hooks/provider.rs:182/244`（hook snapshot provider）
   - `vfs/provider_lifecycle.rs:213/447/588/593/608`（`lifecycle://` VFS provider 把 step_states 投影成 agent 可读文件）
   - `workflow/lifecycle/journey/mod.rs:455/496/503`（journey 投影）
   - `api/session_use_cases/construction.rs:764`（session 构造读 `current_step_key` + `lifecycle.steps`）

5. **半迁移的内在不一致已存在**：builtin 模板仅以 Activity 形态 seed（`workflow/definition.rs:47 BuiltinWorkflowBundle.lifecycle: ActivityLifecycleDefinition`，经 `ActivityLifecycleDefinitionRepository::create` 写入时 `steps='[]'`），但 `task/service.rs::resolve_task_for_step` 通过 **不过滤的** `LifecycleDefinitionRepository::get_by_id`（`infrastructure/.../workflow_repository.rs:566`，故意不带 `entry_activity_key <> ''`）把同一行当 Step `LifecycleDefinition` 读回并遍历 `lifecycle.steps`。即 task-start 路径与 Activity seeding 已处于不一致的中间态。

6. **infra magic discriminator `entry_activity_key <> ''` 是 load-bearing**：正因为 Step 与 Activity 两实体共用 `lifecycle_definitions` 表且都活跃，`LifecycleDefinitionRepository::get_by_id` 故意不加该过滤以便跨读 Activity 行。安全移除该 discriminator 依赖于先完成 domain 级 Step 退役——被本冲突阻塞。

### 阻塞结论与建议

删除 Step 轨不是「删死代码」，而是需要把 **task 启动/续跑（`activate_story_step`）、companion workflow overlay（`setup_companion_workflow`）、`lifecycle://` VFS 投影（`provider_lifecycle`）、session 构造（`construction.rs`）、hook provider、advance_node 工具** 全部改写到 Activity 模型，并补一条 builtin 行类型统一的 migration。这是一次 feature 迁移而非 slop 清理，盲改会破坏 task runtime 核心链路（编译跨 ~15 个 live 文件、运行期 break `start_task`/`continue_task`）。

建议把本任务降级 / 拆分为：
- (P0a) 先实现 Activity 版的「task-start 绑定入口」与「companion overlay 入口」，让 `activate_story_step` / `setup_companion_workflow` 改走 Activity；
- (P0b) 将 `provider_lifecycle` / `journey` / `advance_node` / `construction` 的 `step_states` 投影改读 `activity_state`（Activity attempt → 视图）；
- (P0c) 上述完成后，再删 domain `step_states`/`LifecycleStepState`/`LifecycleStepExecutionStatus`/Step `LifecycleRunService`/`WorkflowCatalogService` 旧轨/`LifecycleDefinitionRepository`，并加 migration 统一 `lifecycle_definitions` 行类型、消除 `entry_activity_key <> ''`。

未做任何代码改动；`cargo check --workspace` 状态与任务开始时一致（未触碰编译单元）。
