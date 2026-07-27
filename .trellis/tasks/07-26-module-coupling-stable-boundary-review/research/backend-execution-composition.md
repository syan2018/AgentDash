# Research: 后端执行与系统装配稳定边界审计

- Query: 全量核验 AgentRun facade、Agent Runtime / Host / Service API / Runtime Wire / Integration、Hook、VFS / Tool / Terminal、API / MCP、Relay / Local / Tauri、持久化 / migration / production composition 的真实生产入口、事实权威、事务与外部副作用、断线/重启/重放语义、消费者和测试门禁；区分 production reachable、dead/unmounted、evidence gap 与合理内聚。
- Scope: internal
- Date: 2026-07-26

## Findings

### 1. 结论摘要

本研究面核验了 `backend-entrypoint-coverage-index.md` 分配给“执行与系统装配”的全部入口族。当前最值得保留的稳定边界是：

- AgentRun Product command facade 只持有 Product target association 与稳定 `client_command_id`，具体 Agent 自己拥有 command admission、effect/idempotency 与 history；Product 不复制 Agent mailbox/operation 状态（`crates/agentdash-application-agentrun/src/agent_run/product_command_facade.rs:65-74`、`:84-223`）。
- Complete Agent Host 的 callback route 由 runtime thread、binding generation、source、applied surface 与 deadline 共同 fence；Host 不保存 Product callback outcome（`crates/agentdash-agent-runtime-host/src/complete_agent_callbacks.rs:49-71`、`:90-135`、`:171-275`）。
- Runtime tool authorization 从 committed Product binding 与 exact applied surface 同时解析 grant，并对 VFS path/patch 的每个目标做 mount scope 校验（`crates/agentdash-infrastructure/src/runtime_tool_authorization.rs:64-172`、`:223-460`）。
- Relay Runtime Wire 使用项目自有 typed envelope/frame，并把 advertisement、placement、generation、digest、ack/lost 与业务 Agent command 分开；Relay 断线只改变 placement/availability，Product durable binding 仍由重启后的 provisioner 恢复（`crates/agentdash-api/src/relay/ws_handler.rs:524-542`、`crates/agentdash-infrastructure/src/complete_agent_product_provisioning.rs:260-336`）。
- PostgreSQL 是 Product binding、terminal projection、runtime health 与 concrete Dash Agent source/effect 的 durable owner；`serve` 只做 schema readiness，`migrate` 显式执行 migration，listener 在 DB、AppState 和 router 完成后才 bind（`crates/agentdash-api/src/lib.rs:166-212`、`:221-255`；`crates/agentdash-infrastructure/src/migration.rs:177-190`）。

已确认的高风险边界如下：

| ID | 等级 | 种类 | Production reachability | 结论 |
| --- | --- | --- | --- | --- |
| E-01 | P0 | authorization / information authority | reachable | `/api/diagnostics` 明确挂在 public API，返回未做字段级脱敏的完整 tracing record，可泄露内部标识、路径、错误与未来误记入的凭据。 |
| E-02 | P0 | credential / temporal composition | reachable | dynamic runtime tool catalog 只按 tool definition 判断“幂等”，重绑时丢弃新 executor，不能替换/释放旧 MCP client、header、backend anchor；credential/placement 更新或撤销后仍可能调用旧 transport。 |
| E-03 | P1 | protocol / composition | reachable when Product hook exists | Hook plan 按 trigger 声明“所有可能 action”，而 Complete Agent projection/Native Driver 只实现子集；常规 BeforeTool/AfterTool/AfterTurn 等 hook 会在 surface compile 或 callback 决策时系统性失败。 |
| E-04 | P1 | transaction / external effect / direction | reachable | Terminal route 先在 Local spawn PTY，再读取 Product snapshot 并提交 projection；后段失败会留下 inventory/reconcile 无法发现的本机孤儿进程，同时 API adapter 自己拥有 use-case orchestration。 |
| E-05 | P1 | transport / temporal / resource | reachable | Local WS read loop 只把部分 shell/probe 命令放后台；MCP list/call/close、extension、terminal、VFS materialization 等仍 inline，慢调用会阻塞 Ping、Runtime Wire 与其他命令；后台 `tokio::spawn` 又无并发上限。 |
| E-06 | P1 | protocol / data shape | reachable | direct MCP tool 返回完整 `CallToolResult` JSON，relay MCP 把 content 降成拼接字符串；同一 runtime tool 因 placement 不同而改变 output shape，并丢失 structured/image/resource metadata。 |
| E-07 | P1 | authorization / credential transport | reachable | Local 将 backend token 拼入 `/ws/backend?token=...`，Cloud 从 query 取 token；URL 会进入 proxy/access log、trace 与错误上下文的通用可见面，且 Local 本可使用 Authorization header。 |
| E-08 | P1 | composition / extensibility | reachable | AppState 接受任意 `CompleteAgentRegistrationContribution`，但 execution profile discovery、options 与 start validation 只硬编码 `PI_AGENT`/`CODEX`，Host 可注册的 Agent 与产品可选择的 Agent 是两套 catalog。 |
| E-09 | P1 | composition / temporal | reachable | Runtime Product tools 先把 deferred executor 注册进最终 Broker，数百行后 install；Cron 在 install 完成前启动，且 subsystem 没有 freeze/completeness gate。该项与 `backend-global-coupling.md` 的 B-05 交叉确认。 |
| E-10 | P2 | recovery / semantic commit | reachable | Project AgentRun start 使用稳定 identity 可由同一请求重试收敛，但 lifecycle graph、concrete Agent source、Product binding 是顺序提交；仓库未找到自动恢复未完成 launch 的 worker/intent。属于可恢复性门禁缺口，未证明现有重试语义会产生竞争 owner。 |
| E-11 | P2 | protocol reachability | dead/unmounted | Relay protocol 仍有 `CommandDiscover`、`CommandDiscoverOptions`，Local production router 无 handler，仓库也未找到 producer；它们不是 29 类 production Local command 的一部分。 |
| E-12 | P2 | readiness / operations | reachable | `/api/health` 只证明 HTTP 进程已完成 AppState 构建，不表达 DB 后续可用性、worker 或 integration health；当前 Desktop 仅把它用作“API 可渲染”门禁是合理的，但部署若把它解释为完整 readiness 会过度承诺。 |
| E-13 | P0 | authorization / cross-tenant relay | reachable | authenticated 但无 Project scope 的 `/workspaces/detect-git` 把 `user_id=None/project_id=None` 与调用方指定的 backend/root 直接送入 Relay；Cloud 不校验 backend ownership/Project access，Local 只校验物理 root。 |
| E-14 | P1 | retirement / persistence / process lifecycle | reachable | Project delete 会删除 Lifecycle run 并级联 Product runtime binding/terminal projection，但不先关闭 Host route/dynamic tool lease，也不处理 concrete Agent source/effect 保留或删除，形成“Product 已退休、execution 事实仍活”的生命周期缺口。 |
| E-15 | P1 | durable object / derived cache / host lifecycle | reachable | Project delete 只删 Extension artifact metadata；Cloud archive storage 没有 delete port。本机 digest cache 可重建但无 GC，Extension Host 又只有 whole-process stop 而无 project/install-scoped deactivation，三个生命周期没有共同 revoke coordinate。 |

### 2. Production use-case coverage ledger

记号：`E`=入口，`A`=授权，`O`=command/read/write owner，`T`=事务/副作用，`C`=合同/消费者，`R`=断线/重启/重放，`G`=现有门禁，`V`=判定。

#### 2.1 API 启动、数据库与 subsystem composition

