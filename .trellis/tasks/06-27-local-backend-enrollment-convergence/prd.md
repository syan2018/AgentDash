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

## Open Questions

- Project Runner token 管理 UI 首次落点放在 Project Settings、System Settings 还是 Local Runtime 设置页中的 Project 子区？
- Desktop ensure response 是否需要显式补齐 `claimed_at`，让它与 Runner claim response 完全同构？
- Registration token 成功 claim 后是否保留在 runner config 中，还是由 setup 提供“claim 后清除 token”的策略选项？
- 前端生成 setup 命令时是否需要同时支持“通用 runner binary”和“已内嵌默认 server URL 的环境专用 runner binary”两种模板？
