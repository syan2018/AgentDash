# Research: project-workspace-backend-placement

- Query: 单域对抗性架构审查：Project / Workspace / Backend Placement；检查 project / workspace / backend / local runner enrollment / machine and workspace identity / settings 的事实源、归属关系、本机后端、云端后端、workspace identity、settings 是否存在重复或绕路路径，并对照 06-14 baseline。
- Scope: internal
- Date: 2026-06-30

## Findings

### Files Found

- `.trellis/tasks/06-30-module-adversarial-review/{prd.md,design.md,implement.md,check.jsonl}`: 当前对抗审查任务输入。
- `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md`: 06-14 baseline 总结，指出 Tauri main 重新实现 profile/claim 协议、desktop shell 不够薄。
- `.trellis/tasks/06-14-module-overdesign-review/research/03-vfs-local-relay-extension.md`: 06-14 相关 baseline 明细，建议 profile/claim 下沉到 `agentdash-local`。
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md`: Project / Backend / Workspace routing 事实源规范。
- `.trellis/spec/cross-layer/desktop-local-runtime.md`: Desktop 与 standalone runner runtime 归属规范。
- `crates/agentdash-application/src/backend/management.rs`: 本机 backend enrollment 的统一 application use case。
- `crates/agentdash-application/src/backend/runner_registration.rs`: runner registration token claim 流程。
- `crates/agentdash-application/src/backend/project_access.rs`: ProjectBackendAccess grant 创建/恢复逻辑。
- `crates/agentdash-application/src/backend/authorization.rs`: backend 访问鉴权，使用 ProjectBackendAccess active grant。
- `crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs`: backend / old user_preferences Postgres 读写。
- `crates/agentdash-domain/src/workspace/{entity.rs,identity_contract.rs,repository.rs}`: Workspace 逻辑身份、identity contract 与 repository port。
- `crates/agentdash-application/src/workspace/{detection.rs,backend_sync.rs,resolution.rs}`: Workspace detect、directory fact helper、candidate/sync。
- `crates/agentdash-api/src/routes/{backend_access.rs,workspaces.rs,settings.rs}`: project backend access、workspace 管理、settings HTTP routes。
- `crates/agentdash-local/src/{machine_identity.rs,runner_config.rs,runner_claim.rs,desktop_runner_host.rs}`: standalone runner / desktop embedded runner 本地 runtime 归属。
- `crates/agentdash-local-tauri/src/main.rs`: Tauri command adapter，同时仍持有 desktop profile/claim/settings 逻辑。
- `packages/core/src/local-runtime/index.ts`, `packages/app-web/src/desktop/localRuntimeBridge.ts`, `packages/app-tauri/src/runtimeApi.ts`: frontend local runtime port/adapters。
- `packages/app-web/src/features/settings/ui/{SettingsPageContent.tsx,SettingsSystemSections.tsx}` and `packages/app-web/src/api/settings.ts`: Settings UI/API 消费。

### Related Specs

- `.trellis/spec/cross-layer/project-backend-workspace-routing.md`: `ProjectBackendAccess` 是 project -> backend 授权事实源；`backend` 表达 runtime identity；Workspace 表达 project 下 logical workspace identity；Workspace physical placement 应通过 directory fact 维护 inventory/binding。
- `.trellis/spec/cross-layer/desktop-local-runtime.md`: Desktop app shell 只桥接 UI/lifecycle；`agentdash-local` owns machine identity、runner lifecycle、standalone runner config；`agentdash-local` 可作为 desktop embedded runner library 与 standalone binary 共同事实源。
- `.trellis/spec/backend/architecture.md`: backend application use cases 应承载业务编排，API route 不应拥有跨 repo / domain 的核心流程。

### Baseline Comparison

- 06-14 baseline 的 backend enrollment 问题已明显收束：`management.rs:18-22` 注释明确 Desktop access token 与 Runner registration token 进入同一 source model，再走同一条 normalize -> backend_id -> device -> `LocalBackendClaim` -> `ensure_local_backend` -> token 流程。
- 06-14 baseline 的 runner backend “project-baked identity” 已解决：`management.rs:228-230` 明确 stable backend id 不包含 project id；`runner_registration.rs:234-235` 明确 project 可见性由 `ProjectBackendAccess` active projection 承载；测试 `runner_registration.rs:976-1008` 覆盖跨 project 同 machine/owner/slot 解析为同一 backend id，且每个 project 各自落 active access。
- 06-14 baseline 的 Tauri 过厚问题只部分改善：`agentdash-local/src/desktop_runner_host.rs:1-3` 已把 runner 启动/复用/停止/日志收束到 `agentdash-local`，但 Tauri main 仍持有 profile persistence、ensure claim payload/HTTP/response validation、desktop settings file IO。

### Code Patterns

- `crates/agentdash-application/src/backend/management.rs:207-314`: `enroll_local_backend` 统一 Desktop 与 Runner enrollment。
- `crates/agentdash-application/src/backend/management.rs:259-264` and `480-496`: stable local backend id 由 `machine_id + share_scope_kind + share_scope_id + capability_slot` 派生，不含 project id。
- `crates/agentdash-application/src/backend/runner_registration.rs:259-273`: runner claim 后调用 `ensure_project_backend_access_grant`，把 project 授权落在 ProjectBackendAccess。
- `crates/agentdash-application/src/backend/project_access.rs:48-129`: grant create/reactivate/conflict 收束为统一 helper。
- `crates/agentdash-application/src/workspace/backend_sync.rs:38-42`: 已存在 `WorkspaceDirectoryFact { binding, inventory }` 组合模型。
- `crates/agentdash-application/src/workspace/backend_sync.rs:50-68`: 只构造 inventory 的 helper。
- `crates/agentdash-application/src/workspace/backend_sync.rs:70-95` and `118-146`: 构造并应用 directory fact 到 WorkspaceBinding。
- `crates/agentdash-api/src/routes/backend_access.rs:246-288`: project backend inventory register route 直接 detect，然后只 upsert `BackendWorkspaceInventory`。
- `crates/agentdash-api/src/routes/workspaces.rs:473-616`: bind-discovered route 自己 detect、构造 fact、upsert inventory、apply binding。
- `crates/agentdash-api/src/routes/workspaces.rs:735-886`: create/update workspace shape 推导与 binding hydration 仍在 API route 内。
- `crates/agentdash-local-tauri/src/main.rs:107-159`: Tauri main 自定义 `RuntimeStartRequest`、`LocalRuntimeProfile`、`DesktopAppSettings`。
- `crates/agentdash-local-tauri/src/main.rs:244-274`: Tauri command 直接读写 profile file。
- `crates/agentdash-local-tauri/src/main.rs:638-809`: Tauri main 直接构造 ensure payload、POST `/api/local-runtime/ensure`、校验 response、normalize profile/start request。
- `crates/agentdash-local/src/desktop_runner_host.rs:36-110`: runtime lifecycle 已在 local crate 内部收束。
- `crates/agentdash-domain/src/settings.rs:22-101`: 新 `SettingsRepository` 有 system/user/project scope model。
- `crates/agentdash-api/src/routes/settings.rs:47-118` and `157-191`: Settings route 按 scope 读写 `settings_repo`，project scope 走 project permission。
- `crates/agentdash-domain/src/backend/repository.rs:25-26` and `crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs:295-320`: 旧 `BackendRepository::get_preferences/save_preferences` 仍读写 `user_preferences`。
- `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:341-366`: Runtime session 从新 settings 读取 `agent.pi.user_preferences`。
- `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:178-189` and `crates/agentdash-api/src/routes/lifecycle_agents.rs:1578-1590`: AgentRun workspace/mailbox 仍从旧 backend preferences 读取 `hide_system_steer_messages`。

### Issue 1: Workspace directory fact 写路径分散，inventory 与 binding 未完全收束到一个 application use case

- Classification: 重复事实源 / 路径冗余 / API route 过厚 / workspace placement 归属漂移。
- Priority: P1。
- Evidence:
  - `backend_sync.rs:38-42` 已有 `WorkspaceDirectoryFact` 组合模型，`backend_sync.rs:70-95` 可从 detect 结果构造 binding+inventory，`backend_sync.rs:118-146` 可把 fact 应用到 workspace binding。
  - 但 `backend_access.rs:246-288` 的 manual register 路径直接调用 `invoke_workspace_detect`，再用 `workspace_inventory_from_detection` 只 upsert `BackendWorkspaceInventory`，没有走 `WorkspaceDirectoryFact` / binding apply。
  - `backend_access.rs:290-327` 又提供单独 candidates/sync route；`backend_sync.rs:177-242` 的 sync 只在唯一匹配 workspace 时才应用 binding。
  - `workspaces.rs:473-616` 的 bind-discovered route 另一套 detect + fact + inventory + binding 写入；`workspaces.rs:735-886` 的 create/update/hydrate 逻辑仍在 API route 内完成 workspace shape 与 binding 事实推导。
- Impact:
  - 同一次 directory detect 的结果可能先成为 inventory，后续再靠 candidates/sync 或 bind-discovered 进入 WorkspaceBinding；状态、priority、source、last_verified_at 的演进路径不唯一。
  - Project settings 的手动登记、Workspace create/update、bind-discovered、sync 各自维护 physical placement 的不同切面，后续改 identity matching/source/status 时需要跨 route/application helper 同步。
  - API route 承载业务编排，削弱 application 层作为 workspace placement owner 的边界。
- Convergence Boundary:
  - 在 application 层建立 `WorkspaceDirectoryFactService` / `WorkspacePlacementService` 一类 use case，统一承载 detect result -> directory fact -> inventory/binding transaction。
  - API route 只做权限、DTO、调用 use case；manual register 应返回 inventory 与明确的 candidate/applied binding 结果，不再只写 inventory。
  - create/update/bind-discovered/sync 共享同一个 fact apply transaction 与 matching policy；Advanced Maintenance 仍可只维护 Workspace bindings，但必须通过同一 policy 表达“只改 binding、不登记 backend inventory”的意图。
- 06-14 baseline:
  - 06-14 未直接覆盖 workspace directory fact，但它是同类“本地/后端 placement 事实路径分散”问题；相对当前 06-30 spec，这是新的 P1 收束项。

### Issue 2: Desktop Tauri main 仍拥有 local runtime profile/claim/settings 细节，06-14 thin shell 问题未完全收束

- Classification: 06-14 residual / 模块过厚 / 重复 DTO / desktop backend placement 绕路。
- Priority: P1。
- Evidence:
  - 已改善部分：`agentdash-local/src/desktop_runner_host.rs:1-3` 明确 Tauri 负责桌面生命周期与命令桥接，runner 启动、复用、停止、日志收束在 local crate；`desktop_runner_host.rs:36-110` 实现 ensure/start lifecycle，`254-290` 固定 supervisor owner 与 `registration_source=desktop_access_token`。
  - 残留部分：`agentdash-local-tauri/src/main.rs:107-159` 在 Tauri main 内定义 `RuntimeStartRequest`、`LocalRuntimeProfile`、`DesktopAppSettings`。
  - `agentdash-local-tauri/src/main.rs:206-218` and `244-274` 直接实现 desktop settings/profile load/save/delete。
  - `agentdash-local-tauri/src/main.rs:638-809` 在 Tauri main 内 normalize start request、构造 ensure payload、POST `/api/local-runtime/ensure`、校验 machine/scope/capability/registration_source、normalize profile。
  - `packages/app-web/src/desktop/localRuntimeBridge.ts:61-132` 管理 desktop auto-connect 状态并调用 runtimeStart，`156-184` 创建/保存 auto-connect profile 默认值；TS core 还在 `packages/core/src/local-runtime/index.ts:190-247` 维护对应 local runtime DTO/port。
- Impact:
  - Desktop runtime claim protocol、profile persistence、server origin normalization、settings file IO 同时散在 TS bridge、Tauri main、`agentdash-local` local library、cloud API DTO 中。
  - 后续变更 `/api/local-runtime/ensure` 字段、scope/capability slot、registration_source 规则、profile 结构或 desktop settings 文件格式时，需要跨 Tauri main 与 local crate 同步，容易回到 06-14 所说的“desktop shell 不够薄”。
  - Desktop settings 是本地事实，但当前 concrete owner 是 Tauri main 文件，而不是 `agentdash-local` 的 desktop runtime profile/settings API；这让 desktop embedded runner 与 standalone runner 的本地 runtime 归属不对称。
- Convergence Boundary:
  - 在 `agentdash-local` 内新增或补齐 `desktop_profile` / `desktop_claim` / `desktop_settings` 模块，持有 Rust DTO、profile load/save/delete、settings load/save、server origin normalization、ensure claim payload/response validation。
  - Tauri commands 只做 adapter：接收 UI request、调用 `agentdash-local` API、返回 DTO；`DesktopRunnerHost::ensure_started_with` 继续作为 lifecycle owner。
  - TS `LocalRuntimeClient` 保持 UI port，不拥有 server-side claim validation 语义。
- 06-14 baseline:
  - 这是 06-14 `agentdash-local-tauri/src/main.rs` 重实现 profile/claim 协议问题的残留；当前生命周期已下沉，claim/profile/settings 尚未完全下沉。

### Issue 3: 旧 `user_preferences` 与新 scoped `settings` 并存，用户偏好事实源分裂

- Classification: 重复事实源 / settings 归属漂移 / backend repository 历史职责残留。
- Priority: P2。
- Evidence:
  - 新 settings 模型在 `agentdash-domain/src/settings.rs:22-101` 定义 `SettingScope { system,user,project }` 和 `SettingsRepository`，`agentdash-api/src/routes/settings.rs:47-118` 使用 `settings_repo` list/update/delete，`157-191` 按 system/user/project scope 做权限解析。
  - 新用户偏好 UI 写 `agent.pi.user_preferences`：`packages/app-web/src/features/settings/ui/SettingsSystemSections.tsx:24-26` 定义 key，`40-87` 读写字符串列表。
  - Runtime session 启动读取新 settings：`agentdash-application-runtime-session/src/session/launch/preparation.rs:116-140` 构建 guidelines frame，`341-366` 从 user scope 读取 `agent.pi.user_preferences`。
  - 旧偏好仍挂在 backend：`agentdash-domain/src/backend/entity.rs:102-110` 定义 `UserPreferences`，`agentdash-domain/src/backend/repository.rs:25-26` 暴露 `get_preferences/save_preferences`，`agentdash-infrastructure/src/persistence/postgres/backend_repository.rs:295-320` 读写 `user_preferences WHERE key='prefs'`。
  - 旧偏好仍有消费：`agentdash-application-agentrun/src/agent_run/workspace/query.rs:178-189` 和 `agentdash-api/src/routes/lifecycle_agents.rs:1578-1590` 用旧 `hide_system_steer_messages` 控制 mailbox state。
  - DB 初始迁移同时创建 `settings` 与 `user_preferences`：`crates/agentdash-infrastructure/migrations/0001_init.sql:670-676` and `737-740`。
- Impact:
  - 用户偏好作为 settings 事实已经迁到 scoped settings，但部分 UI/AgentRun 行为仍通过 BackendRepository 读取单行 `user_preferences`；用户在新 Settings 页面配置的 `agent.pi.user_preferences` 与旧 `hide_system_steer_messages` 不属于同一事实模型。
  - `BackendRepository` 继续承担与 backend placement 无关的 user preference port，扩大 backend 模块职责，降低 settings scope 模型的权威性。
  - 当前影响集中在用户偏好与 mailbox 显示，不直接破坏 ProjectBackendAccess / backend enrollment 主链路。
- Convergence Boundary:
  - 以 `SettingsRepository` 作为 settings 唯一事实源，把 `hide_system_steer_messages` 迁为 scoped setting key，并把 AgentRun workspace/lifecycle 消费改为 settings_repo。
  - 从 `BackendRepository` 移除 `get_preferences/save_preferences` 与 `UserPreferences` 的偏好职责；如 view config 仍需保留，应单独界定为 view repository 或 settings projection。
  - migration 需要把 `user_preferences.key='prefs'` 中仍有价值的字段迁入 scoped `settings`，随后删除旧表/旧 port。
- 06-14 baseline:
  - 06-14 主要关注 backend/local/relay，不直接点名 settings；当前问题是 backend 模块历史职责在 settings scope 模型引入后未完全收束。

### Positive Findings / Not Issues

- `ProjectBackendAccess` 是 project -> backend 授权事实源，当前主链路收束良好：
  - `runner_registration.rs:259-273` runner claim 后创建/恢复 project access；
  - `project_access.rs:48-129` 统一 grant 行为；
  - `authorization.rs:105-244` 列表与单条鉴权使用 active grants。
- runner enrollment 去 project 化完成：
  - `management.rs:228-230` 明确 stable identity 不含 project id；
  - `runner_registration.rs:976-1008` 有跨 project 同 backend id 的测试。
- Machine identity 归属在 local crate 内：
  - `agentdash-local/src/machine_identity.rs:14-34` 负责 load/create；
  - `runner_config.rs:218-380` 负责 runner CLI/env/file/default config precedence；
  - `runner_claim.rs:51-120` 负责 standalone runner claim。
- Workspace logical identity 模型方向正确：
  - `workspace/entity.rs:12-31` 把 Workspace 定义为 project 下逻辑身份，bindings 承载 backend/root placement；
  - `workspace/identity_contract.rs:136-241` 负责 identity payload normalize/match；
  - 问题集中在 directory fact 写路径，不在 identity contract 本身。
- `runner_claim.rs:122-182` 的 direct credentials 路径不单独列为问题：`desktop-local-runtime.md` 允许 standalone runner 通过 `--backend-id --relay-ws-url --auth-token` 直接运行；只需确保其不绕过 registration/enrollment 的推荐配置路径。

### External References

- None. 本报告只使用仓库内任务、spec 与业务代码。

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)`；本报告按用户显式指定的 `.trellis/tasks/06-30-module-adversarial-review` 作为任务路径写入。
- 未修改业务代码，未运行全量测试。
- 未做数据库实跑验证；settings 迁移建议基于 schema 与读写路径静态证据。
- 未把旧 inventory source string 兼容解析列为问题；当前审查关注活跃事实源与模块归属，兼容解析本身不构成绕路路径。
