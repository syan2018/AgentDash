# story/task 模块 Model C 收官收口

## Goal

主线任务 [04-27-slim-runtime-layer-session-owner](../04-27-slim-runtime-layer-session-owner/prd.md) 已完成 Model C 的主体重构方向，但代码层仍残留几类“已定方向、未完全收口”的问题：

1. `start_task` / `continue_task` 仍未真正收口到 `activate_story_step`
2. Story root session 的创建/查询/唯一性契约尚未固化
3. execution DTO / session 路由 / 过渡容器仍带有旧 task-runtime 痕迹
4. 少量 migration、前端 target kind、空壳模块、尾巴字段与死代码仍未清掉
5. `TaskLock` / `RestartTracker` / `Story.status` 等剩余建模决议尚未冻结

本任务不再按“cleanup tail 的散点收纳”推进，而是作为 **Model C 收官收口任务** 一次性完成：先补主线未真收口的前提，再统一契约、清过渡层、冻结剩余建模决议。

## Why One PRD

虽然这些问题表面上像独立尾巴，但实际存在强依赖：

- `TaskLock` 的去留依赖 task 启动是否真的已并入 `activate_story_step`
- execution DTO / 路由边界依赖 Story session 与 child session 的契约是否先收口
- `executor_session_id` 的语义依赖 session / child session / executor follow-up 的边界定义
- `Story.status` 的定位依赖 command 路径、terminal cancel 与 runtime 真相分层

若拆成多个小 PRD，会在前提未稳定时反复改文档与代码；因此本任务采用**一个 PRD 管总范围**，但内部仍按里程碑顺序落地。

## What I already know

- 主线已经完成：
  - Task 合入 Story aggregate
  - `WorkflowBindingKind` 收敛为 `Project / Story`
  - 启动期 projector 与 terminal cancel 已拆为两个方向
  - `compose_task_runtime` 已删除，`compose_story_step` / `activate_story_step` 已出现
- 当前未完全收口的事实：
  - `activate_story_step` 仍是预留 facade，`start_task_inner` / `continue_task_inner` 仍在走旧主链
  - Story session API 仍允许任意 `label`，而运行期查询硬编码只认 `"companion"`
  - `Task::set_status` / `push_artifact` / `artifacts_mut` 仍对 application 层公开
  - 前端仍暴露 `WorkflowTargetKind = "task"`
  - `0021_workflow_binding_kind_no_task.sql` 的注释与非法 JSON 处理实现不一致

## Requirements

### M0 · activation path 真收口

- `start_task` / `continue_task` 保留 facade 名字，但内部必须真正委托到 `activate_story_step`
- task 启动主链路统一为：
  - task facade
  - `activate_story_step`
  - `compose_story_step`
  - `PreparedSessionInputs`
  - `finalize_request`
  - session dispatch
- `PreparedTurnContext` 若暂时保留，只能作为薄 wrapper；不再代表 task-runtime 特例分支
- 旧式“task service 自己创建/绑定/派发 task owner session”的主路径必须退出

### M1 · Story root session 契约固化

- Story root session 的 label 值集固定，不再允许客户端透传任意字符串
- 同一 Story 的 root binding 必须唯一；不得依赖 `LIMIT 1` 选择
- `find_story_session_id` / `activate_story_step` / story session API 必须共享同一 root session 约定
- 历史数据若存在不符合约定的 Story binding，需要补迁移或兼容策略说明

### M2 · DTO / route / 字段语义收口

- 明确 `Task.executor_session_id` 的最终语义：
  - 若保留，只能表示 executor 原生会话 id，不能再与 AgentDash 内部 session id 混用
  - 若迁走，需补迁移与调用链调整
- `task/execution.rs` 与实际 activation 链路对齐，删除只服务旧路径的 DTO 壳
- 盘点并收口以下路由的职责边界：
  - `routes/task_execution.rs`
  - `routes/story_sessions.rs`
  - `routes/acp_sessions.rs`
  - `routes/project_sessions.rs`
- 前端去掉 `WorkflowTargetKind = "task"`，与后端 M4 一致

### M3 · 过渡层与尾巴清理

- 删除 `task/tools/` 空壳模块
- 清理明确死代码与明显过时注释
- 修正 `0021_workflow_binding_kind_no_task.sql` 的注释/实现不一致问题
- 评估 `task/meta.rs` 的 ACP meta 桥接归属：
  - 若成本低，直接下沉到 session 消息层
  - 若改动面过大，本次至少冻结归属决议并写进 spec

### M4 · 剩余建模决议冻结

