# Research: permission-frame-vfs-gateway-deep-dive

- Query: 深挖 PermissionGrant、AgentFrame capability/VFS surface、Canvas expose、RuntimeGateway action/channel admission 的事实源与耦合，输出可拆后续任务候选。
- Scope: internal
- Date: 2026-06-21

## 结论摘要

1. PermissionGrant 与 AgentFrame capability revision 已形成主事实链，但仍存在两个漂移风险：`expire_overdue()` 只改 grant 状态、不写 revoke capability frame；approve/revoke 先写 AgentFrame 再更新 grant status，缺少同一事务边界。
2. `CapabilityResolver.granted_capability_keys` 是规格化的 grant-aware visibility 入口，但当前仓库检索只发现定义和 resolver 消费，没有发现 active grants 注入该 context 的生产路径。实际运行时授权主要靠 PermissionGrantService 直接重写当前 AgentFrame capability surface。
3. Canvas expose 是 live VFS mutation -> AgentFrame visible canvas refs -> AgentFrame visible workspace module refs -> hook runtime capability refresh 的串行链，不是原子事实链；缺 capability state 或 hook runtime 时会保留已写 live VFS/frame refs 并返回成功。
4. AgentFrame 同时承担 revision snapshot 和 runtime exposure append。`visible_canvas_mount_ids_json` / `visible_workspace_module_refs_json` 通过 repository 直接 UPDATE 当前 frame 行，不产生新 revision，但 `AgentFrame` 注释仍写着 surface 变更产生新 revision。
5. RuntimeGateway action admission 比 channel admission 更完整：extension action 在 Gateway/provider 侧校验 package artifact、action permission key、input schema 后才进 relay；channel invoker 校验 installation、consumer/dependency、package artifact、input schema，但没有同级 method permission key 预检，权限主要留给 local Host API facade 执行时裁决。
6. AgentFrame runtime visible refs 与 `CapabilityState.workspace_module` 的关系是“base allowlist + runtime frame refs union”。这个设计可成立，但当前 union 分散在 tool 执行时读取，不进入 `CapabilityState.workspace_module`，需要明确谁是 Workspace Module visibility 的审计事实。

## 主链路拓扑

### A. PermissionGrant -> RuntimeCapabilityTransition -> AgentFrame capability surface

1. `PermissionGrant` domain 状态机定义 `Created/PendingPolicy/PendingUserApproval/Approved/Applied/Expired/Revoked/ScopeEscalated`，且 `Applied | ScopeEscalated` 被视为 active（`crates/agentdash-domain/src/permission/value_objects.rs:39`, `crates/agentdash-domain/src/permission/value_objects.rs:84`）。
2. 状态机只允许 `Approved -> Applied/Failed`、`Applied -> Expired/Revoked/ScopeEscalated`（`crates/agentdash-domain/src/permission/entity.rs:138`, `crates/agentdash-domain/src/permission/entity.rs:152`, `crates/agentdash-domain/src/permission/entity.rs:159`, `crates/agentdash-domain/src/permission/entity.rs:166`）。
3. grant compiler 把 `requested_paths` 编译为 `RuntimeCapabilityTransition.declarations`，source 是 `permission_grant`，payload 是 `ToolCapabilityDirective::Add/Remove`（`crates/agentdash-application/src/permission/compiler.rs:27`, `crates/agentdash-application/src/permission/compiler.rs:31`, `crates/agentdash-application/src/permission/compiler.rs:44`）。
4. `PermissionGrantService::request` 在 auto-approved 时先 create grant，再 `apply_frame_effect`，再 mark applied 并 update grant（`crates/agentdash-application/src/permission/service.rs:114`, `crates/agentdash-application/src/permission/service.rs:120`）。
5. user approve 路径先把 grant 从 pending 改为 approved，再写 effect frame，最后 mark applied/update grant（`crates/agentdash-application/src/permission/service.rs:154`, `crates/agentdash-application/src/permission/service.rs:158`, `crates/agentdash-application/src/permission/service.rs:160`）。
6. revoke 路径先 `apply_frame_effect(grant, false)` 写 frame，再 `grant.revoke()` 和 repo update（`crates/agentdash-application/src/permission/service.rs:193`, `crates/agentdash-application/src/permission/service.rs:205`, `crates/agentdash-application/src/permission/service.rs:207`）。
7. `apply_frame_effect` 以 `effect_frame_id` 找 anchor frame，再取 agent current frame，把 current frame 投影为 `CapabilityState`，应用 requested paths，生成新 `AgentFrameBuilder::with_capability_state` revision（`crates/agentdash-application/src/permission/service.rs:270`, `crates/agentdash-application/src/permission/service.rs:284`, `crates/agentdash-application/src/permission/service.rs:291`, `crates/agentdash-application/src/permission/service.rs:294`, `crates/agentdash-application/src/permission/service.rs:295`）。
8. `transition_for_state` 额外追加 `set_tool_access` effect，把更新后的 capabilities / clusters / tool_policy 放进 transition（`crates/agentdash-application/src/permission/service.rs:327`, `crates/agentdash-application/src/permission/service.rs:337`）。
9. TTL expiry repository 只执行 `UPDATE permission_grants SET status='expired' WHERE status='applied'...`，未写 capability revoke frame，也未覆盖 `scope_escalated`（`crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs:196`）。

