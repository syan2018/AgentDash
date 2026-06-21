# Research: capability-permission-extension-vfs-topology

- Query: 盘查 Capability / Permission / Extension runtime / VFS / Runtime Gateway 主链路拓扑与耦合点，产出后续 review 问题清单。
- Scope: internal
- Date: 2026-06-21

## Findings

### 1. 模块/子模块清单与一句话职责

#### Backend capability / session capability

- `crates/agentdash-application/src/capability/resolver.rs`：把 agent/workflow/resource contributions、MCP 候选、permission granted keys 归约为 `CapabilityState`；文件注释明确 resolver 输出应写入 AgentFrame revision 后再被 session 消费（`resolver.rs:235`、`resolver.rs:242`、`resolver.rs:246`）。
- `crates/agentdash-application/src/session/capability_state.rs`：runtime capability dimension replay / validate / projection 管线，要求 transition diff 应用后的 state 写入 AgentFrame revision，session 内存只是投影缓存（`capability_state.rs:247`、`capability_state.rs:275`、`capability_state.rs:397`）。
- `crates/agentdash-application/src/agent_run/frame/builder.rs`：`AgentFrameBuilder::with_capability_state` 一次性拆分写入 capability / VFS / MCP surface；新 revision 默认 carry-forward 既有 visible canvas / workspace module refs（`builder.rs:133`、`builder.rs:137`、`builder.rs:270`）。
- `crates/agentdash-domain/src/workflow/agent_frame.rs`：AgentFrame 是 effective runtime surface snapshot，持有 effective capability、VFS、MCP、runtime visible canvas 和 runtime visible workspace module refs（`agent_frame.rs:6`、`agent_frame.rs:24`、`agent_frame.rs:27`）。

#### Permission

- `crates/agentdash-domain/src/permission/`：`PermissionGrant` 聚合根、状态机、grant scope、policy decision 和 repository trait 的领域事实源；规格要求 application 层不能跳过 domain 状态机。
- `crates/agentdash-application/src/permission/service.rs`：`PermissionGrantService` 编排 policy evaluation -> approval/revoke -> frame effect apply -> grant status update；auto approve 与 user approve 都走 `apply_frame_effect`（`service.rs:71`、`service.rs:120`、`service.rs:141`、`service.rs:265`）。
- `crates/agentdash-application/src/permission/compiler.rs`：把 `PermissionGrant.requested_paths` 编译为 `RuntimeCapabilityTransition` 的 tool dimension declaration，source 为 `permission_grant`（`compiler.rs:17`、`compiler.rs:27`、`compiler.rs:31`）。
- `crates/agentdash-application/src/permission/policy.rs` / `escalation.rs`：分别承载纯策略评估和 post-action scope escalation 协调；本轮只确认边界，未深挖规则完整性。
- `crates/agentdash-api/src/routes/permission_grants.rs`：正式 grant REST 面；06-14 已指出它是权限审批 UI / API 的权威链路。

#### VFS

- `crates/agentdash-application/src/vfs/`：统一 `surface_ref + mount_id + mount_relative_path` 地址模型、provider dispatch、runtime mount、surface mutation、materialization。
- `crates/agentdash-application/src/runtime_tools/vfs_provider.rs`：session runtime VFS provider 只构建 VFS tool factory，并把 `context.turn.capability_state` 传给工具 factory 进行 capability gating（`vfs_provider.rs:61`、`vfs_provider.rs:73`、`vfs_provider.rs:82`）。
- `crates/agentdash-application/src/vfs/mount_canvas.rs`：Canvas runtime mount id 使用 `cvs-{canvas.mount_id}`，provider 是 `canvas_fs`，root_ref 是 `canvas://{canvas.id}`（`mount_canvas.rs:7`、`mount_canvas.rs:11`、`mount_canvas.rs:16`）。
- `crates/agentdash-application/src/canvas/visibility.rs`：session 默认不注入 canvas，只根据 AgentFrame / session 记录的 visible mount ids 追加 Canvas mounts（`visibility.rs:11`、`visibility.rs:14`、`visibility.rs:30`）。

