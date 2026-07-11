# 当前 Workspace / Channel 分支在 PR #93 上的适配地图

## 结论

推荐把 PR #93 head `efdfa5dc585530b1c8285e9b2a399ba92830c45e` 作为新的实现基线，按领域主题重放
当前分支的 Workspace / Channel 变更；不推荐把两个最终 tree 直接 merge 后逐个修文本冲突。

两条分支的目标并不互斥：

- PR #93 负责 Agent conversation runtime：`AgentRunRuntime` facade、Managed Runtime
  Thread/Turn/Item/Interaction、Driver Host、RuntimeWire、Business Agent Surface 与 ToolBroker。
- 当前分支负责平台可调用能力和共享交互：canonical `Operation`、ephemeral `OperationScript`、
  `InteractionInstance`、Canvas/Component、WorkspaceModule 与 Channel V2。

真正需要重写的是两者的接缝，而不是当前分支的领域核心。最终结构应保留两个正交 facade：

```text
AgentRun conversation command
  -> AgentRunRuntime
  -> AgentRuntimeGateway
  -> Managed Runtime / Driver Host

User / Workflow / Agent platform capability
  -> trusted host adapter
  -> OperationGateway
  -> OperationExecutionCore
  -> exact provider

Agent model tool call
  -> Managed Runtime Item
  -> PlatformToolBroker
  -> Operation/OperationScript local executor adapter
  -> OperationExecutionCore
```

`AgentRuntimeGateway` 不替代 `OperationGateway`，`PlatformToolBroker` 也不成为所有非 Agent caller 的
统一执行入口。前者拥有 conversation execution truth，后两者分别拥有 Agent tool side-effect 边界与
actor-neutral provider admission。

## 固定证据基线