### B. CapabilityResolver grant override 与实际 wiring

1. capability spec 允许 `CapabilityContext.granted_capability_keys` 让 Permission Grant 绕过静态 visibility；resolver input 也有 optional `capability_context`（`.trellis/spec/backend/capability/architecture.md`, `crates/agentdash-application/src/capability/resolver.rs:137`, `crates/agentdash-application/src/capability/resolver.rs:150`）。
2. resolver 在 `resolve_checked` 读取 `input.capability_context.granted_capability_keys`，传给 `default_visible_capabilities`（`crates/agentdash-application/src/capability/resolver.rs:272`, `crates/agentdash-application/src/capability/resolver.rs:279`）。
3. `default_visible_capabilities` 对 well-known key 先检查 `granted_keys.contains(key)`，命中即 visible，再走静态规则（`crates/agentdash-application/src/capability/resolver.rs:439`, `crates/agentdash-application/src/capability/resolver.rs:448`）。
4. 仓库检索只发现 `granted_capability_keys` 的字段定义和 resolver 消费；未发现 `PermissionGrantRepository::list_active_*` 结果被注入 `CapabilityContext` 的生产路径（`rg granted_capability_keys`, `rg list_active_by_frame`）。
5. 当前实际授权收敛点是 PermissionGrantService 写新的 `AgentFrame.effective_capability_json`，而不是每次 resolver baseline 都从 active grants 重新合成。

### C. AgentFrame capability/VFS/workspace module surface

1. `AgentFrame` 注释定义为 effective runtime surface snapshot，字段包括 `effective_capability_json`、`vfs_surface_json`、`mcp_surface_json`、runtime visible canvas refs 和 runtime visible workspace module refs（`crates/agentdash-domain/src/workflow/agent_frame.rs:6`, `crates/agentdash-domain/src/workflow/agent_frame.rs:15`, `crates/agentdash-domain/src/workflow/agent_frame.rs:19`, `crates/agentdash-domain/src/workflow/agent_frame.rs:24`, `crates/agentdash-domain/src/workflow/agent_frame.rs:27`）。
2. `AgentFrameBuilder::with_capability_state` 一次性拆分写入 capability / VFS / MCP surface，读写对称（`crates/agentdash-application/src/agent_run/frame/builder.rs:133`, `crates/agentdash-application/src/agent_run/frame/builder.rs:137`）。
3. builder 生成新 revision 时会 carry-forward current frame 的 capability、VFS、MCP、visible canvas refs、visible workspace module refs（`crates/agentdash-application/src/agent_run/frame/builder.rs:245`, `crates/agentdash-application/src/agent_run/frame/builder.rs:255`, `crates/agentdash-application/src/agent_run/frame/builder.rs:270`, `crates/agentdash-application/src/agent_run/frame/builder.rs:273`）。
4. Workspace Module 声明式可见性在 owner bootstrap 中由 preset `visible_workspace_module_refs` 投影进 `CapabilityState.workspace_module`（`crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs:260`）。
5. Canvas visible ids 会从 existing frame 带入 owner bootstrap 并用于 append Canvas VFS mounts（`crates/agentdash-application/src/agent_run/frame/construction/composer_project_agent.rs:131`, `crates/agentdash-application/src/agent_run/frame/construction/composer_project_agent.rs:132`, `crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs:457`）。
6. Runtime visible workspace module refs 不进入 `CapabilityState.workspace_module`，而是 WorkspaceModule tool 执行时从当前 frame 读取后与 base visibility union（`crates/agentdash-application/src/workspace_module/tools.rs:66`, `crates/agentdash-application/src/workspace_module/tools.rs:68`, `crates/agentdash-application/src/workspace_module/tools.rs:78`, `crates/agentdash-application/src/workspace_module/tools.rs:88`, `crates/agentdash-application/src/workspace_module/tools.rs:165`）。
7. 如果 runtime visible refs 读取失败，tool 会 warn 后退回 base visibility（`crates/agentdash-application/src/workspace_module/tools.rs:88`, `crates/agentdash-application/src/workspace_module/tools.rs:95`）。