#### Runtime Gateway

- `crates/agentdash-application/src/runtime_gateway/gateway.rs`：统一 runtime action registry / dynamic provider / actor-context admission / invocation；`surface_for_actor` 做 actor/context 校验，`invoke` 先定位 provider 再 validate request（`gateway.rs:11`、`gateway.rs:64`、`gateway.rs:82`、`gateway.rs:104`）。
- `crates/agentdash-application/src/runtime_gateway/session_actions.rs` / `setup_actions.rs`：内置 Session / Setup action provider；本轮只作为 Gateway 分类边界确认。
- `crates/agentdash-application/src/runtime_gateway/tool_adapter.rs`：AgentTool 到 Gateway 的桥接基础件；规格要求它不自行做 capability 裁决。

#### Extension runtime / management / package

- `crates/agentdash-application/src/runtime_gateway/extension_actions.rs`：dynamic extension runtime action provider；按 `project_id + action_key` 从 enabled installations 查找 action，要求 package artifact、Backend target、schema 和 runtime permission 校验后才进入 transport（`extension_actions.rs:73`、`extension_actions.rs:114`、`extension_actions.rs:123`、`extension_actions.rs:160`、`extension_actions.rs:169`）。
- `crates/agentdash-application/src/runtime_gateway/extension_actions.rs`：`ExtensionRuntimeChannelInvoker` 是 protocol channel 的独立 admission / relay bridge；本轮只列边界，未深挖 channel 内部规则。
- `crates/agentdash-application/src/extension_runtime.rs` / `extension_management.rs` / `extension_package.rs`：Project extension projection、installation lifecycle、package artifact 管理；runtime Gateway 以 `ProjectExtensionInstallation.package_artifact` 为执行事实。
- `crates/agentdash-application-ports/src/extension_runtime.rs`、`crates/agentdash-relay/src/protocol/extension_runtime.rs`、`crates/agentdash-api/src/relay/extension_runtime_impl.rs`：云端到本机执行侧的 transport 边界；Local/Relay 执行内部由其它 subagent 负责。

#### Workspace module / Canvas promotion

- `crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs`：Workspace module runtime provider 在 WorkspaceModule cluster 开启时注入 list/describe/create/invoke/present；invoke 依赖 `RuntimeGateway` 和 extension channel transport 延迟注入（`runtime_tool_provider.rs:55`、`runtime_tool_provider.rs:80`、`runtime_tool_provider.rs:112`、`runtime_tool_provider.rs:168`）。
- `crates/agentdash-application/src/workspace_module/tools.rs`：`workspace_module_create(kind="canvas")` 创建或 attach Canvas，并暴露 Canvas VFS mount；`workspace_module_invoke` 可走 Gateway runtime action 或 extension protocol channel；`workspace_module_present` 发送 `workspace_module_presented` session meta event（`tools.rs:384`、`tools.rs:425`、`tools.rs:724`、`tools.rs:761`、`tools.rs:990`、`tools.rs:1022`）。
- `crates/agentdash-application/src/canvas/tools.rs`：Canvas expose 会先 append live VFS mount，再写 AgentFrame visible canvas / workspace module refs，并同步 live VFS capability state 到 hook runtime（`tools.rs:242`、`tools.rs:248`、`tools.rs:253`、`tools.rs:262`、`tools.rs:333`）。
- `crates/agentdash-application/src/canvas/promotion.rs`：Canvas 发布为 packaged extension artifact 的入口，受 Shared Library / Extension package contract 约束；本轮只列边界。

#### Contracts / Frontend bridge

