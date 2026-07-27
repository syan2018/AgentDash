# 后端 Production Entrypoint 覆盖基线

## 目的

本文件只定义“完整审计必须覆盖什么”，不提前给出耦合结论。审计单位是用户或系统意图对应的
production command/query；crate fan-out、文件数量和 route 数量只用于发现遗漏，不能代替业务链路分析。

基线：`main@8dc12f73`。该提交相对产品代码基线只包含并行发生的 Trellis 更新；若后续 production
入口发生变化，最终矩阵必须记录新的 commit，并对增删入口重新核验。

## 入口总账

| 入口类别 | 当前可达入口 | 覆盖要求 | 主审计面 |
| --- | ---: | --- | --- |
| HTTP API | `routes/` 中 38 个含 route 的模块、197 个 `.route(...)` 声明；另有 2 个被 route handler 使用的 helper 模块 | 从 router 挂载、鉴权、handler 追到 use case/owner/persistence/consumer/test | 三面共同 |
| MCP HTTP transport | health、relay、story、workflow 4 条 route | 核验 scope identity、session/auth、MCP server 构造与 service 注入 | 执行与系统装配 |
| MCP tools | relay 7、story 7、workflow 5，共 19 个静态工具 | 每个 tool 映射回同一业务 command owner，不允许 MCP 自建第二套状态迁移 | 业务资产；控制编排 |
| Runtime tool catalog | 16 个 platform descriptor、19 个 platform MCP descriptor，加 project dynamic MCP | 核验 capability、permission、executor、product service、dynamic discovery 和 runtime projection 是否同源 | 控制编排；执行与系统装配 |
| Local Relay command | 29 类 `Command*` 分支，另有 ping/pong | 核验 cloud admission 与 local execution 分权、command identity、超时/重试/断线恢复 | 执行与系统装配 |
| Tauri IPC | `invoke_handler!` 挂载 25 个 command | 核验 Rust/TS 合同、desktop state owner、sidecar/runtime 生命周期和平台语义 | 执行与系统装配 |
| Background/startup | auth session cleanup、workflow recovery、cron scheduler、desktop runner/sidecar/update startup | 核验 lease/幂等、恢复 owner、启动 completeness、失败可观察性和关闭语义 | 控制编排；执行与系统装配 |
| WebSocket/stream | backend relay WS、project NDJSON event stream、Agent live stream、terminal/session notifications | 核验 source sequence、snapshot/delta、owner fence、重连/重放和前端 projection | 执行与系统装配；前端横向审计 |
| Extension host/process | action、protocol、backend service、artifact activation/cache、host process | 核验 package authority、activation identity、process/resource 生命周期和跨端 contract | 业务资产；执行与系统装配 |

计数不是稳定合同。最终 `business-coupling-matrix.md` 必须按 capability 合并同一意图的多入口，并保留
entrypoint 列，才能发现 HTTP、MCP、Tool、worker、Local/Tauri 是否绕过同一个 owner。

## HTTP capability 覆盖分配

| 能力族 | route 模块 | 主审计面 |
| --- | --- | --- |
| 身份与访问 | `me`、`auth_routes`、`identity_directory`、`backend_access`、`runner_registration_tokens` | 业务资产与所有权 |
| Project / Backend / Workspace | `projects`、`project_vfs_mounts`、`project_agents`、`backends`、`workspaces`、`file_picker` | 业务资产与所有权 |
| Provider 与配置 | `llm_providers`、`mcp_presets`、`skill_assets`、`settings`、`execution_profiles` | 业务资产与所有权；执行与系统装配交叉核验 |
| Story / Task / Interaction | `stories`、`story_runs`、`task_plan`、`interactions`、`operation_workshop`、`companion_gates` | 控制编排与协作；业务资产交叉核验 |
| Workflow / Lifecycle / Routine | `workflows`、`lifecycle_agents`、`lifecycle_views`、`routines` | 控制编排与协作 |
| Library / Marketplace / Extension | `shared_library`、`marketplace`、`project_extensions`、`extension_package_artifacts`、`extension_runtime` | 业务资产与所有权 |
| Workspace Module / VFS / Terminal | `workspace_module`、`vfs`、`vfs_surfaces`、`terminals` | 执行与系统装配 |
| 平台状态 | `release_info`、`health`、`diagnostics` | 执行与系统装配；可判为合理 composition，但必须显式核验 |

`agent_run_workspace` 与 `lifecycle_contracts` 没有自己的 router；它们分别被 AgentRun workspace
解析和 lifecycle route contract 转换使用，作为 helper boundary 核验，不计为独立 production capability。