### D. Canvas expose -> live VFS -> AgentFrame refs -> hook capability refresh

1. VFS spec 明确 Canvas expose 应先追加 live runtime VFS、写 visible canvas mount ids，并刷新 `CapabilityState.vfs.active`；present 应在展示事件前执行同一 exposure（`.trellis/spec/backend/vfs/vfs-access.md`）。
2. `expose_canvas_to_session` 当前先 `vfs.append_canvas_mount(canvas).await`（live shared VFS mutation）（`crates/agentdash-application/src/canvas/tools.rs:242`, `crates/agentdash-application/src/canvas/tools.rs:248`）。
3. 随后写 `visible_canvas_mount_ids_json`，失败即返回 error，但 live VFS 已被 mutate（`crates/agentdash-application/src/canvas/tools.rs:253`, `crates/agentdash-application/src/canvas/tools.rs:258`）。
4. 再写 `visible_workspace_module_refs_json`，失败同样返回 error，但 live VFS 和 canvas ref 已写（`crates/agentdash-application/src/canvas/tools.rs:262`, `crates/agentdash-application/src/canvas/tools.rs:263`, `crates/agentdash-application/src/canvas/tools.rs:268`）。
5. 最后同步 live VFS capability state 到 hook runtime：先读 latest capability state；没有 state 时只 debug 并成功返回（`crates/agentdash-application/src/canvas/tools.rs:289`, `crates/agentdash-application/src/canvas/tools.rs:293`, `crates/agentdash-application/src/canvas/tools.rs:299`）。
6. 没有 hook runtime 时也只 debug 并成功返回（`crates/agentdash-application/src/canvas/tools.rs:308`, `crates/agentdash-application/src/canvas/tools.rs:313`, `crates/agentdash-application/src/canvas/tools.rs:319`）。
7. 真正 sync 时从 live VFS snapshot 写入 `after_state.vfs.active`，然后走 runtime context transition（`crates/agentdash-application/src/canvas/tools.rs:333`, `crates/agentdash-application/src/session/capability_service.rs:183`, `crates/agentdash-application/src/session/capability_service.rs:199`）。
8. AgentFrame repository 对 visible canvas refs / workspace refs 是直接 UPDATE 当前 frame JSON 列，不创建 revision（`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:320`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:328`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:337`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:350`）。

### E. RuntimeGateway action/channel admission

1. `RuntimeGateway::surface_for` 只按 action kind 过滤；`surface_for_actor` 先做 actor/context 校验；`invoke` 选择 provider/dynamic provider 后调用 `validate_request` 再执行 provider（`crates/agentdash-application/src/runtime_gateway/gateway.rs:54`, `crates/agentdash-application/src/runtime_gateway/gateway.rs:64`, `crates/agentdash-application/src/runtime_gateway/gateway.rs:82`, `crates/agentdash-application/src/runtime_gateway/gateway.rs:104`）。
2. Session runtime action 校验要求 context 有 session_id、actor 也绑定同一 session（`crates/agentdash-application/src/runtime_gateway/gateway.rs:161`, `crates/agentdash-application/src/runtime_gateway/gateway.rs:166`, `crates/agentdash-application/src/runtime_gateway/gateway.rs:179`, `crates/agentdash-application/src/runtime_gateway/gateway.rs:185`）。
3. 生产调用 `surface_for_actor` 目前只在 Canvas bridge surface route 出现；未发现产品路径调用 `surface_for`（`rg surface_for`, `crates/agentdash-api/src/routes/canvases.rs:414`）。
4. Extension action provider 从 request 中解析 session/project/backend，读取 enabled installations，匹配 action，要求 action 是 SessionRuntime、installation 有 package artifact，校验 action permissions 和 input schema，再进 transport（`crates/agentdash-application/src/runtime_gateway/extension_actions.rs:123`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:125`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:154`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:160`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:169`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:170`）。
5. `validate_action_permissions` 对 action 声明的 permission key 做 known-key 分类，未知 key 返回 capability denied（`crates/agentdash-application/src/runtime_gateway/extension_actions.rs:267`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:312`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:337`）。
6. Extension action transport payload 包含 package artifact、runtime extensions、workspace、trace、invocation id（`crates/agentdash-application/src/runtime_gateway/extension_actions.rs:178`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:185`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:189`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:190`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:191`）。
7. Channel invoker 读取 enabled installations，resolve provider/consumer/dependency/method，要求 provider package artifact，校验 method input schema，再进 channel transport（`crates/agentdash-application/src/runtime_gateway/extension_actions.rs:625`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:629`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:639`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:640`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:649`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:682`）。
8. Channel resolve 会校验 extension panel consumer 已安装、dependency alias 已声明、provider channel 已启用、method 已声明、consumer dependency 约束（`crates/agentdash-application/src/runtime_gateway/extension_actions.rs:735`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:741`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:752`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:754`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:770`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:784`）。
9. Channel invoker 未发现与 action provider 等价的 `method.permissions` known-key 预检；local host 在 Host API 调用时按 channel method permissions 裁决（`crates/agentdash-local/src/extensions/host/permission_guard.rs:48`, `crates/agentdash-local/src/extensions/host/permission_guard.rs:71`）。
10. Local host 校验 action/channel output schema（`crates/agentdash-local/src/extensions/host/manager.rs:115`, `crates/agentdash-local/src/extensions/host/manager.rs:135`, `crates/agentdash-local/src/extensions/host/manager.rs:145`, `crates/agentdash-local/src/extensions/host/manager.rs:167`）。