- `crates/agentdash-contracts/src/extension/runtime.rs`、`surface/workspace_module.rs`、`surface/vfs.rs`、`surface/canvas.rs`、`system/permission.rs`：生成前端 contract 的边界。
- `packages/app-web/src/services/extensionRuntime.ts`：前端 extension runtime service 只调用 generated contract API：projection、invoke-action、invoke-channel、webview asset URL（`extensionRuntime.ts:12`、`extensionRuntime.ts:20`、`extensionRuntime.ts:30`、`extensionRuntime.ts:49`）。
- `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts`：webview bridge 选择 runtime surface / workspace backend，并解析 panel VFS target（`webviewBridge.ts:208`、`webviewBridge.ts:227`）。
- `packages/app-web/src/features/workspace-module/model/presentation.ts`：前端收到 `workspace_module_presented` 后，Canvas presentation URI 要求是 `canvas://...`，并触发 runtime refresh（`presentation.ts:16`、`presentation.ts:32`、`presentation.ts:37`）。

### 2. 主链路拓扑：capability/effective frame -> permission/policy -> runtime gateway/action -> VFS/extension surfaces

#### A. Frame construction / baseline capability

1. Agent / workflow / resource contributions 进入 `CapabilityResolverInput`，包含 owner scope、MCP candidates、optional `CapabilityContext.granted_capability_keys` 和 authority state（`resolver.rs:137`、`resolver.rs:150`、`resolver.rs:151`）。
2. `CapabilityResolver::resolve_checked` 合并 contributions，先计算 `default_visible_capabilities`，再按 directive reduction 调整 effective caps，最后生成 `CapabilityState.tool.capabilities / enabled_clusters / tool_policy / mcp_servers`（`resolver.rs:272`、`resolver.rs:276`、`resolver.rs:283`、`resolver.rs:366`、`resolver.rs:379`）。
3. Permission Grant override 在默认可见性中优先于静态 visibility：`granted_capability_keys` 命中 well-known key 时直接 visible（`resolver.rs:439`、`resolver.rs:448`）。
4. `AgentFrameBuilder::with_capability_state` 把 `CapabilityState` 拆成 frame 的 effective capability / VFS / MCP surface，作为 session runtime surface 的权威 snapshot（`builder.rs:133`、`builder.rs:137`）。

#### B. Permission grant -> capability transition -> new frame

1. Agent/runtime/platform broker 创建 `GrantRequest`，包含 run、effect frame、source runtime session / turn / tool call、requested paths、scope、TTL、scope escalation intent（`service.rs:27`）。
2. `PermissionGrantService::request` 创建 domain grant，执行状态机 submit + policy evaluation + persist；auto approve 时立即 `apply_frame_effect`，user approval 由 `approve` 后执行同一路径（`service.rs:80`、`service.rs:98`、`service.rs:104`、`service.rs:120`、`service.rs:154`）。
3. `apply_frame_effect` 从 `effect_frame_id` 取 anchor frame，再取 agent 当前 frame，在当前 frame capability state 上增删 requested paths，构造 transition，并用 `AgentFrameBuilder` 写新 revision（`service.rs:270`、`service.rs:284`、`service.rs:291`、`service.rs:294`、`service.rs:295`）。
4. `PermissionGrantCompiler` 只负责编译 declaration；`transition_for_state` 额外追加 `SetToolAccessEffect`，把更新后的 tool capabilities / clusters / policy 写入 transition effect（`compiler.rs:27`、`compiler.rs:44`、`service.rs:327`、`service.rs:337`）。
5. Capability dimension registry 按 module validate + replay effects，VFS dimension 在 built-in 注册顺序中先于 tool/mcp/companion，符合 VFS 派生 projection 需求（`capability_state.rs:335`、`capability_state.rs:397`）。

#### C. Session runtime tools -> Gateway / VFS / workspace module

