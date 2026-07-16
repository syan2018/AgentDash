# 清理 Runtime Surface 来源模型并收敛 Permission 架构

## Goal

修复 Runtime Surface adoption 把 Workflow transition phase 错当成全局前置条件的问题，并清理当前横跨 Grant、AgentFrame、Business Surface、VFS、Hook、Project Agent 配置、API 与前端的 Permission 过度实现。

本任务完成后的当前产品状态是：

- 所有现有项目的权限判定默认通过；
- 执行边界只保留一个由 AgentRun 对外提供的权限验证入口；
- RuntimeInteraction 继续承载确定需要的动态审批等待与回答；
- 本任务不实现长期 Grant；
- 未来 Grant 是 `LifecycleRun` 运行实例内的长期结构化状态，随同一 `lifecycle_runs` 聚合仓储 CRUD；
- Grant 的读取、验证、审批与管理能力只由 AgentRun 层暴露，其他模块只面向 AgentRun 契约，不直接读取 `LifecycleRunRepository` 或 Grant 字段。

## Confirmed Facts

### Runtime Surface 回归

- `crates/agentdash-agent-runtime/src/context_projection/artifact.rs:20-35` 在 normalized surface delta 非空时强制读取 `transition_phase_node`，缺失即返回 `MissingTransitionPhase`。
- 错误文本位于 `crates/agentdash-agent-runtime/src/context_projection/artifact.rs:111-114`：`changed surface adoption requires workflow transition phase provenance`。
- `transition_phase_node` 只从 `crates/agentdash-application-agentrun/src/agent_run/context_sources.rs:371` 的 Workflow provenance `node_path` 投影。
- Canvas、VFS、MCP、Skill、Workspace Module 等合法 live surface update 不一定来自 Workflow node，因此天然可能没有该字段。
- 该全局校验由 `9e9643769 feat(runtime): 恢复 ContextFrame canonical 生产链` 引入，后续提交扩展了 projection，但没有修正错误前提。
- Canvas 更新当前在 `crates/agentdash-application-agentrun/src/agent_run/runtime_surface_update.rs:182-196` 先写入新 AgentFrame，再调用 active adoption；确定性 presentation 校验失败时会留下“Frame 已写、Runtime 未采用”的半成功状态。

### 原始架构任务的正确方向

归档任务 `.trellis/tasks/archive/2026-07/07-10-agent-runtime-architecture-convergence/` 已经确定：

- AgentRun 是用户可见的产品运行容器与授权边界；
- RuntimeInteraction 是 Runtime 向 Host 发起并等待回答的 approval、user input、elicitation；
- Tool Broker 每次调用重新验证 binding generation、capability、permission 与 VFS；
- Driver/Adapter 不拥有 AgentRun 授权事实；
- canonical Runtime contract 应使用 AgentDash-owned vocabulary，不直接泄漏 vendor DTO；
- 结构与 seam 只应覆盖当前正确性不变量，长期治理等复杂度应在真实需求出现后再设计。

本任务保留这些边界，但修正原任务未收敛好的 Grant 持久化与暴露方式。

### 当前 Permission 实现偏差

当前代码同时存在：

- 独立 `PermissionGrant` aggregate、repository、service、policy engine、scope escalation、TTL 与状态机；
- 独立 `permission_grants` 表、索引、清理 SQL、API contracts 与前端卡片；
- PermissionGrant 向 Capability、VFS、Business Surface、AgentFrame 与 Runtime Surface revision 的投影；
- Project Agent `permission_policy`、Shared Library override、Relay/Executor 传播、Hook snapshot 与 supervised approval preset；
- Tool Broker permission decision 与 capability admission 的重复判定；
- Codex command/file/permission approval RuntimeInteraction；
- canonical Runtime contract 直接嵌入 Codex vendor approval DTO；
- `temporary_permission_approval` 等以空权限、进程 cwd、零时间拼装请求的临时路径；
- 后端 mutation route 已删除，但前端仍调用 approve/reject/revoke 路径的断裂入口。