- `TaskLock`：明确保留 / 收窄 / 删除的理由
- `RestartTracker`：明确属于 task / step / session 哪一层
- `Story.status`：明确是否保持“纯业务审计字段”定位，是否引入 suggested transition
- effect payload 中字符串状态映射的去向需要明确：
  - 若本次保留字符串协议，需统一到 `FromStr` / serde 入口
  - 若转向 workflow transition，需写清楚本次只冻结方向、不强行一步到位

## Acceptance Criteria

- [ ] `start_task` / `continue_task` 的主链路统一到 `activate_story_step`
- [ ] Story root session 的创建、查询、唯一性语义一致，API 与运行期不再各说各话
- [ ] `Task.executor_session_id` / internal session id / child session 的语义明确，代码与文档一致
- [ ] 前端不再暴露 `WorkflowTargetKind = "task"`
- [ ] 空壳模块、死代码、明显错误注释与 migration 风险已清理
- [ ] `TaskLock` / `RestartTracker` / `Story.status` 都有明确决议与理由
- [ ] `.trellis/spec/backend/story-task-runtime.md` 同步本任务结论
- [ ] 本次不新增新的 runtime 特例层；已有过渡层若保留，必须在 PRD 与代码注释中说明保留原因

## Definition of Done

- `cargo check`
- 受影响 crate 的相关单测通过
- 受影响前端类型检查通过
- spec / task 文档与实现一致
- 不引入新的双真相或新的 session 语义分叉

## Technical Approach

### 阶段顺序

1. 先收口 activation path
2. 再固化 Story root session 契约
3. 再定 `executor_session_id` 与 execution DTO
4. 再清路由与前端 target kind
5. 再删空壳与死代码
6. 最后冻结 `TaskLock` / `RestartTracker` / `Story.status` 的建模决议
7. 最后统一更新 spec

### 关键设计原则

- 不在“前提未稳定”时先处理依赖它的尾巴项
- 对外 facade 名字可以保留，但内部链路必须真实统一
- Story root session 与 child session 的边界优先于字段与 DTO 清理
- 需要讨论的项先冻结决议，再决定是否同 PR 落代码

## Decision (ADR-lite)

**Context**

原 cleanup-tail PRD 把所有尾巴项视为“彼此独立的清理项”，但代码 review 说明这不成立：M5 仍未真收口，Story root session 契约也未固定，导致 lock / retry / DTO / route 等多项收尾都缺少稳定前提。

**Decision**

本任务不再按“16 条离散 cleanup 项”推进，而是合并为一个收官 PRD，按 `activation path → session contract → DTO/route → cleanup → modeling decisions` 的顺序执行。

**Consequences**

- 优点：避免在错误抽象上重复清理，减少返工
- 代价：单任务范围更大，需要严格按里程碑推进，避免再次扩散
- 结果：本任务完成后，story/task 在 Model C 下应进入“无明显主线遗留”的稳定状态

## Out of Scope

- 再次重做主线 M1-M8 的主体重构
- 新增 runtime 实体或新业务面
- 为兼容历史错误设计保留长期双轨实现
- 强制在本次完成所有 projector transaction / workflow transition 的彻底理想化收敛
  - 这些可以定方向，但若改动面失控，可在本次只冻结原则，不强行一步到位

## Technical Notes

### 当前已识别的关键残留

- `task/service.rs` 中 `activate_story_step` 仍为预留 facade，旧主链仍在
- `story_sessions.rs` 仍允许任意 `label`
- `session_binding_repository.rs` 尚无 Story root binding 唯一性约束
- `task/entity.rs` 中投影字段写入口仍对 application 层可见
- `task/gateway/turn_context.rs` 仍承载大量过渡逻辑
- `0021_workflow_binding_kind_no_task.sql` 对非法 JSON 的注释与实现不一致

### 相关文件

- [task/service.rs](../../../crates/agentdash-application/src/task/service.rs)
- [task/gateway/turn_context.rs](../../../crates/agentdash-application/src/task/gateway/turn_context.rs)
- [session/assembler.rs](../../../crates/agentdash-application/src/session/assembler.rs)
- [routes/story_sessions.rs](../../../crates/agentdash-api/src/routes/story_sessions.rs)
- [session_binding_repository.rs](../../../crates/agentdash-infrastructure/src/persistence/postgres/session_binding_repository.rs)
- [task/entity.rs](../../../crates/agentdash-domain/src/task/entity.rs)
- [task/execution.rs](../../../crates/agentdash-application/src/task/execution.rs)
- [story-task-runtime.md](../../spec/backend/story-task-runtime.md)

### 与旧 cleanup-tail 条目的映射

- R4 → M3
- R7 → M3 / M4
- R8 → M4
- R9 → M4
- R10 → M2
- R11 → M2
- R14 → M2
- R15 → M4
- 原“前端 WorkflowTargetKind=task 清理” → M2
- 原“Projector tx-a / command path 全转 workflow transition” → 本次只冻结方向，必要时不强制一步到位