1. API bootstrap 构造 `SessionRuntimeToolComposer`，组合 VFS、workflow、collaboration、task、workspace module providers（`bootstrap/session.rs:253`、`bootstrap/session.rs:264`、`bootstrap/session.rs:286`）。
2. `SessionRuntimeToolComposer` 只是顺序调用各 domain provider 的 `build_tools`，没有自行读 repository 或执行 action（`provider.rs:63`、`provider.rs:78`）。
3. VFS provider 从 `ExecutionContext.session.vfs` 创建 shared runtime VFS，把 `context.turn.capability_state` 传给 `VfsToolFactory`，由工具面按 `CapabilityState` 裁决可见性（`vfs_provider.rs:66`、`vfs_provider.rs:73`、`vfs_provider.rs:82`）。
4. Workspace module provider 只在 `ToolCluster::WorkspaceModule` 打开后注入工具；每个具体工具再通过 `is_capability_tool_enabled` 检查 tool-level 可见性（`workspace_module/runtime_tool_provider.rs:61`、`runtime_tool_provider.rs:80`、`runtime_tool_provider.rs:96`、`runtime_tool_provider.rs:129`）。
5. `workspace_module_invoke` 对 runtime action operation 构造 `RuntimeInvocationRequest`，host 侧填入 `AgentSession` actor、`RuntimeContext::Session { project_id }`、Backend target 和 extension invocation workspace metadata，然后调用 Gateway（`workspace_module/tools.rs:724`、`tools.rs:735`、`tools.rs:741`、`tools.rs:748`、`tools.rs:761`）。
6. Protocol channel operation 不走 `RuntimeGateway::invoke`，而是走 `ExtensionRuntimeChannelInvoker`，携带 project/session/backend/workspace/consumer/channel/method/trace（`workspace_module/tools.rs:768`、`tools.rs:777`）。

#### D. Runtime Gateway -> extension action -> Relay/Local boundary

1. `RuntimeGateway::invoke` 先从 static providers 或 dynamic providers 定位 provider，再做 actor/context 校验和 `provider.supports` 校验，最后调用 provider（`gateway.rs:89`、`gateway.rs:104`、`gateway.rs:116`）。
2. Extension runtime action provider 是 dynamic provider：支持 dotted action key + SessionRuntime context（`extension_actions.rs:114`）。
3. Provider invocation 从 request 中提取 session/project/backend，读取 Project enabled installations，按 action_key 匹配 installation manifest，要求 action 是 session runtime action 且 installation 有 package artifact（`extension_actions.rs:123`、`extension_actions.rs:125`、`extension_actions.rs:136`、`extension_actions.rs:154`、`extension_actions.rs:160`）。
4. Gateway admission 在 relay 前校验 runtime permission key 和 action input schema，并把 package artifact、runtime extensions、workspace、trace、invocation id 写入 transport request（`extension_actions.rs:169`、`extension_actions.rs:170`、`extension_actions.rs:178`、`extension_actions.rs:189`）。
5. API panel 调用面也通过后端组装 actor/context/target/trace：`invoke-action` 只接收前端 request，后端校验 Project view、backend access、workspace，然后组装 `SessionUser` actor + Session context + Backend target 调用 Gateway（`routes/extension_runtime.rs:111`、`routes/extension_runtime.rs:139`、`routes/extension_runtime.rs:146`、`routes/extension_runtime.rs:159`）。

#### E. Canvas / VFS / workspace module presentation

1. `workspace_module_create(kind="canvas")` 创建或 attach Canvas 后调用 `create_or_attach_canvas_for_session`，返回 `canvas:{mount_id}` descriptor 和 `cvs-{mount_id}://...` skill path（`workspace_module/tools.rs:421`、`tools.rs:425`、`tools.rs:446`）。
2. Canvas expose 先更新 live shared VFS，再写 AgentFrame 的 visible canvas mount ids 和 visible workspace module refs，最后用 live VFS 刷新 capability state / hook runtime（`canvas/tools.rs:242`、`tools.rs:248`、`tools.rs:253`、`tools.rs:262`、`tools.rs:333`）。
3. `workspace_module_present` 在 renderer 是 canvas 时也会执行 `expose_existing_canvas_for_session`，然后注入 `workspace_module_presented` notification；前端 presentation helper 对 `canvas://...` 设置 `refreshRuntime: true`（`workspace_module/tools.rs:1004`、`tools.rs:1022`、`presentation.ts:32`、`presentation.ts:37`）。

