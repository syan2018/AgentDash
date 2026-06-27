# 本机执行面 Enrollment 路径收束

## Goal

收束 AgentDash 当前两条本机执行面挂载路径，让 **Windows Desktop Local Runtime** 与 **独立 Local Runner** 在云端 backend 创建/复用、relay 凭据签发、状态诊断和前端展示上共享同一套概念模型，同时保留两种不同的授权入口：

- Desktop Local Runtime：用户已登录桌面 App，使用用户 access token 调用 `/api/local-runtime/ensure`，默认挂到 user/personal scope。
- Standalone Local Runner：服务器无 UI、无用户登录态，使用 project-scoped runner registration token 调用 `/api/local-runtime/runner/claim`，默认挂到 project scope。

目标不是把两条 API 强行合并，而是让它们在进入云端 application service 后共享 enrollment 语义、返回凭据形状、诊断事实源和前端理解方式，减少后续 Desktop 与 Runner 行为漂移。

## User Value

- 桌面用户继续保持“登录后自动挂本机 runtime”的体验，不需要理解或复制 registration token。
- 服务器部署用户通过云端生成 runner registration token 和 `agentdash-local setup` 命令完成无 UI 部署，不需要保存用户 access token。
- 前端可以用统一的“本机执行面 / Local Execution Backend”视角展示桌面 runtime 与 server runner，而不是让用户理解两套 backend 挂载机制。
- 后端实现者可以在同一处维护 backend id 稳定生成、relay auth token 签发、scope/access 投影和 runtime metadata，降低重复逻辑和测试遗漏。
- 诊断和发布验收可以围绕统一的 relay credential 与 registration source 模型检查，不再分别猜测 Desktop 与 Runner 状态。

## Confirmed Facts

- Desktop Tauri 当前通过 `ensureDesktopLocalRuntimeStarted(accessToken)` 自动启动本机 runtime；它由登录态触发，读取/创建 desktop profile，然后调用 Tauri `runtime_start`。
- `agentdash-local-tauri` 的 `runtime_start` 会调用 `/api/local-runtime/ensure`，请求带用户 access token，云端返回 `backend_id`、`relay_ws_url`、`auth_token` 后启动本机 runtime。
- `/api/local-runtime/ensure` 当前依赖 `CurrentUser`，适合 desktop/user scope enrollment。
- 独立 runner 当前使用 `agentdash-local setup` / `run`，通过 registration token 调用 `/api/local-runtime/runner/claim` 领取 `backend_id`、`relay_ws_url`、`auth_token`。
- Runner registration token 是 project-scoped；metadata/list/revoke 不返回明文 token，create/rotate 返回一次性 `registration_token`。
- `/ws/backend` 接受的是 server-issued backend relay `auth_token`，不是 desktop access token，也不是 runner registration token。
- 诊断层已经区分 `desktop_access_token` 与 `runner_registration_token` 两种 `registration_source`。
- 当前前端已有 Desktop Local Runtime 设置和诊断入口，但还没有 Project 级 Runner registration token 管理和部署命令生成 UI。

## Requirements

- 保留两个外部认证入口：
  - `/api/local-runtime/ensure` 继续服务 Desktop Local Runtime，认证来源是用户 access token。
  - `/api/local-runtime/runner/claim` 继续服务 Standalone Local Runner，认证来源是 runner registration token。
- 在 application layer 引入或收束到统一的本机 backend enrollment use case，负责共享这些行为：
  - 根据 `machine_id + share_scope_kind + share_scope_id + capability_slot` 创建或复用 stable backend。
  - 签发或复用 backend relay `auth_token`。
  - 返回统一的 relay credentials：`backend_id`、`relay_ws_url`、`auth_token`。
  - 写入统一的 backend metadata：machine identity、display name、executor enabled、client version、device、capability slot、registration source。
  - 对 project-scoped runner 确保 `ProjectBackendAccess` active projection。
- 定义统一的 enrollment source 模型：
  - `desktop_access_token`：来自已登录桌面 App 的用户授权。
  - `runner_registration_token`：来自项目级服务器 runner 部署令牌。
- 收束 Desktop ensure response 与 Runner claim response 的概念形状，至少共享：
  - `backend_id`
  - `name`
  - `relay_ws_url`
  - `auth_token`
  - `machine_id`
  - `machine_label`
  - `share_scope_kind`
  - `share_scope_id`
  - `capability_slot`
  - `registration_source`
  - `claimed_at` 或等价 claim/update timestamp
