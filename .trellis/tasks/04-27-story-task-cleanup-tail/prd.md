# story/task 模块重构 · 清理收尾

## Goal

主线任务 [04-27-slim-runtime-layer-session-owner](../04-27-slim-runtime-layer-session-owner/prd.md) 在 Model C 下完成核心架构重构（Task 合入 Story aggregate、状态真相单一化、task 启动路径并入标准 workflow 装配、Story-as-durable-session 文档化），本任务承接主线路径之外的独立清理与决议。

每一项可独立评估、独立落地 PR。

## 依赖

**必须等主线任务完成**。主线锁定三层架构定位（Story aggregate 含 Vec<Task> · 独立 LifecycleRun 挂 Story session · Session event 为唯一审计源）后，本任务的每一项决议都建立在这个基础上。

## 范围

### 1. 半迁移空壳模块清理（R4）
- [task/tools/mod.rs](../../../crates/agentdash-application/src/task/tools/mod.rs) 仅剩一行注释"已迁移到 crate::companion 模块"
- 目标：删除整个目录

### 2. ACP meta 桥接下沉（R7）
- [task/meta.rs](../../../crates/agentdash-application/src/task/meta.rs) 60 行的 ACP meta 工具（`build_task_lifecycle_meta` / `extract_turn_id_from_meta` / `turn_matches` / `parse_turn_event`）
- Model C 下 session event stream 是真相，ACP meta 桥接更适合作为 session 消息层通用能力
- 目标：下沉到 session 消息层或 `agentdash_acp_meta` crate

### 3. TaskLock 语义决议（R8）
- [task/lock.rs](../../../crates/agentdash-application/src/task/lock.rs) 207 行 per-task 锁
- M5 之后 task 启动走 `activate_story_step`，串行化约束应由 Story session / LifecycleRun 层保证
- 决议：保留作为幂等闸门 / 改造 / 删除
- 前置：核查 session hub 并发语义

### 4. RestartTracker 归属层次（R9）
- [task/restart_tracker.rs](../../../crates/agentdash-application/src/task/restart_tracker.rs) 330 行 AutoRetry 指数退避
- 决议：保留 task 级（task 重试策略）/ 下沉到 step（step-level retry）/ 下沉到 session
- 可能影响 `Task.execution_mode` 字段未来