### F. Host-side assembly vs iframe/tool inputs

1. Extension iframe SDK 只发 `action_key + input` 或 `channel_key + method + input + dependency_alias`（`packages/extension-ui/src/index.ts:128`, `packages/extension-ui/src/index.ts:132`）。
2. App web bridge 从 workspace data 组装 session_id/backend_id，并把 panel extension key 作为 channel consumer；iframe 不能直接传 project/backend/session（`packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:86`, `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:97`, `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:112`, `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:118`）。
3. API route 仍接收 frontend request 中的 `session_id` / `backend_id`，但 route 负责 project permission、backend access、workspace resolution、actor/context/target assembly（`crates/agentdash-api/src/routes/extension_runtime.rs:127`, `crates/agentdash-api/src/routes/extension_runtime.rs:139`, `crates/agentdash-api/src/routes/extension_runtime.rs:146`, `crates/agentdash-api/src/routes/extension_runtime.rs:159`, `crates/agentdash-api/src/routes/extension_runtime.rs:206`, `crates/agentdash-api/src/routes/extension_runtime.rs:220`）。
4. Workspace module tool runtime action 分支由 host 侧构造 `AgentSession` actor、Session context、Backend target，并附加 workspace metadata（`crates/agentdash-application/src/workspace_module/tools.rs:724`, `crates/agentdash-application/src/workspace_module/tools.rs:735`, `crates/agentdash-application/src/workspace_module/tools.rs:748`, `crates/agentdash-application/src/workspace_module/tools.rs:751`）。
5. Workspace module protocol channel 分支由 host 侧构造 project/session/backend/workspace/consumer/channel/method/trace（`crates/agentdash-application/src/workspace_module/tools.rs:768`, `crates/agentdash-application/src/workspace_module/tools.rs:776`, `crates/agentdash-application/src/workspace_module/tools.rs:779`）。

## 耦合矩阵

