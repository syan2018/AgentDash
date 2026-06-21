# 后续 Trellis Task 候选

本 backlog 由第二轮 subagent deep-dive 汇总而来。每项都可拆成独立 Trellis task；是否实际创建子 task 由后续开发排期决定。

## P0

### 1. Project event NDJSON contract 化

- 问题：Project-level event stream 仍由 domain `StreamEvent`、API `stream.rs`、前端 `eventStream.ts` / `types/acp.ts` 三处手写表达。
- 影响模块：`agentdash-contracts`、`agentdash-api` stream、`packages/app-web/src/api/eventStream.ts`、`eventStore`。
- 建议 task scope：定义 Rust contract 的 Project event NDJSON envelope，生成 TS，替换前端手写 stream type/parser。
- 验收方向：`pnpm run contracts:check` 能捕获 Project stream drift；前端 Project stream hook 只消费 generated union。
- 来源：`research/10-contract-boundary-deep-dive.md`

### 2. application contract DTO 依赖归属审计

- 问题：`agentdash-application` 直接依赖 `agentdash-contracts` 并构造 browser-facing DTO，use case/read model 与 wire DTO 边界不清。
- 影响模块：`agent_run/conversation_snapshot.rs`、`agent_run/workspace/query.rs`、`session/eventing.rs`、`workspace_module`、`capability/tool_catalog.rs`、`agentdash-contracts`。
- 建议 task scope：逐项标注 application read model、API adapter、contract DTO 的 owner；迁移或明确允许的 projection assembly 边界。
- 验收方向：每个 application -> contract import 都有明确归属；不需要兼容层。
- 来源：`research/10-contract-boundary-deep-dive.md`

### 3. 收敛 AgentRun delivery runtime target resolver

- 问题：workspace query 与 API route context 各自按 run+agent latest anchor 选 delivery runtime，影响 read projection、composer/cancel/mailbox target、resource surface。
- 影响模块：`agent_run/workspace/query.rs`、`routes/lifecycle_agents.rs`、`RuntimeSessionExecutionAnchorRepository`、mailbox command target。
- 建议 task scope：新增 application-level delivery runtime resolver，统一 run_id/agent_id/frame/node policy，API 与 workspace query 共用。
- 验收方向：删除 API route local duplicate resolver；测试覆盖多 runtime session、frame replacement、orchestration node attempt。
- 来源：`research/11-agentrun-control-deep-dive.md`

### 4. 收紧 hook mailbox NotFound direct fallback

- 问题：hook delivery 写 mailbox 返回 `NotFound` 时直接回退 steering/follow-up，anchored AgentRun 异常可能绕过 durable mailbox。
- 影响模块：`session/mailbox_delegate.rs`、AgentRun mailbox、AgentLoop turn boundary。
- 建议 task scope：fallback 前显式判定 unbound trace；anchored AgentRun 的 NotFound 进入 diagnostic/error。
- 验收方向：anchored missing anchor 不注入 direct messages；unanchored runtime 可保留 direct path；补 hook delivery tests。
- 来源：`research/11-agentrun-control-deep-dive.md`

### 5. 拆分 lifecycle start 与 ready-node drain public command

- 问题：`POST /lifecycle-runs` 同时 create run 和 drain ready nodes，违反 service/spec 的 Ready orchestration 可观察合同。
- 影响模块：`routes/workflows.rs`、`LifecycleDispatchService`、`OrchestrationExecutorLauncher`、frontend lifecycle start flow。
- 建议 task scope：`POST /lifecycle-runs` 只创建 Ready run；新增显式 continue/drain command。
- 验收方向：start API test 断言 entry node Ready、无 runtime session、无 agent/frame/anchor；drain command test 覆盖 materialization。
- 来源：`research/12-lifecycle-runtime-facts-deep-dive.md`

### 6. 统一 RuntimeSessionExecutionAnchor selection semantics

- 问题：workspace、cancel、mailbox、SubjectExecutionView、repository raw latest 各自解释 latest/primary/current-frame/run-scoped anchor。
- 影响模块：anchor repository、AgentRun workspace、subject cancel、mailbox、SubjectExecutionView。
- 建议 task scope：建立 AnchorDeliverySelectionService 或等价 application selection API，显式输入 run、agent、frame、orchestration、policy。
- 验收方向：不再用全局 `latest_for_agent` 后过滤 run；多 run/frame/anchor fixture 中各消费者选择一致。
- 来源：`research/12-lifecycle-runtime-facts-deep-dive.md`

