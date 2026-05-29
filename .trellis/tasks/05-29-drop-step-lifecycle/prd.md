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
