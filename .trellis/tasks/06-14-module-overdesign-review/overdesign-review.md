# 模块过度设计重新评估报告

## 结论摘要

本轮 review 的主结论是：当前项目的核心风险不是“缺少抽象”，而是多个模块已经把同一事实重复投影、重复校验、重复承载控制面，导致后续维护成本被放大。最需要收敛的是事实源归属和命令边界，而不是继续补更多 facade 或兼容层。

优先清理方向：

- `LifecycleRun.orchestrations[] -> OrchestrationInstance -> RuntimeNodeState` 应成为 Lifecycle runtime 的唯一运行态事实源；cancel、Task projection、status aggregation 都应回到 reducer/anchor/node 坐标。
- AgentRun command/control 应收敛为一个 workspace conversation/mailbox 投影；RuntimeSession endpoint 保持 trace/detail，不再复制 AgentRun action/mailbox 控制面。
- Permission grant 应以 `PermissionGrantService + generated Permission contract` 为唯一授权事实源；companion capability grant 只能作为 broker/交互入口，不能并列成为授权结果事实。
- VFS/Local/Relay/Extension 的问题主要是装配层过厚：`RelayRuntimeToolProvider`、local `CommandHandler`、`vfs/mount.rs`、Tauri `main.rs` 都已吸收了跨域职责。

本轮只读 review 覆盖四个 subagent 输出和主会话抽样扫描。没有修改业务代码，没有运行测试。

## 优先级总览

| Priority | Area | 结论 |
| --- | --- | --- |
| P0 | Lifecycle cancel | `subject_execution_control.rs` 直接改 `RuntimeNodeState`，绕过 orchestration reducer。 |
| P0 | Task projection | boot projection 使用 run-scoped association，漏掉 agent-scoped Task association，并用缺失事实推断 Failed。 |
| P0 | Permission / Companion | `PermissionGrant` 与 companion `capability_grant_request/result` 形成两条审批事实链。 |
| P1 | Lifecycle status | domain aggregate 和 application runtime 各自实现 run status 聚合，规则不一致。 |
| P1 | Lifecycle start | `/lifecycle-runs` route 把 create Ready run 和 ready-node drain 粘在一个 API。 |
| P1 | AgentRun workspace | workspace projection、conversation snapshot、command policy、frontend command state 重复表达 action/mailbox/control。 |
| P1 | RuntimeSession control | `/sessions/{id}/runtime-control` 已漂移成第二个 AgentRun workspace 投影入口。 |
| P1 | Session runtime | `SessionRuntimeInner` 是 order-sensitive 服务定位器，AgentRun mailbox boundary 缺依赖时会静默失效。 |
| P1 | Agent runtime delegate | `AgentRuntimeDelegate` 把 hook、tool policy、compaction、turn boundary、provider observer 混成一个宽 trait。 |
| P1 | VFS tool provider | `RelayRuntimeToolProvider` 已从 VFS provider 漂移成跨域 session tool composer。 |
| P1 | Local runtime | local `CommandHandler` 是全域 command hub，集中承载 prompt/tool/VFS/MCP/extension/terminal。 |
| P1 | Extension contract | extension workspace/process/env/schema contract 过宽，schema 声明未成为执行校验。 |
| P1 | Contracts / Capability | capability catalog、permission nested DTO、tool catalog 仍有手写/JsonValue/绕过 generated contract 的事实分裂。 |

## Lifecycle / Workflow / Task 深度结论

### P0: cancel 必须走 orchestration reducer

证据：

- `crates/agentdash-application/src/workflow/subject_execution_control.rs:216` 的 `materialize_cancelled_node` 直接遍历并修改 `run.orchestrations`。
- `crates/agentdash-application/src/workflow/subject_execution_control.rs:299` 的 `mark_runtime_node_cancelled` 直接设置 `node.status = RuntimeNodeStatus::Cancelled`、`completed_at`、`error`。
- `.trellis/spec/backend/workflow/activity-lifecycle.md:26` 规定 runtime node status、executor refs、trace refs、error、state exchange 由 common orchestration runtime reducer 写入。

影响：