- 收束本机端对 relay credentials 的处理：
  - Desktop profile 与 Runner config 可以保留各自文件格式，但都应把 `backend_id + relay_ws_url + auth_token` 视为 server-issued relay credentials。
  - 本机端日志、status、doctor、diagnostics UI 都必须脱敏 token-bearing 字段。
- 前端产品模型收束为“本机执行面 / Local Execution Backend”：
  - Desktop runtime 显示为当前设备上的 user-scoped execution backend。
  - Standalone runner 显示为 project-scoped service-managed execution backend。
  - 设置/诊断中明确显示 registration source 和 scope，避免把 server runner 当成桌面 runtime 控制。
- 增加 Project 级 Runner token 管理入口的产品需求：
  - 创建 token。
  - 列表展示 token metadata。
  - 撤销 token。
  - 轮换 token。
  - 创建/轮换后展示一次性明文 token。
  - 基于当前云端 origin 生成 `agentdash-local setup ...` 复制命令。
- 文档和验收流程要明确两条用户路径：
  - Desktop：登录 -> 自动 ensure -> runtime connect。
  - Runner：创建 token -> 复制 setup 命令 -> claim -> service online。

## Scope Boundaries

- 两个 API endpoint 保持面向不同认证来源，原因是 Desktop 与 Server Runner 的安全边界和用户体验不同。
- Desktop 路径继续使用用户登录态，原因是桌面 App 已经拥有 access token，自动连接是核心易用性。
- Server Runner 路径继续使用 project-scoped registration token，原因是服务器常驻进程不应保存用户 access token，且需要项目级撤销、过期、轮换和审计。
- 收束重点放在 application service、relay credential 模型、诊断事实和前端展示语义，原因是这些部分才是两条路径后续最容易漂移的共同核心。
- 第一阶段不要求把 Desktop profile JSON 与 Runner TOML 合并成同一种文件格式，原因是它们运行形态和操作系统部署语境不同。

## Acceptance Criteria

- [ ] PRD/Design 明确 Desktop access-token enrollment 与 Runner registration-token enrollment 的共同模型和差异边界。
- [ ] 后端存在统一或等价收束的 enrollment service/use case，Desktop ensure 与 Runner claim 复用同一套 backend create/reuse、relay credential issue/reuse、metadata projection 逻辑。
- [ ] `/api/local-runtime/ensure` 继续通过用户 access token 工作，且桌面自动连接流程不需要 runner registration token。
- [ ] `/api/local-runtime/runner/claim` 继续通过 runner registration token 工作，且独立 runner 不需要保存用户 access token。
- [ ] Desktop ensure 与 Runner claim 的 response 在 relay credential 和 backend identity 字段上保持同构或有明确 mapping。
- [ ] `registration_source` 在 Desktop 与 Runner 两条路径中稳定写入并可被诊断 UI 消费。
- [ ] Project-scoped Runner claim 成功后仍确保 active `ProjectBackendAccess`，项目内可见和派发行为不回退。
- [ ] 前端提供 Project 级 Runner token 管理入口，支持 create/list/revoke/rotate，并在 create/rotate 时只展示一次明文 token。
- [ ] 前端生成可复制的 `agentdash-local setup` 命令，包含 server origin、registration token、runner name、workspace root、service install/start 参数。
- [ ] Desktop Local Runtime 设置页继续支持当前登录用户自动启动本机 runtime，并能显示 `desktop_access_token` 来源。
- [ ] Runtime diagnostics 能同时展示当前 Desktop runtime 和独立 Runner，且不会把 service-managed runner 渲染成桌面可直接 restart 的 runtime。
- [ ] 日志、status、doctor、UI 复制内容不泄露 access token、registration token、relay auth token。
- [ ] 测试覆盖 Desktop ensure 与 Runner claim 对统一 enrollment 行为的关键分支：stable backend id、relay token、scope、registration source、ProjectBackendAccess。
- [ ] 手工验收覆盖：
  - 桌面 App 登录后自动挂上本机 runtime。
  - 云端创建 runner token 后复制 setup 命令，Linux/Windows server runner 能上线。
  - 撤销或过期 registration token 后，新 claim 被拒绝，已领取的 relay credential 生命周期按设计保持可解释。