| Coupling | From | To | Relationship | Evidence | Risk |
| --- | --- | --- | --- | --- | --- |
| Grant status vs effective frame | PermissionGrantService / repository | AgentFrame effective capability | approve/revoke 写 frame 与 grant status update 分步；TTL expire 只改 grant status | `service.rs:120`, `service.rs:205`, `permission_grant_repository.rs:196` | P0 |
| Dormant grant-aware visibility | CapabilityResolver | PermissionGrant active query | resolver 支持 granted keys override，但未发现 active grants 注入 production path | `resolver.rs:279`, `resolver.rs:448`, `rg granted_capability_keys` | P1 |
| Frame revision snapshot vs direct runtime append | AgentFrameRepository | AgentFrame visible refs | visible canvas/module refs 直接 UPDATE frame 行，不产生 revision | `agent_frame.rs:6`, `lifecycle_anchor_repository.rs:328`, `lifecycle_anchor_repository.rs:350` | P0 |
| Canvas expose multi-surface chain | SharedRuntimeVfs | AgentFrame visible refs / hook runtime capability | live VFS 先 mutate，后续 frame/ref/hook 失败会留下中间态 | `canvas/tools.rs:248`, `canvas/tools.rs:253`, `canvas/tools.rs:272`, `canvas/tools.rs:293` | P0 |
| Workspace module visibility split | CapabilityState.workspace_module | AgentFrame visible_workspace_module_refs_json | base allowlist 在 capability JSON，runtime grant 在 frame refs，tool 执行时 union | `owner_bootstrap.rs:260`, `workspace_module/tools.rs:68`, `workspace_module/tools.rs:165` | P1 |
| AgentRun resource surface frame coordinate | AgentRun workspace query | RuntimeSessionExecutionAnchor / current frame | base_vfs 来自 current frame，projector address 使用 anchor.launch_frame_id | `query.rs:341`, `query.rs:350`, `query.rs:355` | P1 |
| RuntimeGateway action admission | RuntimeGateway dynamic provider | Extension action relay transport | package artifact、permission key、schema、workspace metadata 在 relay 前校验 | `extension_actions.rs:160`, `extension_actions.rs:169`, `extension_actions.rs:170`, `extension_actions.rs:178` | Base |
| Runtime channel admission | ExtensionRuntimeChannelInvoker | Extension channel relay transport / local host | package artifact、consumer/dependency、schema 校验存在；method permission key 预检不对称 | `extension_actions.rs:640`, `extension_actions.rs:649`, `extension_actions.rs:784`, `permission_guard.rs:48` | P1 |
| Frontend bridge facts | Extension iframe bridge | API/Gateway host assembly | iframe 只交 action/channel/input；app route/bridge 补 session/backend/consumer | `extension-ui/src/index.ts:128`, `webviewBridge.ts:97`, `routes/extension_runtime.rs:146` | P2 |

## P0 Backlog Candidates

### P0-1: 收敛 PermissionGrant expiry/revoke 与 AgentFrame capability effect 的事务事实链

- 问题：approve/revoke 都是 frame write 与 grant status update 分步完成；TTL expire 只改 status，不生成 Remove transition 或 AgentFrame revision。
- 影响范围：permission service、permission grant repository、AgentFrameRepository、capability transition tests、permission API active/terminal list。
- 建议 owner：backend/permission + backend/capability。
- 验收方向：
  - `expire_overdue` 或 scheduler 路径必须对 expired applied grants 生成与 revoke 等价的 capability remove effect，或明确 TTL 不参与 runtime revocation 并移除 active-tool 语义。
  - approve/revoke/expire 的状态变更与 frame effect 具备单一 application service 边界和失败语义。
  - 测试覆盖 grant approve/revoke/expire 后 `PermissionGrant.status`、`AgentFrame.effective_capability_json`、tool surface 一致。

### P0-2: 为 Canvas expose 建立可恢复的单一事实顺序

- 问题：`expose_canvas_to_session` 先 mutate live VFS，再写 frame refs，再刷新 hook runtime；后两步失败或缺 state/hook 时会留下 live VFS/frame refs 的部分状态。
- 影响范围：canvas tools、session capability service、AgentFrameRepository direct append、hook runtime refresh、workspace module create/present tests。
- 建议 owner：backend/vfs + backend/session。
- 验收方向：
  - 明确 frame visible refs 或 capability transition 是 Canvas exposure 的可恢复事实源，live VFS 从该事实派生/刷新。
  - 失败路径不留下不可观测中间态，或有幂等补偿/重放机制。
  - 测试覆盖 frame append 失败、hook runtime missing、capability state missing 三类路径。

### P0-3: 拆清 AgentFrame revision snapshot 与 runtime exposure append 的写入模型