| 项目 | 证据 |
| --- | --- |
| 目标 PR | [#93 feat(agent-runtime): 完成可插拔 Agent Runtime 架构收敛](https://github.com/syan2018/AgentDash/pull/93) |
| PR 状态 | OPEN、ready、mergeable，base `main`，head `codex/agent-runtime-architecture-convergence` |
| PR base / head | `957fa9d60ea3d67efa1bb278fe5b376cf0c34598` / `efdfa5dc585530b1c8285e9b2a399ba92830c45e` |
| 当前分支 / 存档点 | `codex/workspace-duplex-interaction-planning` / `7070f6b0` |
| 双方 merge-base | `957fa9d60ea3d67efa1bb278fe5b376cf0c34598`，与 PR base 相同 |
| 双向提交 | 当前分支独有 73 个；PR 独有 11 个 |
| 双向文件 | 当前分支修改 356 个；PR 修改 626 个；路径交集 95 个 |

PR 的决定性提交是：

- `b43d2be5` Runtime Contract / Wire；
- `63dbd623` Managed Runtime kernel；
- `ef4bdec6` PostgreSQL state / recovery；
- `b47164bc` Business Surface / ToolBroker / Driver Host；
- `e934c287` Native/Codex/Relay drivers；
- `af21f9d7` AgentRun facade 生产切换、`0065` cutover 与旧 RuntimeSession 删除。

当前分支的领域依据来自：

- `.trellis/tasks/archive/2026-07/07-10-workspace-module-duplex-interaction-system/design.md`；
- `.trellis/tasks/archive/2026-07/07-10-workspace-module-duplex-interaction-system/work-items/decisions.md`；
- `.trellis/tasks/archive/2026-07/07-10-channel-domain-boundary-refactor-evaluation/design.md`；
- `.trellis/tasks/archive/2026-07/07-10-channel-domain-boundary-refactor-evaluation/work-items/decisions.md`；
- 当前 `.trellis/spec/backend/runtime-gateway.md`、`.trellis/spec/backend/interaction/architecture.md`、
  `.trellis/spec/backend/channel.md` 与 `.trellis/spec/frontend/component-guidelines.md`；
- PR head 的 `.trellis/spec/backend/agent-runtime-agentrun-facade.md`、
  `.trellis/spec/backend/agent-runtime-kernel.md`、
  `.trellis/spec/backend/agent-runtime-surface-tool-broker.md` 与
  `.trellis/spec/backend/session/architecture.md`。

## 逐域适配结论

| 当前分支主题 | 结论 | 在 PR runtime facade 上的目标形态 |
| --- | --- | --- |
| `OperationRef`、descriptor、catalog、execution core | 保留 | 保持 actor-neutral；与 `RuntimeOperationId` 明确区分，不折叠进 `RuntimeCommand` |
| MCP / ExtensionProtocol / Setup / Interaction providers | 保留领域逻辑，重接 composition | provider 继续进入 Operation core；Agent caller 由 Business Surface / ToolBroker adapter 接入 |
| 旧 `session_actions`、`extension_actions`、`tool_adapter` | 删除 | 当前分支对这些旧 Session-bound adapter 的删除优先于 PR 对旧文件的修改 |
| Rhai `OperationScript` engine、port、preflight token、call evidence | 保留 | UserWorkshop / Workflow 直接使用；Agent 以一个 ToolBroker 顶层工具进入 |
| OperationScript nested invoke | 保留 core re-entry，重写 Agent trace adapter | 每个 nested call 重进 Operation core；不伪造 ToolBroker Item，不递归进入 Broker |
| Interaction definition / revision / instance / command / event / effect | 保留 | 独立于 Managed Runtime 的同名 `RuntimeInteraction`；命名和 contract 必须区分 |
| Canvas 最终替换与 Component iframe ABI | 保留 | PR 改过的旧 Canvas route/diagnostic 仍删除；恢复新 Interaction routes/contracts/frontend |
| Interaction Agent attachment | 重写 identity | 从仅 `run_id` 收紧为 `run_id + agent_id` 的 AgentRunAgent subject；不绑定 RuntimeThread |
| WorkspaceModule descriptor / presentation | 保留 | UserWorkshop HTTP projection 保持直接 Operation surface；Agent projection 迁到 AgentFrame surface compiler |
| `WorkspaceModuleRuntimeToolProvider` | 重写 | 不再从 `ExecutionContext.agent_run_execution` 或 RuntimeSession 取坐标，改用 provision target + current AgentFrame |
| Channel V2 domain / owner registry / binding provider index | 保留 | delivery target 保持 `run_id + agent_id`，进入 PR mailbox/product delivery facade |
| Channel mailbox materializer | 重写薄 adapter | 使用 PR 的 `NewAgentRunMailboxMessage` 新字段和 `AgentRunProductDeliveryPort`；不写 runtime session/turn/receipt 字段 |
| migrations `0061` / `0062` | 重编号 | PR `0061..0065` 原样保留；Channel 为 `0066`，Interaction/Canvas 为 `0067` |

## Canonical Operation 与 ToolBroker

### 保留的核心

以下当前实现表达的是平台能力事实，不与 PR 的 conversation runtime 重复，应按原语义保留：

- `crates/agentdash-domain/src/operation.rs` 的 provider-qualified exact identity、effect、replay policy、
  principal/scope/origin；
- `operation_types.rs`、`operation_core.rs`、`operation_gateway.rs`、`operation_provider.rs`；
- ExtensionProtocol、MCP、Setup、Interaction 的 provider adapters；
- input/output schema、readiness、actor visibility、capability、placement、deadline、result ref、audit；
- direct invoke、OperationScript nested invoke、Interaction replay-safe effect 共用同一个 core。

PR 中 `RuntimeOperationId` / `OperationReceipt` 表示 Managed Runtime 接受的一条 conversation command；当前
`OperationRef` 表示一个可调用平台能力。二者只能通过 adapter 关联，不能共享 identity 或 persistence table。

### Agent ToolBroker 接缝

PR 的 `PlatformToolBroker` 要求真实的 `ToolCallCoordinates`：Runtime Thread、Turn、Item、Host binding、
generation 和 tool-set revision。它还拥有 before/after hook、permission、credential、VFS、approval、
idempotency 与 terminal convergence。因此 Agent 发起的顶层 tool call 必须先过 Broker。

推荐新增一个 Agent Surface contribution / local executor adapter，而不是恢复 Session-bound tool adapter：

1. `AgentFrameNativeSurfaceCompiler` 已持有 `AgentRunRuntimeProvisionRequest.target`、current `AgentFrame`、
   Runtime Thread 与 Host binding；以这些 server-owned facts 构建 actor-specific Operation catalog。
2. 将 WorkspaceModule/OperationScript 等 Agent-facing entry 编译为 PR 的 `ToolContribution`，并在
   binding-scoped local executor registry 中保存对应 Operation host binding。
3. Driver tool call 先形成 canonical Runtime Item，再进入 ToolBroker；Broker executor 才调用
   `OperationGateway`。
4. executor 每次调用仍重新解析 authority、descriptor、attachment、placement 和 readiness；编译时的
   ToolCatalog 只负责 immutable delivery surface，不升级为最终权限事实源。

当前 `WorkspaceModuleRuntimeToolProvider` 依赖 `ExecutionContext.agent_run_execution`。PR 已移除这条
RuntimeSession launch 链，因此不应把字段加回 connector context。应改为显式 compile context，例如：

```text
AgentRunOperationCompileContext {
  target: AgentRunRuntimeTarget,       // run_id + agent_id
  frame_id + frame_revision,
  project_id,
  runtime_thread_id,
  host_binding_id + generation,
  authenticated/delegated identity,
  tool_set_revision
}
```

其中 Runtime Thread / binding / generation 只服务 ToolBroker delivery fencing 与 trace；Operation authority
revision 应由 LifecycleRun、AgentFrame revision、effective capability、installation/attachment revision 等
业务事实计算。

### OperationScript nested call

Agent 调用 OperationScript 时只在最外层进入一次 ToolBroker：

```text
Runtime ToolCall Item(operation_script)
  -> PlatformToolBroker
  -> Rhai OperationScript executor
  -> op.invoke(exact ref)
  -> OperationExecutionCore
  -> provider
```

nested call 不应再次进入现有 ToolBroker，理由是：

- Broker 要求 Driver 已创建的 canonical ToolCall Item，nested call 没有合法 Item 坐标；
- 伪造 child Item 会破坏 Runtime journal、binding generation 与 idempotency；
- 递归 Broker 会重复 before/after hook、approval 和 terminal convergence；
- OperationScript 已保存 bounded partial-call evidence、child trace 与 outcome-unknown。

但 nested call 必须继续执行当前实现的逐次 re-admission：exact ref、current surface、schema、capability、
effect/replay、readiness、placement、deadline/cancellation 全部在实际调用点重新校验。若未来产品需要把
nested call 暴露为独立 Runtime Item，应先为 Managed Runtime 设计正式的 host-origin child item command，
不能复用或伪造 Driver `ToolCallCoordinates`。

UserWorkshop 和 Workflow caller 没有 Agent Runtime Thread 时仍直接调用同一个 Operation core；这是目标
能力，而不是 fallback。

## RuntimeSession 删除后的可信上下文

| Operation envelope 字段 | 新事实源 | 明确不再使用 |
| --- | --- | --- |
| principal | HTTP `CurrentUser`；server-owned `AgentRunRuntimeTarget(run_id, agent_id)`；Workflow node coordinate；Extension installation service identity | browser/iframe 自报 principal、RuntimeSession owner |
| scope | Project/Workspace/Interaction repositories；Agent 路径由 LifecycleRun.project_id + active attachment 得出 | RuntimeSession project/owner 字段 |
| origin | 创建 host adapter 的入口：UserWorkshop、Canvas、ComponentEvent、AgentTool、Workflow、EffectReplay | client 自报任意 origin |
| authority revision | current AgentFrame revision/capability digest + Project permission + installation/attachment facts | Runtime Thread revision、Host generation 充当业务权限 |
| placement | exact provider policy + Workspace binding；Agent 本机资源从 current AgentFrame VFS/backend anchor；Driver placement由 RuntimeOffer/Host binding管理 | RuntimeSession backend id、browser backend id |
| trace | User/Workflow root trace；Agent tool 使用 Runtime Thread/Turn/Item/binding generation 坐标派生 parent trace | RuntimeSession event seq / execution anchor |
| attachment | Interaction repository 按 instance + `run_id + agent_id` 实时解析 active attachment | RuntimeSession id、仅 run_id 的模糊 attachment |

PR 的 `agent_run_runtime_binding` 是 `run_id + agent_id -> runtime_thread_id + runtime_binding_id` 的产品
锚点；它可用于从 trace 回到产品坐标，但不能反向成为 Operation scope 或 Interaction owner。

## Interaction / Canvas / Component 与 AgentFrame

Interaction、Canvas 和 Component 代码大多是当前分支独有文件，可以机械重放。需要人工重写的是
Agent-facing attachment 与 surface composition：

1. 将 `AttachmentSubject::AgentRun { run_id }` 收紧为表达 `run_id + agent_id` 的 subject。数据库中的
   active-subject 唯一键也必须覆盖 agent identity，避免同一 LifecycleRun 的多个 Agent 共享权限。
2. attachment 生命周期仍跟 InteractionInstance/explicit detach；Runtime Thread、Turn、Host binding、
   AgentFrame revision 更新都不自动删除 attachment。
3. current AgentFrame 只投影“当前 Agent 可见的 Interaction/WorkspaceModule/Operation”；每次 command 或
   invoke 的最终 admission 重新读取 active attachment。
4. `AgentFrameNativeSurfaceCompiler` 从 current frame + attachment repository 生成 immutable
   Tool/Workspace contributions，并把 `frame_id` 写入 binding-scoped executor materialization 作为来源证据。
5. frame/surface 更新走 PR 的 surface revision / tool-set replacement/rebind 边界；不恢复
   `RuntimeSessionExecutionAnchor` 或 `visible_canvas_mount_ids_json`。

PR 修改过而当前分支删除的旧 `routes/canvases.rs`、Canvas diagnostics、旧 WorkspaceModule
`runtime_bridge/runtime_context/runtime_tool_provider/surface/tools/visibility` 文件应继续删除；恢复的是当前
分支的新 Interaction routes、contracts、WorkspaceModule descriptor 和 Component host。

需要避免的同名误合并：PR 的 `RuntimeInteraction` 是 Agent conversation 中的 approval/input 等等待项；
当前 `InteractionInstance` 是 Human/Agent 长生命周期共享 state。两者可以通过 ref 关联，但不能合表、
共享 command 或生命周期。

## Channel V2

Channel V2 的 owner-local registry、`ChannelKey`、participant admission、message origin/reply 拆分、
binding provider registry 与 reverse index 都应保留。它们不依赖 RuntimeSession。

唯一需要用 PR 实现重写的是 mailbox delivery seam：

```text
Channel admitted delivery(run_id, agent_id)
  -> NewAgentRunMailboxMessage (PR #93 shape)
  -> AgentRunProductDeliveryPort / runtime mailbox
  -> AgentRunRuntime facade
  -> accepted RuntimeOperationId
```

Channel 不读取或保存 Runtime Thread id、Host binding、driver generation、active turn 或 accepted operation。
PR mailbox 负责在消费时 provision/resolve binding 并把 accepted operation id 写回 mailbox fact。当前分支
materializer 里的 `delivery_runtime_session_id`、queued/consuming turn、protocol turn 和旧 command receipt
字段不得恢复。

重放 `crates/agentdash-application/src/channel.rs` 时，以当前分支的 V2 domain/service/admitted delivery 为
主体，但手工套用 PR 已收紧的 `NewAgentRunMailboxMessage` 字段和 product delivery port。Companion delivery
继续先经过 `ChannelService` admission，再进入同一 mailbox facade。

## Migration 顺序

PR 的 migration 顺序必须先完成 Runtime cutover：

| 序号 | 内容 | 处理 |
| --- | --- | --- |
| `0061..0064` | Managed Runtime、Hook、ToolBroker、Driver Host | 原样保留 PR 版本 |
| `0065_agent_runtime_cutover.sql` | `agent_run_runtime_binding`、surface snapshot、删除 RuntimeSession tables/columns | 原样保留并先执行 |
| `0066_reset_channel_registry_v2.sql` | 当前 `0061_reset_channel_registry_v2.sql` | 仅重编号；继续 reset owner registry 到 schema v2 |
| `0067_interaction_canvas_replacement.sql` | 当前 `0062_interaction_canvas_replacement.sql` | 重编号后执行；创建最终 Interaction tables，删除旧 Canvas tables/columns |

`0067` 实施时同时把 Agent attachment subject 固定为 `run_id + agent_id` 的 exact identity。由于项目未上线，
不添加 RuntimeSession/旧 Canvas backfill、双读、兼容 decoder 或 fallback。PR `0065` 删除 runtime session 后，
`0067` 也不得新建任何指向旧 session tables 的 FK。

必须同步：migration 文件名、`include_str!` 断言、fresh-root migration 测试、SQLx migration metadata、
repository fixture 与 schema 残留检查。当前 Interaction repository 中硬编码的 `0062` 测试应改到 `0067`。

## 关键冲突分类

### 删除优先于 PR 修改

- PR 修改、当前删除：旧 Canvas route/diagnostic、旧 `extension_actions`、旧 WorkspaceModule runtime bridge
  系列。应保留删除，不能为了消冲突把旧 adapter 复活。
- PR 删除、当前修改：`agentdash-application-runtime-session` launch/tool/turn 文件、旧 AgentRun mailbox
  scheduler/delivery、runtime surface/session bootstrap。应以 PR 删除为基线，把当前语义迁到新 facade。

### 可机械重放

- Interaction/Canvas/Component 的新 domain/application/repository/contracts/frontend 文件；
- canonical Operation core/provider 新文件；
- Rhai OperationScript engine/port；
- Channel V2 domain value objects/provider index；
- WorkspaceModule descriptor/presentation contracts。

### 必须手工重写

- API `app_state`、bootstrap、repositories、runtime gateway composition；
- AgentFrame surface compiler、tool registry/executor、runtime tool provider；
- AgentRun authority/trace/placement resolver；
- Interaction Agent attachment 与 WorkspaceModule presentation adapter（PR 删除了旧 `bootstrap/session.rs`）；
- Channel mailbox materialization/Companion delivery；
- migrations 编号、contract generation 与 specs。

## 推荐主题提交顺序

1. `chore(integration): 切换 Agent Runtime PR 基线`
   - 从 PR head 建立整合分支，记录 `7070f6b0` 为可回滚存档点；只解决基线和 task/spec 文件。
2. `feat(operation): 恢复 canonical Operation 核心与 providers`
   - 重放 domain/core/provider；删除 Session-bound action adapters；先证明 UserWorkshop direct invoke。
3. `feat(operation-script): 适配 ToolBroker 与 Rhai 调用边界`
   - 恢复 engine/ports/UserWorkshop/Workflow callers；新增 Agent top-level Broker adapter；nested call 直回 core。
4. `feat(interaction): 恢复 Interaction、Canvas 与 Component`
   - 重放最终 domain/application/repository/contracts/frontend；一次性删除旧 Canvas authority。
5. `refactor(workspace-module): 接入 AgentFrame Business Surface compiler`
   - 用 provision target/current frame/attachment 构建 contributions；移除 `ExecutionContext.agent_run_execution`
     与 RuntimeSession 依赖；恢复 present/command 金线。
6. `refactor(channel): 在 AgentRun Runtime facade 上恢复 Channel V2`
   - 重放 V2 domain/provider/index；手工适配 PR mailbox/product delivery shape。
7. `feat(database): 顺延 Channel 与 Interaction 最终迁移`
   - 固定 `0066/0067`、AgentRunAgent attachment identity、fresh-root schema 与 repository tests。
8. `refactor(api): 收束 routes、contracts 与前端投影`
   - 恢复新 routes/generated TS/WorkspacePanel；确保 DTO 不暴露 authority/placement/runtime thread。
9. `test(integration): 固定 Runtime facade 与双工交互金线`
   - 补 Agent ToolBroker、OperationScript partial evidence、attachment、Channel mailbox 和 Canvas promotion E2E；
     最后同步 specs。

每个主题提交都应保持可编译或至少限定在一个可验证 crate 集合；不要把 73 个旧提交逐个原样 cherry-pick
作为最终历史，因为其中多个中间 adapter 已被 PR 删除。

## 验证矩阵

| 层次 | 必须证明 | 建议门禁 |
| --- | --- | --- |
| Operation core | exact provider identity、schema、actor visibility、authority revision、placement、result ref、timeout | `cargo test -p agentdash-application-runtime-gateway` |
| Rhai engine | preflight zero-side-effect、manifest/token binding、limits、cancel、partial/outcome-unknown | `cargo test -p agentdash-infrastructure operation_script` |
| ToolBroker adapter | 顶层 Item/Hook/permission/VFS/credential/idempotency；nested call 不伪造 Item且重新 admission | `cargo test -p agentdash-agent-runtime` + adapter crate tests |
| AgentFrame compiler | target/frame/project 一致、attachment exact、surface/tool-set digest、stale generation fenced | `cargo test -p agentdash-application-agentrun` |
| Interaction | command CAS/idempotency/event/effect 原子性；`run_id + agent_id` attachment；instance 不随 runtime terminal | domain/application/repository tests |
| WorkspaceModule | list/describe/invoke/present；exact OperationRef；`interaction://` presentation | `cargo test -p agentdash-workspace-module` |
| Channel V2 | owner-local key、participant admission、provider index rebuild、Companion delivery 经 service | `cargo test -p agentdash-application channel` |
| Runtime facade | mailbox -> Runtime operation、duplicate receipt、binding Lost/generation fencing | PR 原有 AgentRun facade / runtime mailbox / enterprise remote tests |
| Migration | fresh root 依次到 `0067`；旧 RuntimeSession/Canvas tables 和旧 frame column 均不存在 | PostgreSQL migration tests + migration guard |
| Contracts/UI | browser DTO 无 principal/authority/backend/runtime thread；generated TS 无漂移 | `pnpm contracts:check`、前端 typecheck/targeted tests |
| 全量 | crate graph、fmt、clippy、workspace tests | `cargo check --workspace --all-targets`、`cargo clippy --workspace --all-targets -- -D warnings`、`git diff --check` |

重点端到端场景：

1. Agent ToolCall 经 ToolBroker 调用 WorkspaceModule exact Operation，trace 可从 Runtime Item 回到
   `run_id + agent_id + frame_id`，但 Operation authority 不读取 Runtime Thread。
2. Agent 调用 OperationScript，nested provider 在 preflight 后被撤权时拒绝；顶层 Runtime Item 保留
   partial/outcome-unknown，不出现伪造 child Item。
3. 两个 Agent 属于同一 LifecycleRun 时，只有持有对应 active attachment 的 Agent 可写 Interaction。
4. Channel inbound 经 provider reverse index、Channel admission、mailbox、AgentRunRuntime 接受为一个
   RuntimeOperation；重复 provider event 不产生第二 operation。
5. fresh PostgreSQL root 执行 `0061..0067` 后，Managed Runtime 与 Interaction/Channel 同时可用，
   `runtime_sessions`、旧 Canvas tables、`visible_canvas_mount_ids_json` 全部不存在。

## 回滚点与禁止的整合捷径

- 回滚点 A：PR head `efdfa5dc`；用于判断问题是否来自 PR 本身。
- 回滚点 B：当前远端存档 `7070f6b0`；用于查阅完整 Workspace/Channel 实现和逐主题 diff。
- 每个上述主题提交是独立回滚点；migration 提交在代码/contract 都适配完成后落地。

不接受的捷径：恢复 RuntimeSession adapter、为旧 Canvas/Session 做双路径、把 Runtime Thread 当
Interaction owner、让 Channel 直接调用 Driver、让 browser 提交 placement/authority、用模糊 tool name
替代 exact OperationRef、或让 OperationScript nested call 伪造 ToolBroker 坐标。