- `E`：`agentdash-server serve|migrate|doctor`；Desktop sidecar 也复用 API server build。
- `A`：启动阶段无用户 actor；环境与 deployment 是 operator authority。
- `O`：`PostgresRuntime` 解析连接，migration owner 执行 `0001_init.sql`，`AppState::new_with_integrations` 是 Cloud composition root（`crates/agentdash-api/src/lib.rs:180-212`、`:221-255`；`crates/agentdash-api/src/app_state.rs:242-823`）。
- `T`：`serve` 只 `CheckReady`；`migrate` 才 `RunMigrations`；schema guard 同时要求 57 张 final table、`lifecycle_agents.frames/runtime_binding`，并拒绝 retired table（`crates/agentdash-infrastructure/src/migration.rs:4-175`、`:186-190`）。
- `C`：HTTP router、MCP router、Relay registry、Runtime Host、workers、Desktop sidecar。
- `R`：AppState 构建失败即不 bind listener；integration contract failure fatal，optional materialization failure隔离（`crates/agentdash-api/src/app_state.rs:397-434`）。
- `G`：schema readiness/migration tests 与 AppState optional/fatal classification unit tests；缺 subsystem `freeze()` 和 required-binding negative composition test。
- `V`：migration 与 listener 顺序是合理 composition；deferred service 与 Cron 提前发布是 E-09。

#### 2.2 Project AgentRun 创建、Product launch 与命令

- `E`：Project Agent start HTTP；workflow/routine/companion 的 Agent launch；lifecycle runtime composer/fork/fork-submit/cancel 与 command endpoints。
- `A`：HTTP 入口以 Project `Use` 为统一边界；lifecycle runtime snapshot/context/live/command/terminal routes 都先验证 run-agent-project target（`crates/agentdash-api/src/routes/lifecycle_agents.rs:51-105`、`:614-809`、`:821+`）。
- `O`：`ProjectAgentRunStartService` 生成稳定 run/agent/frame/runtime identity；Lifecycle owner 写 graph；`AgentRunProductLaunchService` 创建 concrete Agent source并提交 Product binding；`AgentRunProductCommandFacade` 将意图转交 concrete Agent（`crates/agentdash-application/src/project_agent_run_start.rs:102-187`、`:197-252`；`crates/agentdash-application-agentrun/src/agent_run/product_launch.rs:142-200`）。
- `T`：Lifecycle materialization、Agent source、Product binding 顺序提交；Agent source/effect 自己在 PostgreSQL transaction 内原子提交，Product binding 是 `lifecycle_agents.runtime_binding` 的 conditional update；没有跨 owner DB transaction，稳定 identity 支持 caller replay。
- `C`：HTTP receipt、Product managed snapshot/live stream、workflow/routine、frontend AgentRun feed。
- `R`：Product resolver 在 Host 为空时从 immutable frame + committed binding 恢复 route/generation（`crates/agentdash-infrastructure/src/complete_agent_product_provisioning.rs:260-336`）；command facade 先 inspect stable effect，再 replay Accepted/Applied。
- `G`：stable start identity、restart persistence、command replay、Host generation/source/surface tests；缺“进程在 source create 与 binding commit 之间退出”的自动恢复/E2E。
- `V`：Product/concrete Agent 分权是必要内聚；跨步骤 launch recovery 为 E-10 门禁缺口。

#### 2.3 AgentRun query/live/terminal snapshot

- `E`：runtime snapshot、context、live、workspace、terminal snapshot/changes。
- `A`：Project `Use` + exact run/agent membership。
- `O`：Product projection gateway 先读 committed binding，再向 concrete Agent read/inspect；terminal projection单独读 Product terminal store（`crates/agentdash-application-agentrun/src/agent_run/product_projection_gateway.rs:156-340`）。
- `T`：query 不写；live 首帧 snapshot，随后 ephemeral Agent events；terminal change 由 projection UoW 持久化 sequence/revision。
- `C`：generated lifecycle contracts、NDJSON live consumer、frontend managed runtime feed。
- `R`：Host route可按 binding恢复；Product presentation 对 runtime unavailable 可返回 `None`，但 durable binding不丢失。
- `G`：API route shape tests、projection digest/restart tests、frontend live tests；跨 transport occurrence coordinate 的前端风险由 `frontend-crosslayer-coupling.md` 负责。
- `V`：owner 分离合理；API workspace helper直接拼 repository/query 是较低等级 adapter 厚度，不单独升级。

#### 2.4 Runtime tool / VFS / shell

- `E`：Complete Agent tool callback；platform runtime catalog；VFS HTTP query/mutation/surface；Local file/shell/materialization relay commands。
- `A`：Product runtime tool authorizer核验 runtime thread、generation、source、profile digest、bound/applied surface revision/digest与 exact resource grant（`crates/agentdash-infrastructure/src/runtime_tool_authorization.rs:64-172`）。
- `O`：Product/VFS application services拥有业务操作；`PlatformToolBroker` 选择 executor；Local ToolExecutor/ShellSessionManager拥有物理文件与进程。
- `T`：文件/shell是外部副作用；shell以 session/event registry承载增量输出；VFS mount policy在执行前校验所有 path。
- `C`：AgentDash-owned `RuntimeToolDefinition/Invocation/Result`；Relay typed file/shell envelopes；Agent item/tool presentation。
- `R`：Local shell session在连接期间存在；Relay timeout为 30s，断线后物理 shell/terminal由各自 owner inventory/reconcile；动态 executor 生命周期例外见 E-02。
- `G`：authorization negative tests、path traversal/patch scope tests、tool catalog isolation tests；缺 rebind/credential revoke/unbind lifecycle tests。
- `V`：authorization与 VFS canonicalization 是合理强边界；VFS application 对 Relay live registry/concrete Agent tool type 的依赖方向问题已由 `backend-global-coupling.md` B-07 收录。

#### 2.5 Hook compile、surface negotiation 与 callback

- `E`：AgentFrame hook plan编译；Complete Agent surface create/rebind；Native Agent before/after tool callbacks。
- `A`：hook definition来自 pinned Product frame；callback由 Host generation/source/surface/deadline fence。
- `O`：`AppExecutionHookProvider` 解释 Product rule；provisioner把 requirement投影为 Agent-owned surface；Driver只执行已协商 action。
- `T`：blocking/rewrite应同步 exact；effect必须由幂等 side-effect owner提交。
- `C`：`AgentFrameHookPlan` → `AgentSurfaceRequirement` → `AgentHookDecision`。
- `R`：callback超时/route mismatch typed reject；当前 unsupported action/composite decision 无可恢复转换。
- `G`：空 plan、单一 blocking/rewrite projection、decision conversion unit tests；缺“每个 production trigger × 每个注册 Driver profile”的composition matrix。
- `V`：E-03。`RuntimePlatformHookHandler` 未进入当前 AppState，production使用 `ProductCompleteAgentHookHandler`，前者只算非生产 helper。

#### 2.6 Interactive Terminal

- `E`：HTTP spawn/input/resize/kill；Relay terminal spawn/input/resize/kill/inventory；terminal output/state event；Agent live terminal snapshot/changes。
- `A`：HTTP使用 Project `Use`并解析 exact runtime backend anchor；后续控制从 Product terminal owner fence取 backend。
- `O`：Local拥有 PTY/process/source sequence；Cloud Product terminal store拥有 durable projection、availability与output tail；当前 API route拥有 spawn orchestration。
- `T`：Local spawn是外部副作用，之后才 `register_spawned` transaction（`crates/agentdash-api/src/routes/terminals.rs:86-203`；`crates/agentdash-api/src/relay/terminal_projection.rs:46-82`）。
- `C`：Relay terminal payload含 terminal id、owner epoch、source sequence、process id；Product projection含 owner fence与bounded output。
- `R`：disconnect只标 Offline，不推断 process lost；reconcile必须先找到 central projection，故无法发现“spawn成功、projection失败”的孤儿（`crates/agentdash-api/src/relay/terminal_reconcile.rs:44-86`）。
- `G`：response kind/error unit tests与projection UoW tests；缺 failure injection after-local-spawn、duplicate spawn idempotency、orphan inventory adoption/kill E2E。
- `V`：E-04。Local/Cloud事实分权本身合理，缺的是跨 owner command intent与恢复 owner。

#### 2.7 MCP HTTP transport 与 19 个静态 MCP tools