- cancel 不经过 `OrchestrationRuntimeEvent::NodeCancelled`，不会统一执行 ready queue 清理、orchestration status 派生、LifecycleRun status sync、terminal idempotency。
- 同一个 node terminal 事实被 complete tool / executor launcher / cancel control 三条路径写入，后续很难保证一致。

建议：

- 删除直接 mutation helper，让 cancel 解析 anchor 后调用 `apply_orchestration_event_to_run(..., OrchestrationRuntimeEvent::NodeCancelled { ... })`。
- graphless subject cancel 只发 delivery cancel command；存在 orchestration binding 时才 materialize node cancellation。

### P0: Task projection 事实链错误

证据：

- `crates/agentdash-application/src/workflow/dispatch_service.rs:580` 使用 `LifecycleSubjectAssociation::new_agent_scoped(...)` 创建 task/story association。
- `crates/agentdash-application/src/task/view_projector.rs:123` boot projector 只调用 `association_repo.list_by_anchor(run.id, None)`，只覆盖 whole-run association。
- `crates/agentdash-application/src/task/view_projector.rs:139` 对同一 run 下的 task 使用 `statuses.last()`，没有绑定 agent/frame/node 坐标。
- `crates/agentdash-application/src/task/view_projector.rs:214` 将没有 active run 覆盖的 Running task fallback 为 Failed。
- `.trellis/spec/backend/story-task-runtime.md:104` 规定 Task execution view 应从 `SubjectRef(Task)` 找 association，再沿 anchor agent / current frame / runtime anchor / node state 派生。

影响：

- 真实 task execution 通常是 agent-scoped，启动投影会漏掉。
- 漏投后 Running task 会被“孤儿 fallback”误判为 Failed，Task 状态由 absence 推断，而不是由 runtime fact 写入。
- 多 task、多 node、append orchestration 场景下，多个 task 可能吃到同一个 run 的最后 node status。

建议：

- Task projection 从 `SubjectRef(kind=Task)` 出发，使用 `list_by_subject`，再沿 `anchor_agent_id -> LifecycleAgent.current_frame -> RuntimeSessionExecutionAnchor -> orchestration_id + node_path + attempt` 定位 runtime node。
- 删除“无 active run 覆盖则 Running -> Failed”的 fallback；没有 lifecycle fact 时不写终态。
- `SubjectExecutionView` 应成为唯一 task/story execution read model，填充 `latest_runtime_node` 和 artifacts；`/tasks/{id}/execution` 复用或删除。

### P1: Lifecycle status 聚合重复

证据：

- `crates/agentdash-domain/src/workflow/entity.rs:265` 有 domain `aggregate_orchestration_status`。
- `crates/agentdash-application/src/workflow/orchestration/runtime.rs:953` 有 application `derive_orchestration_status`。
- `crates/agentdash-application/src/workflow/orchestration/runtime.rs:995` 又有 `sync_lifecycle_run_status_from_orchestrations`。

影响：

- run status 取决于最后一次写入走 aggregate method 还是 runtime reducer。
- mixed terminal / paused / cancelled 场景规则不一致，会影响 Story active run、Task projector、project active agent view。

建议：

- 保留一个 status aggregation owner。推荐 domain 暴露 pure helper，application reducer 复用同一个 helper。
- 所有 run write path，包括 start、cancel、append orchestration、terminal callback，都调用同一聚合函数。

### P1: lifecycle start API 混合 create 和 drain

证据：

- `crates/agentdash-application/src/workflow/dispatch_service.rs:306` 的 `start_lifecycle_run` 只 compile graph、创建 run 和 root orchestration。
- `crates/agentdash-api/src/routes/workflows.rs:452` 在 API route 内立即构造 `OrchestrationExecutorLauncher` 并 `drain_ready_nodes(run.id)`。
- `.trellis/spec/backend/workflow/architecture.md:306` 规定 start 只初始化 orchestration，entry node 仍为 Ready。

影响：

- 调用者无法稳定观察 Ready orchestration。
- API 的 “start” 实际变成 “create + continue/drain”，会阻碍审批、预览、人工确认、批量调度等后续能力。

建议：

- `POST /api/workflows/lifecycle-runs` 只创建 lifecycle run + Ready orchestration。
- 增加显式 continue/drain command，由调用方决定何时启动 ready nodes。