### 3. 与其它模块的耦合点：只列边界

#### Session / AgentFrame

- AgentFrame 是 capability/VFS/MCP effective surface 的持久化 snapshot，同时有 runtime visible canvas / workspace module refs 两列；review 需要确认这些 runtime accumulate 字段与 `CapabilityState.workspace_module`、`CapabilityState.vfs` 的权威关系。
- Session hook runtime / capability service 是 live refresh 入口：Canvas expose、permission grant apply、runtime launch 都可能触发 frame revision 或 live state refresh。
- `RuntimeSessionExecutionAnchor` 在本轮只作为 session -> AgentFrame / run / project 的 backlink 边界；session control 细节由其它 review 覆盖。

#### Workflow procedure / Agent lifecycle

- Workflow / lifecycle 通过 contribution、subject association、AgentFrame construction 影响 capability visibility 和 effect frame 选择。
- Permission grant 的 `run_id`、`effect_frame_id` 和 scope escalation intent 绑定 lifecycle fact，但 grant 状态机事实源在 permission domain。
- Workflow runtime tools 已从 VFS provider 拆到 `WorkflowRuntimeToolProvider`；本轮只检查 composer 边界，不重复 review lifecycle reducer / task projection。

#### Frontend

- Extension panel / Canvas panel 只能提交 action/channel key、session/backend/input 等 API contract 字段；actor/context/target/workspace 由后端 route 组装。
- Workspace module presentation 通过 session meta event 驱动 UI tab 打开；Canvas presentation URI 是 `canvas://{mount_id}`，Agent 编辑 URI 是 `cvs-{mount_id}://...`。
- VFS mount/backend selection 在 webview bridge 仍是前端边界点：它读取 runtime surface mounts 和 workspace backend，只应作为 UI bridge input，不应成为授权事实源。

#### Local / Relay

- Relay/local 执行侧边界包括 `command.extension_action_invoke`、`response.extension_action_invoke`、extension channel transport、VFS materialization transport、local host archive download/cache/activation。
- 本轮不评价 local `CommandHandler`、Extension Host permission guard、workspace root 解析；06-14 已覆盖 local/relay 执行侧过厚问题，且另有 subagent 负责。

#### Shared Library / Marketplace / Extension Package

- `Shared Library Contract` 规定 ExtensionTemplate 的 runtime actions / protocol channels / workspace tabs / bundles 任一非空时需要 package artifact；Project installation 有 package artifact 后才可 runtime execute。
- Runtime action 的安装事实源是 Project extension installation；LibraryAsset payload 不直接影响当前 session。
- Canvas 发布为插件后进入同款 packaged extension artifact 语义；需要 review Canvas promotion 是否只产出 Project/Library artifact，不绕开 extension package artifact fact。

#### Contracts / generated types

- Generated frontend contracts 是 DTO 边界；本轮发现 permission route 已使用 typed nested DTO 名称，06-14 的旧 typed gap 需在“不重复 review”中作为已覆盖项处理，除非后续发现新 contract drift。
- Extension runtime / workspace module / VFS / canvas contracts 的事实源在 `agentdash-contracts`；前端手写 helper 只能做 UI selection / bridge，不应重定义后端 policy。

### 4. 值得下一轮深挖的 review 问题

#### P0