- `E`：`/mcp/health`、`POST /mcp/relay`、`/mcp/story/{story_id}`、`/mcp/workflow/{project_id}`；Relay tools 7 个：`list_projects/get_project/create_story/list_stories/get_story_detail/update_story_status/update_project_context_config`；Story tools 7 个：`get_story_context/update_story_context/update_story_details/create_task/batch_create_tasks/list_tasks/advance_story_status`；Workflow tools 5 个：`list_workflows/get_workflow/get_lifecycle/upsert_workflow_tool/upsert_lifecycle_tool`。
- `A`：整个 MCP router先经 API authenticate middleware；各 server再用 `ProjectAuthorizationService` 做 Project permission（`crates/agentdash-api/src/routes.rs:55-71`；`crates/agentdash-mcp/src/authz.rs:25-52`）。
- `O`：MCP server是 protocol adapter，读写仍落到对应 repository/application owner；本研究未发现 MCP 自建第二份 execution/runtime状态。
- `T`：Story/Workflow mutation沿各自业务写路径；MCP Streamable HTTP session由 rmcp LocalSessionManager拥有。
- `C`：MCP Streamable HTTP/JSON-RPC；tool result为 rmcp `CallToolResult`。
- `R`：HTTP session重连由 MCP protocol manager处理；业务重放取决于各 tool command identity，静态 MCP mutation并未统一暴露 client operation id，跨业务事务由另外两个研究面审计。
- `G`：MCP authz、server tool unit/contract tests；缺 route/tool registration completeness manifest。
- `V`：transport与项目授权是合理内聚；runtime dynamic MCP的 direct/relay分叉见 E-02/E-06。

#### 2.8 Project NDJSON、Agent live 与 Backend WebSocket

- `E`：`/api/events/stream/ndjson`、Agent runtime live、`/ws/backend`。
- `A`：Project NDJSON与Agent live使用 bearer/session identity + Project `Use`；Backend WS使用 backend credential。
- `O`：`state_changes` 是 Project durable replay owner；broadcast只负责 runtime/control-plane invalidation；Agent live首帧由Product snapshot owner产生；BackendRegistry拥有在线连接/pending request。
- `T`：NDJSON poll durable changes并混入非 durable backend/control-plane通知；BackendRegistry每个 command使用 oneshot + timeout；disconnect顺序写 lease lost、Runtime Wire lost、registry unregister、runtime health offline与terminal Offline（`crates/agentdash-api/src/stream.rs:33-170`；`crates/agentdash-api/src/relay/ws_handler.rs:489-596`）。
- `C`：ProjectEventStreamEnvelope、AgentLiveEvent、RelayMessage/RuntimeWireFrame。
- `R`：Project stream用 cursor补发 state_changes；Agent live需重新取 snapshot；Backend pending在 unregister时drop，caller得到 response dropped/timeout；Runtime Wire有独立 ack/lost。
- `G`：stream cursor/relay registry timeout/backend anchor/Runtime Wire tests；缺 backend token header gate与 Local inline-stall control-plane test。
- `V`：durable replay与live availability分层合理；query credential与Local HOL分别为 E-07/E-05。

#### 2.9 29 类 production Local Relay command

- `E`：Workspace 4：`workspace_detect/workspace_detect_git/workspace_discover_by_identity/browse_directory`；文件 8：`file_read/file_read_binary/file_write/file_delete/file_rename/file_list/search/apply_patch`；Shell 4：`shell_exec/shell_read/shell_input/shell_terminate`；VFS 1：`materialize`；MCP 4：`probe/list_tools/call_tool/close`；Extension 3：`action/protocol/backend_service invoke`；Terminal 5：`spawn/input/resize/kill/inventory`；另有 Ping/Pong。
- `A`：多数 execution 路径由 Cloud 先依据 Project/Product/VFS/backend anchor 决定目标，Local 再以 configured workspace roots、protect mode、extension permission guard 校验物理执行；`/workspaces/detect-git` 是明确例外，它只有 outer authentication，没有 actor/backend access admission（E-13）。
- `O`：`LocalCommandRouter`只分发 envelope；Workspace/Tool/Materialization/MCP/Extension/Terminal handler分别拥有本机 adapter逻辑（`crates/agentdash-local/src/handlers/mod.rs:62-273`）。
- `T`：filesystem/process/network/extension host均为本机副作用；event/output通过 outbound channel回传。
- `C`：`agentdash-relay::RelayMessage` typed payload。
- `R`：WS session断开后整体重连；shell/terminal有独立 session/inventory；普通 file/MCP/extension command依赖 Cloud request timeout与caller retry。
- `G`：handler局部测试、path/root/extension permission tests；缺“所有 Relay Command variant 必须被 production handler分类”的exhaustive gate，以及控制面不被慢命令阻塞的测试。
- `V`：命令域拆分合理；执行调度为 E-05；`CommandDiscover/CommandDiscoverOptions` 为 E-11。

#### 2.10 Runtime Wire Remote Complete Agent

- `E`：offer advertise/withdraw；placement open/open ack/reject；placement frame/ack/closed/lost。
- `A`：Backend WS credential先建立 authenticated transport；remote contribution还经 pinned verification/trust/admission catalog。
- `O`：Wire拥有 frame/sequence/ack；Cloud admission拥有可信 offer与placement；Complete Agent Host拥有 runtime binding generation；Product仍拥有 run/agent association。
- `T`：连接/placement是 process-local；Agent durable source/effect由实际 Agent service owner持久化。
- `C`：`agentdash-agent-runtime-wire` schema覆盖 service/callback request/response和Complete Agent公开操作。
- `R`：disconnect exact placement lost，不删除Product binding；重连 advertisement重新注册服务，后续Product resolver恢复route。
- `G`：wire schema closure、advertisement/admission、disconnect/lost与generation tests。
- `V`：高连接但合理的 transport/composition boundary；未发现 Relay解释Agent业务状态。

#### 2.11 25 个 Tauri IPC command

- `E`：Codex OAuth 2：`codex_oauth_start/cancel`；Desktop设置/系统 5：`desktop_settings_load/save`、`desktop_autostart_is_enabled/set_enabled`、`desktop_quit_request`；Profile 3：`profile_load/save/delete`；Runtime 4：`runtime_start/stop/restart/snapshot`；MCP 3：`mcp_servers_load/save`、`mcp_server_probe`；日志 2：`logs_tail/clear`；平台 6：`desktop_api_snapshot`、`desktop_browse_directory`、`desktop_update_policy_snapshot/refresh`、`desktop_update_install`、`open_external_url`（`crates/agentdash-local-tauri/src/main.rs:378-404`）。
- `A`：Tauri renderer是本机受信调用方；Cloud请求仍携带access token；更新期间 mutation/runtime命令由update policy guard；external URL只允许http/https（`crates/agentdash-local-tauri/src/main.rs:48-151`、`:181-297`）。
- `O`：`DesktopRunnerHost`拥有runner lifecycle/log snapshot；settings/profile owner在`agentdash-local`；DesktopApiManager拥有sidecar child；OAuth owner持有本机callback listener/PKCE flow；update manager拥有update policy。
- `T`：配置文件、autostart OS设置、runner/sidecar进程、MCP probe、browser/update均是本机副作用。
- `C`：Rust command DTO与TS client目前手工镜像；该跨层漂移已由`frontend-crosslayer-coupling.md` F6定为P1。
- `R`：DesktopRunnerHost `ensure_started/restart/stop`；sidecar health poll 120s；OAuth 5分钟timeout/cancel；app exit停止sidecar。
- `G`：Rust manager/unit tests与少量frontend adapter tests；缺25-command manifest parity、runtime decoder与packaged invoke smoke。
- `V`：Tauri只做本机平台组合是合理边界；generated IPC contract整改复用F6，不重复建立第二个owner。

#### 2.12 Background/startup

- `E`：auth session cleanup loop；workflow recovery每秒；Cron scheduler每5秒；Desktop runner auto-start；Desktop API sidecar/update startup；Runtime Wire remote admission task。
- `A`：系统actor；每个worker只应调用对应application recovery/command owner。
- `O`：auth service、workflow recovery repository/orchestrator、RoutineExecutor、DesktopRunnerHost各自拥有状态。
- `T`：workflow/routine依赖durable effect/identity；cleanup删除过期session；Desktop启动本机进程。
- `C`：内部typed command/recovery port；无通用event bus。
- `R`：post-AppState workers在完整state后启动（`crates/agentdash-api/src/bootstrap/background_workers.rs:10-81`）；Cron例外在AppState中段启动（`crates/agentdash-api/src/app_state.rs:556-569`）。
- `G`：workflow/routine recovery局部测试；缺worker inventory/composition freeze与shutdown lifecycle gate。
- `V`：各worker按业务owner拆分合理；Cron/deferred发布顺序为E-09。