### P2: LifecycleDispatchService 和 runtime adapter 需要瘦身

证据：

- `crates/agentdash-application/src/workflow/dispatch_service.rs:101` 的 service 同时持有 run、graph、agent、frame、association、gate、lineage、anchor、runtime session creator。
- `crates/agentdash-application/src/workflow/dispatch_service.rs:330` 的 `dispatch_common` 一次完成 graph resolve/compile、run/orchestration update、agent/session/frame/anchor/association/gate/lineage 和 `NodeStarted`。
- `crates/agentdash-application/src/workflow/projection.rs:73` 的 `activity_definition_from_plan_node` 把 frozen runtime `PlanNode` 重新投成 `ActivityDefinition`。
- `crates/agentdash-application/src/workflow/projection.rs:93` 对 LocalEffect / ExtensionAction / None 伪造 `BashExec { command: "true" }`。

建议：

- 保留薄 `LifecycleDispatchService` facade，把内部切成 `RunOrchestrationStarter`、`AgentRuntimeAllocator`、`SubjectAssociationWriter`、`InteractionGateWriter`。
- runtime activation/frame compose 直接消费 `PlanNode` 或窄 `LifecycleNodeActivationSpec`，删除 fake executor adapter。

## AgentRun / Session / Runtime 深度结论

### P1: AgentRun workspace control projection 重复

证据：

- `crates/agentdash-application/src/workflow/agent_run_workspace/projection.rs` 派生 `state_code`、`delivery_status`、`control_plane`、`actions`、`runtime_command_state`。
- `crates/agentdash-application/src/workflow/conversation_snapshot.rs:336` 再次派生 conversation execution。
- `crates/agentdash-application/src/workflow/conversation_snapshot.rs:388` 再次派生 submit/cancel/promote/delete/resume commands。
- `crates/agentdash-application/src/workflow/agent_run_workspace/command_policy.rs:41` 后端 command policy 又读取 execution state 并重新 derive workspace projection。
- `crates/agentdash-contracts/src/workflow.rs:1221` 的 `AgentRunWorkspaceView` 同时输出 top-level `actions`、`mailbox`、`mailbox_messages` 和 `conversation`。
- `packages/app-web/src/features/session/ui/SessionChatViewTypes.ts:93` 前端 props 同时接收 `mailboxMessages`、`mailboxSnapshot`、`mailboxState`。

影响：

- 一个用户动作是否可执行，至少由 top-level actions、conversation commands、command policy、mailbox row 四处表达。
- 新增 command 要同时改 projection、conversation snapshot、policy、contract、frontend command hook，漂移风险高。

建议：

- 以 `AgentConversationSnapshot` 作为唯一 chat command/mailbox control surface。
- top-level `AgentRunWorkspaceView` 保留 identity、delivery refs、frame/resource surface、display shell 等事实，移除或降级 command/action 投影。
- command policy 复用同一个 resolver 的 stale guard / availability 计算，只做 durable precondition 校验。

### P1: RuntimeSession runtime-control 是第二个 AgentRun 投影入口

证据：

