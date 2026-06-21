# 跨模块耦合矩阵

本矩阵只汇总 subagents 已落盘的结论。主会话不新增未由 subagent 产出的模块 review 判断。

## P0 Couplings

| Coupling | Modules | Type | Risk | Source |
| --- | --- | --- | --- | --- |
| Project event NDJSON 未进入 generated contract | API stream / domain events / frontend eventStore | 契约耦合 | Project stream 由 domain `StreamEvent`、API `stream.rs`、前端 `eventStream.ts`/`types/acp.ts` 三处手写表达，无法用 `contracts:check` 捕获 drift。 | `research/10-contract-boundary-deep-dive.md` |
| application 直接依赖 contracts | application / contracts / API / frontend | 分层契约耦合 | use case/read model 直接构造 browser-facing DTO，wire shape 与 application projection 同步演进。 | `research/10-contract-boundary-deep-dive.md` |
| AgentRun delivery runtime target 选择重复 | AgentRun workspace / API route / anchor repo / mailbox | 运行态耦合 | workspace read projection 与 command route 各自按 run+agent latest anchor 选 runtime，影响 command target、frame、resource surface。 | `research/11-agentrun-control-deep-dive.md` |
| Hook mailbox NotFound direct fallback | Session hook delegate / AgentRun mailbox / Agent loop | 控制面耦合 | anchored AgentRun 的 mailbox 写入 NotFound 可能回退 direct steering/follow-up，绕过 durable mailbox envelope。 | `research/11-agentrun-control-deep-dive.md` |
| Lifecycle start API 混合 create 与 drain | Workflow API / LifecycleDispatchService / OrchestrationExecutorLauncher | 控制面耦合 | public start command 同时创建 Ready run 并立即 drain ready nodes，调用方无法观察纯 Ready orchestration。 | `research/12-lifecycle-runtime-facts-deep-dive.md` |
| Anchor latest semantics 分散 | RuntimeSessionExecutionAnchor repo / workspace / cancel / mailbox / SubjectExecutionView | 运行态耦合 | `latest_for_agent`、workspace latest、subject projection latest、cancel target 等各自解释 latest，multi-run/multi-frame 时可能选不同坐标。 | `research/12-lifecycle-runtime-facts-deep-dive.md` |
| PermissionGrant expiry/revoke 与 frame effect 不在同一事实链 | Permission / Capability / AgentFrame | 事实源耦合 | approve/revoke 分步写 frame 与 grant status，TTL expire 只改 grant status，不生成 capability remove effect。 | `research/13-permission-frame-vfs-gateway-deep-dive.md` |
| Canvas expose 非原子跨多 surface | Canvas / VFS / AgentFrame / Session hook runtime | 事实源耦合 | live VFS 先 mutate，再写 frame refs，再刷新 hook capability；失败路径可能留下中间态。 | `research/13-permission-frame-vfs-gateway-deep-dive.md` |
| AgentFrame revision snapshot 与 runtime exposure append 混用 | AgentFrame / Canvas / WorkspaceModule / Capability | 事实源耦合 | AgentFrame 注释承诺 surface 变更走 revision，但 visible canvas/module refs 直接 UPDATE 当前 frame 行。 | `research/13-permission-frame-vfs-gateway-deep-dive.md` |
| Extension invocation backend target 不一致 | Frontend bridge / Extension runtime API / WorkspaceModule tool / RuntimeGateway | 控制面耦合 | panel API 从 frontend request 接收 `backend_id`，workspace module tool 优先 `session.backend_execution`，同一 action/channel 可能不同 target。 | `research/14-local-placement-relay-deep-dive.md` |
| Relay command target taxonomy 未固化 | Relay / Session launch / MCP / VFS / Terminal / Extension | 运行态耦合 | prompt/cancel、MCP、extension、terminal、VFS file tool 各有 target 解析方式，哪些绑定 execution lease、哪些绑定 mount utility 未形成统一 contract。 | `research/14-local-placement-relay-deep-dive.md` |

## P1 Couplings