#### 2.13 Execution profile、release、health、diagnostics

- `E`：`/api/agents/discovery`、`/api/agents/discovered-options/stream`；public `/api/version`、release discovery/desktop update manifest、`/.well-known/agentdash`、`/api/health`、`/api/diagnostics`。
- `A`：execution profile secured；release/version/health/diagnostics public。
- `O`：Provider DB catalog与Complete Agent live catalog共同生成execution options；release handler拥有manifest fetch/cache；health只报告process/version；DiagnosticBuffer拥有进程内ring。
- `T`：profile/release query不写Product；release可做外部HTTP fetch/cache；diagnostics读取内存。
- `C`：execution options NDJSON是一次性三行 `Ready/JsonPatch/finished`；release/health JSON；diagnostics返回原始 `DiagnosticRecord`。
- `R`：release cache支持临时远端失败；diagnostics重启清空；health无dependency状态。
- `G`：profile builtin/Codex unit tests、release cache tests；diagnostics没有auth/redaction negative test，profile没有第三方integration composition test。
- `V`：release与process health是合理平台composition；E-08、E-01、E-12为具体缺口。

### 3. 代表性纵向链路

#### 3.1 AgentRun 创建 → concrete Agent → Product projection

1. HTTP/Workflow/Routine生成actor、Project scope与stable operation identity。
2. Lifecycle owner写run/agent/frame/subject/gate/lineage；frame是immutable launch facts。
3. Product launch provisioner依据execution profile、VFS/MCP/hook facts编译desired surface。
4. Complete Agent Host选择已验证live service，concrete Agent创建durable source并返回receipt/association。
5. Product只把association写入`lifecycle_agents.runtime_binding`。
6. 后续command facade读binding → inspect stable Agent effect → execute/replay。
7. query gateway读binding → Agent read/inspect → Product managed projection；live只追加ephemeral observation。

Canonical owner分别是Lifecycle graph、concrete Agent source/effect、Product association；Host只拥有process-local route/generation。该分权应保留。变化爆炸主要来自surface compile时动态executor/hook合同没有独立生命周期，而不是Product facade过窄。

#### 3.2 Runtime tool → Product authorization → Local physical effect

1. Driver callback携带runtime thread/generation/source/surface/effect identity。
2. Host先做route/deadline fence。
3. Broker按runtime catalog选executor。
4. Product authorizer从committed binding + applied resource surface解析permission/effect/mount scope。
5. VFS/MCP executor使用Relay backend anchor投递Local。
6. Local再次验证workspace root/protect mode/permission并执行物理副作用。
7. typed result/event回到Agent item projection。

双层校验是合理的：Cloud决定“该Product是否获准”，Local决定“本机资源是否真实可执行”。E-02破坏的是第3步executor lease与第4步新surface之间的一致性。

#### 3.3 Terminal spawn → Product projection → disconnect/reconcile

当前链路是API route解析Product runtime surface → Relay spawn PTY → Local返回owner epoch/source seq → API再读Product snapshot → terminal UoW注册projection → output/state event更新projection。断线只标Offline，重连inventory按已存在projection恢复。缺口是Relay spawn与首次projection之间没有durable intent，因此该窗口内的Local terminal没有Cloud可查询coordinate。

#### 3.4 Remote Complete Agent Runtime Wire

Local advertisement → Cloud verification/admission → service contribution注册Host → Product provisioner选择live attachment → Runtime Wire placement open/frame/ack → Host generation-fenced callback → Product facade/projection。Relay断线先将placement lost，再撤销live selection；Product binding不重写。该链路符合“transport availability不成为Product事实”的spec。

### 4. 高风险发现与可执行整改包

#### E-01 — P0：public diagnostics暴露未经授权、未经字段脱敏的内部诊断事实

- 生产证据：`routes::create_router`在secured API之外merge diagnostics（`crates/agentdash-api/src/routes.rs:115-125`）；模块注释明确为认证异常时仍可访问的public入口（`crates/agentdash-api/src/routes/diagnostics.rs:1-8`）。
- 数据证据：response直接返回`DiagnosticRecord`，包含message、target、任意`fields`、session/run/backend id（`crates/agentdash-diagnostics/src/record.rs:9-32`）；`FieldVisitor`对未知字段与Debug error原样收集，没有key/value redaction（`crates/agentdash-diagnostics/src/layer.rs:158-218`、`:252-289`）。
- 触发器：任一subsystem新增带路径、用户输入、endpoint、error chain或凭据片段的`diag!`；无需修改diagnostics API。
- 爆炸半径：所有诊断生产者、所有部署、任意未认证网络调用方；可能跨Project看到run/backend/session标识和本机路径。
- 目标边界：public只保留常量liveness/correlation ticket；完整diagnostics必须进入authenticated operator capability，并在DiagnosticRecord构造前执行central redaction/schema allowlist。
- Work package `secure-operational-diagnostics`：新增`OperationalDiagnosticsQuery` application port与operator permission；把`/api/diagnostics`移入secured/operator router；DiagnosticLayer只接受owned safe fields或统一redactor；为public troubleshooting返回无业务字段的typed状态。
- Negative guard：匿名请求必为401/403；注入`authorization/token/database_url/path/user_input`字段后response/log snapshot不含secret；不同Project actor不能按猜测run/backend id查询。

#### E-02 — P0：dynamic runtime tool catalog把tool definition误当executor/credential lease identity

- 生产证据：surface create和rebind每次都会基于最新MCP server、VFS backend anchor、HTTP headers解析新executor，然后调用`bind_runtime_catalog`（`crates/agentdash-infrastructure/src/complete_agent_product_provisioning.rs:559-577`、`:642-706`）。
- 根因证据：Broker以`RuntimeThreadId`为唯一key；已有catalog若definitions相等就直接返回，新的executor被丢弃；definitions变化则报DuplicateTool（`crates/agentdash-agent-runtime/src/platform_tool_broker.rs:264-303`）。仓库没有unbind/remove路径。
- 触发器：MCP credential/header、URL、relay backend anchor、workspace/VFS context变化但tool schema/name不变；或surface rebind增删/修改tool。
- 爆炸半径：该runtime thread全部dynamic MCP calls；旧client/secret/backend anchor存活到server进程结束；changed definitions使正常hot rebind直接失败。
- 目标边界：以`runtime_thread + binding generation + applied surface digest`建立immutable `RuntimeToolCatalogLease`；Host prepare阶段构建新catalog，surface apply与catalog swap使用同一prepared operation，旧generation立即不可调用并显式close/cancel client；run close/delete释放lease。
- Work package `generation-scoped-runtime-tool-catalog`：扩展Broker为prepare/activate/revoke API；dynamic catalog返回definition + executor identity/close handle；provisioner在Host apply成功时原子切换active lease；删除definition-equality replay捷径与永久thread cache。
- Negative guard：同definition不同header/endpoint必须命中新executor；credential revoke后旧secret无法发起请求；changed definition rebind成功且旧generation callback拒绝；run delete后catalog/client计数归零。

#### E-03 — P1：Hook plan声明能力上界，surface/Driver却要求每条rule都精确支持全部action