- **P0: Permission grant 的三份事实是否会漂移。** 当前 grant apply 会同时产生 `PermissionGrant.status`、`RuntimeCapabilityTransition` 和新 `AgentFrame.effective_capability_json`；CapabilityResolver 也有 `granted_capability_keys` override。下一轮应验证：active grants 是仅用于重建 / baseline visibility，还是也可能与 frame effective state 并列成为运行时工具可见性的事实源；审批、撤销、TTL expire、scope escalation 后是否统一收敛到 frame revision 和 capability delta。
- **P0: Canvas expose 的 live VFS、AgentFrame refs、CapabilityState refresh 是否具备原子事实源。** `expose_canvas_to_session` 先 append live VFS，再写 frame visible refs，再刷新 hook runtime；若 frame 写入或 hook refresh 失败，live VFS 与 frame/capability state 可能处于不同步中间态。下一轮应确认是否有事务性/补偿性事实约束，或是否应把 frame revision 作为唯一可恢复事实后再刷新 live VFS。
- **P0: Extension action 与 protocol channel admission 是否拥有同等级事实源。** Runtime action 走 `RuntimeGateway::invoke`，protocol channel 直接走 `ExtensionRuntimeChannelInvoker`。下一轮应验证 channel 是否同样在 transport 前完成 Project installation、schema、permission、package artifact、consumer/dependency alias 校验，并且不让 panel / canvas / SDK 传入 project/backend/session 权威事实。

#### P1

- **P1: `SharedRuntimeGatewayHandle` 延迟注入是否会让 workspace module invoke 静默缺面。** Provider 在缺 RuntimeGateway 或 channel transport 时 warning 后跳过 `workspace_module_invoke`。下一轮应检查 bootstrap 顺序、session refresh、测试覆盖和运行中 gateway reset 是否会造成 tool surface 与 capability state 不一致。
- **P1: AgentFrame runtime visible refs 与 `CapabilityState.workspace_module` 的边界。** AgentFrame 同时保存 declaration-derived `CapabilityState.workspace_module` 和 runtime accumulate `visible_workspace_module_refs_json`。下一轮应明确 `workspace_module_list/describe/invoke/present` 聚合时这两类来源的优先级、去重、撤销和 session 结束收口。
- **P1: VFS surface fact 是 frame snapshot、live `SharedRuntimeVfs`、还是 API surface resolver。** VFS 工具从 `ExecutionContext.session.vfs` 读取 live surface，Canvas expose 直接 mutate shared VFS，AgentFrame 也保存 VFS surface。下一轮应核对 session launch、runtime refresh、resource browser、frontend VFS tab 是否都从同一个 closed surface 派生。
- **P1: Extension package artifact ownership和 runtime installation fact 是否唯一。** Shared Library 允许 LibraryAsset-owned 和 Project-owned artifact；runtime action 要求 installation.package_artifact。下一轮应验证 install、publish、Canvas promotion、source-status、runtime download 的 artifact id / digest / owner_kind 是否不会形成第二套 package fact。
- **P1: Extension invocation workspace metadata 是否是 host 组装事实。** `attach_extension_invocation_workspace` 通过 request metadata 传递 mount/root；下一轮应确认 metadata 只由 API/tool host 侧解析 runtime surface/backend 得出，webview/canvas/SDK 不能自行注入 workspace root 或 backend target。
- **P1: Runtime Gateway surface discovery 是否可能被当成授权 manifest。** 规格说 `surface_for` 仅调试用，消费端必须用 `surface_for_actor`；代码保留两个入口。下一轮应搜索所有调用方，确认产品路径没有把 `surface_for` 暴露为可执行权限清单。
- **P1: Permission scope escalation 的 post-action hook 与 capability grant apply 的顺序。** 规格规定 scope escalation 只在 action 成功后触发；下一轮应沿 tool action success -> `ScopeEscalationCoordinator` -> `LifecycleSubjectAssociation(role=ControlScope)` -> grant status `ScopeEscalated` 验证事实源和幂等性。

#### P2

