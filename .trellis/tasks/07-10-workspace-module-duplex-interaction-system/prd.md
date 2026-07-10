# Workspace Module 通用双工交互系统

## Goal

交付一套用户和 Agent 可以共同操作的通用交互系统，并把现有 Canvas 中“用户在浏览器里用 JavaScript 组合工具/Extension 调用”的能力抽成 Agent 可直接使用的服务端 `OperationScript`。

目标系统具备两条相互正交的主线：

- `OperationScript`：调用方即时提交一段受限 Rhai source，在当前 actor/scope 内组合 canonical Operations，并完成分支、筛选、聚合和结果清理。它不是持久化资产、后台 job 或 Workflow runtime。
- `InteractionInstance`：Human 与 Agent 共同读写的 canonical state/revision/command/event 事实源。Canvas 是 `InteractionDefinitionRevision(kind=canvas)` 的 authoring/presentation 形态，不再拥有独立 runtime state authority。

Canvas、Extension panel/component 和 Interaction renderer 不以 AgentRun、AgentFrame 或 RuntimeSession 为运行前提。AgentRun 通过自己的 AgentFrame adapter，用户工作坊通过 UserWorkshop adapter，进入同一个 RuntimeGateway/OperationExecutionCore。

本任务处于 planning。用户最终评审并执行 `task.py start` 后，在同一个父任务内按 `work-items/` 统一追踪生产代码、数据库、contracts、frontend、spec 和旧路径清理。

## Current Facts

- Canvas 当前保存 Project 关联的 Personal/Project scoped 多文件代码资产；浏览器把 TS/TSX 编译为 Blob module，iframe JavaScript 通过 bridge 调用 Runtime Action、MCP 和部分 Extension protocol。
- Agent 可以编辑 Canvas source，但没有 headless 脚本执行入口；现有组合执行只能由用户打开 Canvas 后触发。
- `CanvasRuntimeObservation` / `CanvasInteractionSnapshot` 是 AgentRun scoped latest diagnostics，采用 last-write-wins，不是共享 canonical state，也不会在 renderer reload 时恢复。
- Extension 当前贡献 runtime actions、protocol methods、backend services 与 whole workspace tabs；这些属于 Operation/UI provider，不是 Interaction canonical reducer。
- RuntimeGateway 当前仍以 Session/Setup actor/context 为中心，Canvas/Extension invocation 依赖 runtime session/backend routing。
- 项目尚未上线且没有需要保留的 Canvas/Interaction 生产存量；可以新增 migration 直接落最终 schema、删除旧表/列/路由/DTO，并重建 fixtures/seed，不建设 backfill、旧 decoder 或兼容路径。

## Product Decisions