- 生产证据：每条实际rule都由`actions_for_trigger`获得一组“可能action”，且`required=true`（`crates/agentdash-application-hooks/src/plan.rs:36-93`）。BeforeTool包含RequestApproval/RefreshSurface/EmitEffect，AfterTool包含RefreshSurface/EmitEffect，AfterTurn/BeforeStop包含ContinueTurn等（`:131-179`）。
- 适配证据：Complete Agent projection遇到RequestApproval/ContinueTurn/RefreshSurface即返回None并把required rule判为incompatible（`crates/agentdash-infrastructure/src/complete_agent_product_provisioning.rs:1245-1272`、`:1348-1363`）。
- Driver证据：Native profile只声明BeforeTool block+rewrite input、AfterTool block+rewrite result（`crates/agentdash-integration-native-agent/src/service.rs:297-342`）；callback只能消费Allow/Deny/ReplaceInput或Allow/ReplaceResult（`crates/agentdash-integration-native-agent/src/core_callbacks.rs:163-203`、`:270-314`）。
- 第二合同裂缝：Product handler可同时产生block/rewrite/injection/effect，但`AgentHookDecision`只允许一个，多个同时出现返回unsupported（`crates/agentdash-infrastructure/src/complete_agent_product_hook_handler.rs:115-171`）。
- 触发器：为AgentFrame配置常规BeforeTool/AfterTool/AfterTurn hook，或脚本同时输出两个合法Product语义。
- 爆炸半径：所有Hook-enabled Complete Agent create/rebind；Driver profile扩展必须同时修改plan compiler、surface projector、handler与每个integration。
- 目标边界：Hook definition在preflight后产生“该rule真实会发出的typed outcome contract”，而不是trigger action上界；callback使用可组合的`HookOutcome`，或在Product rule schema中静态限制为单一decision；Driver negotiation逐facet声明exact支持。
- Work package `compile-exact-hook-outcome-contracts`：调整rule schema/compiler；删除`actions_for_trigger`宽集合；统一Product resolution→Agent callback合同；为unsupported语义在配置/compile阶段给出pathful diagnostic，不在运行中猜。
- Negative guard：表驱动覆盖所有WorkflowHookTrigger × builtin/remote Driver profile；BeforeTool block-only和rewrite-only可运行；unsupported approval/continue/refresh在frame preflight失败；composite outcome要么typed执行，要么静态拒绝。

#### E-04 — P1：Terminal外部spawn与Product projection之间没有semantic commit/recovery owner

- 生产证据：API route自己检查backend、生成terminal id、发Relay spawn，成功后再读runtime snapshot并`register_spawned`（`crates/agentdash-api/src/routes/terminals.rs:86-203`）。
- 恢复证据：reconcile先从central projection查terminal，否则Conflict，不会做backend-wide orphan discovery（`crates/agentdash-api/src/relay/terminal_reconcile.rs:44-86`）。
- 触发器：Local spawn成功后，Product snapshot读取、binding校验、JSON/ID转换或terminal UoW commit失败；Cloud在两步间退出。
- 爆炸半径：本机残留PTY/process与资源；UI/API看不到、不能kill；重连inventory也没有central cursor可采用。
- 目标边界：`AgentRunTerminalCommandService`拥有stable command id与durable launch intent。先提交reservation/intent，再向Local幂等spawn，最后提交receipt/projection；recovery worker按intent查询inventory并adopt或kill。
- Work package `terminal-semantic-launch-commit`：把route orchestration下沉application；Relay spawn contract加入operation id与idempotent replay；terminal store持有launch phase；reconcile支持按intent/backend owner inventory。
- Negative guard：在spawn后注入DB失败，重启后必须adopt或kill；同operation重复spawn只产生一个PTY；API handler不再直接引用BackendRegistry/Postgres projection producer。

#### E-05 — P1：Local单WS把控制面与慢命令放在同一inline执行lane

- 生产证据：注释声称read不直接执行命令，但只有`Background`分支spawn；Inline直接await handler（`crates/agentdash-local/src/ws_client.rs:401-445`）。
- 分类证据：MCP只有probe是Background，list/call/close是Inline；extension三类、terminal五类、materialization与多数file/workspace命令也是Inline（`crates/agentdash-local/src/handlers/mod.rs:266-273`及各handler `dispatch_plan`）。
- 阻塞证据：MCP call直接await client，无timeout（`crates/agentdash-local/src/mcp_client_manager.rs:158-205`）；extension activation/invoke可启动进程、下载artifact或等待外部HTTP。
- 资源证据：Background每条命令裸`tokio::spawn`，无semaphore/queue容量（`crates/agentdash-local/src/ws_client.rs:419-435`）。
- 触发器：MCP server/extension hang、慢磁盘/网络、大materialization；或Cloud并发发送大量shell/probe。
- 爆炸半径：Ping/Pong、Runtime Wire ack/frame、terminal input、能力变化事件、所有后续command；可能触发Cloud误判断线并丢placement。
- 目标边界：WS reader只decode/classify/enqueue；control/runtime-wire独立高优先级lane；各domain有bounded worker queue、timeout/cancellation和typed overload；单writer保序。
- Work package `local-relay-bounded-dispatch`：替换Inline/Background二元模型为lane manifest；所有可能I/O命令进入bounded executor；定义per-domain并发与command deadline；session shutdown取消workers并清理responses。
- Negative guard：挂起MCP/extension时Ping、RuntimeWireAck、terminal input仍在deadline内；超过队列容量返回typed busy；并发峰值task数有上限；断线后无orphan worker继续使用旧outbound channel。

#### E-06 — P1：direct与relay MCP破坏同一RuntimeTool output合同

- 生产证据：direct executor序列化完整rmcp result（`crates/agentdash-infrastructure/src/mcp/runtime_tool_catalog.rs:145-168`）；relay executor只返回`{"content": string}`（`:181-213`）。
- 数据丢失源：Local把每个content `render_content`后join为String，只保留`is_error`（`crates/agentdash-local/src/mcp_client_manager.rs:158-205`；`crates/agentdash-relay/src/protocol/mcp.rs:91+`）。
- 触发器：同一MCP preset从direct切到relay，或tool返回image/resource/embedded resource/structured content/annotations。
- 爆炸半径：Agent/tool consumer、model-visible结果、integration测试；placement变化变成业务shape变化。
- 目标边界：AgentDash-owned `RuntimeMcpCallResult` lossless envelope，direct/relay都从rmcp投影一次；renderer/text fallback只在最终model projector。
- Work package `lossless-runtime-mcp-result`：扩展Relay protocol/result与Local mapper；direct也投影到同一owned type；删除joined-string canonical result。
- Negative guard：同一fixture经direct/relay得到字节等价owned result；image/resource/structured/error metadata不丢；Rust schema与wire golden同步。

#### E-07 — P1：Backend credential进入WebSocket URL query

- 生产证据：Local直接`format!("{}?token={}", cloud_url, token)`再`connect_async`（`crates/agentdash-local/src/ws_client.rs:157-177`）；Cloud从Query提取token并授权（`crates/agentdash-api/src/relay/ws_handler.rs:28-45`）。
- 触发器：任何reverse proxy/access log、HTTP trace、错误采集、URL telemetry或诊断dump记录request URI。
- 爆炸半径：backend credential可被重放建立具有本机文件/process/extension能力的连接；影响该backend可访问的所有Project。
- 目标边界：Local使用`Authorization: Bearer`或AgentDash-owned一次性WS handshake header；Cloud拒绝query credential；日志层统一redact auth header。
- Work package `relay-header-authentication`：修改Local request builder与Cloud extractor；runner claim仍只返回credential本体和无secret relay URL；删除query parser/token拼接。
- Negative guard：`?token=`必拒绝；header token成功；trace/access-log fixture不含credential；invalid/expired/backend-id mismatch仍typed reject。

#### E-08 — P1：Host integration catalog与产品execution profile catalog分裂

- 生产证据：integration registration可收集任意Complete Agent definition/instance并注册Host（`crates/agentdash-api/src/integrations.rs:103-169`；`crates/agentdash-api/src/app_state.rs:397-434`）。
- 分裂证据：execution profile只识别`PI_AGENT`和`CODEX`，discovery只返回两项（`crates/agentdash-api/src/routes/execution_profiles.rs:20-49`、`:91-117`），start validation也调用该硬编码函数。
- 触发器：新增enterprise/remote Complete Agent integration或调整instance/profile mapping。
- 爆炸半径：integration已成功materialize且Host可用，但UI/API无法发现、选择或启动；每个新integration都必须修改API route。
- 目标边界：application层`ExecutionProfileCatalogQuery`从final frozen Host definition/live catalog + Product provider catalog投影typed profile/options；route只做auth/DTO。
- Work package `unified-execution-profile-catalog`：给integration contribution声明Product-selectable profile metadata；AppState freeze后构建catalog handle；start/discovery/options共用。
- Negative guard：测试integration注册第三个profile后discovery/options/start全部可达；disabled/unhealthy profile仍可见但不可启动；删除任一registration line使composition test失败。