## Persistent fact 覆盖基线

当前 baseline migration 有 57 个 table。最终 data-owner ledger 必须逐表分类，不能只按 repository
trait 抽样：

| owner 候选族 | table 数 | 当前 table |
| --- | ---: | --- |
| Agent execution / Product projection | 9 | `agent_lineages`、`agent_run_lineages`、三张 terminal projection 表、`backend_execution_leases`、`dash_complete_effect`、`dash_complete_source`、`runtime_health` |
| Auth / Identity | 4 | `auth_sessions`、`users`、`groups`、`group_memberships` |
| Project / Backend / Workspace | 10 | `projects`、`backends`、`backend_workspace_inventory`、`project_backend_access`、`project_subject_grants`、`project_vfs_mounts`、`workspaces`、`workspace_bindings`、`inline_fs_files`、`settings` |
| Asset / Provider / Package | 10 | `agent_procedures`、`project_agents`、`mcp_presets`、`skill_assets`、`library_assets`、`extension_package_artifacts`、`project_extension_installations`、`llm_providers`、`llm_provider_user_credentials`、`runner_registration_tokens` |
| Workflow / Lifecycle / Routine / Story | 10 | `workflow_graphs`、`workflow_executor_effects`、`lifecycle_agents`、`lifecycle_gates`、`lifecycle_runs`、`lifecycle_subject_associations`、`routines`、`routine_executions`、`stories`、`state_changes` |
| Interaction / Canvas | 14 | definition/source/revision/lineage、instance/state/attachment/runtime binding/presentation/renderer lease、event/effect intent/command receipt 全套表 |

此处的分组是 coverage 分派，不是最终 aggregate owner 结论。特别要核验共表 concrete repository、
跨表 semantic transaction 和 read/claim/recovery 路径。

数据库之外还存在 Extension package archive 的 durable filesystem object，以及本机 Extension
artifact/backend-service cache。前者必须有 metadata、retention、delete/replay owner；后者必须明确为
可重建 cache 并验证 cache identity，不能把两类文件生命周期混为一个“storage”结论。

## Local / Desktop 覆盖分组

Local Relay 的 29 类 command 必须至少覆盖：

- workspace detect、git detect、identity discovery、directory browse；
- file read/binary read/write/delete/rename/list/search/apply patch；
- shell exec/read/input/terminate；
- VFS materialization；
- MCP probe/list/call/close；
- extension action/protocol/backend-service invoke；
- terminal spawn/input/resize/kill/inventory。

Tauri 的 25 个挂载 command 必须至少覆盖：

- Codex OAuth start/cancel；
- desktop settings、autostart、quit；
- runtime profile load/save/delete；
- runtime start/stop/restart/snapshot；
- local MCP config load/save/probe；
- directory browse、logs tail/clear；
- desktop API snapshot；
- update policy snapshot/refresh/install；
- external URL open。

## 完整性判据

一个 capability 只有在以下证据都存在时才标记为“已核验”：

1. 至少一个真实挂载的 production entrypoint，或明确标为 background/startup intent；
2. authorization、command owner、canonical read/write owner；
3. transaction / external effect / recovery 语义；
4. public contract、事件或 projection owner及全部跨端 consumer；
5. 会在 owner 绕过、装配缺失或恢复失效时失败的现有 gate，或明确记录 gate gap；
6. 三路审计重叠处有一致结论；不一致时保留证据与分歧，不用平均意见消解。

以下情况不能标记“已覆盖”：

- 只列出 crate dependency 或 repository trait；
- 只检查 HTTP，未核对同意图的 MCP/tool/worker/local 入口；
- 只看到测试或 spec，未证明 production composition 可达；
- 入口存在但没有追到持久化、对象存储或外部副作用；
- 以“基础设施/工具代码”名义跳过其持有的业务 admission、scope 或 recovery 决策。

## 收敛检查

三路研究完成后，以本文件为 checklist 执行：

- 每个 HTTP capability 族至少出现在一个 use-case ledger，交叉归属项有第二视角；
- 19 个 MCP tool、29 类 Local command、25 个 Tauri command 均能映射到 capability 或明确的
  platform operation；
- background/startup、stream 和 extension host 不被 CRUD 路径遮蔽；
- 未映射入口进入 `reachability / coverage gap`，不得静默省略；
- 同一业务意图的多入口若落到不同 command owner，自动进入 authority 候选清单。