- 问题：AgentFrame 注释说 surface 变更产生新 revision，但 visible canvas/module refs 直接 UPDATE 当前 frame；它们又会被 builder carry-forward，实际具备持久 runtime fact 语义。
- 影响范围：AgentFrame domain、AgentFrameRepository、builder carry-forward、Canvas expose、workspace module visibility、AgentRun frame runtime projection。
- 建议 owner：backend/session + backend/workflow-frame。
- 验收方向：
  - 决定 runtime exposure refs 是 AgentFrame revision 的一部分、独立 session exposure 表，还是 capability dimension transition。
  - 若留在 AgentFrame，append 应有 revision/audit 语义或文档明确 direct append 是 runtime mutable column。
  - Workspace/Canvas UI projection 与 tool visibility 都从同一 exposure fact 读取。

## P1 Backlog Candidates

### P1-1: 对齐 ExtensionRuntimeChannelInvoker 与 RuntimeGateway action 的 admission 级别

- 问题：channel invoker 有 package artifact、consumer/dependency、input schema 校验，但没有 action provider 那样的 method permission known-key 预检；权限主要依赖 local Host API 调用时裁决。
- 影响范围：runtime_gateway/extension_actions、extension-dev manifest validation、local host permission guard、relay channel payload tests。
- 建议 owner：backend/runtime-gateway + local/extension-host。
- 验收方向：
  - channel method permissions 在云端 invoker 入站阶段做 known-key 校验，错误不进入 relay。
  - 保留 local Host API 二次裁决，覆盖 output schema 与 Host API permission tests。
  - action/channel admission 测试矩阵并列：package artifact missing、unknown permission、input schema mismatch、output schema mismatch。

### P1-2: 明确 CapabilityResolver granted keys 的使用边界

- 问题：resolver 支持 `granted_capability_keys` override，但生产路径未发现 active grants 注入；如果未来补齐，会与 PermissionGrantService 写 frame effect 形成第二条 grant application 路径。
- 影响范围：capability resolver、frame construction owner bootstrap、permission service active grant queries、spec/backend/capability。
- 建议 owner：backend/capability + backend/permission。
- 验收方向：
  - 若选择 frame revision 为唯一 runtime grant fact，移除或限制 granted keys override 的生产含义。
  - 若选择 resolver 重建 active grants，必须定义与 existing frame transitions 的 fold 顺序和撤销/TTL 语义。
  - 增加检索级测试或 integration test 防止 active grant 同时被 resolver override 与 transition apply 双算。

### P1-3: 收口 WorkspaceModule visibility 的审计事实源

- 问题：声明式 allowlist 在 `CapabilityState.workspace_module`；Canvas runtime grant 在 `AgentFrame.visible_workspace_module_refs_json`；工具执行时 union，读取失败降级为 base visibility。
- 影响范围：workspace_module tools/provider、CapabilityState workspace_module dimension、AgentFrame visible refs、ProjectAgent preset editor、frontend WorkspaceModulesPanel。
- 建议 owner：backend/workspace-module + backend/capability。
- 验收方向：
  - 定义 `CapabilityState.workspace_module` 是否必须包含 runtime refs，或 runtime refs 是否是独立 exposure fact。
  - `workspace_module_list/describe/invoke/present` 使用同一 visibility resolver。
  - 读取 runtime refs 失败时有明确错误/diagnostic，而不是静默收窄到 base visibility。

### P1-4: 审计 AgentRun resource surface 的 current frame 与 anchor frame 坐标

- 问题：workspace query 取 current frame 作为 base_vfs，但 `AgentRunLifecycleSurfaceProjector` address 使用 latest anchor 的 launch_frame_id；frame revision 刷新后两者可能不是同一 frame coordinate。
- 影响范围：agent_run/workspace/query、RuntimeSessionExecutionAnchor、AgentRunLifecycleSurfaceProjector、frontend resource surface。
- 建议 owner：backend/session + backend/vfs。
- 验收方向：
  - AgentRun workspace surface 明确使用 current frame、delivery launch frame，还是二者组合。
  - 输出 DTO 能解释 frame_ref 与 VFS surface source 的坐标。
  - 测试覆盖 permission/canvas refresh 后 AgentRun workspace surface 选择。

### P1-5: 把 Canvas presentation stream payload 纳入 generated contract

- 问题：HTTP presentation 返回 generated `WorkspaceModulePresentation`，session meta event 前端仍以 raw `Record<string, unknown>` 解析。
- 影响范围：workspace_module present event、Backbone platform event payload、frontend presentation parser。
- 建议 owner：cross-layer/contracts + frontend。
- 验收方向：
  - `workspace_module_presented` platform event payload 使用同源 contract DTO。
  - 前端 presentation parser 消费 typed DTO，不重复手写字段 shape。