### 7. 收敛 PermissionGrant expiry/revoke 与 AgentFrame capability effect

- 问题：approve/revoke 分步写 frame 与 grant status；TTL expire 只改 grant status，不生成 remove transition 或新 frame。
- 影响模块：permission service、permission grant repository、AgentFrameRepository、capability transition。
- 建议 task scope：定义 approve/revoke/expire 的单一 application service 边界和失败语义；expire 后能力效果与 revoke 一致或明确 TTL 不控制 runtime revoke。
- 验收方向：tests 覆盖 grant approve/revoke/expire 后 status、AgentFrame capability、tool surface 一致。
- 来源：`research/13-permission-frame-vfs-gateway-deep-dive.md`

### 8. 为 Canvas expose 建立可恢复事实顺序

- 问题：Canvas expose 先 mutate live VFS，再写 AgentFrame refs，再刷新 hook runtime，失败路径可能留下中间态。
- 影响模块：canvas tools、VFS runtime surface、AgentFrame visible refs、session capability service、WorkspaceModule presentation。
- 建议 task scope：选择 frame refs 或 capability transition 作为可恢复事实源，live VFS 从事实派生/刷新。
- 验收方向：frame append 失败、hook runtime missing、capability state missing 三类路径都有幂等或可观测行为。
- 来源：`research/13-permission-frame-vfs-gateway-deep-dive.md`

### 9. 拆清 AgentFrame revision snapshot 与 runtime exposure append

- 问题：AgentFrame 注释承诺 surface 变更产生新 revision，但 visible canvas/module refs 直接 UPDATE 当前 frame。
- 影响模块：AgentFrame domain/repository、Canvas expose、WorkspaceModule visibility、AgentFrameBuilder carry-forward。
- 建议 task scope：决定 runtime exposure refs 是 frame revision 的一部分、独立 exposure 表，还是 capability dimension transition。
- 验收方向：Workspace/Canvas UI projection 与 tool visibility 从同一 exposure fact 读取。
- 来源：`research/13-permission-frame-vfs-gateway-deep-dive.md`

### 10. 统一 extension invocation backend target resolver

- 问题：panel API 由 frontend request 提供 `backend_id`，workspace module tool 优先 `session.backend_execution`，同一 action/channel target resolver 不一致。
- 影响模块：extension runtime API、webview/canvas bridge、WorkspaceModule tools、RuntimeGateway extension provider。
- 建议 task scope：后端提供唯一 server-side resolver，panel 与 workspace module 使用同一优先级；frontend 只传 command intent 或 workspace/tab context。
- 验收方向：route tests 覆盖 panel 与 workspace module 对同一 session 得到同一 backend/workspace。
- 来源：`research/14-local-placement-relay-deep-dive.md`

### 11. 明确 relay command target taxonomy

- 问题：prompt/cancel、MCP、extension、terminal、VFS file tool 各自解析 target，哪些绑定 execution lease、哪些绑定 mount utility 未固化。
- 影响模块：Relay protocol、Session launch、MCP relay、Extension runtime、Terminal、VFS relay_fs、cross-layer specs。
- 建议 task scope：建立命令分类表：`execution-placement-bound`、`session-route-bound`、`mount-utility-bound`、`setup-bound`。
- 验收方向：prompt 不可从 VFS 执行期 fallback；MCP fallback 仅限无 session route 场景；VFS/terminal 明确为 mount utility。
- 来源：`research/14-local-placement-relay-deep-dive.md`

## P1

### 12. 收敛 contracts crate 内部转换边界

- 问题：`agentdash-contracts` 同时承担 wire DTO 和 domain/SPI/protocol adapter；MCP preset 存在 request/domain 双向转换。
- 验收方向：列出允许保留的 outbound projection conversion 与需要迁移的 incoming command conversion。
- 来源：`research/10-contract-boundary-deep-dive.md`

### 13. BackendAccess / BackendWorkspaceInventory contract 化

