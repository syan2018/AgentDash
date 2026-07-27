# Research: 后端业务资产与所有权完整审计

- Query: 审计 Project、Backend、Workspace、Auth/authorization、Story、Task、Canvas、Shared Library、Skill、MCP Preset、Extension、Workspace Module 及补充的 identity/me、LLM Provider/credential、settings、runner registration token、marketplace、execution profile、Project Agent、Project VFS Mount 的 production command/query；建立 use-case ledger，并给出 P0/P1 的稳定边界收敛包。
- Scope: internal
- Date: 2026-07-26

## 结论摘要

本研究确认的最高风险不是“crate 太多”，而是若干业务意图没有唯一的原子 command owner：

1. `POST /api/workspaces/detect-git` 只经过全局登录中间件，没有 Project、Backend 或 ProjectBackendAccess 授权，且向 Runtime Gateway 注入 `user_id=None`；任意已登录用户只要知道 backend id 和 root ref，就能在该本机 backend 上发起 Git 目录探测。
2. Task HTTP 写入口只要求 `ProjectPermission::Use`，会放行普通 member 和仅因 `template_visible` 可见的用户；写入又调用无 revision 条件的 `LifecycleRunRepository::update`，可覆盖同时发生的 orchestration/task/execution-log 变化。代码虽然存在 `compare_and_swap` 和 `TaskLockMap`，production Task route 均未使用。
3. Project grant 的“至少一个 owner”检查位于 route，采用“读取 grants → 判断 → 单独 upsert/delete”；两个 owner 并发降级/撤销可同时通过检查并把 owner 清空。
4. Project/Story/ProjectAgent/VFS Mount/Extension artifact/Shared Library 等删除、发布、安装横跨多个 repository 或 filesystem object，普遍没有 command transaction、outbox/receipt 或补偿。多个路径会在返回失败时已经提交部分事实，甚至先删 inline content 再删 owner。
5. Story HTTP 与 MCP 对同一 Story 变化存在两套 producer：HTTP 走 application management 并同步 inline files；MCP 在 server 内直接改 Story repository。Story 删除则先删 Story、后 append `StoryDeleted`，第二步失败时 canonical record 已不可恢复且删除事件缺失。
6. Canvas definition 自身是本次最好的合理内聚反例：application service 持有 access resolver、revision CAS 和 publish/copy/archive policy；但“promote to extension”仍被 route 重新编排成 object write、artifact metadata、installation 三步，和 spec 声明的 application owner 不一致。
7. LLM Provider 的持久配置/凭据 owner 基本清楚，但 Codex OAuth flow 是 `agentdash-api` route 文件里的进程内全局 `HashMap`；重启、多实例、完成请求中断都没有 durable recovery。

以上均来自当前 production router/composition。未挂载的 `TaskLockMap` 单独列为 reachability gap，不把它当作现有保护。

## 相关规范与判定基线