## P2 Backlog Candidates

### P2-1: 梳理 extension invocation workspace metadata 的 host-only 约束

- 问题：workspace metadata 由 host route/tool 组装是正确方向，但它通过 request metadata JSON 传入 provider；需要防止 webview/SDK 形成注入入口。
- 影响范围：runtime_gateway/extension_actions、extensionRuntime.ts、webviewBridge.ts、workspace_module tools。
- 建议 owner：backend/runtime-gateway + frontend/extension-runtime。
- 验收方向：
  - 只允许 API/tool host 调用 `attach_extension_invocation_workspace`。
  - DTO 不暴露 workspace root/backend override 给 iframe SDK。
  - tests 覆盖 iframe 请求中的 project/backend/workspace 字段被忽略或拒绝。

### P2-2: 为 RuntimeGateway surface discovery 增加调用方守卫

- 问题：规格要求 `surface_for` 仅调试用；当前检索未发现生产调用，但最好用测试或 lint 保护。
- 影响范围：runtime_gateway、Canvas runtime bridge route。
- 建议 owner：backend/runtime-gateway。
- 验收方向：
  - 产品 route 只调用 `surface_for_actor`。
  - `surface_for` 保持内部/测试用途，或改名表达 debug surface。

### P2-3: 把 WorkspaceModule runtime deps 缺失从 warn-only 变成可观测诊断

- 问题：provider 缺 RuntimeGateway 或 channel transport 时 warning 后不注入 `workspace_module_invoke`，可能造成 capability surface 与 tool surface 看起来不一致。
- 影响范围：workspace_module/runtime_tool_provider、session bootstrap、tool assembly diagnostics。
- 建议 owner：backend/workspace-module + backend/session。
- 验收方向：
  - 缺依赖时在 session/tool assembly diagnostics 中可见。
  - bootstrap tests 覆盖 workspace_module cluster enabled 时 invoke tool 的装配完整性。

## 不重复项

- 不重复 06-14 的 Permission/companion grant 双事实源结论；本轮只确认正式 PermissionGrantService 与 AgentFrame/capability surface 的一致性问题。
- 不重复 capability/tool catalog contract 化；本轮只触及 `CapabilityResolver.granted_capability_keys` 与 active grant wiring。
- 不重复 Local `CommandHandler` 或 Relay envelope 过厚；本轮只检查 extension action/channel 在进入 relay 前的 admission。
- 不重复 AgentRun mailbox/control surface；本轮只引用 AgentRun workspace surface 与 frame/VFS coordinate 的邻接事实。
- 不重复前端大组件拆分；本轮只记录 extension iframe bridge 与 workspace module presentation payload 的事实源关系。

## Files Found

