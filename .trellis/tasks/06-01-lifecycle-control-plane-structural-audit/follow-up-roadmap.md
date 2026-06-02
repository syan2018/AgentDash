# 后续推进路线图

本文记录截至 2026-06-02 `HookRuntimeAccess` provenance query slice 之后，仍需要继续推进的结构性工作。它按当前最合理的修复顺序排列，目标是让后续任务直接围绕封装边界和验收 gate 推进，而不是围绕零散 grep 命中做机械替换。

## 当前收口基线

- Terminal association、graph instance activity state ownership、dispatch taxonomy、graph key resolution、Story root launch、runtime delivery command、Routine reuse、Task cancel/control、Companion gate-first 等主线已分别通过对应 slice 验证。
- Hook runtime 已具备 `HookControlTarget`、`RuntimeAdapterProvenance`、frame query、rule-engine frame evaluation 与 runtime provenance query；runtime caller 已不能通过 refresh/evaluate query 改写 hook owner。
- Phase 4 仍保持 partial，原因是 session-shaped provider adapters、session facade getter、capability runtime adapter 与部分测试 mock 仍需被明确限定为 provenance/trace sink。
- Phase 5 仍保持 partial，原因是 Task execution preference 尚未进入 SubjectExecution contract，Permission query 的 effect owner/provenance 边界还缺测试。
- Phase 6-8 尚未开始系统落地，仍需要稳定 read models、命名契约清理和架构级验证。

## Batch 1: 关闭 Hook / capability target gate

推进原因：Hook/capability 是运行中动态控制面的核心入口。只要 public service 仍允许 raw runtime session 作为命令 owner，AgentFrame 作为 runtime surface owner 的模型就还没有真正内聚。

封装目标：

- 将 `SessionHookService::get_hook_runtime` / `ensure_hook_runtime` 的可见业务入口收束为 target-first API。
- 将 session-shaped hook getter 改为明确的 runtime delivery / trace adapter，只服务 hub、HTTP trace 或 runtime callback provenance。
- 将 provider `load_session_snapshot` / `refresh_session_snapshot` / `evaluate_hook` 保持为 adapter entry，并在命名、调用路径和测试中证明它们会映射到 provenance 或 frame target。
- 将 capability runtime adapter 中的 `resolve_runtime_session_frame_id` 继续收束到 `AgentFrameRuntimeTarget`，业务 apply path 只消费 target。
- 更新 Hook/capability spec，记录 adapter 与 control owner 的最终分工。

验收 gate：

- `rg -n "SessionHookSnapshotQuery|SessionHookRefreshQuery|HookEvaluationQuery \{|ensure_hook_runtime\(|get_hook_runtime\(|resolve_runtime_session_frame_id\(" crates/agentdash-application/src` 只命中 runtime adapter、tests 或显式 provenance/trace sink。
- Hook snapshot load / refresh / evaluate 的测试从 `frame_id + assignment_id` 或 `HookControlTarget` 执行，不需要 raw runtime session id 作为 owner。
- PhaseNode、canvas、companion、workflow refresh 的生产调用点都只传 `AgentFrameRuntimeTarget` 或 `HookControlTarget`。
- `cargo test -p agentdash-application hooks::provider --lib -- --format terse`
- `cargo test -p agentdash-application workflow::frame_hook_runtime --lib -- --format terse`
- `cargo test -p agentdash-application session::hub::tests --lib -- --format terse`
- `cargo check -p agentdash-spi -p agentdash-application`

## Batch 2: 将 StepActivation 完全纳入 frame surface transition

推进原因：`StepActivation` live apply 已经 target-first，但 activation surface 仍可在 builder 外被多个消费者解释。只要 activation DTO 继续承担 frame surface 组装责任，AgentFrameBuilder 就不是完整的 frame revision owner。

封装目标：

- 将 StepActivation 的 procedure、context、capability、VFS/MCP、runtime refs 归一化逻辑纳入 `AgentFrameBuilder` 或 `AgentFrameSurfaceService` 内部阶段。
- 让 live apply、pending next-turn、companion skill projection 共享同一份 frame transition 输入。
- 保留 runtime delivery 只表达 frame revision 投递，不拥有 activation surface truth。

验收 gate：

- `AgentFrameBuilder` 测试覆盖 activation 输入到完整 frame revision 的同源输出。
- StepActivation consumer 不再各自解释 capability/VFS/MCP/context surface。
- live / pending transition 测试证明同一 frame transition 派生 runtime context update 与 pending replay。
- `cargo test -p agentdash-application workflow::frame_builder --lib -- --format terse`
- `cargo test -p agentdash-application workflow::step_activation --lib -- --format terse`
- `cargo test -p agentdash-application workflow::agent_executor --lib -- --format terse`

## Batch 3: SubjectExecution contract 与 Task execution preference

推进原因：Task 当前已经能通过 subject/assignment/frame 取消与推进，但 execution command/result 仍没有完全进入 generated contract，Task 自身也仍携带 execution preference。Task 应表达业务规格，执行偏好应归入 SubjectExecution / dispatch policy。

封装目标：