- `crates/agentdash-api/src/routes/sessions.rs:149` 的 `get_session_runtime_control` 从 runtime session id 反查 anchor，再加载 run、agent、frame、association。
- `crates/agentdash-api/src/routes/sessions.rs:246` 独立派生 control plane/action set。
- `crates/agentdash-api/src/routes/sessions.rs:295` 独立读取 mailbox 并构造 state/messages。
- `crates/agentdash-contracts/src/workflow.rs:1528` 的 `SessionRuntimeControlView` 包含 run、agent、frame_runtime、subject_associations、actions、mailbox、mailbox_messages。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:621` 已有 AgentRun workspace endpoint 输出同类 control 信息。

影响：

- RuntimeSession trace identity 和 AgentRun workspace identity 形成两条 command/control 入口。
- RuntimeSession detail 被拉入 AgentRun mailbox/action 领域，违背“RuntimeSession 是 delivery/trace substrate”的边界。

建议：

- `/sessions/{id}/runtime-control` 收窄为 trace/detail/read-only：session shell、trace meta、anchor backlink、workspace ref、frame ref。
- command/control 统一跳转或请求 AgentRun workspace endpoint。

### P1: SessionRuntimeInner 与 AgentRuntimeDelegate 过宽

证据：

- `crates/agentdash-application/src/session/hub/mod.rs:47` 的 `SessionRuntimeInner` 同时持有 connector、hook provider、runtime registry、turn supervisor、store set、VFS、skill discovery、terminal callback、runtime tool provider、MCP discovery、backend placement、AgentFrame repo、anchor repo、LifecycleAgent repo、mailbox deps、gate repo。
- `crates/agentdash-application/src/session/hub/factory.rs:202` 的 `with_agent_run_mailbox_boundary` 在关键 repo 未先注入时会直接返回 self，缺依赖不报错。
- `crates/agentdash-agent-types/src/runtime/delegate.rs:25` 的 `AgentRuntimeDelegate` 同时定义 compaction、context transform、tool call、turn boundary、provider request hooks。
- `crates/agentdash-application/src/session/mailbox_delegate.rs:173` 的 mailbox delegate 只需要 turn boundary，却必须实现并转发全部方法。
- `crates/agentdash-application/src/session/hook_delegate.rs` 约 1800 行，成为 hook runtime、audit injection、token stats、provider observer、trace 的万用拦截器。

影响：

- SessionHub 变成 order-sensitive 服务定位器，bootstrap 顺序错误时 mailbox boundary 会静默失效。
- mailbox scheduling、hook runtime、tool policy、provider observer 被揉进同一个 agent loop extension point，后续新增 hook 方法会扩大影响面。

建议：

- 用显式 `AppSessionRuntimeDeps` 或 `AgentRunRuntimeBindings` 一次性注入并校验必需依赖，缺依赖直接失败。
- 拆分 `AgentRuntimeDelegate` 为 `ContextTransformDelegate`、`ToolCallPolicyDelegate`、`CompactionDelegate`、`TurnBoundaryDelegate`、`ProviderRequestObserver`。
- `LaunchPlan` 持有结构化 delegate set，mailbox 只实现 turn boundary。

### P2: AgentRunMailboxService 和 direct steer 路径

证据：

- `crates/agentdash-application/src/workflow/agent_run_mailbox.rs:106` 的 service 注入 lifecycle run/agent、AgentFrame、anchor、command receipt、mailbox repo、session core/control/eventing/launch。
- `crates/agentdash-application/src/workflow/agent_run_mailbox.rs:751` 的 `schedule` 决定 barrier/drain/claim。
- `crates/agentdash-application/src/workflow/agent_run_mailbox.rs:951` 和 `:1073` 分别处理 delegate steering 和 route/scheduler steering。
- `crates/agentdash-application/src/workflow/agent_steering.rs:30` 保留 direct steer service，逻辑与 mailbox steer 重叠，产品路径未见使用。

建议：

- mailbox 作为 durable envelope/scheduler 事实源保留，但内部拆 `MailboxIntake`、`MailboxScheduler`、`MailboxDeliveryExecutor`、`MailboxReceiptProjector`。
- 删除或私有化 `AgentRunSteeringService`，测试迁移到 mailbox command/scheduler/current frame resolution。

## VFS / Local / Relay / Extension 结论

### P1: RelayRuntimeToolProvider 是跨域 composer

证据：

- `crates/agentdash-application/src/vfs/tools/provider.rs:55` 的 `RelayRuntimeToolProvider` 持有 VFS、RepositorySet、SessionToolServices、InlineContentPersister、PlatformConfig、FunctionRunner、ShellOutputRegistry、VfsMaterializationService、RuntimeGateway、ExtensionRuntimeChannelTransport。
- `crates/agentdash-application/src/vfs/tools/provider.rs:183` 先组装 VFS tools。
- `crates/agentdash-application/src/vfs/tools/provider.rs:200` 继续组装 workflow lifecycle tool。
- `crates/agentdash-application/src/vfs/tools/provider.rs:217` 继续组装 companion collaboration tools。
- `crates/agentdash-application/src/vfs/tools/provider.rs:247` 继续组装 workspace module tools。
- `crates/agentdash-application/src/vfs/tools/factory.rs:46` 的 `VfsToolFactory` 本身职责清晰，只处理 VFS read/write/execute。

建议：

- 保留 `VfsToolFactory` 作为 VFS cluster 工厂。
- 新增 `SessionRuntimeToolComposer` / `CompositeRuntimeToolProvider` 汇总 cluster providers。
- 拆出 `WorkflowToolProvider`、`CollaborationToolProvider`、`WorkspaceModuleToolProvider`，让 RuntimeGateway 和 extension channel 只进入 workspace-module provider。

### P1: local CommandHandler 是全域 command hub

证据：

- `crates/agentdash-local/src/handlers/mod.rs:40` 的 `CommandHandler` 持有 backend identity、workspace roots、ToolExecutor、session runtime、connector、MCP manager、workspace config、event channel、terminal manager、materialization store、session forwarders、extension host、artifact API/token/cache root。
- `crates/agentdash-local/src/handlers/mod.rs:114` 的 `handle()` 集中 match prompt/cancel/discover/workspace/file/shell/search/materialize/MCP/extension/terminal。
- `crates/agentdash-local/src/handlers/tool_calls.rs:26` 和 `crates/agentdash-local/src/handlers/extension.rs:158` 都从共享 handler 抽取各自 domain state。

建议：

- 保留 `RelayMessage` 顶层 wire envelope。
- local 执行侧拆 `LocalCommandRouter + domain handlers`：Prompt、Workspace、Tool、Mcp、Extension、Terminal、Materialization。
- `LocalCommandContext` 只承载 backend id、event tx、runtime shutdown 等真正共享事实。

### P1: Extension Host contract 过宽，schema 未执行

证据：

- `crates/agentdash-local/src/extensions/host/host_api.rs:86` / `:93` 接受 `workspace_root` 参数覆盖默认 session workspace root。
- `crates/agentdash-local/src/tool_executor.rs:98` 在 `workspace_roots` 未配置时接受任意存在目录。
- `crates/agentdash-local/src/extensions/host/process_api.rs:19` / `:59` 使用单一 `process.execute` 同时保护 raw shell 和 argv exec。
- `crates/agentdash-local/src/extensions/host/process_api.rs:139` 用 `env.read` 权限保护 process env overlay。
- `packages/extension-sdk/src/index.ts:30` 要求 action 声明 `input_schema` / `output_schema`。
- `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:169` 将 `request.input` 直接放入 transport request。
- `crates/agentdash-local/src/extensions/host/manager.rs:114` 将 raw `Value` 直接传给 JS runner。

建议：

- Extension Host API 不接受 raw `workspace_root`，workspace 只能来自 activation 的 session workspace context；多 root 使用 typed `workspace_handle`。
- 权限拆成 `process.exec`、`process.shell`、`process.cwd:<scope>`、`process.command:<name-or-pattern>`，并区分 `env.get` 与 `process.env.set:<key>`。
- RuntimeGateway 校验 action/channel input JSON schema，local runner 返回后校验 output schema。

### P2: VFS mount、Tauri shell、前端 mount selection 需要收窄

证据：

- `crates/agentdash-application/src/vfs/mount.rs:58` 的 `build_derived_vfs()` 处理 workspace/project/story/context/agent/lifecycle/routine/skill/canvas 多类 mount。
- `crates/agentdash-application/src/vfs/mount.rs:428` / `:450` 使用 `serde_json::Value` metadata 写入和解析 inline owner 坐标。
- `crates/agentdash-application/src/vfs/surface_query.rs:32` 和 `crates/agentdash-application/src/vfs/mutation_dispatcher.rs:110` 也依赖 mount metadata 还原 purpose / inline storage key。
- `crates/agentdash-local-tauri/src/main.rs:140` / `:321` / `:408` 在 Tauri main 内实现 profile load/save、claim DTO、runtime start/claim。
- `packages/app-web/src/features/vfs/vfs-browser-panel-policy.ts:8` 和 `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:223` 各自实现 mount/backend selection。

建议：

- `vfs/mount.rs` 按 provider/owner 拆 builder，并引入 typed `RuntimeMountMetadata`，只在 DTO/SPI 边界序列化为 JSON。
- profile/claim 下沉到 `agentdash-local`，Tauri 保持 thin command adapter。
- surface summary 增加 usage hints，或至少把前端 mount selection 收敛为共享 selector。

## Frontend / Contracts / Permission 结论

### P0: PermissionGrant 与 companion grant 双事实源

证据：

- `crates/agentdash-application/src/permission/service.rs:71` 的 `PermissionGrantService::request` 创建 grant、执行 policy、持久化，并自动批准时应用 frame effect。
- `crates/agentdash-application/src/permission/service.rs:141` 的 `approve` 走 domain 状态机并应用 capability frame effect。
- `packages/app-web/src/features/permission/PermissionGrantCard.tsx:31` 已有正式 grant 审批 UI。
- `crates/agentdash-application/src/companion/payload_types.rs:87` 注册 `capability_grant_request -> capability_grant_result`。
- `packages/app-web/src/features/session/model/companionRequestViewModel.ts:42` 和 `packages/app-web/src/features/session/ui/SessionCompanionRequestCard.tsx:52` 在 session feed 内渲染 companion capability grant 审批。
- `crates/agentdash-application/src/companion/tools.rs:916` / `:1373` 明确 platform capability grant broker 未闭环。

影响：

- 用户可能在 session 内看到“批准”交互，但这条路径不一定产生 `PermissionGrant` 聚合、capability delta 或 tool schema delta。
- 审计、撤销、TTL、scope escalation 与 runtime capability update 会分裂。

建议：

- `PermissionGrantService` / `PermissionGrant` 是唯一授权事实源。
- `companion_request target=platform capability_grant_request` 只能作为 broker input，broker 必须创建或返回 `PermissionGrantResponse` / grant id。
- broker 完成前，session 内 companion grant 审批按钮不应产生授权结果事实。

### P1: Permission contract 和 catalog 仍有 typed gap

证据：

- `crates/agentdash-contracts/src/permission.rs:28` 的 list query 暴露 `status?: PermissionGrantStatusDto`。
- `crates/agentdash-api/src/routes/permission_grants.rs:102` 实现却先调用 active-only repository，再按 status 二次过滤。
- `crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs:116` SQL 只返回 `applied | scope_escalated`。
- `crates/agentdash-domain/src/permission/value_objects.rs:98` 已有 typed `ScopeEscalationIntent` 和 `PolicyDecision`。
- `crates/agentdash-contracts/src/permission.rs:55` 却将 `scope_escalation_intent`、`policy_decision` 定义为 `Option<Value>`。
- `packages/app-web/src/features/permission/PermissionGrantCard.tsx:119` 再从 `JsonValue` 猜字段。

建议：

- repository/API 支持 `effect_frame_id | run_id + optional status` 查询 pending/active/terminal grants。
- permission contract 增加 `ScopeEscalationIntentDto`、`PolicyDecisionDto`、`PolicyOutcomeDto`，API 显式映射。

### P1: capability catalog / tool catalog 绕过 generated contract

证据：

- `packages/app-web/src/features/workflow/ui/panels/shared.ts:99` 手写 `CAP_EDITOR_WELL_KNOWN_KEYS`、label、description。
- `packages/app-web/src/features/workflow/ui/panels/shared.ts:141` 手写 `AUTO_GRANTED_BASELINE`，注释说明镜像后端 visibility rule。
- `crates/agentdash-spi/src/platform/tool_capability.rs:73` / `:714` 是后端 well-known keys、tool descriptors、visibility rules 权威实现。
- `crates/agentdash-api/src/routes/workflows.rs:1077` 直接返回 `Vec<agentdash_spi::ToolDescriptor>`。
- `packages/app-web/src/types/workflow.ts:197` 前端手写 `ToolDescriptor` / `ToolSource` union。

建议：

- capability catalog projection 纳入 `agentdash-contracts`，生成前端 DTO，包含 key、label、description、allowed scopes、auto_granted、agent/workflow grantability、tools。
- `/tool-catalog` 不直接暴露 SPI 类型，由 API 显式映射为 contract DTO。

### P2: 前端大组件消费过宽 DTO

证据：

- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx` 页面承载 route、owner story/project、workspace binding、runtime projection、hook refresh、module tabs、chat command、navigation、runtime data。
- `packages/app-web/src/features/session/ui/SessionChatViewTypes.ts:32` 的 props 覆盖 sessionId、workspaceId、executor state、commandState、mailbox messages/snapshot/state、promote/delete/resume/recall/move、input injection、header/input slots、promptTemplates。
- `packages/app-web/src/features/session/ui/SessionSystemEventCard.tsx:22` 直接接收完整 `BackboneEvent`，组件内解析 platform type/data/message 和大量 detail fields。
- `packages/app-web/src/features/executor-selector/model/types.ts:35` 手写 discovery response / discovered-options stream DTO。