### 5. Task.executor_session_id 尾巴字段（R10）
- [task/entity.rs:25](../../../crates/agentdash-domain/src/task/entity.rs#L25) 字段仍在，注释已声明迁移
- M1 后 Task 合入 Story aggregate，这个字段随 Task 结构变动
- 决议：
  - 彻底迁到 SessionBinding metadata / session 自身
  - 或保留并明确语义为 "executor 原生会话 id（非 AgentDash 内部 session id）"
- 评估对 executor follow-up / resume 路径影响

### 6. execution.rs DTO 梳理（R11）
- [task/execution.rs](../../../crates/agentdash-application/src/task/execution.rs) 67 行 DTO
- M5 之后 `start_task` 走 `activate_story_step` facade，部分 DTO 字段可能不再需要
- 决议：与 session hub / lifecycle step 入口的 DTO 复用 / 清晰划分

### 7. API 路由重叠核查（R14）
- [routes/task_execution.rs](../../../crates/agentdash-api/src/routes/task_execution.rs)
- [routes/story_sessions.rs](../../../crates/agentdash-api/src/routes/story_sessions.rs)
- [routes/acp_sessions.rs](../../../crates/agentdash-api/src/routes/acp_sessions.rs)
- [routes/project_sessions.rs](../../../crates/agentdash-api/src/routes/project_sessions.rs)
- 四条路由都涉及 session 发起/操作，盘点职责边界与是否有死路径

### 8. Story.status 定位决议（R15）
- Model C 下 Story.status 是**面向用户的业务审计字段**（非 runtime 真相投影），与 Task.status（step state 投影）性质不同
- 决议：
  - 保留纯业务字段，用户/API 直接写入
  - 是否引入 "suggested transition" 机制（如 all steps done → suggest Completed，但不强制）
  - RuntimeReconciler 对 Story 终态的级联策略

### 9. WorkflowBindingKind::Task 历史数据迁移收尾（R16 扩展）
- 主线 M4 删除 `WorkflowBindingKind::Task` 分支
- 若 DB 里存在 `binding_kind='task'` 的 lifecycle definition 实例，主线 migration 未能一次搞定的收尾
- enum 向后兼容策略落地（如需要）

### 10. 前端 WorkflowTargetKind = "task" 清理（M4 frontend follow-up）
- 主线 M4 已在后端删除 `WorkflowBindingKind::Task`，但前端仍保留该选项
- 需改动：
  - `frontend/src/types/workflow.ts`：`WorkflowTargetKind = "project" | "story"`（去 `"task"`）
  - `frontend/src/services/workflow.ts:35`：`WORKFLOW_TARGET_KINDS` 数组去 `"task"`
  - `TARGET_KIND_LABEL` / `AUTO_GRANTED_BASELINE` 相关入口更新
  - `frontend/src/features/workflow/workflow-editor.tsx:1316` dropdown 去 task
  - `frontend/src/components/dag-lifecycle-panel.tsx:104` dropdown 去 task
- 独立 frontend subtask；20 行 diff，纯 UI 修复
- 后端已先行 enforce（MCP 解析 `"task"` 报错），即使前端未改也不会造成 runtime 灾难，只是 UX 残留

### 11. Projector 事务边界 tx-a（D-M2-3 [UNRESOLVED]）
- 主线 M2 projector 采用 tx-b 非事务语义：`story_repo.update + state_change.append_change` 两步 IO 各自提交
- 失败场景：story view 已更新但 state_change 漏写 → 索引与真相短暂不一致
- 目标：引入 `UnitOfWork` / `TransactionalRepositories` 使两步写入 ACID 事务
- 代价：Repository trait 可能需加 `&mut Transaction` 参数，跨 domain/infrastructure crate 改动面大
- 参考：`LifecycleStepProjector` doc comment 已记录 fallback 策略

### 12. 命令路径彻底走 workflow transition（D-M2-4 方案 1）
- 主线 M2-c 命令路径采用方案 2（双写保留）：直接写 task.status + append state_change
- 目标：让 projector 成为 `TaskStatusChanged` 的**唯一**写入源 —— 所有命令路径（API PATCH / task service start/continue/cancel / MCP update_task_status / gateway::update_task_status）改为通过 `LifecycleRunService` 的 step transition 完成状态变化
- 代价：破坏现有"强制完成/失败"业务语义（比如标记已完成但跳过 workflow step transition 的场景需要单独处理）
- 前置工作：分类每个 TaskStatus 变化能否自然映射到 step transition

### 13. LifecycleRunRepository::list_active_by_project 性能优化
- 主线 M2-c 的 `view_projector` 走 `list_by_project` 全量拉取后内存过滤
- 如 project 下历史 run 累积，加 `list_active_by_project(project_id)` repo 方法 + 索引
- 现阶段数据规模不是性能瓶颈；等实际 metrics 再说

### 14. PreparedTurnContext 下沉到 PreparedSessionInputs + wrapper
- M5 后 `task/gateway/turn_context.rs` 的 `PreparedTurnContext` 保留作为 identity + post_turn_handler 的过渡容器
- `acp_sessions.rs::augment_session_prompt` 仍消费 PreparedTurnContext 的中间字段（built / vfs / resolved_config 独立存放）
- 目标：评估 `augment_session_prompt` 改走 `PreparedSessionInputs` + wrapper 后，`turn_context.rs`（当前 337 行）整体删除的可行性
- 当前是 gateway 目录最胖的文件之一

### 15. persist_turn_failure_artifact 死代码清理
- M7 搬家时发现 `artifact_ops.rs::persist_turn_failure_artifact` 第 223-224 行有 `let (story_id, project_id) = (story.id, story.project_id); let _ = (story_id, project_id);` 这种"先绑再丢"的无效赋值
- 早期 refactor 遗留，可以直接删

### 16. effect_executor.rs 字符串 → TaskStatus 映射（R6 延伸）
- M7 确认 `effect_executor.rs:138-145` 仍存在 `"completed" => TaskStatus::Completed ...` 手写映射
- 主线 M2 已让 Task.status 变只读投影，但 hook effect payload 走 `update_task_status` 命令路径仍用字符串
- 决议：若 hook 规则仍保留字符串形态 effect payload，考虑统一到 `FromStr` / serde；若能改走 step transition，随 #12 一起解决

## Acceptance Criteria

- [ ] 16 项决议每项都有明确去向（保留 / 改造 / 删除）与理由
- [ ] 可被 ≤ 16 个独立 PR 依次落地
- [ ] 不重复动主线已改过的代码
- [ ] `.trellis/spec/backend/story-task-runtime.md` 补充本任务决议

## Out of Scope

- 主线任务覆盖的 M1–M8 范围——不重复处理
- 新增 runtime 实体 / 新增业务面
- **已在主线处理的原 R 项不再列入**：R1/R2/R3/R5/R6/R12/R13/R16（主线 M2/M4/M7）、R17（主线 M1 完整落地 Task 合入 Story）、R18/R19（主线 M3/M5）、R20（Model C 下消解）

## Notes

### 建议落地顺序
1. R4（纯删除，无决策）
2. R10（字段决议，配合 M1 完成后的 Task 结构）
3. R7（ACP meta 下沉）
4. R11（DTO 梳理，配合 M5 完成后的 facade 链路）
5. R14（路由盘点）
6. R15（Story.status 定位）
7. R16 扩展（enum 迁移收尾）
8. R8 / R9（锁与重试归属，最需要讨论）