- `.trellis/spec/backend/architecture.md:12-15`：Interface → Application → Domain/SPI；route 只负责鉴权、DTO、错误映射；跨聚合一致性必须使用显式 command port / unit of work。
- `.trellis/spec/backend/architecture.md:62,94` 与 `.trellis/spec/backend/repository-pattern.md:18-20`：`RepositorySet` 只用于 bootstrap/composition；业务 use case 使用具名 deps，不能把全量 set 当 service locator。
- `.trellis/spec/backend/architecture.md:81-83`：Extension artifact 属于独立 application use case；Canvas promotion 应归 `agentdash-application::canvas::promotion`，route 只作为入口。
- `.trellis/spec/backend/architecture.md:85`：Project authorization 由 domain service 统一表达；Backend authorization 由 application service 统一表达。
- `.trellis/spec/backend/shared-library.md:63-81`：安装/发布必须产生可运行 Project copy、记录 `InstalledAssetSource`；Workflow bundle 已明确要求单事务。
- `.trellis/spec/cross-layer/shared-library-contract.md:382-408`：Extension package artifact 是可校验运行事实；publish 后 LibraryAsset 与 artifact 共同构成可安装资产。
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md:37-44`：detect 必须绑定可编辑 Project 和 active access；`WorkspacePlacementService` 是 detect→inventory/binding 写事务 owner，route 不应复制业务编排。
- `.trellis/spec/backend/repository-pattern.md:38-44`：Workspace 内部 bindings 可由 aggregate repository 原子维护；跨 aggregate 必须由独立 command port。

## 文件与边界清单

| 文件 | 一句话说明 |
| --- | --- |
| `crates/agentdash-api/src/routes.rs:55-139` | production HTTP/MCP composition root；本研究列出的 route 均挂载在 secured/public router。 |
| `crates/agentdash-api/src/routes/projects.rs` | Project CRUD 与 grant HTTP 入口；grant invariant 仍在 route。 |
| `crates/agentdash-application/src/project/management.rs` | Project create/update/clone/delete 编排；接收全量 `RepositorySet`，delete 逐 repository 提交。 |
| `crates/agentdash-domain/src/project/authorization.rs` | Project role/Use/Configure/ManageSharing 的 canonical policy。 |
| `crates/agentdash-api/src/routes/backends.rs`、`backend_access.rs`、`workspaces.rs` | Backend、ProjectBackendAccess、Workspace 与本机目录 setup 入口。 |
| `crates/agentdash-application/src/workspace/placement.rs` | Workspace directory fact / binding / inventory 的目标 application owner。 |
| `crates/agentdash-api/src/routes/stories.rs`、`crates/agentdash-application/src/story/management.rs` | Story HTTP 与 application owner。 |
| `crates/agentdash-mcp/src/servers/story.rs` | production MCP Story tools；目前直接写 Story repository。 |
| `crates/agentdash-api/src/routes/task_plan.rs`、`crates/agentdash-application/src/task/plan.rs` | Task HTTP command/query 与 LifecycleRun 内 task plan owner。 |
| `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs` | LifecycleRun 普通 update 与 CAS 两条持久化路径。 |
| `crates/agentdash-application/src/task/lock.rs` | 未挂载的 process-local Task lock。 |
| `crates/agentdash-api/src/routes/interactions.rs`、`crates/agentdash-application/src/interaction/canvas_definition.rs` | Canvas definition/revision 与 route-local promotion。 |
| `crates/agentdash-application-shared-library/src/install.rs`、`publish.rs` | Shared Library install/publish；多聚合写入目前除 Workflow bundle 外无事务。 |
| `crates/agentdash-api/src/routes/skill_assets.rs`、`mcp_presets.rs` | Project Skill/MCP Preset CRUD、remote import/probe。 |
| `crates/agentdash-application/src/extension_package.rs` | Extension archive validate/store/read/install。 |
| `crates/agentdash-platform-spi/src/extension_package.rs` | object storage port；只有 write/read，没有 delete/retention/GC。 |
| `crates/agentdash-infrastructure/src/storage/extension_package_artifact_fs.rs` | filesystem object adapter。 |
| `crates/agentdash-api/src/routes/workspace_module.rs`、`crates/agentdash-workspace-module/src/workspace_module/mod.rs` | Project/User Workshop 的 Workspace Module descriptor/presentation。 |
| `crates/agentdash-api/src/routes/project_agents.rs`、`project_vfs_mounts.rs` | Project Agent / Mount CRUD，当前 route 直接拥有 mutation 与 delete ordering。 |
| `crates/agentdash-api/src/routes/llm_providers.rs` | system provider、user credential、Codex OAuth；OAuth flow 是 route-local 内存状态。 |
| `crates/agentdash-api/src/routes/settings.rs` | system/user/project setting scope 与 masking。 |
| `crates/agentdash-api/src/routes/identity_directory.rs`、`me.rs` | identity provider 查询、local projection 与当前身份。 |
| `crates/agentdash-api/src/routes/runner_registration_tokens.rs` | token 管理与公开 claim；写入收束到 application enrollment。 |
| `crates/agentdash-api/src/routes/marketplace.rs` | external provider browse/import/refresh；import 物化 user-scoped LibraryAsset。 |
| `crates/agentdash-api/src/routes/execution_profiles.rs` | profile/option query；主要属于执行装配，本研究只核验业务入口和配置 consumer。 |
| `crates/agentdash-infrastructure/migrations/0001_init.sql` | 业务表 schema；大量 Project child 依赖 owner_id/project_id 约定而非 FK，object 无 schema lifecycle。 |

## Scoped production use-case ledger

缩写：`P`=Project，`B`=Backend，`W`=Workspace，`SL`=Shared Library，`ExtObj`=Extension archive object。`tx` 表示数据库事务；“无”表示没有用例级原子边界，而不是 repository 内单条 SQL 不原子。

| Capability / use case | Production entrypoint | Authorization owner | Command owner | Read owners | Write owners | Transaction / external effect | Public contract / event / projection | Recovery / retry | Consumers | Tests / gates | Coupling verdict |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 登录、会话恢复、登出、`/me` | `/auth/*` public/secured；`/me` | AuthProvider；session fallback | `AuthSessionService` | provider、`auth_sessions` | session、identity projection | login/provider + DB 分步；cleanup worker | auth contracts / `AuthIdentity` | DB session 可跨重启；provider claim freshness 取决于 provider authorize | Web/Desktop/API middleware | auth route/service tests | 基本合理；session hash/expiry owner 清楚。 |
| Identity user/group browse/resolve | `/directory/*` | 仅全局 authenticate/provider authorize；无 directory-specific policy | route | external directory、user/group projection | resolve 时写 projection | provider call 后 upsert；无 operation receipt | directory contract；projection/source flags | provider unavailable 时 list/tree 回退 projection | sharing UI、grant subject picker | route tests有限 | **P1/证据缺口**：任何登录用户可列举/resolve 全目录是否符合企业策略未被显式合同证明；fallback 是 spec 允许与否也未声明。 |
| Project list/get/create/update/clone | `/projects*` | domain `ProjectAuthorizationService` | application management | P、W、Story、Skill、inline | P、owner grant、builtin Skill、inline | create/clone 先 P tx，再 Skill/inline；无 use-case tx | generated Project contract | 重试可能得到已创建但返回失败的 Project | Web、MCP project scope、其它所有 assets | Project repo/application tests | **P1**：authorization owner合理；create/clone/config+inline 非原子，且 use case 接收全量 set。 |
| Project grant/revoke | `/projects/{id}/grants/*` | Project ManageSharing + last-owner domain query | **route helper** | P/grants、identity provider/projection | grants、identity projection | check-then-write 分离；无 lock/CAS | Project grant contract | 并发失败无自动收敛 | sharing UI、所有 auth checks | subject resolution tests；无并发 invariant gate | **P0 authority/transaction**。 |
| Project delete | `DELETE /projects/{id}` | Project ManageSharing | application function但用 full `RepositorySet` | 15+ repos | runs/routines/agents/installations/artifact metadata/mount/skill/MCP/workflow/story/W/inline/settings/state/P | 每步独立 commit；filesystem object 不参与 | `DeletedIdResponse`；无 deletion receipt | 无 resume cursor/operation id；重试依赖剩余状态且部分 delete 返回 NotFound | 全部 Project consumers | 未见 failure-injection/completeness test | **P0 persistence/object lifecycle**。 |
| Backend CRUD/summary/browse | `/backends*`, `/local-runtime/ensure` | `BackendAuthorizationService` | application management；summary projection | B、access、runtime health/lease/registry | B / enrollment | backend ensure 在 repository；外部 relay browse | backend contracts/runtime summary | relay request id 层恢复；删除依赖 FK/DB | Web/Desktop/Local | authorization tests | 大体合理；global composition 高连接属必要内聚。 |
| Runner token create/list/revoke/rotate/claim | project routes + public `/local-runtime/runner/claim` | Project Configure；claim=secret possession+policy | application runner registration/enrollment | token、P、B、access | token usage、B、ProjectBackendAccess | claim 的 B ensure/access/usage 跨 repos；没有显式 unit of work，测试覆盖冲突分类 | backend contracts | repeated claim 有 idempotency test；partial DB failure仍依赖 repo语义 | Runner/Local/Web | `runner_registration.rs` 多个 claim tests | **P1 transaction**，但共享 enrollment owner 是合理反例。 |
| ProjectBackendAccess CRUD/policy | `/projects/{p}/backend-access*` | create 同时 P Configure + B Manage；update/revoke 仅 P Configure | ensure create在 application；update/revoke在 route | P、B、access | access | 单 repository；policy mutation在 route | backend access contracts | 幂等 ensure仅 create/claim共享 | Project settings、placement、backend auth | ensure tests | **P1 direction**：同一 grant 的 create/reactivate owner 已集中，update/revoke仍绕过；需统一 command。 |
| Workspace CRUD/status | `/projects/{p}/workspaces*`, `/workspaces/{id}*` | P Use/Configure | placement service（create/update/bind）；status/delete仍 route | P、W、access、inventory、Gateway | W/bindings/inventory | WorkspaceRepository 内 aggregate tx；placement 跨 facts需核验 command tx | workspace contracts | detect/discover 外部效果无 durable receipt | Web、runtime placement | placement tests | 混合：create/update合理；status/delete route authority leak 为 P2/P1。 |
| Workspace detect/discover/bind | project-scoped detect/discover/bind；另 `/workspaces/detect-git` | project-scoped 路径=P Configure+active access；detect-git=仅登录 | placement/Gateway；detect-git route | P、access、B online/local files | bind/inventory；detect-git只外部读 | relay external call；bind由 placement收束 | workspace/relay contracts | relay timeout；无 setup operation receipt | setup UI/Local | provider/local handler tests，没有 auth negative test | project-scoped 路径合理；**detect-git P0 authorization bypass**。 |
| Story list/create/update | HTTP `/stories*` | P Use/Configure | application story management | P、Story | Story、inline files | Story row先写、inline后写；无 tx | story generated contract | 重试可能看到 root已更新但返回失败 | Web | local unit tests | **P1 transaction**。 |
| Story context/details/status via MCP | MCP Story server tools | MCP Project Configure | **MCP server** | Story/P | Story | 单 repo，绕过 inline sync/application validation入口 | MCP tool JSON/text | 无 command id | MCP clients/agents | schema tests | **P1 duplicate producer/protocol**。 |
| Story delete | `DELETE /stories/{id}` | P Configure | application helper | Story | Story、StateChange；未删 Story inline | delete后append event | HTTP deleted response + `StoryDeleted` state change | event失败时返回失败但 Story已消失；无补发worker | Web/event stream | 无 failure injection | **P0 fact/event divergence + orphan inline**。 |
| Task query/create/update/status/archive | HTTP run/agent task routes；MCP Story create/batch；runtime `task_read/task_write` | HTTP仅 P Use；MCP P Configure；runtime tool由运行 surface | application task plan / task workspace | LifecycleRun、association | `LifecycleRun.tasks`（整行 document） | HTTP/MCP调用普通 `update`; batch逐项写；runtime workspace需另审 | task contracts、Story projection、runtime tool output | 无 HTTP idempotency/CAS；batch部分成功 | Web/MCP/agents/Story projection | plan/tool tests；未用 TaskLock | **P0 authorization + lost-update**；三个入口 policy不一致。 |
| Canvas definition CRUD/revision/publish/copy/archive | `/interaction-definitions*` | Canvas service + Project access resolver | `CanvasDefinitionService` | definition/revision/P | definition/revision/lineage | repository create/commit_revision原子/CAS | generated interaction contract | base revision conflict可安全重试 | Web/Workspace Module | canvas service tests | **合理内聚反例**。 |
| Canvas promote to Extension | `/interaction-definitions/{id}/promote-extension` | route先Canvas view再 P Configure | **route** | revision | ExtObj、artifact metadata、installation | object→metadata→installation 三步，无 receipt/compensation | promotion response/extension contract | install失败留下 artifact/object；重试可能冲突/复用 | Canvas UI、Extension runtime | package validation tests，无 promotion failure suite | **P1（局部可造成P0数据生命周期后果）**。 |
| SL browse/get/seed | `/shared-library/assets*`; startup seed | 只登录；seed endpoint也只登录 | `SharedLibraryService` | SL | SL | repo-level upsert | shared-library contracts | seed幂等 | Marketplace/assets UI/startup | seed tests | **P1 auth**：手工 seed 是否应为 system admin 未显式裁决。 |
| SL install Project asset | `/projects/{p}/shared-library/install`; remote skill import最终复用 | P Configure | shared-library install | SL + 9 Project asset repos | Agent/MCP/Skill/Mount/Workflow/Extension/inline | Workflow bundle有 tx；Agent dependency MCP→Agent、Mount→inline等分步 | install response + InstalledAssetSource | 无 operation id/rollback；部分 dependency 可留存 | Project assets/runtime/frame | helper tests，Workflow tx integration test | **P0 semantic transaction**。 |
| SL publish/source status | `/projects/{p}/shared-library/publish|source-status` | P Configure/Use | shared-library publish/status | Project assets、SL、artifact metadata | SL、library-owned artifact metadata | Extension publish先 SL 后 artifact metadata；object ref共享但无 lifecycle transaction | LibraryAsset DTO/source status | 后一步失败留下不可安装 LibraryAsset；无 repair worker | Marketplace/assets | mapper tests | **P0 publish completeness**。 |
| Marketplace browse/import/refresh | `/marketplace/*` | 登录；写限定 owner=current user | route adapter + shared-library external use case | provider、SL | user-scoped SL | provider fetch→DB；写前有类型校验 | marketplace contracts | retry为 upsert；外部调用无 receipt但读/fetch可重试 | Marketplace UI | route/provider tests | 边界总体合理；provider选择仍在 route但属 adapter职责。 |
| Skill CRUD/upload/remote import/blob | `/projects/{p}/skill-assets*` | P Use/Configure | `SkillAssetService`; remote import cross-SL use case | Skill/SL/external source | Skill/SL | repo写；remote import SL→Skill分步但先做冲突预检 | skill generated contract | 同源 upsert；跨写失败可留SL资产 | assets/VFS/runtime | extensive service tests | 普通CRUD合理；remote materialize/install仍需 semantic UoW。 |
| MCP Preset CRUD/clone/probe | `/projects/{p}/mcp-presets*` | P Use/Configure | `McpPresetService`; probe Gateway | MCP preset/B/access | preset；probe无DB | CRUD单 repo；probe external | generated MCP contract | probe timeout/result，不落库 | assets/runtime MCP | service/probe tests | 基本合理；Shared Library dependency install例外见上。 |
| Extension upload/install/download/uninstall/runtime | artifact routes、project_extensions、extension_runtime | P Use/Configure；backend download=relay token+B enabled+Project access | extension application helpers；部分 route编排 | P、artifact、installation、B/W/access | ExtObj、artifact metadata、installation | upload object→metadata；install metadata→installation；uninstall只installation；storage无delete | extension contracts/manifest/digest | verified read；无 write receipt/GC | Web/Local/Extension host/Workspace Module | validation/runtime tests | **P0 object lifecycle / P1 command locality**。 |
| Workspace Module list/present/invoke | HTTP project modules；runtime tools `workspace_module_*` | HTTP P Use；runtime applied frame/capability/OperationGateway | `agentdash-workspace-module` projection；HTTP assembly在 route，runtime assembly在 tool service | installations、Canvas revisions、operation catalog、frame surface | invoke可能外部 effect；present只projection | invoke有 idempotency key=invocation id；HTTP present无effect | generated workspace-module contract | OperationGateway replay policy | Web panels/agents/extensions | module/tool tests | 高连接但共享 projection builder合理；HTTP允许当前用户personal Canvas、agent runtime只Project Canvas是有上下文理由的差异。 |
| Project Agent CRUD | `/projects/{p}/agents/configs*`; run start另属执行面 | P Use/Configure | **route** | P Agent、Workflow、Routine、execution profile | P Agent、inline delete | delete先inline后Agent；无 tx；update校验分散 | project-agent contract | delete第二步失败会保留Agent但丢内容 | Project settings/frame/run start | route tests有限 | **P0 destructive ordering，P1 route authority**。 |
| Project VFS Mount CRUD | `/projects/{p}/vfs-mounts*`; SL install | P Use/Configure | **route** / SL install | Mount、inline | Mount、inline | delete先inline后Mount；SL overwrite先更新Mount、再删/写files | VFS contracts/InstalledSource | 失败可丢旧inline内容或产生空Mount | UI/VFS/runtime/SL | mapper tests | **P0 destructive ordering + duplicate command owner**。 |
| Settings list/batch/delete | `/settings*` | system=admin/personal；user=self；project=Use/Configure | route | settings/P | settings | `set_batch` repository tx；单scope | settings contract/masked projection | batch原子性由repo | Web/config consumers | repo/route tests | 权限与scope owner合理；业务 key schema仍是开放字符串（P2 data-shape）。 |
| LLM Provider admin CRUD/reorder | `/llm-providers*` | admin/personal | application CRUD | providers/secrets | providers/credentials on delete | reorder需核验repo tx；secret加密在app | LLM contracts, masked preview | DB durable | Settings/execution profiles/native runtime | application/route tests | 基本合理。 |
| User LLM credential/OAuth/probe | `/me/llm-providers/*`, `/llm-providers/*/codex-oauth*` | self；global OAuth=admin | credential writes大多route；OAuth flow route-local | Provider/credential/external OAuth | credential DB + in-memory flow | external exchange→DB credential→flow status；flow无durability | LLM/OAuth contracts | restart丢flow；多实例不共享；completion_claimed可能悬挂 | Web/Desktop/execution profiles | PKCE/single-use tests | **P1 temporal/composition**。 |
| Execution profile discovery/options | `/agents/discovery`, `/agents/discovered-options/stream`; Project Agent config consumer | 登录/self effective credentials | route query adapter | provider catalog、credential、Complete Agent live catalog | 无 | 无业务写；NDJSON一次性投影 | project-agent contracts | 每次重算 | Web agent config | route tests | 合理 query hub；执行 availability 深审转交 execution/composition reviewer。 |

## P0 发现与可执行 work package

### P0-1：`detect-git` 绕过 Backend/Project 授权

- 证据：
  - production router 挂载 secured workspace router：`crates/agentdash-api/src/routes.rs:73-113`、`crates/agentdash-api/src/routes/workspaces.rs:92`。
  - handler 没有 `CurrentUser`，只校验 `backend_id/root_ref`：`workspaces.rs:309-320`。
  - Runtime request 明确使用 `RuntimeActor::PlatformUser { user_id: None }` 且 `project_id=None`：`workspaces.rs:642-665`。
  - provider 只解析 input 并调用 detector，没有再做 actor/scope授权：`crates/agentdash-application-extension-gateway/src/extension_gateway/setup_actions.rs:165-209`。
  - setup port 只检查 backend online 后调用 relay：`crates/agentdash-api/src/bootstrap/extension_gateway.rs:233-253`。
- 触发器：任意登录用户提交一个可猜测/泄露的 backend id 与任意 root ref。
- 爆炸半径：跨用户/跨 Project 的本机路径存在性、Git repository、remote/branch/commit 元数据；还可作为目录枚举侧信道。
- 目标稳定边界：删除无 scope 的 route；Git detect 只能作为 `WorkspaceSetupCommand(project_id, access_id, root_ref, actor)`，authorization owner 同时验证 P Configure、active ProjectBackendAccess、Backend visibility，Gateway request必须携带 server-resolved actor/scope。
- Work package `BA-01 workspace-setup-admission`：
  1. characterization：增加 owner、非owner、inactive access、unknown root negative E2E；
  2. 建立具名 `WorkspaceSetupCommandDeps` 与唯一 application admission；
  3. 迁移 project detect/git/discover/browse；
  4. hard-delete `/workspaces/detect-git` 和 `user_id=None` setup producer；
  5. relay integration 测试断言 command 只能由 authorized access 产生。
- Guard：router manifest 将所有 `workspace.*` setup action映射到 scope/admission owner；fixture 中出现 `project_id=None` 或 `PlatformUser.user_id=None` 必须失败。

### P0-2：Task 写授权过宽且存在整行 lost update

- 证据：
  - create/update/status/archive 均调用 `load_authorized_run(... ProjectPermission::Use)`：`crates/agentdash-api/src/routes/task_plan.rs:81-140`。
  - `Use` 对任意有 role 的 member、甚至 `template_visible` 用户返回 true；`Configure` 才限定 Owner/Editor：`crates/agentdash-domain/src/project/authorization.rs:60-81,111-120`。
  - Task command load整个 run、mutation后调用普通 `update`：`crates/agentdash-application/src/task/plan.rs:104-204`。
  - PostgreSQL 普通 update 无 revision predicate且覆盖 orchestrations/tasks/status/execution_log：`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:504-516`；同 repo 已有正确 `compare_and_swap`：`:518-570`。
  - `TaskLockMap` 只有定义和自测，production 无引用：`crates/agentdash-application/src/task/lock.rs:14-76`；仓库搜索除该文件外无使用。
- 触发器：普通 member/template viewer 发写请求；或 HTTP/MCP/runtime/orchestration 同时更新同一 run。
- 爆炸半径：越权修改/归档任务；后提交的旧 snapshot 覆盖 task、orchestration、status 或 execution log，影响整个 LifecycleRun。
- 目标稳定边界：Task 写统一进入 LifecycleRun revision command port；policy 接受明确 actor/capability（至少 Configure 或显式 Collaborate，不复用 template visibility）；所有 producer使用 CAS+bounded retry，禁止普通 update 写 LifecycleRun。
- Work package `BA-02 lifecycle-task-command-authority`：
  1. 固定 HTTP/MCP/runtime 三类 actor matrix 与并发测试；
  2. 引入 `LifecycleTaskCommand { expected_revision, command_id, actor }`；
  3. repository public write只暴露 CAS command，迁移所有 task producers；
  4. 删除未挂载 `TaskLockMap` 和普通 task `update` 路径；
  5. 增加并发 task/task、task/orchestration、task/status lost-update tests。
- Guard：静态禁止 `LifecycleRunRepository::update` 出现在 application command；negative fixture 删除 revision predicate 时 contract test失败。

### P0-3：Project owner invariant 是可竞争的 route check

- 证据：
  - upsert 先 `would_leave_project_without_owner`，之后单独 `upsert_subject_grant`：`crates/agentdash-api/src/routes/projects.rs:391-412`。
  - revoke 同样先查后单独 delete：`projects.rs:449-463`。
  - domain service只是读取列表并在内存计算：`crates/agentdash-domain/src/project/authorization.rs:153-175`。
  - repository upsert自身另起 tx，但不包含 owner invariant；delete是单 SQL：`crates/agentdash-infrastructure/src/persistence/postgres/project_repository.rs:170-199`。
- 触发器：两个现存 owner 并发把自己或对方降级/撤销。
- 爆炸半径：Project 无 owner；常规用户无人能 ManageSharing/恢复授权，形成管理锁死。
- 目标稳定边界：`ProjectGrantCommandPort` 在单事务中锁定 Project/grant set、验证actor与last-owner invariant并写入；route不再读取 grants 做决定。
- Work package `BA-03 project-grant-transaction`：实现 transaction port、迁移 user/group upsert/revoke、加入两owner并发撤销/降级数据库测试、删除 route helper policy。
- Guard：架构检查禁止 route 调用 grant write repository；DB failure/concurrency suite 必须证明 committed grant set始终至少一个 owner。

### P0-4：Project/Story 删除不是可恢复的 aggregate operation

- 证据：
  - Project delete 依次删除 run、routine、agent、installation、artifact metadata、mount、skill、MCP、workflow、story、workspace、inline、settings、state change，最后删 Project；每次独立 repository调用：`crates/agentdash-application/src/project/management.rs:271-376`。
  - object storage port只有 write/read，无 delete：`crates/agentdash-platform-spi/src/extension_package.rs:13-24`；filesystem adapter也只有 write/read：`crates/agentdash-infrastructure/src/storage/extension_package_artifact_fs.rs:62-96`。
  - Story delete 先删 Story、再 append `StoryDeleted`：`crates/agentdash-application/src/story/management.rs:180-199`，且没有删除 Story inline files。
  - schema 中多类 Project child 只有文本 `project_id/owner_id`，没有 FK；例如 artifact `owner_kind/owner_id`：`crates/agentdash-infrastructure/migrations/0001_init.sql:224-243`，Project extension/install/mount/skill/story：`:447-506,584-639`。
- 触发器：任一中间 repository/object/event写失败、进程退出或并发 consumer 读取。
- 爆炸半径：部分 Project 可继续被读取但资产缺失；Story 已删除但 event stream不知情；orphan inline/object 持续占用；重试可能在前序 NotFound 处再次失败。
- 目标稳定边界：DB内 Project-owned facts 使用单个 `ProjectDeletionCommand` transaction（schema ownership ledger决定 cascade/restrict）；object side effect写 durable deletion intent/outbox，worker按 object identity幂等删除并记录 receipt；Story 删除与 event/inline同事务或统一 tombstone command。
- Work package `BA-04 aggregate-deletion-ledger`：
  1. 建 owner table/object matrix和失败注入 characterization；
  2. migration补齐可表达的 FK/cascade/restrict；
  3. Project/Story command transaction与deletion operation id；
  4. object storage增加 delete + idempotent NotFound、outbox/worker；
  5. hard-delete逐repo loop和write-only owner kind。
- Guard：schema-owner manifest要求每个 table/object owner有 authoritative reader、delete/retention策略和failure suite；新增 owner kind 未声明 lifecycle 时CI失败。

### P0-5：Shared Library/Extension 语义安装与发布可提交半成品

- 证据：
  - AgentTemplate 先逐个安装 MCP dependency，最后才创建/更新 ProjectAgent：`crates/agentdash-application-shared-library/src/install.rs:402-449`。
  - VFS overwrite 先更新 mount，再删除旧 inline files，随后才解码/校验/写新 files：`install.rs:856-928`；新 payload 出错时旧内容已丢。
  - Extension publish 先 create/update LibraryAsset，再创建 library-owned artifact metadata：`crates/agentdash-application-shared-library/src/publish.rs:112-145,248-302`。
  - archive upload先写 filesystem object、再写 metadata：`crates/agentdash-application/src/extension_package.rs:325-348`。
  - spec只对 Workflow bundle已有 transaction且实现有 integration test；其它 install没有等价 command port。
- 触发器：dependency第N步失败、Agent key冲突、inline decode失败、artifact metadata约束/DB失败、进程退出。
- 爆炸半径：孤立 MCP dependency、Agent未安装；Mount保留但内容丢失；LibraryAsset 对外可见却不可安装；filesystem orphan不可GC。
- 目标稳定边界：按资产类型提供 semantic install/publish transaction。先纯函数生成并完整验证 plan；DB writes在同一 tx；filesystem使用content-addressed object+staged metadata/outbox/receipt，只有 complete generation 可公开读取。
- Work package `BA-05 asset-operation-transaction`：
  1. 为 Agent+MCP、Mount+inline、Extension publish/upload/install写 failure matrix；
  2. 引入 `AssetOperationPlan` 和具名 transaction ports；
  3. 所有payload/base64/冲突预检在写前完成；
  4. artifact增加 staged/complete generation 或operation receipt及GC；
  5. hard-delete逐repository helper。
- Guard：semantic transaction ledger列出每个多写 use case；每一条边必须有“第N步失败零可见写”或可重放receipt测试。

### P0-6：Project Agent / VFS Mount 删除先毁内容再删 owner

- 证据：
  - Project Agent delete 先 `inline_file_repo.delete_by_owner`，再 `project_agent_repo.delete`：`crates/agentdash-api/src/routes/project_agents.rs:414-454`。
  - Project VFS Mount 同顺序：`crates/agentdash-api/src/routes/project_vfs_mounts.rs:201-231`。
- 触发器：owner delete DB错误/并发约束/进程退出。
- 爆炸半径：API 返回失败、owner仍可见，但内容已永久丢失；runtime下一次读取得到不完整资源。
- 目标稳定边界：删除属于 owner aggregate command；DB inline与owner同 tx，或先 tombstone owner再异步清理且读模型不再暴露。route只传意图。
- Work package `BA-06 project-asset-delete-commands`：建立 Agent/Mount delete ports、事务 adapter、并发/失败注入；与 `BA-04` 共享 owner manifest，先于目录整理。
- Guard：静态检查禁止 interface route直接组合 `delete_by_owner + owner.delete`；failure fixture在第二步故障时必须保留完整可读资源或完整 tombstone。

## P1 发现与可执行 work package

### P1-1：业务 route 和 MCP 仍是第二 application 层

- 证据：
  - Project grant完整 policy/write在 route：`projects.rs:363-472`。
  - BackendAccess update/revoke直接 mutate repository：`backend_access.rs:146-210`。
  - Workspace status/delete直接 repository：`workspaces.rs:217-253`。
  - Project Agent create/update/delete直接组合 execution profile、workflow、routine、inline、repo：`project_agents.rs:311-454`。
  - VFS Mount CRUD全部在 route：`project_vfs_mounts.rs:75-231`。
  - MCP Story update context/details/status直接 mutation Story repository：`crates/agentdash-mcp/src/servers/story.rs:324-451,541-575`；HTTP 则走 `story::management`。
  - `ProjectManagement` 仍接收全量 `RepositorySet`：`crates/agentdash-application/src/project/management.rs:55-74,110-134,271-376`，与 architecture/repository spec 相反。
- 触发器：新增字段、权限、inline source、event、删除引用或安装来源。
- 爆炸半径：HTTP/MCP/tool行为漂移；测试fixture必须构造无关repos；route mapper开始拥有领域不变量。
- 目标稳定边界：按业务意图建立小型 deps/command/query services；HTTP/MCP/tool仅共享 application command，不共享 route helper。
- Work package `BA-07 asset-usecase-seams`：依次迁移 ProjectGrant、StoryMutation、BackendAccess、ProjectAgent、VfsMount、Workspace status/delete；每迁一项 hard-delete route/MCP duplicate producer。
- Guard：AST/source ownership rule禁止 `agentdash-api`/`agentdash-mcp` 调用 write repository；application public use case禁止参数 `&RepositorySet`。

### P1-2：Canvas promotion 的 owner 漂移到 route

- 证据：route读取 revision、build package、store archive、install artifact：`crates/agentdash-api/src/routes/interactions.rs:332-398`；spec声明 owner应是 application canvas promotion：`.trellis/spec/backend/architecture.md:83`。
- 触发器：package manifest、artifact generation、installation、authorization或重试语义变化。
- 爆炸半径：Canvas、Extension artifact、installation、contract mapper同步修改；partial failure同 P0-5。
- 目标/Work package `BA-08 canvas-promotion-command`：建立 application `PromoteCanvasExtensionCommand`，输入 actor/definition/revision/target key/idempotency，内部消费 artifact operation port；route只map DTO；负向测试覆盖revision membership、permission、storage/DB/install失败。
- Guard：route import `build_interaction_definition_extension_package` 或 `store_extension_package_archive` 即失败。

### P1-3：Codex OAuth flow 的 canonical state 属于 API 进程内全局变量

- 证据：`CodexOAuthFlowStore = Arc<Mutex<HashMap<...>>>` 与 `OnceLock`：`crates/agentdash-api/src/routes/llm_providers.rs:64-85`；prepare/status/claim/complete/fail都直接操作该 map：`:544-716`。
- 触发器：API重启、多实例负载均衡、credential save前后连接中断、flow完成期间panic。
- 爆炸半径：有效登录无法完成/查询；同flow落到另一实例为404；外部token交换成功但本地状态/credential不一致。
- 目标/Work package `BA-09 durable-oauth-operation`：OAuth flow repository/command service持久化 state、PKCE challenge、target、expiry、claim generation、terminal result；credential写与flow terminal transition使用事务/幂等 completion receipt；cleanup worker。
- Guard：restart/multi-instance integration test；禁止 API route `static` mutable business store。

### P1-4：Project/Story create/update 的 inline document 与 owner document 非原子

- 证据：Project create/update先写 Project后同步inline：`project/management.rs:55-74,110-124`；Story同样：`story/management.rs:53-95`。
- 触发器：inline validation/storage失败。
- 爆炸半径：请求失败但owner document已改变；VFS与context config暂时/永久不一致。
- 目标/Work package `BA-10 contextual-document-transaction`：把 owner config + inline files 作为同一个 semantic document command；write前完成path/content validation，Postgres tx提交二者；Project builtins provisioning作为独立幂等 bootstrap operation，不和Project create伪装成同一原子事务。
- Guard：owner config引用的 inline file set 与 inline table双向一致性测试；任一步失败无可见新revision。

### P1-5：Identity Directory 的浏览/投影写权限缺少显式产品合同

- 证据：所有 `/directory/*` 只要求 `CurrentUser`：`identity_directory.rs:91-110,112-318`；resolve会把provider对象upsert到本地 projection：`:235-318`。
- 触发器：企业目录包含受限用户/组，或 provider search是昂贵/审计操作。
- 爆炸半径：企业主体枚举、projection污染/增长、授权subject picker暴露超范围对象。
- 证据缺口：AuthProvider 的 route-level `authorize` 可能由企业 integration 做额外限制，但仓库内没有可验证的默认 directory policy；因此不定P0。
- 目标/Work package `BA-11 directory-access-contract`：先产品决策 browse/resolve权限和字段可见性；建立 `DirectoryAccessPolicy`/query service与audit，provider和projection返回同一过滤合同。
- Guard：enterprise negative tests覆盖普通成员/admin、provider fallback与projection path同等过滤。

### P1-6：Runner claim 跨 token、Backend、ProjectBackendAccess、usage 的一致性只靠顺序与错误分类

- 证据：public claim进入 application：`runner_registration_tokens.rs:150-186`；application tests覆盖idempotency/conflict，但 deps仍是全量 repos且操作横跨 token/B/access（`backend/runner_registration.rs:177-313`及其 tests `:887-1118`）。
- 目标/Work package `BA-12 runner-enrollment-transaction`：保留统一 enrollment owner（这是正确边界），把 token claim generation、stable backend ensure、access grant、usage receipt纳入显式 command transaction/operation id；relay token只在 committed result后返回。
- Guard：同token/同machine并发、access写失败、usage写失败、response丢失后的retry integration tests。

## 合理内聚与不应机械拆分的反例

1. `ProjectAuthorizationService` 将 role、group/user subject、template visibility统一在 domain：`project/authorization.rs:46-151`。问题是部分 caller选错 permission或把 last-owner transaction放在外层，不是 service 本身需要拆散。
2. `BackendAuthorizationService` 组合 Backend、Project、ProjectBackendAccess 是必要的跨聚合 query policy：`backend/authorization.rs:50-270`。它应保持 application service；`backends.rs` 的 list/get/browse复用它是正确方向。
3. `WorkspacePlacementService` 把 detect fact、binding、inventory合并，是合理高连接 command owner；不要按表拆成三个 service。
4. `CanvasDefinitionService` 对 revision、lineage、owner access、publish/copy/archive保持单一 owner，并通过 repository `commit_revision(expected_current_revision_id)`吸收并发，是值得复制的模式。
5. Workflow template install 把 procedures+graph作为一个 transaction bundle，且有 PostgreSQL integration test：`workflow_repository.rs:287-419,1141-1219`。这说明“多表”本身不是问题，缺少语义 transaction port才是问题。
6. `agentdash-workspace-module::build_workspace_modules` 统一 Extension/Canvas/Operation projection，HTTP与runtime tool复用该纯投影函数；其依赖多是 projection cohesion，不应拆成按来源的重复 mappers。
7. `AuthSessionService` 只拥有 hashed token session、expiry/revoke/cleanup，HTTP middleware只做认证编排；这是窄 application owner。
8. Marketplace provider discovery/fetch属于 interface adapter，写入仍调用 Shared Library import/refresh；当前 provider fan-out不等同于第二资产事实源。
9. Execution profile query同时读取effective LLM catalog和Complete Agent availability是面向UI的组合 projection；只要不写回第二份availability事实，就应作为合理 query hub保留。

## Fact 重复、contract 与前端重复解释风险

- Story 的 producer重复已证实：HTTP application与MCP direct repository。其后果不仅是代码重复，还包括 inline同步、validation、event语义不同。
- Task 同一 facts 有 HTTP、MCP、runtime tool三个command入口；当前至少 authorization policy与并发写机制不同。目标不是删入口，而是共用同一 actor-aware command port。
- Workspace Module 后端已有 canonical generated DTO和纯projection builder。HTTP包含当前用户personal Canvas、agent runtime只允许Project Canvas，属于actor surface差异；前端不应再从 Extension/Canvas raw stores自行重建 module visibility。需要 cross-layer reviewer核验前端是否只消费 descriptor。
- ProjectAgent、MCP、Skill、Mount 的 `InstalledAssetSource` 是唯一来源事实；`source-status`按该字段查询。前端若用本地 key/payload推断“已安装/可更新”，会重复解释后端 owner。
- Extension artifact 的 `archive_digest`、`manifest_digest`、LibraryAsset `payload_digest` 是不同摘要域；当前 publish helper正确区分，但缺少完整 operation generation。前端只能消费后端 artifact summary，不能把Library payload存在等同于package complete。
- Settings 用开放 string key存放跨模块配置，masking由key substring决定：当前仍有单一DB事实源，但 schema/敏感性是隐式 data-shape coupling，建议列P2治理，不抢占本次P0/P1波次。

## Migration、object storage 与 owner ledger

当前 `0001_init.sql` 已对少数强关系使用 FK cascade（如 ProjectBackendAccess、RunnerToken、Lifecycle child），但 ProjectAgent、Workspace、Story、Skill、MCP、Mount、ExtensionInstallation、artifact owner等多处只保存 text owner id。Project delete因此不得不枚举 repository，而该枚举没有 completeness gate。

建议 owner ledger 至少包含：

| Resource | Canonical owner | Physical store | Delete/retention target |
| --- | --- | --- | --- |
| Project metadata/grants | Project aggregate | `projects`, `project_subject_grants` | grant invariant tx；Project deletion root |
| Workspace+bindings | Workspace aggregate | `workspaces`, `workspace_bindings` | repository tx；Project cascade/restrict |
| Story+inline | Story contextual document | `stories`, `inline_fs_files` | same command tx/tombstone |
| Lifecycle task facts | LifecycleRun revision | `lifecycle_runs.tasks` JSONB | CAS only；随run/project lifecycle |
| Project Agent/MCP/Skill/Mount/Extension | respective Project asset aggregate | dedicated tables + inline | aggregate delete command |
| LibraryAsset | Shared Library | `library_assets` | scope/owner retention；package completeness generation |
| Extension artifact metadata | Artifact aggregate | `extension_package_artifacts` | references/owner lifecycle |
| Extension archive bytes | Artifact object | filesystem/object store | content-addressed refcount/mark-sweep or outbox delete |
| StateChange | Project event ledger | `state_changes` | event与command同commit；Project retention |
| OAuth flow | Auth operation | 当前内存（目标DB） | expiry cleanup、terminal receipt |

Guard 必须从这张 ledger 生成 schema/object checks；owner root不存在时检查本身应失败，不能静默跳过。

## 可达性、未挂载代码与证据缺口

### 已确认 production reachable

- `routes.rs:73-124` 挂载 projects、Project VFS、LLM、Project Agent、runner tokens、MCP preset、skill、workspace、backend access、story、interaction、task plan、backend、settings、Shared Library、marketplace、extension、workspace module、execution profiles。
- MCP router在 `routes.rs:55-71,127-129` 构造并受同一认证中间件保护；Story/Workflow tools为生产可达。
- Extension Gateway provider在 bootstrap注册；workspace detect/git最终进入 relay/local handler。
- runtime task/workspace module tools进入 product tool catalog的深层 composition属于 Agent Runtime/执行 reviewer主责；本研究只确认其application实现和与业务owner的交点。

### 未挂载 / residual

- `crates/agentdash-application/src/task/lock.rs::TaskLockMap` 除自测外无production引用；不能算并发保护，建议在采用DB CAS后删除，不要把process lock重新挂载成跨实例方案。
- Spec `.trellis/spec/backend/architecture.md:83` 提到 `agentdash-application::canvas::promotion`，当前仓库没有该owner，promotion仍在 API route；这是未实现的目标边界，不是隐藏production实现。

### 证据缺口

- Identity provider 的企业实现可能在 `AuthProvider::authorize(path, method)` 中进一步限制 `/directory/*`，但默认/生产 integration policy未在本次业务资产代码中找到可执行矩阵；因此列P1待确认，不直接判跨租户P0。
- Runner claim 的各 repository concrete methods是否在同一个数据库transaction中被更深层 adapter统一包裹，本次从 application调用形态未发现统一command transaction；需 failure injection确认实际partial state。
- Settings `set_batch` 可见为repository batch，但本次未展开所有adapter；其核心风险低于P1。
- Project delete未穷举所有执行面/Agent Runtime owner documents（任务明确不围绕Agent Runtime）；父报告应与 execution/composition research交叉，确认删除清单是否还漏 canonical runtime/product bindings。
- Extension archive bytes可能被Project-owned和LibraryAsset-owned metadata共享同一 `storage_ref`；删除策略必须先建引用图，不能直接把metadata cascade翻译为物理删object。

## 未覆盖 / 转交清单

- Agent Runtime、AgentRun binding、Host/Service/Wire、Hook、Terminal、Relay recovery、Tauri：转交 `backend-execution-composition.md`；本文件只记录业务资产入口与其交点。
- Workflow/Lifecycle/Routine/Companion/Channel/Permission/Capability 的 reducer/gate/dispatch完整审计：转交 `backend-control-orchestration.md`；本文件只覆盖其作为 Project delete、Task owner、Project Agent/Runner consumer 的关系。
- Frontend store/mapper/renderer 是否重复解释 Project raw event、Shared Library source status、Workspace Module visibility、Extension/Canvas tab：转交 `frontend-crosslayer-coupling.md`。
- Execution profiles 的 Complete Agent live availability、adapter completeness和deferred binding：转交 execution/composition reviewer；本文件已核验其HTTP query与LLM credential consumer。
- Project VFS provider/materialization、本机path semantics：转交 execution/VFS reviewer；本文件只审计Project Mount aggregate CRUD、inline生命周期与SL install。
- LLM provider外部模型probe的网络重试/credential redaction深审不在本文件；已覆盖其业务授权、持久凭据owner与OAuth operation state。
- `identity_directory` 的最终企业可见性产品决策需要用户/产品确认；代码证据不足以决定“所有登录用户可浏览全目录”是否预期。

## 建议整改依赖图

```text
Wave 0 characterization / owner ledger
  BA-01 workspace-setup-admission
  BA-02 lifecycle-task-command-authority
  BA-03 project-grant-transaction
  BA-04 aggregate-deletion-ledger
  BA-05 asset-operation-transaction
      ├─ BA-06 project-asset-delete-commands
      ├─ BA-08 canvas-promotion-command
      └─ BA-10 contextual-document-transaction
  BA-09 durable-oauth-operation
  BA-11 directory-access-contract (requires product decision)
  BA-12 runner-enrollment-transaction
      ↓
  BA-07 asset-usecase-seams（逐owner迁移并hard delete旧producer）
      ↓
  physical crate/directory cleanup
```

优先级理由：先封跨租户/越权和lost-update入口，再建立transaction/object owner；随后迁移重复producer，最后才整理crate/目录。任何“先加 facade 保留旧route/repository写路径”的方案都会继续保留第二authority，不符合预研期正确终态。

## Caveats / Not Found

- 本研究只写入本文件，没有运行测试、migration或修改产品代码。
- 结论基于 2026-07-26 工作区当前可见源码；并行会话可能继续改变行号，父报告合并前应以符号名重新 `rg` 校验 P0/P1 锚点。
- 没有把 spec 单独当成实现证据；每个P0/P1至少包含production route/application/persistence代码。Identity Directory例外已明确标成证据缺口。
- 没有把高 fan-out composition root自动判为问题；BackendAuthorization、WorkspacePlacement、CanvasDefinition、WorkspaceModule projection、execution profile query均记录了合理内聚理由。
