# M5 决策点草案（等 M1-b 完成后与 M2 决策点合并给用户确认）

M5 · compose_task_runtime 删除 + activate_story_step 统一入口。基于 `m5-preconditions.md` 的 6 项 [BLOCKER]。

---

## D-M5-1 · facade 方法归属于哪个 Service？

spec §7.3 写了 `activate_story_step(story_id, step_key, user_input)` 签名，未指定实现归属。

| 方案 | 描述 | 代价 |
|---|---|---|
| A · 挂在 `TaskLifecycleService` | 现有 service 新增 `activate_story_step` 方法；`start_task` / `continue_task` 内部 delegate；DI 不改 | 最小侵入；服务名"TaskLifecycle"语义与 Model C "task 不自持 runtime" 有轻微违和 |
| B · 新建 `StoryLifecycleService` | 独立 service；`TaskLifecycleService::start_task` 改为转发到 `StoryLifecycleService::activate_story_step` | 语义清晰；需改 DI；有 2 个 service 同时存在时心智负担 |
| C · 挂在 `LifecycleRunService` | 现有 repo wrapper 服务里加 compose + dispatch 职责 | 违反"LifecycleRunService 只动 run 状态"的原有语义 |
| D · 独立 facade struct（如 `StoryStepCoordinator`） | 纯组合 struct，持有 `TaskLifecycleService` / `LifecycleRunService` / `SessionRequestAssembler` | 设计最清晰，但引入新概念 |

**倾向 A**：最小侵入，不改 DI；未来若 TaskLifecycleService 想改名（进一步贴合 Model C），作为独立任务处理。

---

## D-M5-2 · Story → Run 查询路径

| 方案 | 描述 | 代价 |
|---|---|---|
| α · 新增 `LifecycleRunRepository::list_by_story(story_id)` | domain trait + pg + memory 三处加方法；内部仍经 `session_binding → session_id → list_by_session` 链实现 | 代码量小；但要碰 repo trait（M1-b 可能在动 session_binding 相关） |
| β · 两跳组合 · 写在 facade 内 | facade 内部：`session_binding_repo.list_by_owner(Story, id, "companion")` → `lifecycle_run_repo.list_by_session(session_id)` → `select_active_run` | 复用现有 repo 方法；无需改 trait；但 facade 带 2 个 repo 依赖 |

**倾向 β**：避开 M1-b 改动面；可以作为 private helper `find_active_run_for_story` 住在 facade 所在模块，未来若性能成问题再升级为 α。

---

## D-M5-3 · compose 函数形式（`&self` method vs free function）

当前：
- `compose_lifecycle_node`：free function `(repos, platform_config, spec)` → 被 orchestrator 调用
- `compose_task_runtime`：`&self` method on `SessionRequestAssembler`，持有 `vfs_service / availability / contributor_registry`

| 方案 | 描述 | 代价 |
|---|---|---|
| I · 升级 `compose_lifecycle_node` 为 assembler method | 统一形式；orchestrator 要拿到 `SessionRequestAssembler` 实例 | 改 orchestrator DI；函数风格一致 |
| II · 新增 assembler method `compose_story_step`，保留 free `compose_lifecycle_node` | task 路径走 method，orchestrator 继续用 free function | 两套并存；职责清晰 |

**倾向 II**：task 启动路径需要 `vfs_service / availability / contributor_registry` 三个服务依赖，走 method 自然；orchestrator 的 phase node 场景不需要这些依赖，保持 free function 无压力。与 D-M5-4 配合良好。

---

## D-M5-4 · spec 结构：扩展 `LifecycleNodeSpec` vs 新 `StoryStepSpec`

TaskRuntimeSpec 有 9 字段（task / story / project / workspace / phase / override_prompt / additional_prompt / explicit_executor_config / strict_config_resolution），LifecycleNodeSpec 只有 5 字段。

| 方案 | 描述 | 代价 |
|---|---|---|
| i · 扩展 `LifecycleNodeSpec` | 原结构加 9 字段（其中 5 个 optional） | orchestrator 调用方要传 None，噪音大 |
| ii · 新增 `StoryStepSpec` | 两个并存：LifecycleNodeSpec（简单 phase node）+ StoryStepSpec（task 启动完整上下文） | 两个结构；但职责清晰 |