#### E-09 — P1：Deferred Product tools与Cron提前发布缺少freeze门禁

- 生产证据：AppState先创建并注册6个deferred Product tool到最终Broker（`crates/agentdash-api/src/app_state.rs:320-378`），Cron在`:563-569`启动；workspace/companion/lifecycle service直到`:654-752`才install，AppState在`:761-823`才完整并启动其他workers。
- 执行证据：deferred service是OnceLock，未install时runtime invocation返回`product_runtime_tool_not_installed`（`crates/agentdash-infrastructure/src/runtime_tool_executors.rs:109-160`）。
- 触发器：已有scheduled routine在启动后首个5秒tick触发，而后续composition因慢I/O/大catalog/未来初始化变慢尚未完成；或遗漏一条install线但server仍构建。
- 爆炸半径：routine-triggered AgentRun、companion/workspace module/lifecycle tool；行为取决于隐式行序和机器时序。
- 目标边界：按subsystem builder构建所有真实service/executor，`freeze()`校验required binding后才发布Broker、Cron、routes/workers；不保留deferred compatibility layer。
- Work package `freeze-runtime-subsystem-composition`：把Runtime工具、workflow、companion、workspace module、Cron组装成typed builder；删除DeferredProductRuntimeToolService；所有worker统一在frozen AppState后启动。
- Negative guard：缺任一required executor时build失败；Cron在freeze前不可获得handle；构造完成后catalog中无deferred/uninstalled状态。

#### E-10 — P2：稳定identity支持caller重试，但未证明partial launch会自动收敛

- 证据：Project start先完成Lifecycle graph，再调用Product launch（`crates/agentdash-application/src/project_agent_run_start.rs:142-252`）；Product launch先create concrete source再commit binding（`crates/agentdash-application-agentrun/src/agent_run/product_launch.rs:142-200`）。
- 已有缓解：所有run/agent/frame/runtime id从client command稳定派生；concrete Agent effect与Product binding commit均幂等。相同caller retry可继续完成。
- Evidence gap：未找到pending launch intent/worker，也未找到进程在三个提交点退出后的E2E；不能据此断言当前会重复Agent source或竞争owner。
- 建议：在E-09 composition freeze之后增加failure-injection characterization；若client retry已由所有入口可靠保证，只补gate；若routine/workflow/HTTP存在无caller retry入口，再建立轻量durable launch phase owner，不能恢复已retired的第二套Runtime状态机。

#### E-13 — P0：无 Project scope 的 Git 探测可跨用户选择任意在线 backend/root

- 生产入口：`/workspaces/detect-git` 挂在 secured API，但 handler 不提取 `CurrentUser`，request 只含调用方提供的 `backend_id/root_ref`（`crates/agentdash-api/src/routes/workspaces.rs:80-103`、`:309-327`）。
- Admission 证据：route 构造 `RuntimeActor::PlatformUser { user_id: None }` 与 `RuntimeContext::Setup { project_id: None, workspace_id: None, backend_id, root_ref }`（`crates/agentdash-api/src/routes/workspaces.rs:642-665`）；setup port 只检查 backend 在线和输入非空，然后调用 transport（`crates/agentdash-api/src/bootstrap/extension_gateway.rs:232-254`、`bootstrap/runtime_gateway.rs:1370-1392`）。
- Local 证据：`CommandWorkspaceDetectGit` 复用 workspace detect 并在本机读取指定 configured root 的 Git 事实；Local 物理 root 校验不能证明 Cloud actor 有权访问该 backend（`crates/agentdash-local/src/handlers/workspace.rs:67-95`）。
- 触发器：任一 authenticated user 猜到或观察到别人的 online backend id 与 root_ref。
- 爆炸半径：跨 Project/用户读取 source repo、branch、commit 与本机 workspace 身份；同一无 scope setup pattern 若复用到 write/process action 会扩大为副作用越权。
- 目标边界：若探测用于已有 Project，必须使用 Project `Use/Configure` + `ProjectBackendAccess`；若用于创建 Project 前 setup，则以真实 `RuntimeActor::PlatformUser { user_id }` 解析“该用户拥有/已领取的 backend capability”，不能接受裸 backend id 作为 authority。
- Work package `scope-workspace-setup-capabilities`：建立 `WorkspaceSetupCapabilityQuery`，route 只传 actor/intent；Gateway 先解析 actor 可用 backend/root capability，再发 Relay；删除 `user_id=None/project_id=None` production 路径。
- Negative guard：用户 A 对用户 B backend/root 返回 403 且 Local 不收到 command；offline 与 not-authorized 错误不同；Project-scoped 和 pre-Project-owned-backend 两种合法 fixture 通过。

#### E-14 — P1：Project retirement 删除 Product association，却没有 execution retirement owner

- 业务入口：Project delete 通过 `ManageSharing` 后调用 `delete_project_record`（`crates/agentdash-api/src/routes/projects.rs:181-198`）。
- Product persistence 证据：delete service 逐个删除 Lifecycle run（`crates/agentdash-application/src/project/management.rs:271-277`）；DDL 以 `lifecycle_agents.run_id ON DELETE CASCADE` 删除含 `runtime_binding` 的 Agent row，并继续级联 terminal projection/change/head（`crates/agentdash-infrastructure/migrations/0001_init.sql:1081-1097`）。
- Execution 缺口：delete service 没有调用 AgentRun Product retirement/Host close、Broker catalog revoke 或 concrete Agent delete/retention port；`dash_complete_source/effect` 没有 Project/Product FK，删除 Lifecycle graph 后仍存在。
- 触发器：删除包含 active/idle AgentRun、dynamic MCP client、interactive terminal 或 durable Dash Agent source 的 Project。
- 爆炸半径：Host route/client/credential handle 继续存活；Product binding 与 terminal control route 已消失；concrete source/effect 成为无 Product association 的 durable orphan。该项也放大 E-02 的永久 catalog 问题。
- 目标边界：Project retirement command 在删除 graph 前提交 typed retirement plan，逐 Agent 调用 `AgentRunRetirementPort`：fence 新 command、close/kill 或明确 retain 物理资源、revoke runtime tool lease、按 retention contract 删除 concrete source/effect；完成 receipt 后再以一个 semantic DB commit 删除 Project facts。
- Work package 归属：主 owner 应是业务资产研究面的 `project-semantic-retirement`；执行面提供 `AgentRunRetirementPort` 与 failure-injection suite，E-02 提供 catalog/client revoke。不要另建一套 Project delete orchestration。
- Negative guard：含 active Agent/terminal/MCP catalog 的 Project 删除后，Host target、Broker lease、Local terminal 与 concrete source 均达到声明的 retired/retained 终态；任一 external retirement 失败时 Project 仍处于可恢复 `retiring` 而非半删。

#### E-15 — P1：Extension durable archive、Local derived cache、Host activation 没有统一 revoke identity