这些机制没有形成一个清晰 owner，反而让 Permission 变化能够触发 AgentFrame/Surface adoption，从而与本次 Canvas provenance 回归互相放大。

### LifecycleRun 当前持久化形态

- `LifecycleRun` 已经是运行实例聚合，位于 `crates/agentdash-domain/src/workflow/entity.rs:158`。
- `orchestrations`、`tasks`、`execution_log`、`channel_registry` 都是该聚合的结构化字段。
- `PostgresWorkflowRepository` 将这些字段持久化到同一 `lifecycle_runs` 行的 JSONB 列。
- `LifecycleRunRepository` 已经承担运行实例的创建、查询、更新与删除。
- `channel_registry` 已有专门 mutation 方法，证明高并发结构化字段可以在同一聚合仓储内提供窄化原子更新，而无需拆成独立产品聚合。

因此未来 Grant 的正确持久化归属是 `LifecycleRun`，不是静态 Lifecycle definition，也不是独立表。

## Product Decisions

- Grant 是长期授权事实，不是一次审批请求。
- 动态审批是确定的长期需求，不能因当前默认允许而删除 RuntimeInteraction approval 能力。
- pending approval 属于 RuntimeInteraction；用户批准后才形成或更新长期 Grant。两者不共享一套状态机。
- 未来 Grant 作为 `LifecycleRun` 的 typed structured document 持久化在同一 `lifecycle_runs` 行；不建立独立 Grant 表或独立 Grant repository。
- Grant 只通过 AgentRun application facade 暴露。Tool Broker、Managed Runtime、Driver、Adapter、Surface compiler、API route 与前端不得直接读写 Grant 或 `LifecycleRunRepository`。
- 当前所有现有项目权限默认通过。本任务只实现最小验证入口与 allow-all production behavior，不实现 LifecycleRun Grant 字段、Grant CRUD、Grant policy、Grant UI 或 Grant migration。
- Permission 是执行时授权判定，不是 Agent 可见 Business Surface contribution。Grant 变化不创建 AgentFrame revision，也不触发 Surface adoption。
- 项目未上线，清理采用直接切换，不保留兼容字段、双轨 API 或 fallback。
- 规划与后续执行采用 Codex 内联流程，不派发 subagent。

## Requirements

- R1：`workflow transition phase` 只作为可选 presentation metadata，不能参与 Surface adoption 的 identity、CAS、digest、幂等或合法性前置校验。
- R2：Canvas、VFS、MCP、Skill、Workspace Module 等非 Workflow surface delta 在没有 phase provenance 时可以生成稳定 ContextFrame 并完成 adoption。
- R3：能前移的确定性 compile/presentation preflight 必须发生在持久化前；失败不得留下可被产品查询误认为 active 的 revision。是否需要独立 adopted pointer 由失败注入测试决定。
- R4：Workflow transition 仍可携带 node path，用于 ContextFrame 展示与审计，但缺失不构成 Runtime 错误。
- R5：删除当前独立 PermissionGrant domain/application/infrastructure/API/contracts/frontend 实现及 `permission_grants` 表。
- R6：删除 PermissionGrant 对 Capability、VFS、Business Surface、AgentFrame、ContextFrame 和 Runtime Surface update 的 contribution。
- R7：删除 Project Agent `permission_policy` 及其 Shared Library、Hook、Relay、Executor、contract 和 frontend 传播链。
- R8：建立唯一的 AgentRun 权限验证门面。输入使用 AgentDash-owned、protocol-neutral 执行坐标；输出只需要 `Allowed`、`Denied`、`PendingApproval`。
- R9：当前 production implementation 固定返回 `Allowed`，不读取任何 repository、AgentFrame、Surface、Hook、Project Agent config 或 Grant 状态。
- R10：Tool Broker 与 Driver/Adapter 只调用 AgentRun 权限门面，不直接拥有 Grant policy，不直接查询 `LifecycleRunRepository`。
- R11：保留 RuntimeInteraction approval 能力。只有 AgentRun 权限门面返回 `PendingApproval` 时才创建并等待 approval interaction；当前 allow-all 路径不产生 pending interaction。
- R12：RuntimeInteraction request/response 使用 AgentDash-owned canonical DTO，删除 Codex vendor DTO 泄漏和临时伪造 request。
- R13：未来 Grant 作为 `LifecycleRun` typed document，通过同一聚合仓储持久化；建议使用 `lifecycle_runs` 同行的专用 JSONB 字段，不塞入 unrelated metadata，也不建立独立表。
- R14：未来 Grant 的读取、创建、撤销、审批协调和 read model 全部由 AgentRun 层提供；外部 API 只能是 AgentRun-scoped API。
- R15：未来动态批准采用“先持久化 Grant，再 resolve RuntimeInteraction，失败可幂等重试”的 AgentRun 协调顺序；恢复执行前再次经过 AgentRun 权限验证。
- R16：明确良好框架与过度设计的边界，每个保留或新增的抽象必须能指出当前 producer、consumer、owner、正确性不变量和失败测试。
- R17：本任务不实现未来 Grant 字段或 CRUD，只在设计中固定 ownership、dependency direction、interaction boundary 与 persistence shape。
- R18：规划、实施与检查均在主会话内联完成，不派发 subagent。