- PD1：Agent-facing 能力命名为 `OperationScript`。调用方提交 inline Rhai source + input + allowed Operation manifest；生命周期限定为当前调用，持久内容由 Canvas/Workflow 等调用方 definition 自己保存。
- PD2：Application 定义 `OperationScriptEngine` port，Infrastructure 先复用现有 Rhai limits、JSON bridge 与 AST cache 设计并扩展 execution-scoped evaluator factory；未来增加其它 sandbox 只新增 engine adapter，不改变 Agent/Canvas/Workflow/Gateway 合同。
- PD3：Canvas 可以把 `.rhai` 文件或 inline source 保存为 definition/source 的一部分，也可以由 Canvas JS/component event 通过 host bridge 触发；脚本始终在服务端 sandbox 执行，不在 iframe 中执行。
- PD4：每个 script 内部 Operation 调用都重新进入共同 `OperationExecutionCore`，重新校验 operation manifest、schema、capability、readiness、limits、cancellation 与 trace；外层 script 获准不是内部调用的 blanket authorization。
- PD5：RuntimeSession 不进入目标 authority、scope、Operation surface、placement 或 Interaction identity；最终从 Canvas/Extension Gateway/provider/API contracts 中删除。
- PD6：InteractionInstance 继承 definition scope：Personal definition 产生 User-owned instance，Project definition 产生 Project-owned instance。AgentRun 只作为 attachment；tab、AgentRun 或 RuntimeSession 结束不删除 instance，instance 由 explicit close/retention 管理。
- PD7：Human 与 Agent 使用同一 typed command use case。Command 对 Agent 只有 `direct` 或 `human_only`；不建设 proposal/approval aggregate。Agent 遇到 human-only command 时只能发送非权威 attention/suggestion，Human 重新提交正式 command。
- PD8：Interaction canonical state transition 由平台 Interaction service 在服务端确定性执行。只保留有限通用 state commands 与少量平台 typed handlers；不建设通用 reducer registry、declarative reducer DSL，也不允许 Extension/Canvas 提交任意 reducer code。
- PD9：Extension 继续贡献 Component + canonical Operations。Component 接收 props/state projection、发 typed event；Interaction definition 把 event 映射到平台 command 或 OperationScript。Extension 自有复杂业务状态继续由自己的 backend service/Operation 持有。
- PD10：InteractionDefinition 使用 immutable revision + optimistic CAS。Human/Agent 保存时提交 base revision；冲突返回当前 revision/结构化 conflict。草稿留在客户端，不建设 durable draft aggregate、CRDT 或实时协同编辑。
- PD11：InteractionInstance 固定 definition revision；Extension component 在实例化时固定 exact artifact digest。Extension 安装升级只影响新 definition/new instance；既有 instance 不自动迁移。当前任务不建设通用 state migration engine。
- PD12：旧 Canvas aggregate/runtime snapshot/state 路径无数据兼容地整体替换。Canvas 的 owner/scope/source/presentation 能力进入 InteractionDefinition；runtime shared state 进入 InteractionInstance；diagnostics 进入明确只读 projection。
- PD13：父任务使用 `work-items/` 统一管理落实步骤、依赖和验收证据。
- PD14：首次稳定合同从 V1 开始显式版本化。Definition 固定 format/interaction contract version；OperationScript 固定 AgentDash `rhai_v1` dialect 与 host API version；平台 state handler 使用 `state_patch_v1` 等封闭 identity。未来 breaking change 新增 V2 reader/handler 与显式 migration，不改变既有 revision/instance 语义。
- PD15：Canvas 既有 Personal/Project distribution 行为进入 InteractionDefinition：publish 从 exact Personal revision 创建或更新独立 Project-owned definition，copy 创建独立 User-owned definition，lineage 固定 source revision；unpublish/archive 移除目录可见性，但仍被 instance/artifact/lineage 引用的 revision 保持可寻址。
- PD16：V1 通用 canonical mutation 固定为有界 `state_patch_v1`，仅支持 allowlisted JSON Pointer 上的 `add/remove/replace`，并在单事务内完成 expected revision、schema、event 与 state revision。Component event 只做 schema validation + payload pass-through，不引入服务端 mapping/reducer DSL。
- PD17：即时 OperationScript 具有同步脚本语义和异步 executor，不自动 replay。Interaction command 需要可靠外部副作用时，只在同一事务写入 replay-safe 的单 Operation `OperationEffectIntent`；复杂 durable multi-step effect 通过 Workflow 承担。
- PD18：Canvas source 使用 immutable SourceBundle + digest 随 definition revision 固定；VFS changeset 通过 base revision CAS 产生新 revision。Definition 声明 resource slots，instance/attachment runtime binding 只保存 authorized resource/artifact/provider refs，不把 runtime binding 写回 source。

## Requirements