- Durable object 证据：`ExtensionPackageArtifactStorage` 只有 write/read，没有 delete（`crates/agentdash-platform-spi/src/extension_package.rs:1-25`）；Filesystem adapter 同样只实现原子写与读（`crates/agentdash-infrastructure/src/storage/extension_package_artifact_fs.rs:62-96`）。
- Project delete 证据：只删除 `extension_package_artifact_repo` metadata 与 installation row（`crates/agentdash-application/src/project/management.rs:290-310`），未调用 archive storage。
- Derived cache 证据：Local cache identity 是 `artifact_id + archive_digest`，下载后保存 archive/unpacked/manifest；只在同 key 刷新时替换 unpacked 目录，没有全局/Project GC port（`crates/agentdash-local/src/extensions/artifact_cache.rs:9-107`、`:144-177`）。
- Host 证据：Local Extension Host manager 可 activate cached artifact，但只有 whole-process `stop()` 执行 deactivate/kill；没有 project/installation/artifact-scoped deactivate（`crates/agentdash-local/src/extensions/host/manager.rs:57-104`、`:187-223`）。
- 触发器：Project 删除、Extension uninstall、artifact revision 撤销或同 extension key 换 owner/revision。
- 爆炸半径：Cloud archive 永久孤儿；Local 磁盘 cache 无限保留；已激活代码/process 可继续占资源，并缺少“该 activation 是否仍由有效 installation 授权”的统一 coordinate。
- 目标边界：区分三类 owner 但共享 `ExtensionArtifactRevisionId(artifact_id,digest)` 与 installation/revocation receipt：Cloud metadata+archive 是 durable semantic delete；Local cache 是可重建、可按 ref/TTL GC；Host activation 是 process-local lease，installation revoke 必须显式 deactivate。
- Work package 归属：Cloud metadata+archive 删除纳入业务资产的 `project-semantic-retirement`/`extension-artifact-lifecycle` 子包；Local cache GC 与 Host deactivation 作为依赖其 revoke contract 的独立 `local-extension-runtime-lifecycle` 子包。两者属于同一父级 retirement graph，不应塞进同一个数据库 transaction 实现。
- Negative guard：metadata commit 失败不删除 archive；durable delete intent 重放最终清除 archive；uninstall/delete 后 Local 收到同 revision revoke 并停止 activation；仍被其他有效 installation 引用的 content-addressed cache 不被误删。

### 5. 合理但高度连接的边界

- `AppState`作为唯一Cloud composition root依赖很多concrete adapter本身合理；问题是它对routes公开全量`RepositorySet/ServiceSet`且没有subsystem freeze。整改应暴露窄handle，而不是机械把每十行拆成crate。
- Complete Agent Host、Service API、Runtime Wire、Native/Codex/Remote integration需要共同理解AgentDash-owneddescriptor/surface/generation是必要内聚；vendor DTO没有进入Product facade。
- Product binding存于`lifecycle_agents.runtime_binding`，concrete Dash Agent事实存于`dash_complete_source/effect`，两者不做跨owner FK/事务是有意分权；稳定source/effect identity与binding恢复负责收敛。
- Local对workspace root/protect mode/extension permission做第二层物理资源校验，不是Cloud Product authorization的重复owner。
- Terminal的Local PTY truth与Cloud Product projection分离合理；E-04只要求补launch semantic commit，不应把PTY状态迁到Cloud。
- `/api/health`作为Desktop “API listener/AppState ready”信号成立，因为listener在AppState完成后才bind；应避免将其命名/文档扩张为全部dependency health。
- Release/version/desktop manifest属于平台交付composition，放在API边缘合理；外部manifest fetch/cache可后续抽成service，但当前没有第二份业务事实。
- MCP HTTP transport统一经authenticate middleware且MCP server再做Project授权，属于协议层必要双检查。

### 6. Dead / unmounted / evidence gap

- `RelayMessage::CommandDiscover`与`CommandDiscoverOptions`只存在protocol和response kind映射；`LocalCommandRouter::handle`没有分支，仓库未找到producer。建议从final protocol删除并用exhaustive command manifest防复发。
- `RuntimePlatformHookHandler`没有进入当前AppState；production hook callback是`ProductCompleteAgentHookHandler`。不能用前者的实现证明production hook语义。
- Product hook `EmitEffect`路径在handler可编码，但builtin Native Driver不声明/消费；Remote Driver是否存在effect-capable profile取决于运行时advertisement。对其“重复副作用”风险只记证据缺口，不定为production bug。
- `ProjectAgentRunStart` partial failure可由相同client command retry收敛；仓库未证明所有HTTP/frontend/routine/workflow caller一定自动重试，也未证明会产生重复durable事实，故定P2而非P0/P1。
- `/api/health`后续DB/worker/integration failure不会反映；当前deployment是否有外部更强readiness probe未在本研究面找到，定为操作语义缺口。
- Tauri command已逐个核验Rust挂载和owner，但Rust/TS参数/结果逐字段parity由前端研究面取证；本研究复用其F6，不重复声称已做packaged E2E。

### 7. Work package依赖顺序

```text
WP-E0 secure-operational-diagnostics + relay-header-authentication
WP-E1 freeze-runtime-subsystem-composition
  ├─ WP-E2 generation-scoped-runtime-tool-catalog
  │    └─ WP-E5 lossless-runtime-mcp-result
  ├─ WP-E3 compile-exact-hook-outcome-contracts
  └─ WP-E6 unified-execution-profile-catalog
WP-E4 terminal-semantic-launch-commit
WP-E7 local-relay-bounded-dispatch
WP-E8 scope-workspace-setup-capabilities
WP-R  project-semantic-retirement
  ├─ AgentRunRetirementPort（复用 WP-E2 的 catalog revoke）
  ├─ extension-artifact-lifecycle
  └─ local-extension-runtime-lifecycle（消费 revoke contract）
WP-X  generated Tauri IPC contract（复用 frontend F6）
WP-G  production entrypoint/composition manifest + negative fixtures
```

执行原则：

- E0先消除跨认证/凭据泄露面，可独立完成。
- E1先建立frozen subsystem handle，E2/E3/E6才有稳定注册与activation落点。
- E2先统一dynamic executor lifecycle，E5再改变MCP result wire，避免同时保留两套catalog。
- E4独立移动Terminal command authority，先固定failure injection再切换API/Local consumer。
- E7不改变Relay业务payload，只改变Local调度/资源合同，可与E1并行。
- E8 是独立 authorization hard cut，不应等待 Project retirement。
- WP-R 由 Project 业务 owner 统一提交 retirement plan；AgentRun 与 Extension 提供窄 retirement adapter。durable archive 删除和 Local cache/Host 清理共享 revision/revoke contract，但分别以 Cloud durable effect 与 Local derived lifecycle 实现。
- 每个包完成时直接删除旧路径；项目未上线，不设计query-token/deferred/joined-string/route-orchestration兼容层。

### 8. Files found

- `.trellis/workflow.md` — planning/research流程与任务产物约束。
- `.trellis/tasks/07-26-module-coupling-stable-boundary-review/{prd,design,implement}.md` — 本次全仓审计目标、ledger字段、风险分级、work package要求。
- `.trellis/tasks/07-26-module-coupling-stable-boundary-review/research/backend-entrypoint-coverage-index.md` — HTTP/MCP/Runtime tool/Local/Tauri/worker/stream覆盖基线。
- `.trellis/spec/backend/agent-runtime/*.md` — Product facade、Host/Driver、persistence、surface/tool、native/kernel owner合同。
- `.trellis/spec/backend/{architecture,directory-structure,repository-pattern,database-guidelines,error-handling,quality-guidelines}.md` — backend依赖方向、事务、API与测试约束。
- `.trellis/spec/backend/capability/tool-capability-pipeline.md` — capability→surface→tool与MCP health投影。
- `.trellis/spec/backend/hooks/{architecture,execution,scripts}.md` — Hook owner、failure、effect语义。
- `.trellis/spec/backend/vfs/{architecture,access-scope,materialization}.md` — VFS address/policy/local materialization边界。
- `.trellis/spec/cross-layer/{architecture,runtime-wire-relay,desktop-local-runtime,deployment-runtime-backbone,frontend-backend-contract}.md` — Cloud/Local/Desktop/Relay/Wire分权。
- `crates/agentdash-api/src/{lib,main,app_state,routes,stream}.rs` — Server/migration/AppState/router/NDJSON production composition。
- `crates/agentdash-api/src/routes/{lifecycle_agents,terminals,execution_profiles,diagnostics,health,release_info}.rs` — execution/platform HTTP入口。
- `crates/agentdash-api/src/routes/{workspaces,projects}.rs` — 无 scope setup 探测与 Project retirement 入口。
- `crates/agentdash-api/src/relay/{ws_handler,registry,mcp_relay_impl,terminal_projection,terminal_reconcile,complete_agent_admission}.rs` — Cloud Relay、pending request、MCP、terminal与Remote Complete Agent。
- `crates/agentdash-application/src/{project_agent_run_start,scheduling/cron_scheduler}.rs` — stable Project Agent start与Cron worker。
- `crates/agentdash-application-agentrun/src/agent_run/{product_command_facade,product_launch,product_projection_gateway}.rs` — AgentRun Product command/launch/query owner。
- `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs` — Lifecycle graph materialization顺序。
- `crates/agentdash-agent-runtime/src/platform_tool_broker.rs` — static/dynamic tool registry与authorization入口。
- `crates/agentdash-agent-runtime-host/src/{complete_agent,complete_agent_callbacks,live_catalog}.rs` — Host binding/generation/callback/live service owner。
- `crates/agentdash-agent-service-api/src/**`、`crates/agentdash-agent-runtime-wire/src/**` — AgentDash-owned service与wire合同。
- `crates/agentdash-infrastructure/src/{complete_agent_product_provisioning,complete_agent_product_hook_handler,runtime_tool_authorization,runtime_tool_executors}.rs` — production Product→Host/Hook/Tool adapter与deferred实现。
- `crates/agentdash-infrastructure/src/mcp/runtime_tool_catalog.rs` — direct/relay dynamic MCP catalog与executor。
- `crates/agentdash-infrastructure/src/{agent_run_product_projection_repository,dash_complete_agent_store,migration}.rs` — Product binding/terminal、concrete Agent与schema persistence。
- `crates/agentdash-platform-spi/src/extension_package.rs`、`crates/agentdash-infrastructure/src/storage/extension_package_artifact_fs.rs` — Extension durable archive storage contract/adapter。
- `crates/agentdash-infrastructure/migrations/0001_init.sql` — final prelaunch PostgreSQL schema、unique/FK/check/index。
- `crates/agentdash-application-hooks/src/{plan,provider,rules,script_engine}.rs` — Product Hook plan/evaluation。
- `crates/agentdash-integration-native-agent/src/{service,core_callbacks}.rs` — builtin Native Complete Agent profile与callback能力。
- `crates/agentdash-mcp/src/{transport,authz,servers/**}.rs` — Streamable HTTP MCP与19个静态tools。
- `crates/agentdash-relay/src/protocol*.rs` — Cloud/Local typed Relay command/event合同。
- `crates/agentdash-local/src/{ws_client,mcp_client_manager,handlers/**,extensions/**}.rs` — Local连接、29 command、本机MCP/Extension/Terminal执行。
- `crates/agentdash-local-tauri/src/{main,runtime_host,desktop_api,codex_oauth,desktop_update,settings,state}.rs` — 25 Tauri commands与Desktop process/update/OAuth composition。
- `crates/agentdash-diagnostics/src/{record,layer}.rs` — DiagnosticRecord与无脱敏ring buffer。

