# Implementation Plan：可执行批次与提交计划

## 启动结论

当前计划已经达到 Trellis 复杂任务的 planning review gate：`prd.md`、`design.md`、`implement.md` 已存在，`task.py validate` 通过。

但不建议把整个 session 系统重构作为一个单体 task 直接 `task.py start` 后一次性实现。正确推进方式是：

1. 当前 task 作为 parent planning task，保存目标架构、迁移地图和批次计划。
2. 每个 batch 创建一个 child task，单独 `task.py start`，单独实现、验证、提交。
3. 每个 batch 必须减少一个旧分叉、旧可变壳或旧隐式行为；只新增 wrapper 不算完成。
4. 每个 batch 通过后再进入下一批，后续批次可根据前一批实现反馈回到 planning 修正。

可立即正式推进的范围：Batch 0 与 Batch 1。

暂不应直接推进的范围：Batch 4 之后的 runtime/effects/pending/final hub 删除。它们依赖 Batch 1-3 产出的 construction/launch 边界，否则容易变成并行拆壳。

## Trellis 工作流

### 当前 parent task

- 路径：`.trellis/tasks/05-14-session-launch-refactor-assessment`
- 状态：`planning`
- 作用：架构规划、迁移地图、批次计划、最终收口清单。
- 当前不直接承载 Rust 生产代码实现。

### 每个实现 batch 的流程

```text
Phase 1 Plan
  -> create child task under parent
  -> write child prd/design/implement from this batch
  -> user review gate
  -> task.py start <child>

Phase 2 Execute
  -> load trellis-before-dev
  -> implement directly in main session
  -> run focused tests
  -> run broader checks required by touched packages

Phase 3 Finish
  -> trellis-check
  -> trellis-update-spec
  -> staged commit plan
  -> git commit per batch/commit group
  -> finish/archive child task
```

Inline mode 下不派发 subagent；实现和检查都由 main session 执行。

### 建议命令节奏

```powershell
python ./.trellis/scripts/task.py create "Session refactor batch 0 characterization" --slug session-refactor-batch-0-characterization --parent .trellis/tasks/05-14-session-launch-refactor-assessment
python ./.trellis/scripts/task.py start .trellis/tasks/<created-child-dir>
```

每个 child task 开始前必须重新确认 scope，不能把下一批内容顺手带入。

## Batch 0：Characterization 与安全护栏

目标：先把当前行为固定成可回归事实，避免后续迁移靠猜。

范围：

- 固化入口矩阵：HTTP、Task、Workflow、Routine、Companion dispatch、Companion parent resume、Hook auto-resume、Local relay。
- 固化 fallback 矩阵：owner、VFS、MCP、capability、executor、working_dir、follow-up、hook reload、repository restore。
- 固化失败路径：connector.prompt failure、owner bootstrap commit、pending command、terminal effects。
- 增加当前行为测试或 characterization fixtures，不改变生产架构。

明确不做：

- 不引入新 `LaunchService`。
- 不迁移入口。
- 不删除 `PromptSessionRequest`。

推荐提交：

1. `test(session): 固化现有启动链路行为`
   - 覆盖 prompt pipeline fallback、owner bootstrap、connector failure、pending transition 消费。
2. `test(api): 固化 session context 与 owner 选择现状`
   - 覆盖 launch augment 与 context query owner priority 差异。

验证：

- `cargo test -p agentdash-application session::hub`
- `cargo test -p agentdash-api acp_sessions`
- 若目标测试名不同，以实际新增测试模块为准。

退出条件：

- 当前关键背离点都有测试或 fixture 保护。
- 后续 batch 改行为时能明确看到测试 diff 是预期迁移，而不是误伤。

## Batch 1：Owner Resolver 与 SessionConstructionPlan

目标：先形成 query/launch 同源的 construction fact，收掉 route 层 owner/context/VFS/capability 重建。

范围：

- 新增 `session/ownership`，定义 `ResolvedSessionOwner` 与单一 resolver。
- 新增 `session/construction`，定义 `SessionConstructionPlan` 与 trace。
- 将 `SessionAssemblyBuilder` / `PreparedSessionInputs` 中可复用 composer 逻辑迁入 construction builder。
- context endpoint、project/story/task context response 改为投影 construction。
- 删除 `finalize_augmented_request` 或让它只剩迁移期调用点，并绑定删除条件。

明确不做：

- 不删除 `PromptSessionRequest` 主链路。
- 不大改 `start_prompt_with_follow_up`。
- 不做 terminal outbox。