- R1：绘制并验证 Workspace Module、Canvas、Extension、MCP、RuntimeGateway、Agent capability、AgentRun/AgentFrame、RuntimeSession、VFS 和 WorkspacePanel 的当前调用链与事实源。
- R2：建立 provider-qualified canonical `OperationRef/Descriptor`，统一 MCP tool、ExtensionProtocol method、Runtime Action 与 host operation 的 schema、effect、capability、readiness、provenance 和 dispatch identity。
- R3：把 RuntimeGateway invocation 正交拆为 principal、scope、origin、execution placement 与 trace correlation；browser 不提交 Session、Backend、workspace root 或预组装 capability。
- R4：在 RuntimeGateway 内建立 direct invocation 与 OperationScript nested invoke 共用的 `OperationExecutionCore`，统一 schema/capability admission、output validation、cancellation、trace、audit 和 result ref。
- R5：提供 `operation_script_preflight` 与 `operation_script_run`：编译/校验 `rhai_v1` source、解析 allowed Operation manifest、产出绑定完整 execution plan 的短期 token，并由 async executor 执行同步脚本语义。
- R6：OperationScript 支持普通 Rhai 分支、循环、数组/map/filter 与 JSON result shaping；Operation 调用通过 execution-scoped `ops.invoke()`/`ops.invoke_all()`，受 worker admission、timeout、Rhai sandbox limits、最大调用数、并行上限和 output size 限制。
- R7：OperationScript 不调用 LLM、不创建 AgentRun、不包含 human gate、不后台运行、不自动 retry/rollback、不承担跨会话恢复。已完成副作用不伪装成自动回滚；失败返回 bounded diagnostic、trace 与已执行 call evidence。
- R8：建立 standalone UserWorkshop runtime surface；Canvas、Extension panel/component 与 Interaction renderer 在没有 AgentRun/AgentFrame/RuntimeSession 时发现并调用授权 Operation/OperationScript。
- R9：建立 `InteractionDefinitionRevision + InteractionInstance + Attachment + RuntimeBinding + PresentationState + RendererLease` 分层，明确 owner、生命周期与事实源。
- R10：定义 Human/Agent typed command、actor policy、expected revision、command idempotency、server-owned state transition、event ordering、state rebuild、subscription、audit 和 agent projection。
- R11：Canvas 直接成为 `InteractionDefinition kind=canvas`；删除独立 Canvas canonical persistence/runtime snapshot/state contract，迁移所有 CRUD、personal/project scope、source files、VFS、presentation、Extension promotion 与 frontend consumers 到新模型。
- R12：定义 Extension component descriptor、props/events/state projection、slots/sizing、isolated iframe、CSP、MessageChannel、artifact resolution/pinning 与 structured unavailable。
- R13：Interaction attention 只向 Channel 投递 typed ref/summary；Channel 不拥有 Interaction command/event/state，Mailbox/Gate/notification 不成为第二事实源。
- R14：Workspace Module 只保留 Agent-facing discovery/lifecycle/presentation projection；不拥有第二套 Operation catalog、Extension dispatch、Canvas state 或 Interaction state。
- R15：AgentRun 通过 AgentFrame adapter 消费同一 Gateway；RuntimeSession 只可作为迁移期间 trace/delivery evidence，并在最终 cleanup 中从相关 contracts 删除。
- R16：新增 migration 直接创建最终 Interaction schema，并删除 `canvases`、`canvas_files`、`canvas_bindings`、`agent_run_canvas_runtime_observations`、`agent_run_canvas_interaction_snapshots` 及其旧 contract/route/repository 使用；不保留兼容读取或数据 backfill。
- R17：先完成 RuntimeGateway、Session、Capability、VFS、Frontend 与 cross-layer architecture/spec gate，再修改生产代码；目标规范与 Session-bound 旧规范不能并行生效。
- R18：所有落实步骤、依赖、write set、检查证据和最终残留清扫由父任务 `work-items/` 跟踪。
- R19：冻结稳定 public identity：`canvas:{definition_id}` / `canvas://{definition_id}` 表达 authoring/preview，`interaction:{instance_id}` / `interaction://{instance_id}` 表达共享 runtime；VFS mount、attachment、presentation 和 renderer lease 不复用这些对象的生命周期。
- R20：OperationScript preflight token 绑定 dialect/host API、source、input、allowed Operation descriptor/effect manifest、limits、principal/scope 和 expiry；V1 禁止递归脚本调用，result ref 继承 caller owner/scope/capability/TTL。
- R21：OperationScript evaluator 在有界专用 worker pool 中运行，使用 execution-scoped `ops` capability object 连接 async OperationExecutionCore；`ops.invoke` 隐式等待，`ops.invoke_all` 有界并行，progress/cancellation/deadline 同时中止纯脚本循环与 nested invocation。
- R22：Interaction command transaction 原子写入 command idempotency、event、state revision 和可选 `OperationEffectIntent`。只有 descriptor 声明 replay-safe/idempotent 的单 Operation 可进入该 outbox；任意 OperationScript 和多步 effect 使用即时结果或 Workflow。
- R23：完整保留并迁移 Canvas CRUD、publish/copy/unpublish、lineage、VFS source changeset、data/resource binding、Extension promotion、Workspace Module module/presentation 与 effective access 行为，不把旧字段机械复制成新事实源。

## Acceptance Criteria

