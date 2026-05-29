# Step lifecycle 清剿：收口到 Activity 唯一信道 — 技术设计

> 2026-05-29 复查后重写。事实源：本次三路 Explore 取证 + migration 0050 + repo 判别式实读。
> 推翻 prd「冲突记录」里「Step 是 load-bearing 活跃 runtime、需 feature 迁移」的结论——那是只数代码引用、未查数据状态的误判。

## 一、决定性事实（推翻「冻结」）

1. **migration `0050_migrate_activity_lifecycle_payloads.sql:76-87`** 已把**所有** Step 形态定义行就地转成 Activity 形态，并 `SET steps='[]', edges='[]'`。生产中不存在仍带 steps 的定义行。
2. **Step repo 读取（`workflow_repository.rs:566-626 LifecycleDefinitionRepository`）无判别式**，照常返回这些行，但 `lifecycle.steps` 反序列化恒为空。
3. 因此 Step 轨三处启动入口在 0050 后**静态可证必然失败**：`task/service.rs` task 启动（读空 steps → NotFound）、`workflow/run.rs:269 start_run`（`LifecycleRun::new(steps=[])` → "至少需要一个 step"）、`companion/tools.rs:957` workflow overlay（遍历空 steps → NotFound）。
4. **Story session 实际挂的是 `builtin.freeform_session` 单活动 open-ended Activity run**（`freeform.rs:105-131`，由 `story_sessions.rs:321` 创建）——不是多步 lifecycle。
5. 状态机推进早已全切 Activity（`apply_event`/orchestrator）；投影层 `resolve_active_workflow_projection_for_session` 已纯 Activity 驱动；`view_projector.rs:248` 已从 `activity_state.attempts` 合成展示用 `LifecycleStepState`。

**综合**：Step 轨不是「活跃 load-bearing」，而是「数据已被 0050 抽空、生产必报错、且其代表的多步 task 编排从未在产品中真正运行」的死路径 + 死概念。

## 二、关键判断：P1 走「解耦」而非「接 Activity attempt」

task 启动的核心是「建 execution session + launch + bridge event」，**不需要** lifecycle step 激活。`start_task_inner`/`continue_task_inner` 入口本就是 `task_id`，却绕了 `task_id → step_key → task` 的荒谬往返，整个 step_key 中间环节纯粹为喂 Step 激活。

- **采纳**：task 启动从 lifecycle 激活中**解耦**——直接 `task_id → task → session + launch`。Step 轨在 task/companion 侧即刻无消费方。
- **不采纳**：把 task 手动 start 接入 Activity attempt 自动编排（需 story 改用多步 DAG lifecycle + 手动 claim + 实现 `AttachExisting` launcher policy）。那是**新建多步编排能力**，超出「清剿冗余、统一信道」范畴。

**产品含义（已向用户透明声明）**：解耦即移除「Story 多 task 按 lifecycle step 顺序推进」这一能力——但它当前已死且未使用，故无行为回归。未来若要多步 task 编排，应基于 Activity 轨（`trellis_dag_task` 已具完整 DAG/人工决策/条件转换能力，engine 测试已证）重建，属独立 feature，不在本次收口。解耦顺带把 task 启动从「0050 后必失败」修回「可用」。

## 三、目标终态（Activity 唯一信道）

- domain `workflow`：仅留 `ActivityLifecycleDefinition` / `ActivityLifecycleRunState` / `activity_state`。删 `LifecycleDefinition`、`LifecycleStepDefinition`、`LifecycleStepState`、`LifecycleStepExecutionStatus`、`LifecycleEdge`、`LifecycleRun.step_states` 及 `activate_step/complete_step/fail_step/bind_step_session/record_gate_collision/advance_dag_successors/new()`（Step 构造器）。`LifecycleRun` 只经 `new_activity` 构造。
- application `workflow`：删 `LifecycleRunService` 的 step 方法（或整体）、`step_activation.rs`、Step 版 catalog；`provider_lifecycle`/`journey` 投影改读 `activity_state.attempts`。
- task/companion：启动路径解耦，不再触碰 lifecycle 定义/run 的 step 面。
- infra：`LifecycleDefinitionRepository` trait + impl 删除；`lifecycle_definitions` 表 drop `entry_step_key/steps/edges` 列，`lifecycle_runs` drop `step_states` 列；`ActivityLifecycleDefinitionRepository` 的 `entry_activity_key <> ''` 判别式移除（不再需要与 Step 行共存）。

## 四、阶段（每阶段 build-gate + 全测试 + 独立 commit；高风险标注人工 review）

- **P1 解耦 task/companion 启动**：重构 `task/service.rs`（删 `resolve_task_for_step`/`resolve_or_bind_step_key_for_task`/`validate_step_key_for_story`/`find_active_run_for_story_session` 中 step 依赖，`activate_story_step` 收敛为 `task_id`-驱动的 session+launch），`companion/tools.rs` workflow overlay 改走 Activity 或移除。gate。**集成验证：task start/continue 能跑通。**
- **P2 投影迁移**：`vfs/provider_lifecycle.rs` + `workflow/lifecycle/journey/mod.rs` 的 step_states/find_step 等改读 `activity_state.attempts`；`advance_node.rs`/`hooks/provider.rs` 的 step_states 读取改 activity 或删；`api/session_use_cases/construction.rs` 若读 step 同步。gate。
- **P3 清剿 domain + repo 死代码**：编译驱动删尽 Step 类型/方法/字段、`LifecycleRunService` step 路径、`step_activation.rs`、Step catalog、`LifecycleDefinitionRepository`、各 test 桩。`grep step_states|LifecycleStepState|LifecycleStepExecutionStatus|LifecycleDefinition\b|LifecycleStepDefinition` 归零。gate。
- **P4 清除冗余数据/列**：新增 migration drop `lifecycle_definitions.entry_step_key/steps/edges` + `lifecycle_runs.step_states`；移除 repo 判别式与 `LC_COLS`/Step row 映射；contracts/ts 同步。gate。

## 五、风险与回退

- 每阶段独立 commit，可单点回退。
- P1 高风险（task 核心链路）：完成后写集成测试覆盖 start/continue，commit 标注建议人工 review。
- P4 含 DB migration：预研期无兼容包袱，但 migration 不可逆需确认 down 策略（本项目 migration 单向，drop 列即终态）。
- 验收：`cargo check --workspace` 绿 + 全 crate 测试绿 + task 启动集成测试绿 + 上述 grep 归零。