推荐提交：

1. `feat(session): 引入统一 owner 解析`
   - 新增 owner resolver 与单元测试。
   - launch/query 共用同一 owner priority。
2. `feat(session): 引入 SessionConstructionPlan`
   - 新增 construction plan、trace、projection。
   - 迁移 assembler 中可复用构建逻辑。
3. `refactor(api): 让 session context 查询投影构建计划`
   - 改 `/sessions/{id}/context`、project/story/task context response。
   - 删除 route-local 重建主线。

验证：

- `cargo test -p agentdash-application session::ownership`
- `cargo test -p agentdash-application session::construction`
- `cargo test -p agentdash-api acp_sessions`
- `cargo test -p agentdash-api project_sessions story_sessions`

退出条件：

- context endpoint 与 launch construction 使用同一事实源。
- owner priority 不再分裂。
- route 层不再拥有 owner/task/story/project context 组装主逻辑。

## Batch 2：LaunchExecution 与 prompt pipeline 拆分

目标：把执行前的 launch 策略解析从 `start_prompt_with_follow_up` 中抽出。

范围：

- 新增 `LaunchCommand` 与 `LaunchExecution`。
- 从旧 pipeline 前置解析 payload、lifecycle、restore、hook、follow-up、pending command apply plan、terminal effect plan、connector input。
- `ExecutionContext` 改为 connector boundary projection。
- `start_prompt_with_follow_up` 收缩为执行函数，只做 reservation、event write、connector prompt、supervision。

明确不做：

- 不迁移所有入口到 `LaunchCommand`。
- 不删除 `PromptSessionRequest`。
- 不拆 runtime registry。

推荐提交：

1. `feat(session): 引入 LaunchCommand 与 LaunchExecution`
   - 新增类型、trace、summary。
2. `refactor(session): 将 prompt pipeline 改为执行 launch 计划`
   - pipeline 读取 `LaunchExecution`，减少临时 fallback。
3. `test(session): 覆盖 launch execution 解析矩阵`
   - 覆盖 lifecycle、restore、hook、follow-up、pending apply plan。

验证：

- `cargo test -p agentdash-application session::launch`
- `cargo test -p agentdash-application session::hub`
- `cargo test -p agentdash-application session::prompt_pipeline`

退出条件：

- connector.prompt 前可打印完整 launch summary。
- 执行函数不再读取 request/meta/profile 做策略 fallback。
- connector.prompt 失败不会提前 commit owner bootstrap。

## Batch 3：入口 Adapter 与 PromptSessionRequest 主链路删除

目标：所有来源只构造 `LaunchCommand`，删除生产主链路半成品 request。

范围：

- HTTP prompt 改为 `LaunchCommand::http_prompt`。
- Task / Workflow / Routine / Companion / Hook / Local relay 迁移为 source adapter。
- `SessionLaunchIntent` 吸收到 `LaunchCommand` / source policy 或删除。
- `PromptSessionRequest` 从生产主链路删除，只允许测试或已标记删除的迁移边界短暂存在。

明确不做：

- 不拆 terminal effects。
- 不拆 pending runtime command storage。

推荐提交：

1. `refactor(api): 将 HTTP prompt 迁移到 LaunchCommand`
2. `refactor(session): 将系统启动入口迁移到 LaunchCommand`
   - Task、Workflow、Routine、Companion、Hook。
3. `refactor(local): 将本机 relay prompt 迁移到 LaunchCommand`
4. `refactor(session): 删除 PromptSessionRequest 生产主链路`

验证：

- `cargo test -p agentdash-api acp_sessions`
- `cargo test -p agentdash-application task workflow routine companion`
- `cargo test -p agentdash-local`
- `rg "PromptSessionRequest" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src`

退出条件：

- 生产入口不再构造 `PromptSessionRequest`。
- 外围模块不再直接调用 `launch_*prompt` 业务 wrapper。
- `SessionLaunchIntent` 不再作为主链路分发层存在。

## Batch 4：Runtime Registry 与 TurnSupervisor

目标：把运行态从 `SessionHub.sessions` 中拆出，建立可监督 turn lifecycle。

范围：

- 新增 `SessionRuntimeRegistry`。
- 新增 `TurnSupervisor`。
- 区分 `has_live_executor_session` 与 `has_active_turn`。
- cancel/stall/delete 改为通过 runtime/supervisor。

推荐提交：