- **P2: Frontend bridge selectors 是否重复后端路由事实。** `webviewBridge.ts` 选择 backend / VFS mount，`presentation.ts` 解释 `canvas://`。下一轮只需确认它们是 UI bridge selector，不承担 capability / permission / backend authorization。
- **P2: Capability catalog / generated contract 的剩余漂移。** 06-14 已覆盖 well-known capability/tool catalog contract 化；下一轮若继续看，应只抽样当前 generated contract 是否已替代前端手写 catalog，不重复讨论旧问题。
- **P2: VFS mount metadata typed 化的剩余风险。** 06-14 已覆盖 `vfs/mount.rs` metadata 过宽；本轮只需补充 Canvas / extension workspace metadata 是否仍有 untyped JSON 成为跨模块事实源。
- **P2: Tests should match topology, not implementation split.** 后续 review 应找的是跨链路断言：grant approve updates frame + tool surface、canvas present exposes VFS + refreshes frontend runtime、extension action invalid schema fails before relay、channel invalid permission fails before local host。

### 5. 不应重复 review 的内容

以下内容已由 `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md` 覆盖，本轮后续 review 不应重新展开同一论证，只在发现新事实时引用：

- Permission / companion grant 双事实源：06-14 已给出 P0，结论是 `PermissionGrantService` / `PermissionGrant` 是唯一授权事实源，companion grant request 只能作为 broker 输入。
- Permission typed contract / grant list gap：06-14 已覆盖 pending/active/terminal 查询和 nested DTO typed 化；当前如果复查，只确认是否已闭环。
- Capability catalog / tool catalog contract 化：06-14 已覆盖前端手写 well-known keys、baseline、tool descriptor 绕过 generated contract 的问题。
- VFS tool provider 过厚：06-14 指出旧 `RelayRuntimeToolProvider` 是跨域 composer；当前代码已有 `SessionRuntimeToolComposer` + VFS/workflow/collaboration/task/workspace-module providers，后续不要重复旧拆分建议，应 review 新 composer 边界是否正确。
- Local `CommandHandler` 全域 command hub、Extension Host workspace/process/env/schema contract 过宽、Relay wire envelope 边界：06-14 已覆盖，且 Local/Relay 执行侧另有 subagent 负责。
- AgentRun workspace projection、RuntimeSession runtime-control、SessionRuntimeInner / AgentRuntimeDelegate 过宽、Lifecycle cancel / Task projection / status aggregation：属于其它模块 review 主线，本文件只在 capability / VFS / runtime action 入口交叉处引用。

## Files Found

- `.trellis/tasks/06-21-module-topology-coupling-review/prd.md` — 当前任务 PRD，仍是 TBD 初稿。
- `.trellis/spec/backend/capability/architecture.md` — Capability role、invariants、dimension baseline、grant-aware visibility contract。
- `.trellis/spec/backend/permission/architecture.md` — PermissionGrant 聚合、policy、compiler、scope escalation 架构。
- `.trellis/spec/backend/permission/grant-lifecycle.md` — PermissionGrant 状态机、REST API、schema、测试契约。
- `.trellis/spec/backend/vfs/architecture.md` — VFS role、provider baseline、session runtime tool composition contract。
- `.trellis/spec/backend/vfs/vfs-access.md` — VFS 地址模型、runtime mount、provider、Canvas visibility、inline storage、runtime tools。
- `.trellis/spec/backend/runtime-gateway.md` — RuntimeGateway action 分类、actor/context admission、extension runtime action/channel contract。
- `.trellis/spec/cross-layer/shared-library-contract.md` — Shared Library / Marketplace / Project Asset / ExtensionPackageArtifact 跨层契约。
- `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md` — 已覆盖的过度设计与事实源分裂清单。
- `crates/agentdash-application/src/capability/resolver.rs` — CapabilityResolver 和 grant-aware visibility 实现。
- `crates/agentdash-application/src/session/capability_state.rs` — capability dimension registry、transition replay、frame projection contract。
- `crates/agentdash-application/src/permission/service.rs` — PermissionGrantService lifecycle orchestration and frame effect apply。
- `crates/agentdash-application/src/permission/compiler.rs` — PermissionGrant -> RuntimeCapabilityTransition compiler。
- `crates/agentdash-application/src/runtime_tools/provider.rs` — SessionRuntimeToolComposer、SharedRuntimeGatewayHandle、shared session/runtime helper。
- `crates/agentdash-application/src/runtime_tools/vfs_provider.rs` — VFS runtime tool provider。
- `crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs` — workspace module runtime tool provider and invoke dependency injection。
- `crates/agentdash-application/src/workspace_module/tools.rs` — workspace_module create/invoke/present tool execution。
- `crates/agentdash-application/src/runtime_gateway/gateway.rs` — RuntimeGateway registry / dynamic provider / actor-context validation。
- `crates/agentdash-application/src/runtime_gateway/extension_actions.rs` — extension runtime action provider and channel invoker boundary。
- `crates/agentdash-application/src/canvas/tools.rs` — Canvas create/attach/expose and live capability refresh。
- `crates/agentdash-application/src/canvas/visibility.rs` — visible Canvas mount append policy。
- `crates/agentdash-application/src/vfs/mount_canvas.rs` — Canvas VFS mount construction。
- `crates/agentdash-domain/src/workflow/agent_frame.rs` — AgentFrame effective surface and runtime visible refs。
- `crates/agentdash-api/src/routes/extension_runtime.rs` — API panel extension runtime action/channel invocation routes。
- `packages/app-web/src/services/extensionRuntime.ts` — frontend extension runtime generated-contract service wrapper。
- `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts` — extension webview backend/VFS bridge selectors。
- `packages/app-web/src/features/workspace-module/model/presentation.ts` — workspace module presentation event to tab target mapper。