建议：

- 后端先收敛 AgentRun command/mailbox projection；前端再减少 `SessionChatView` 多源 mailbox/action props。
- `SessionSystemEventCard` 改收窄 view model，platform/companion 解析集中到 `features/session/model`。
- executor discovery response 和 stream envelope 纳入 generated contracts，或明确降级为 connector-private internal API。

## 明确不建议作为本轮清理目标的边界

- `OrchestrationRuntimeEvent` reducer 和 `OrchestrationExecutorLauncher` 的方向是正确的；问题是周边 command/projection 绕过它。
- graphless lifecycle run 不是问题；它是 Agent runtime 的正常拓扑。
- `RuntimeSessionExecutionAnchor` 是正确的 trace-to-control-plane backlink；问题是 Task projection/cancel/runtime-control 没有一致使用它。
- AgentRun mailbox 作为 durable message intake/scheduler/recovery 的事实源不是问题；要拆的是服务内部职责。
- `RuntimeGateway::invoke` / provider boundary 暂不构成本轮主要问题；dynamic surface manifest 可以另立小任务。
- `VfsToolFactory` 边界清晰；问题在外层 provider 吸收了 workflow/companion/workspace module。
- Relay 顶层 `RelayMessage` enum 作为 wire envelope 合理；问题在 local 执行侧 command handler 过厚。
- `BackboneEvent` 作为 session feed 主流式事件契约合理；问题是 UI 大组件直接承担 platform/companion payload 解析。