**倾向 ii**：与 D-M5-3 倾向 II 配对；`compose_story_step(StoryStepSpec)` 专为 task facade 服务，`compose_lifecycle_node(LifecycleNodeSpec)` 给 orchestrator 的 phase node 场景。

---

## D-M5-5 · `api/routes/workflows.rs::activate_workflow_step` 是否升级

当前 HTTP POST `/workflows/runs/{run_id}/steps/{step_key}/activate` 只调 `LifecycleRunService::activate_step` 更新 run 状态后返回 JSON，**不 compose / 不 dispatch**。

| 方案 | 描述 | 对前端契约 |
|---|---|---|
| x · 保持不升级 | workflows.rs 路由保持"只状态变更"语义 | 无影响 |
| y · 升级为调 `activate_story_step` | 路由升级成 "激活 + 装配 + dispatch"；语义从"改状态"变"真正启动" | **改变前端契约**（`services/workflow.ts:496` 目前依赖"只状态变更"） |

**倾向 x**：M5 范围只动 task 启动路径（task_execution 入口）；workflows.rs 是给"开发者/UI 手动推 step"用，本轮保持现状。若未来要统一 UI 手动推 step = task 启动，作为独立任务。

---

## D-M5-6 · `PreparedSessionInputs` 需要承接 `identity` / `post_turn_handler`

[NEED_FURTHER_INVESTIGATION]。M5 实施 agent 自己读 `assembler.rs:75-130` 核对；若 `PreparedSessionInputs` 不支持这两字段，facade 输出需要一个 "full turn context" wrapper（如 `StoryStepTurnContext = PreparedSessionInputs + identity + post_turn_handler`），M5 agent 可自主决定结构。

---

## 死字段删除（不用决策，直接落地）

`TaskRuntimeOutput` 11 字段中 5 个可删：`use_cloud_native_agent / workspace / executor_resolution / workflow / resolved_bindings`（dispatcher 未消费；compose 内部消耗完）。

## PreparedSessionInputs 需要补齐的字段

合并 `compose_task_runtime` 所必需的 5 个字段（dispatcher 会读）：`prompt_blocks`（builder 已支持）、`working_dir`、`relay_mcp_server_names`、`source_summary`；`executor_config`（已有 `with_executor_config`）。

---

## 总结：M5 spawn 前需用户敲定的 5 项

| 决策 | 倾向 | 需用户确认 |
|---|---|---|
| D-M5-1 facade Service 归属 | A · 挂在 `TaskLifecycleService` | ✅ |
| D-M5-2 Story→Run 查询路径 | β · 两跳组合 helper | ✅ |
| D-M5-3 compose 函数形式 | II · 新增 method，保留 free function | ✅ |
| D-M5-4 spec 结构 | ii · 新增 `StoryStepSpec` | ✅ |
| D-M5-5 `workflows.rs` 路由升级 | x · 保持不升级 | ✅ |

M1-b 完成后，M2 的 5 个决策点 + M5 的 5 个决策点一次性发给用户对齐。

---

## M5 依赖链

M5 必须等：
1. M1-b 完成（task 模块迁移到 Story aggregate，task 字段 reshuffle 结束）
2. M2 完成（Task.status 字段私有化机制定案；StepState projection pipeline 就位——facade dispatch 完一次 turn 后 step 完成会走 M2 的 projector）

M5 实施时会删除：
- `compose_task_runtime` / `TaskRuntimeSpec` / `TaskRuntimeOutput`（assembler.rs 约 300 行）
- `task/session_runtime_inputs.rs` 整个文件
- `resolve_workflow_via_task_sessions` 两处定义
- `PreparedTurnContext`（若 facade 输出换成 PreparedSessionInputs + wrapper）

M5 实施时会新增：
- `compose_story_step` method
- `StoryStepSpec` struct
- `activate_story_step` facade 方法
- `find_active_run_for_story` helper（在 facade 模块）