- [ ] Runner backend 身份不再含 `project_id`：同一 `machine_id + capability_slot` 的 runner 产生**唯一稳定 backend_id**，不因 project 不同而裂成多个 backend（测试覆盖）。
- [ ] `ProjectBackendAccess` 为 project→backend 权威授权层：claim 成功写 active grant；`authorization.rs` 对「User-scoped backend + active grant」放行给该 project 成员，且无 grant 时维持 owner-only 不回退（测试覆盖正反两路）。
- [ ] Project Settings 工作空间区可见、可操作 Runner token（create/list/revoke/rotate），create/rotate 仅一次性展示明文，并能复制 `agentdash-local setup` 命令。
- [ ] 工作空间区完成心智重构：先产出 IA 评估（现状术语负担 → 目标用户语言分组），再落地——工业术语以用户语言重述、本机发现收进 workspace 动作、Workspace Modules 降级诊断、Runner token 管理作为「运行环境」子块融入；全程不引入新的回退或断链。

## Decisions (2026-06-27)

经 review 与确认，本任务范围在原 PRD 基础上做如下收敛（项目尚未部署，破坏性改动成本最低，优先做对模型）：

1. **保留两个 HTTP endpoint**（`/api/local-runtime/ensure`、`/api/local-runtime/runner/claim`）。两者认证边界根本不同（`CurrentUser` session vs 公开路由 + bearer registration token），合并成单 endpoint 在 axum 里反而更乱、更难测。收束发生在 application 层与 response 模型，而非 URL 数量。
2. **application 层收一个 `enroll_local_backend(request, source)` use case**，`EnrollmentSource = DesktopAccessToken | RunnerRegistrationToken`。合并当前三处近重复逻辑（`ensure_local_runtime_record`、`ensure_runner_project_runtime_record`、`ensure_runner_project_runtime_record_with_ports`），统一 token 生成（单一 helper）、device 拼装、rotate 语义。
3. **response 收一个共享核心 `LocalBackendEnrollment`**，两条路径字段同构，均返回 `registration_source` 与 `claimed_at`。Desktop 显式写 `registration_source = desktop_access_token`，废除前端「字段缺失=desktop」的隐式推断。
4. **Runner backend 身份去 project 化（本任务真改）**：`stable_local_backend_id` 不再把 `project_id` 烤进 hash；runner backend 以机器级身份存在，`share_scope = User(owner = token.created_by_user_id)`。`ProjectBackendAccess` 成为 project→backend 的**唯一权威授权层**。
5. **携最小 auth 放行规则**：`authorization.rs` 增加一条——User-scoped backend 若存在 active `ProjectBackendAccess` 授权给某 project，则该 project 成员按权限可见/可用。Desktop 个人 backend（无 project 授权）维持 owner-only，不回退。
6. **Runner token 管理 UI 落在 Project Settings → 工作空间区**：create / list / revoke / rotate，create/rotate 仅一次性展示明文，并生成可复制的 `agentdash-local setup` 命令（含 server origin、token、runner name、workspace root、service 参数）。
7. **Workspace 设置心智重构在本任务内正式评估处理**（升级，不再仅轻量）：围绕用户三问（这个项目能在哪儿跑 / 我在搞哪份代码 / 代码在那台机器上落在哪）重整工作空间区 IA——重命名收拢「Backend Access / Inventory / discovery / binding / resolution / priority」等工业术语、把本机发现收进 workspace 条目动作、Workspace Modules 降级为诊断、Runner token 管理作为「运行环境」子块自然融入。先出 IA 评估再落地，硬约束是不新增回退/断链。
8. **本任务边界外的延伸**：拆为**独立任务** `06-27-runner-multi-project-access`（不挂父任务），覆盖多 project 复用的完整授权管理（建立/撤销 grant、priority/policy、跨 owner 策略、审计反向视图），并把「是否引入 Org/Team scope 让 runner 所有权脱离个人」作为该任务的后续追踪项。

### 由上述决策回答的原 Open Questions

- Token 管理 UI 落点 → **Project Settings 工作空间区**（决策 6）。
- Desktop ensure response 是否补 `claimed_at` → **是，两路径同构**（决策 3）。
- Registration token claim 后是否保留在 runner config → 维持现状（setup helper 既有行为），本任务不改。
- setup 命令是否支持「内嵌默认 server URL 的专用 binary」模板 → 本任务只做「通用 binary + 显式 origin」一种，专用模板延后。

## Open Questions

- 机器级 runner backend 的 owner 暂定为 token 创建者（User scope）；长期是否需要引入 Org/Team scope 让所有权脱离个人，留待兄弟任务评估（当前只有 User/Project/System 三种 scope）。