1. `feat(session): 引入运行态 registry`
2. `feat(session): 引入 turn supervisor`
3. `refactor(session): 迁移 cancel 与 stall 到运行态服务`

验证：

- `cargo test -p agentdash-application session::runtime`
- `cargo test -p agentdash-application session::hub`
- concurrent prompt / cancel / interrupted / stall 相关测试。

退出条件：

- 并发 prompt 只有一个 reservation 成功。
- 后台 task 被 supervisor 持有并可取消。
- `SessionHub` 不再暴露 runtime 内部状态入口。

## Batch 5：Terminal Event + Durable Outbox

目标：终态事实与业务副作用解耦。

范围：

- terminal event 先持久化。
- 新增 terminal effect outbox store 与 dispatcher。
- task post-turn、workflow lifecycle、hook auto-resume、companion parent resume 改为 typed effect。
- processor 不再直接执行业务副作用。

推荐提交：

1. `feat(session): 增加 terminal effect outbox`
2. `refactor(session): 将终态副作用迁移到 outbox`
3. `test(session): 覆盖 terminal effect 重试与幂等`

验证：

- `cargo test -p agentdash-application session::effects`
- `cargo test -p agentdash-application workflow task companion`
- PostgreSQL + SQLite migration tests。

退出条件：

- effect 失败不影响 terminal fact。
- 进程重启后未完成 effect 可重放。
- processor 不再持有 terminal callback / post-turn handler / auto-resume 派发职责。

## Batch 6：Pending Runtime Command 事件化

目标：删除 `SessionMeta.pending_capability_state_transitions` hidden queue。

范围：

- 新增 requested / applied / failed event。
- 新增可重建 projection。
- `LaunchExecution` 只生成 apply plan。
- PostgreSQL / SQLite migration 迁出旧 meta 字段。

推荐提交：

1. `feat(session): 增加 runtime command 事件模型`
2. `refactor(session): 迁移 pending capability transition`
3. `db(session): 迁移 pending runtime command 存储`

验证：

- `cargo test -p agentdash-application session::pending`
- migration tests for PostgreSQL / SQLite。
- apply-once、失败恢复、connector failure。

退出条件：

- `SessionMeta.pending_capability_state_transitions` 删除。
- pending command 可审计、可恢复、可并发安全 apply once。

## Batch 7：SessionHub / Persistence / AppState / working_dir 最终收口

目标：删除剩余业务 facade 与初始化隐式行为。

范围：

- `SessionHub` 职责拆完，删除或只保留无业务逻辑迁移壳并立即清除。
- `SessionPersistence` 拆清 meta/event/projection/outbox/runtime-command projection。
- AppState 引入 ready builder。
- `working_dir` 类型化，拒绝越界输入。

推荐提交：

1. `refactor(session): 删除 SessionHub 业务 facade`
2. `refactor(session): 拆分 session persistence store`
3. `refactor(api): 引入 ready app state builder`
4. `fix(session): 收紧 working_dir 路径策略`

验证：

- `cargo test -p agentdash-application session`
- `cargo test -p agentdash-api`
- `cargo test -p agentdash-local`
- `pnpm dev` 手动烟测：HTTP prompt、local relay、workflow/routine/companion 基本链路。

退出条件：

- `SessionHub` 不再是业务能力入口。
- AppState ready 后必要依赖不可能为空。
- working_dir 安全测试覆盖绝对路径、`..`、Windows separator、空 segment。

## 每批提交规则

- 每个 batch 可以有 1-4 个 commit，按行为边界分组，不按文件分组。
- 提交格式遵循项目要求：`type(scope): 中文提交信息`。
- 每个 commit message body 分点说明具体更新。
- 不把未识别的用户改动混入提交。
- 每批提交前先给出 staged file plan，经确认后再 commit。
- 每批必须在 `trellis-check` 后提交。

## 当前是否可以正式开始

可以正式开始，但开始对象应是 Batch 0 child task，而不是把整个 parent planning task 当成一个巨型实现任务。

正式开始前还差一个明确动作：用户确认进入 Batch 0。确认后执行：

```powershell
python ./.trellis/scripts/task.py create "Session refactor batch 0 characterization" --slug session-refactor-batch-0-characterization --parent .trellis/tasks/05-14-session-launch-refactor-assessment
python ./.trellis/scripts/task.py start .trellis/tasks/<created-child-dir>
```

Batch 0 完成并提交后，再创建 Batch 1 child task。后续每批同理。