### 9. Code patterns

- Stable facade pattern：Product association + stable client identity → Agent inspect/replay/execute，不复制concrete state（`product_command_facade.rs:65-223`）。
- Durable association recovery：committed binding + immutable frame → recompile exact surface → Host route/generation restore（`complete_agent_product_provisioning.rs:260-336`）。
- Callback fence：route id + generation + source + surface action + deadline（`complete_agent_callbacks.rs:90-275`）。
- Exact grant pattern：committed Product binding与applied surface双证据（`runtime_tool_authorization.rs:64-172`）。
- Semantic UoW pattern：terminal projection change/revision/source sequence一次transaction commit（`terminal_projection.rs:46-82`及Postgres store）。
- Temporal placeholder anti-pattern：Deferred OnceLock注册到final catalog，后续按源码顺序install（`app_state.rs:320-378`、`:654-752`）。
- Definition/handle conflation anti-pattern：definition相同即丢弃新dynamic executor（`platform_tool_broker.rs:268-303`）。
- Adapter orchestration anti-pattern：HTTP route先产生Local effect再提交Product fact（`routes/terminals.rs:86-203`）。
- Transport semantic drift：direct lossless JSON、relay joined text（`runtime_tool_catalog.rs:145-213`；`mcp_client_manager.rs:158-205`）。
- Control/data lane coupling：WS read loop inline await可能I/O handler（`ws_client.rs:401-445`）。
- Public arbitrary diagnostic pattern：unknown tracing fields原样进入anonymous response（`diagnostics.rs:59-68`；`diagnostics/layer.rs:158-218`）。
- Actorless setup anti-pattern：authenticated route 把 `PlatformUser { user_id: None }` 与裸 backend/root 当作 Relay admission（`routes/workspaces.rs:642-665`）。
- Split retirement anti-pattern：Product graph 级联删除，concrete Agent/object/cache/Host lease 不在 retirement plan 中（`project/management.rs:271-375`；`extension_package.rs:13-25`）。

### 10. External references

- 无。本结论不依赖外部框架行为或版本建议，全部基于仓库内production composition、合同、测试、migration与Trellis spec。
- 仓库实际使用的rmcp/Tauri/tokio行为只作为实现上下文；整改目标是AgentDash-owned identity、contract与lifecycle，不把框架默认行为当稳定边界。

### 11. Related specs

- `.trellis/spec/backend/agent-runtime/product-runtime-facade.md` — Product不拥有Agent内部状态，命令用stable client identity。
- `.trellis/spec/backend/agent-runtime/driver-host.md` — Host process-local binding/generation/callback fence。
- `.trellis/spec/backend/agent-runtime/persistence.md` — concrete Agent durable source/effect与Product association分权。
- `.trellis/spec/backend/agent-runtime/surface-tool-protocol.md` — immutable surface、applied evidence、tool callback。
- `.trellis/spec/backend/capability/tool-capability-pipeline.md` — capability/MCP/surface/tool同源与health变化。
- `.trellis/spec/backend/hooks/architecture.md`、`execution.md`、`scripts.md` — Product rule、execution site、effect/failure语义。
- `.trellis/spec/backend/vfs/architecture.md`、`access-scope.md`、`materialization.md` — Cloud policy/Local physical execution。
- `.trellis/spec/backend/runtime-gateway.md` — runtime gateway与application port边界。
- `.trellis/spec/cross-layer/runtime-wire-relay.md` — Relay只拥有route/sequence/ack/replay/connection health。
- `.trellis/spec/cross-layer/desktop-local-runtime.md` — Tauri薄壳、DesktopRunnerHost与API health gate。
- `.trellis/spec/cross-layer/deployment-runtime-backbone.md` — migration、release、health与部署composition。

## Caveats / Not Found

- 观察基线采用`backend-entrypoint-coverage-index.md`记录的`main@8dc12f73`与2026-07-26共享工作区；没有运行git命令，也没有修改任务research目录以外的文件。
- 本研究没有运行全量构建/测试；任务是只读production path审计，现有gate通过代码与测试枚举确认。所有建议的negative fixture当前均明确标为缺口。
- `design.md`要求P0/P1至少两类证据；本报告每项均给出production producer/composition与第二个consumer/contract/persistence证据。没有仅凭spec、目录或crate fan-out定级。
- Extension action/protocol/backend-service的业务package authority由“业务资产”研究面主审；本文件只核验Local transport/process lane与Tauri/Relay composition。
- MCP 19个静态tool的业务事务与aggregate owner分别由“业务资产/控制编排”研究面主审；本文件只确认transport/auth与未自建execution state。
- Diagnostics实际是否已经泄露某个真实secret未做运行时采样；P0依据是anonymous arbitrary-field channel本身可被任一未来/现有producer污染，且当前没有redaction/auth boundary。
- Dynamic catalog旧credential是否已在某个运行实例被利用未做线上复现；项目未上线。代码路径已经足以证明同definition重绑不会替换executor且没有unbind。
- Remote Runtime Wire driver可在运行时声明builtin以外hook facet；报告没有假设所有remote driver都等同Native，但Product plan的宽action requirement仍会在projection阶段影响所有Driver。
- Health endpoint是否被外部orchestrator当成完整readiness取决于仓库外部署配置；仓库内Desktop只把它用作API listener/AppState ready，故E-12不升级。
- Project delete 对 `dash_complete_source/effect` 应执行 hard delete 还是合规留存，仓库没有 retention policy；E-14 确定的是“没有 owner/plan”，目标实现需由 Project/Product owner 选择一种唯一最终合同。
- Local Extension cache 是否必须在 Project 删除时立即清理取决于 content-addressed 共享/retention 设计；本报告不把 cache 存在本身视为 durable 泄漏，问题是没有引用/TTL/GC 和 Host activation revoke 合同。