| Coupling | Modules | Type | Risk | Source |
| --- | --- | --- | --- | --- |
| contracts crate 同时做 DTO 与内部模型转换 | contracts / domain / SPI / agent protocol | 分层契约耦合 | `agentdash-contracts` 依赖内部模型并内置大量 `From`，MCP preset 还存在 request/domain 双向转换。 | `research/10-contract-boundary-deep-dive.md` |
| BackendAccess / Canvas / SkillAsset 等 route-local DTO 缺口 | API dto / frontend services / generated contracts | 契约耦合 | 跨 feature response 仍由 API-local DTO 与前端 mapper 手写，部分 service 用默认值重建 wire DTO。 | `research/10-contract-boundary-deep-dive.md` |
| command policy 重新构造 conversation snapshot | AgentConversationSnapshot / command policy / API routes | 控制面耦合 | route precondition 用不完整输入构造 UI snapshot 进行 stale/enablement 校验，未来规则容易分裂。 | `research/11-agentrun-control-deep-dive.md` |
| AgentRun direct steer service 仍导出 | AgentRun / SessionControlService / tests | 控制面耦合 | 未发现产品 route 使用，但导出的 direct steer surface 容易被绕过 mailbox 误用。 | `research/11-agentrun-control-deep-dive.md` |
| Status aggregation owner contract 不够显式 | workflow runtime application reducer / domain aggregate | 事实源耦合 | run status owner 已收敛到 domain，但 orchestration status 在 application；需要明确契约与 mixed status tests。 | `research/12-lifecycle-runtime-facts-deep-dive.md` |
| Task execution surfaces 残留 | SubjectExecutionView / TaskExecutionView / task_read tool | 契约耦合 | public API/前端走 SubjectExecutionView，但 AppState 仍注入 narrow TaskExecutionView service，tool 暴露 execution stub。 | `research/12-lifecycle-runtime-facts-deep-dive.md` |
| Runtime action 与 channel admission 不对称 | RuntimeGateway / Extension channel / Local Host API | 控制面耦合 | action 在云端校验 permission key/schema，channel 有 schema/consumer/package 校验但 method permission 主要留到 local host。 | `research/13-permission-frame-vfs-gateway-deep-dive.md` |
| WorkspaceModule visibility 审计源分散 | CapabilityState / AgentFrame visible refs / WorkspaceModule tools | 事实源耦合 | base allowlist 在 `CapabilityState.workspace_module`，runtime refs 在 frame JSON，工具执行时 union。 | `research/13-permission-frame-vfs-gateway-deep-dive.md` |
| AgentRun resource surface frame 坐标混合 | AgentRun workspace / anchor launch frame / current AgentFrame / VFS | 运行态耦合 | query 同时使用 current frame 的 VFS 与 latest anchor launch frame address，需要明确 source coordinate。 | `research/13-permission-frame-vfs-gateway-deep-dive.md` |
| Backend disconnect terminal projection 未验证 | Relay registry / lease repo / session stream / frontend | 运行态耦合 | disconnect 会 mark lease lost 并删除 session sinks，但用户 feed/AgentRun 状态是否收到明确 lost terminal 事件需验证。 | `research/14-local-placement-relay-deep-dive.md` |
| MCP backend target fallback 过宽 | BackendRegistry / MCP relay / local MCP manager | 运行态耦合 | resolved transport 在 payload 中，但投递 backend 可从 session route、VFS default mount、advertised catalog、任意 online backend fallback。 | `research/14-local-placement-relay-deep-dive.md` |
| standalone local backend id 来源不一致 | agentdash-local CLI / desktop ensure / dev-runtime | 事实源耦合 | desktop/dev claim 从 server 获取 backend_id，standalone local 缺 `--backend-id` 时随机生成。 | `research/14-local-placement-relay-deep-dive.md` |

## P2 Couplings

| Coupling | Modules | Type | Risk | Source |
| --- | --- | --- | --- | --- |
| `types/index.ts` 职责过宽 | frontend types / generated aliases / UI view models | 契约归属 | generated aliases、legacy wire gaps、UI view model 混在一个入口，降低 contract 缺口可见性。 | `research/10-contract-boundary-deep-dive.md` |
| top-level AgentRun control_plane 是 convenience duplicate | API mapper / contracts / frontend shell | UI 状态耦合 | 从 conversation execution 派生粗粒度 status，可保留但不能扩张为 command enablement source。 | `research/11-agentrun-control-deep-dive.md` |
| raw anchor repository API 名称易误用 | repository / application selection policy | 命名边界 | `latest_for_agent` 看似业务语义，实为 raw `updated_at DESC LIMIT 1`。 | `research/12-lifecycle-runtime-facts-deep-dive.md` |
| RuntimeGateway `surface_for` debug 入口需守卫 | RuntimeGateway / product routes | 控制面归属 | 当前未发现生产误用，但应防止 debug surface 成为授权 manifest。 | `research/13-permission-frame-vfs-gateway-deep-dive.md` |
| VFS/Terminal utility target 需和 execution placement 区分 | VFS relay_fs / terminal route / WorkspacePanel | 运行态归属 | 按 mount backend/root 投递是合理 utility target，但 UI 文案不能表达为 execution allocation。 | `research/14-local-placement-relay-deep-dive.md` |

## Cross-Cutting Clusters

### Contract Cluster

- Project NDJSON contract 化。
- application/contracts 依赖边界。
- route-local DTO 分级迁移。
- `types/index.ts` 分类。

### Runtime Coordinate Cluster

- `RuntimeSessionExecutionAnchor` selection policy。
- AgentRun delivery target resolver。
- SubjectExecutionView history。
- AgentRun resource surface frame coordinate。

### Control Surface Cluster

- Lifecycle start vs drain。
- AgentRun mailbox/direct steer。
- Extension action/channel backend target。
- Relay command target taxonomy。

### Capability Surface Cluster

- PermissionGrant -> AgentFrame capability fact。
- Canvas expose。
- WorkspaceModule visibility。
- RuntimeGateway action/channel admission parity。