- 问题：Project/Backend/Workspace binding/inventory DTO 仍在 API dto 与 frontend `types/index.ts` 手写。
- 验收方向：Rust contract + generated TS 覆盖 access、inventory、candidate、sync response。
- 来源：`research/10-contract-boundary-deep-dive.md`

### 14. Canvas CRUD 与 SkillAsset HTTP DTO contract 化

- 问题：Canvas CRUD 与 SkillAsset service 仍使用 raw mapper 和默认值重建 wire DTO。
- 验收方向：Canvas CRUD、SkillAsset list/create/update/import response 进入 contracts；editor draft 保留 feature-local。
- 来源：`research/10-contract-boundary-deep-dive.md`

### 15. ExtensionManagement service 回到 generated DTO

- 问题：已有 generated contract，但 frontend service 仍以 `unknown` 手写校验并重建 response。
- 验收方向：service 返回 generated DTO；只保留 UI view model 转换。
- 来源：`research/10-contract-boundary-deep-dive.md`

### 16. workspace_module_presented stream payload contract 化

- 问题：HTTP present DTO 已 generated，session platform event payload 仍 raw `Record<string, unknown>`。
- 验收方向：Backbone/session platform event 使用同源 generated DTO。
- 来源：`research/10-contract-boundary-deep-dive.md`、`research/13-permission-frame-vfs-gateway-deep-dive.md`

### 17. 拆分 command policy 的 snapshot resolver 依赖

- 问题：command policy 为校验 route precondition 重构完整 conversation snapshot，但输入不完整且混入 UI projection。
- 验收方向：抽出 command availability core，workspace snapshot 与 route policy 共用；policy 不再构造完整 UI snapshot。
- 来源：`research/11-agentrun-control-deep-dive.md`

### 18. 移除或封装 AgentRun direct steer service

- 问题：`AgentRunSteeringService` 仍导出 direct steer surface，当前只发现 tests 使用。
- 验收方向：移入 test support、标记 non-product internal，或改为写 mailbox envelope。
- 来源：`research/11-agentrun-control-deep-dive.md`

### 19. 固化 runtime status aggregation owner contract

- 问题：run-level aggregation 在 domain，orchestration-level derivation 在 application；需要明确契约与测试矩阵。
- 验收方向：spec 写明 owner；tests 覆盖 failed、blocked、running、ready、cancelled/completed、append orchestration。
- 来源：`research/12-lifecycle-runtime-facts-deep-dive.md`

### 20. 收敛 Task execution surfaces 到 SubjectExecutionView

- 问题：正式 API/前端走 SubjectExecutionView，但 AppState 注入 `TaskExecutionView` service，`task_read execution` 提供 stub。
- 验收方向：删除/私有化 narrow service；`task_read execution` 调用 subject projector 或移除 execution mode。
- 来源：`research/12-lifecycle-runtime-facts-deep-dive.md`

### 21. 扩展 SubjectExecutionView 表达 execution history

- 问题：当前可遍历多 association/anchor，但最终只输出 `latest_runtime_node`。
- 验收方向：新增 runtime attempts/history 列表；latest 从列表派生；前端可下钻 history。
- 来源：`research/12-lifecycle-runtime-facts-deep-dive.md`

### 22. 对齐 ExtensionRuntimeChannelInvoker 与 RuntimeGateway action admission

- 问题：channel 缺少与 action 同级的 method permission known-key 云端预检。
- 验收方向：channel method permissions 入站阶段做 known-key 校验；local host 保留二次裁决。
- 来源：`research/13-permission-frame-vfs-gateway-deep-dive.md`

### 23. 明确 CapabilityResolver granted keys 使用边界

- 问题：resolver 支持 granted keys override，但未发现 active grants 生产注入路径；未来补齐会与 frame transition 双算。
- 验收方向：决定 frame revision 是唯一 grant runtime fact，或定义 active grants fold 顺序。
- 来源：`research/13-permission-frame-vfs-gateway-deep-dive.md`

### 24. 收口 WorkspaceModule visibility 审计事实源

- 问题：base allowlist 在 `CapabilityState.workspace_module`，runtime refs 在 AgentFrame JSON，工具执行时 union。
- 验收方向：定义 visibility resolver；读取 runtime refs 失败有可观测 diagnostic。
- 来源：`research/13-permission-frame-vfs-gateway-deep-dive.md`