- [ ] Agent 可以提交一段 inline Rhai script，组合至少两个真实 Operations，并对中间结果执行筛选/清理后返回 bounded JSON/result ref。
- [ ] 同一 Rhai source 可由 AgentRun、standalone Canvas/UserWorkshop 和 Workflow 调用同一 executor；Canvas 路径不要求 AgentRun、AgentFrame 或 RuntimeSession。
- [ ] 每个 nested Operation 调用都重新 admission；未声明/撤销 capability、schema 错误、readiness 变化和 caller cancellation 能在调用点阻断。
- [ ] OperationScript 没有独立持久化 asset/job/step records；Canvas 保存脚本时只把 source 当作 definition/source 文件。
- [ ] OperationScript executor 对调用方是 async；Rhai V1 使用 `ops.invoke` 隐式等待与 `ops.invoke_all` structured concurrency，worker exhaustion、CPU loop cancellation、nested cancellation 和 timeout 均可验证。
- [ ] preflight/run token 对 source/input/manifest/limits/principal/scope/dialect version 的任一不匹配都会失败；脚本不能递归调用 OperationScript。
- [ ] Canvas 的资产、definition revision、runtime instance、Agent attachment、presentation 和 renderer lease 不再被视为同一对象。
- [ ] Human/Agent 共享单一 canonical state；command actor policy 只有 direct/human-only，不存在 Interaction proposal lifecycle。
- [ ] Interaction state transition 由平台服务端拥有；Extension 只能贡献 Component + Operation，不能直接运行 canonical reducer。
- [ ] Definition revision 采用 CAS，无 durable draft/CRDT；instance 固定 definition revision 和 exact Extension artifact digest。
- [ ] Definition/Interaction/OperationScript/component contracts 从 V1 起有显式 discriminator；既有 instance 不受未来 V2 handler/dialect 行为变化影响。
- [ ] `state_patch_v1` 只修改 allowlisted paths，拒绝超限 patch、非法 op、schema violation 和 stale revision，并原子提交 event/state。
- [ ] replay-safe command effect 与 state transaction 原子写入 OperationEffectIntent；replay 使用稳定 effect/idempotency identity 收敛到单一成功结果，复杂多步 durable effect 进入 Workflow。
- [ ] Extension 升级不静默改变既有 instance；新版本通过新 definition/new instance 使用，不存在通用 state migration engine。
- [ ] 普通 Project Canvas preview 不再请求已删除 endpoint；旧 Canvas aggregate、runtime snapshot/state 表、DTO、route、repository 和 frontend consumer 全部清除。
- [ ] Personal publish、Project unpublish、copy-to-personal、lineage、read-only shared VFS、data/resource binding 与 Extension promotion 在新 definition model 上保持完整产品语义。
- [ ] authoring definition 与 runtime instance 使用不同 module/presentation identity；archive/unpublish 不破坏已 pin 的 revision、instance 或 artifact。
- [ ] RuntimeGateway envelope 分离 principal/scope/origin/placement/trace；客户端 authority injection 被拒绝。
- [ ] Workspace Module 只消费 canonical Operation/Interaction projection；旧 weak parser、重复 resolver/provider 和手写 DTO 静态扫描为空。
- [ ] Interaction attention 与 Channel message/delivery 边界清晰，Channel 不保存 canonical state/event body。
- [ ] 相关 `.trellis/spec/`、Rust contracts、generated TS、frontend 和 migrations 同步，所有 work items 通过最终全量 gate。
- [ ] 用户最终评审 planning artifacts 后才允许 `task.py start`。

## Out of Scope

- 持久化 OperationScript asset、后台 script job、跨调用 REPL state。
- JavaScript/TypeScript server sandbox；当前只实现 Rhai adapter，未来通过 `OperationScriptEngine` port 扩展。
- 通用 proposal/approval broker、generic reducer registry、declarative reducer DSL、Extension reducer code。
- Durable draft、CRDT、实时协同 definition editor。
- 通用/自动 Interaction/Extension state migration engine；future breaking contract 使用明确的 per-version migration。
- OperationScript 原生 Promise/`async` 语法、detached task 与自动 replay；V1 通过 async executor + 隐式等待/structured concurrency 提供异步能力。
- 把任意多步 OperationScript 作为 durable Interaction outbox effect；持久多步执行归 Workflow。
- 拖拽可视化编辑器、component marketplace、trusted same-realm component tier。
- 把 Channel、Workflow、Tool、MCP 和 OperationScript 合并为单一抽象。