## 建议拆任务顺序

1. 修正 Lifecycle P0：cancel 走 reducer；Task projection 从 SubjectRef/anchor/node 坐标派生，删除 absence -> Failed fallback。
2. 统一 Lifecycle status aggregation，并补 mixed terminal / paused / cancelled 的 focused tests。
3. 收敛 Task/Subject execution read model，填充 latest runtime node/artifacts，移除或复用 `/tasks/{id}/execution` 轻量 DTO。
4. 拆分 lifecycle start 和 ready drain：create route 返回 Ready，新增显式 continue/drain command。
5. 收敛 AgentRun command projection：以 `AgentConversationSnapshot` 作为唯一 chat command/mailbox surface，command policy 复用同一 availability 计算。
6. 瘦身 RuntimeSession runtime-control：保留 trace/meta/anchor/workspace backlink，移除独立 mailbox/action 投影。
7. 硬化 SessionRuntimeBuilder 并拆 AgentRuntimeDelegate：消除 mailbox boundary 静默 no-op，让 turn boundary 与 hook/tool/compaction/provider observer 分离。
8. Permission grant 单事实源：实现或删除 companion capability grant broker 半成品，grant list 支持 pending/active/terminal，typed nested DTO 进入 generated contract。
9. Capability catalog contract 化：后端投影 catalog/visibility/tool descriptors，前端 editor 不再镜像 baseline。
10. 拆 runtime tool composer 和 local command router：先分离 VFS/workflow/collaboration/workspace-module providers，再拆 local domain handlers。
11. 收窄 Extension contract：workspace handle、process/env permission、input/output schema validation 一次性闭环。
12. 拆 VFS mount metadata 和 Tauri profile/claim：typed metadata 下沉，desktop shell 保持 thin adapter。
13. 前端收尾：AgentRunWorkspacePage 建立 feature facade，SessionChatView props 瘦身，SystemEvent/Companion cards 改窄 view model。

## 本轮产物

- `research/01-lifecycle-workflow-task.md`
- `research/02-agentrun-session-runtime.md`
- `research/03-vfs-local-relay-extension.md`
- `research/04-frontend-contracts-permission.md`

这些 research 文件包含更细的代码路径和 caveats；本报告只保留综合排序和清理队列。