### 25. 审计 AgentRun resource surface 的 current frame 与 anchor frame 坐标

- 问题：workspace query 同时使用 current frame VFS 与 latest anchor launch frame address。
- 验收方向：DTO 解释 frame_ref 与 VFS surface source 坐标；测试覆盖 permission/canvas refresh 后选择。
- 来源：`research/13-permission-frame-vfs-gateway-deep-dive.md`

### 26. 收口 backend disconnect 的 session terminal projection

- 问题：disconnect 会 mark lease lost 并删除 sinks，但 feed/AgentRun 是否收到明确 lost terminal 事件未验证。
- 验收方向：backend disconnect 对 running prompt 产生 terminal/lost projection；runtime-summary active lease 消失；session route 清理。
- 来源：`research/14-local-placement-relay-deep-dive.md`

### 27. 限定 MCP backend target fallback

- 问题：session context 下 MCP target 可 fallback 到 VFS/catalog/any online backend。
- 验收方向：session context 下强制 session route/backend_execution；setup/probe 才允许 catalog fallback。
- 来源：`research/14-local-placement-relay-deep-dive.md`

### 28. 收敛 standalone local backend id 来源

- 问题：desktop/dev runtime 从 ensure/claim 获取 backend_id，standalone local 缺 `--backend-id` 时随机生成。
- 验收方向：standalone CLI 明确 internal/debug 并要求 backend_id/token，或也走 ensure/claim。
- 来源：`research/14-local-placement-relay-deep-dive.md`

### 29. 明确 terminal 与 execution lease 的关系

- 问题：terminal 按 session default mount backend/root 投递，不占用 execution lease；语义需明确。
- 验收方向：spec 标明 terminal 是 mount utility 还是 execution surface；UI 展示与 runtime-summary 一致。
- 来源：`research/14-local-placement-relay-deep-dive.md`

## P2

- 拆分 `types/index.ts`：generated aliases、shared view model、legacy wire gaps 分文件。来源：`research/10-contract-boundary-deep-dive.md`
- 确认 SessionExecutionState 消费面：仍是 control UI 事实则 contract 化，否则标注为 route-local wrapper。来源：`research/10-contract-boundary-deep-dive.md`
- Auth/current-user/identity-directory DTO contract 化或明确 auth wrapper 归属。来源：`research/10-contract-boundary-deep-dive.md`
- 保留但瘦身 top-level `AgentRunWorkspaceView.control_plane`，确保 command enablement 只来自 conversation commands。来源：`research/11-agentrun-control-deep-dive.md`
- 将 raw anchor repository API 与 application selection API 分层命名。来源：`research/12-lifecycle-runtime-facts-deep-dive.md`
- 清理 AppState 中未公开消费的 `StoryActivityActivationService`。来源：`research/12-lifecycle-runtime-facts-deep-dive.md`
- 为 RuntimeGateway `surface_for` debug 入口增加调用方守卫。来源：`research/13-permission-frame-vfs-gateway-deep-dive.md`
- WorkspaceModule runtime deps 缺失从 warn-only 变成可观测诊断。来源：`research/13-permission-frame-vfs-gateway-deep-dive.md`
- 前端 workspace routing 文案区分 binding availability 与 execution allocatable。来源：`research/14-local-placement-relay-deep-dive.md`
- 保持 extension relay payload 不携带 backend_id，target 属于 routing 层事实。来源：`research/14-local-placement-relay-deep-dive.md`
- Profile UI 不把 machine_id 输入当 authority。来源：`research/14-local-placement-relay-deep-dive.md`

## 建议拆 task 顺序

1. 先做 Contract Cluster：Project NDJSON、application/contracts 依赖归属、关键 route-local DTO contract 化。
2. 并行做 Runtime Coordinate Cluster：AgentRun delivery resolver、anchor selection policy、lifecycle start/drain 拆分。
3. 再做 Capability Surface Cluster：PermissionGrant/AgentFrame、Canvas expose、WorkspaceModule visibility。
4. 最后做 Relay Target Cluster：extension target resolver、relay command taxonomy、disconnect terminal projection、MCP fallback。