- `.trellis/tasks/06-21-module-topology-coupling-review/prd.md` - 当前 review 父任务 PRD。
- `.trellis/tasks/06-21-module-topology-coupling-review/design.md` - review 分轮、schema、subagent 范围与整合规则。
- `.trellis/tasks/06-21-module-topology-coupling-review/research/04-capability-permission-extension-vfs-topology.md` - 第一轮 capability/permission/extension/VFS 拓扑基线。
- `.trellis/tasks/06-21-module-topology-coupling-review/research/03-session-agentrun-runtime-topology.md` - 第一轮 session/agentrun/runtime 拓扑基线。
- `.trellis/tasks/06-21-module-topology-coupling-review/research/06-frontend-contracts-topology.md` - 第一轮 frontend/contracts 拓扑基线。
- `.trellis/spec/backend/permission/architecture.md` - PermissionGrant lifecycle、compiler、scope escalation 架构约束。
- `.trellis/spec/backend/vfs/vfs-access.md` - VFS address/runtime mount/Canvas session visibility/surface mutation 约束。
- `.trellis/spec/backend/runtime-gateway.md` - RuntimeGateway action/channel admission 与 extension runtime contract。
- `.trellis/spec/backend/capability/architecture.md` - Capability resolver 与 grant-aware visibility contract。
- `.trellis/spec/backend/capability/capability-dimension-pipeline.md` - RuntimeCapabilityTransition、dimension ordering、workspace module grants。
- `crates/agentdash-domain/src/permission/entity.rs` - PermissionGrant domain 状态机。
- `crates/agentdash-domain/src/permission/value_objects.rs` - GrantStatus active/terminal 语义。
- `crates/agentdash-application/src/permission/service.rs` - grant request/approve/revoke/apply frame effect。
- `crates/agentdash-application/src/permission/compiler.rs` - PermissionGrant -> RuntimeCapabilityTransition compiler。
- `crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs` - active/terminal filter 与 TTL expire SQL。
- `crates/agentdash-application/src/capability/resolver.rs` - CapabilityResolver granted keys override。
- `crates/agentdash-domain/src/workflow/agent_frame.rs` - AgentFrame effective surface 与 runtime visible refs 字段。
- `crates/agentdash-application/src/agent_run/frame/builder.rs` - frame surface split/carry-forward。
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs` - visible refs direct UPDATE。
- `crates/agentdash-application/src/canvas/tools.rs` - Canvas expose 串行状态链。
- `crates/agentdash-application/src/session/capability_service.rs` - live VFS capability state refresh。
- `crates/agentdash-application/src/workspace_module/tools.rs` - workspace module list/invoke/present、runtime visible refs union、Gateway/channel calls。
- `crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs` - workspace module tools declaration 与 runtime deps 注入。
- `crates/agentdash-application/src/runtime_gateway/gateway.rs` - RuntimeGateway surface/invoke/actor-context validation。
- `crates/agentdash-application/src/runtime_gateway/extension_actions.rs` - extension runtime action provider、channel invoker、schema/permission admission。
- `crates/agentdash-local/src/extensions/host/manager.rs` - local action/channel output schema validation。
- `crates/agentdash-local/src/extensions/host/permission_guard.rs` - local Host API action/channel method permission guard。
- `crates/agentdash-api/src/routes/extension_runtime.rs` - frontend panel action/channel route assembly。
- `crates/agentdash-api/src/routes/canvases.rs` - Canvas bridge surface uses `surface_for_actor`。
- `packages/extension-ui/src/index.ts` - iframe SDK action/channel request shape。
- `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts` - app-web host bridge assembly。
- `packages/app-web/src/features/workspace-module/model/presentation.ts` - frontend workspace module presentation parser。

## Code Patterns

- Permission grant apply/revoke rewrites `AgentFrame.effective_capability_json` through `AgentFrameBuilder::with_capability_state`, not through long-lived session memory (`service.rs:291`, `builder.rs:133`).
- TTL expiry is status-only SQL and currently bypasses capability remove effect (`permission_grant_repository.rs:196`).
- `CapabilityResolver.granted_capability_keys` exists as an override hook, but current production wiring appears absent from static search (`resolver.rs:279`, `resolver.rs:448`).
- Canvas expose currently crosses live VFS, AgentFrame direct mutable refs, and hook runtime capability state in one non-transactional flow (`canvas/tools.rs:248`, `canvas/tools.rs:253`, `canvas/tools.rs:272`).
- RuntimeGateway action admission rejects invalid extension action input before transport; channel admission has equivalent input-schema test but not equivalent method-permission known-key precheck (`extension_actions.rs:1290`, `extension_actions.rs:1471`).
- Frontend iframe API keeps action/channel calls minimal; host bridge supplies session/backend/consumer facts (`extension-ui/src/index.ts:128`, `webviewBridge.ts:97`).

## External References

- None. 本轮只读取仓库内代码、Trellis specs 和 task research artifacts，未使用外部文档或联网资料。

## Related Specs

- `.trellis/spec/backend/permission/architecture.md`
- `.trellis/spec/backend/vfs/vfs-access.md`
- `.trellis/spec/backend/runtime-gateway.md`
- `.trellis/spec/backend/capability/architecture.md`
- `.trellis/spec/backend/capability/capability-dimension-pipeline.md`
- `.trellis/spec/frontend/type-safety.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)`；本文件按用户显式给出的 task path 和输出路径写入。
- 未修改业务代码、未运行测试、未执行 git 操作。
- `granted_capability_keys` 的“不存在生产注入路径”基于静态 `rg` 检索；未运行动态 trace。
- Local/Relay 内部只读取了 Extension Host schema/permission 相关文件；未全面 review relay command handler。
- Canvas expose 的“非原子”结论来自代码顺序与错误返回观察；未通过故障注入测试复现。