## Acceptance Criteria

- [ ] Canvas 创建、展示、复制、数据绑定等 live surface update 在没有 Workflow node path 时完成 canonical adoption。
- [ ] 真实 AgentRun 中 Agent 可以创建 Canvas、写入或绘制内容、更新数据并展示；该流程不会因 Surface adoption 中断，并可继续当前执行与后续对话。
- [ ] Workflow transition 的 node path 仍进入 presentation metadata；无 node path 使用通用 Runtime Surface Update 表达。
- [ ] `MissingTransitionPhase` 与对应错误文本从生产代码删除。
- [ ] presentation/Business Surface 的确定性错误不会让未采用 AgentFrame 成为 current active surface。
- [ ] 生产代码中不存在独立 `PermissionGrant` aggregate、repository、service、API、contracts、frontend card 或 `permission_grants` 表依赖。
- [ ] 生产代码中不存在 PermissionGrant surface update、capability artifact source、VFS access source 或 Business Surface merge。
- [ ] 生产代码中不存在 `permission_policy`、`supervised_tool_approval` 或 permission-specific preset 传播链。
- [ ] Tool Broker、Managed Runtime 与 Integration 不直接依赖 `LifecycleRunRepository` 或未来 Grant model。
- [ ] 唯一 AgentRun 权限门面使用 protocol-neutral request，并支持 `Allowed / Denied / PendingApproval`。
- [ ] 当前 production 权限门面固定允许，Tool Broker 与 vendor approval 不产生 pending interaction。
- [ ] RuntimeInteraction 的 approval family 被保留，并改为 AgentDash-owned request/response，不引用 Codex generated approval DTO。
- [ ] 数据库通过新增 migration 删除 `permission_grants` 表及相关索引/约束，不修改历史 migration。
- [ ] 最终 spec 明确：未来 Grant 属于 `LifecycleRun` 同行 typed document，只由 AgentRun 层暴露；本任务不添加空字段、CRUD、API 或 UI。
- [ ] 依赖方向检查能够阻止 Runtime、Tool Broker、Integration、Surface 或 API 绕过 AgentRun 直接读取 Grant。
- [ ] `prd.md`、`design.md`、`implement.md` 完成一致性检查并经用户评审后才进入实现。

## Out of Scope

- 实现长期 Grant 的具体字段、subject/resource/action schema、继承、fork 语义、TTL、审计保留或 UI。
- 向 `lifecycle_runs` 添加未来 Grant JSONB 字段。
- 实现真实 `PendingApproval` policy 或用户审批产品流。
- 恢复旧 PermissionGrant mutation routes。
- 改变 capability、VFS、credential、binding generation 等独立执行约束的业务语义。
- 为被删除 Permission 数据提供兼容读取或迁移保留。