## Code Patterns

- Effective capability facts should be written to AgentFrame revision, not long-lived session memory (`resolver.rs:235`, `capability_state.rs:247`, `builder.rs:133`).
- Permission grant approval mutates the current AgentFrame by reconstructing next `CapabilityState` and writing a new frame revision (`service.rs:284`, `service.rs:291`, `service.rs:295`).
- Runtime tool declaration is provider-composed and capability-gated from `ExecutionContext.turn.capability_state` (`provider.rs:78`, `vfs_provider.rs:82`, `workspace_module/runtime_tool_provider.rs:80`).
- Extension runtime action is admitted at Gateway/provider before relay transport: project/session/backend/artifact/schema/permission are checked before `invoke_extension_action` (`gateway.rs:104`, `extension_actions.rs:160`, `extension_actions.rs:169`, `extension_actions.rs:195`).
- Canvas expose currently spans three state surfaces in one flow: live shared VFS, AgentFrame visible refs, live hook capability state (`canvas/tools.rs:248`, `canvas/tools.rs:253`, `canvas/tools.rs:272`).

## External References

- None. 本轮只读项目内代码、Trellis specs、task artifacts 和 generated frontend contract usage；未使用外部文档或联网资料。

## Related Specs

- `.trellis/spec/backend/capability/architecture.md`
- `.trellis/spec/backend/capability/capability-dimension-pipeline.md`
- `.trellis/spec/backend/capability/tool-capability-pipeline.md`
- `.trellis/spec/backend/permission/architecture.md`
- `.trellis/spec/backend/permission/grant-lifecycle.md`
- `.trellis/spec/backend/permission/policy-engine.md`
- `.trellis/spec/backend/vfs/architecture.md`
- `.trellis/spec/backend/vfs/vfs-access.md`
- `.trellis/spec/backend/vfs/vfs-materialization.md`
- `.trellis/spec/backend/runtime-gateway.md`
- `.trellis/spec/cross-layer/shared-library-contract.md`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)`；本文件依据用户显式给出的 task path 写入，不是从 session active task 解析得出。
- 本轮未修改业务代码、未运行测试、未执行 git 操作。
- Local/Relay 执行侧只列 transport / handler 边界；未 review local Extension Host、local command router、relay protocol handler 内部。
- `prd.md` 仍是 TBD 初稿；本研究依据用户 dispatch prompt、指定规格和代码事实完成。