- 定义 generated `SubjectExecutionRequest` / `SubjectExecutionDispatchResult` / Task execution result DTO。
- Task start / continue route 只把 Task 转成 `SubjectRef` 与 execution preference，具体 run/agent/frame/assignment 由 SubjectExecution boundary 返回。
- 将 Task `agent_binding` 或等价执行偏好迁出 Task business spec，落在 dispatch policy 或 `SubjectExecutionPreference`。
- 前端 start/continue 使用 generated result，不再丢弃 dispatch response 后再二次 fetch 拼装状态。

验收 gate：

- `pnpm run contracts:check` 覆盖 Task execution result。
- Task start / continue / cancel API tests 证明 command target 是 `SubjectRef` / assignment / frame。
- Frontend service/store tests 证明 start/continue 消费 generated result。
- `cargo test -p agentdash-application workflow::dispatch_service --lib -- --format terse`
- `cargo test -p agentdash-application workflow::subject_execution_control --lib -- --format terse`
- `pnpm --filter app-web run typecheck`

## Batch 4: Permission provenance 与 effect owner

推进原因：PermissionGrant 已有 run provenance 与 frame anchor，但查询边界仍容易把 source runtime session 当成 permission root。权限 effect 应由 frame/run/subject 定位，runtime session 只解释审计来源。

封装目标：

- 为 permission query 建立 frame/run/subject-first service。
- 将 source runtime session 明确作为 audit provenance filter。
- 让 permission policy engine 与 API response 暴露 effect owner，而不是 runtime session owner。

验收 gate：

- Permission query tests 证明 frame/run/subject 是主查询入口。
- 使用 source runtime session 查询时只能作为 provenance 过滤或审计解释。
- Permission spec 记录 grant lifecycle 中 effect owner 与 provenance 的分工。
- `cargo test -p agentdash-application permission --lib -- --format terse`
- `cargo check -p agentdash-api -p agentdash-application`

## Batch 5: 建立稳定 Read Models

推进原因：控制面事实源已经逐步稳定，但观察面仍存在 route-local 拼装和 project scope 泄漏风险。Read model 应只聚合事实，不成为命令写入的事实源。

封装目标：

- 新增 `ProjectActiveAgentsView` Rust contract、generated TS、API route、service、frontend service/store selector 与 tests。
- 所有 `LifecycleRunView` 由唯一 builder 组装，Story-specific route 调用同一 builder。
- `ExecutorRunRef::RuntimeSession { session_id }` 转为 `RuntimeSessionRefDto` / runtime trace ref。
- `/session/:id` 页面收束为 `RuntimeSessionTraceView`，通过 refs 回链到 agent/frame/subject。

验收 gate：

- `ProjectActiveAgentsView` 有 backend contract、generated TS、API route、frontend selector 和 test。
- 所有 route 返回 `LifecycleRunView` 时调用同一个 builder。
- RuntimeSession trace page tests 证明只消费 trace view，控制面信息通过 refs 回链。
- `pnpm run contracts:check`
- `pnpm --filter app-web run typecheck`
- frontend targeted tests 覆盖 project active list 与 trace page。

## Batch 6: 命名与入口旧语义清理

推进原因：命名是架构边界的一部分。`WorkflowContract`、shared-library legacy step payload、route-local session shape 等残留会让后续开发继续把 WorkflowGraph、AgentProcedure、RuntimeSession trace 混为一个概念。

封装目标：

- 将 `WorkflowContract` 重命名为 `AgentProcedureContract` 或最终确定的 procedure contract 名。
- Shared Library import/update 明确接收新的 graph/procedure payload。
- 清理 route-local lifecycle/task/story session shape，跨层 DTO 进入 `agentdash-contracts`。
- 清理 owner_type / session-first UI types，让 UI 类型跟随 generated contract。

验收 gate：

- `rg "WorkflowContract|entry_step_key|legacy_step_to_activity|TaskSessionPayload|SessionBindingResponse|runsBySessionId"` 只命中迁移说明或测试快照。
- Shared Library import/update tests 覆盖新 payload。
- UI 文案与类型区分 WorkflowGraph 与 AgentProcedure。
- `pnpm run contracts:check`
- `pnpm --filter app-web run typecheck`

## Batch 7: 架构级验证闭环

推进原因：前面的每个 batch 都是局部封装；最终需要证明 clean database、developer database、contracts、backend、frontend 与 E2E 的控制面链条共同成立。

封装目标：

- 建立 schema invariant assertion，检查目标列、旧列删除、索引和约束。
- 对 terminal resolver、graph instance state、dispatch taxonomy、AgentFrame transitions、ProjectActiveAgentsView、SubjectExecution panel、RuntimeSessionTraceView 建立 targeted tests。
- 补 critical E2E：ProjectAgent、Story root、Task SubjectExecution、Companion gate、Routine reuse。
- 更新最终审计文档，逐项记录 P0/P1/P2 checklist 的代码证据或测试证据。

验收 gate：

- clean database migration 与 existing developer database migration 都通过 schema invariant assertion。
- `pnpm run contracts:check`
- backend targeted tests、frontend targeted tests、critical E2E 全部通过。
- `pnpm run check`
- 最终审计文档记录每个 gate 的命令、结果和仍需 follow-up。
