# Auth 身份完整接入修复 Design

## Architecture

身份链路的权威入口是 API 请求认证边界。每个受保护入口都应形成同一条路径：

1. `AuthProvider::authenticate(AuthRequest)` 解析当前请求身份。
2. 身份目录投影记录 `AuthIdentity` 中的 user 与 groups。
3. `AuthProvider::authorize(identity, resource, action)` 执行 provider 级粗粒度授权。
4. `RequestIdentity` 注入 Axum request extensions。
5. Handler 通过 `CurrentUser` 做领域级授权，并把身份传给下游服务或 runtime actor。

Project 仍是领域级权限锚点。`ProjectAuthorizationContext` 使用 `user_id`、`groups`、`is_admin`，用于 Project、Story、Task、Workspace、Session、Workflow、VFS surface 与 project backend access。

Project 授权规则归 domain 层 `ProjectAuthorizationService` 表达，Backend owner/scope/admin/personal 规则归 application 层 `BackendAuthorizationService` 表达。API 与 MCP 不各自复制角色/owner/scope 判断，只负责把 `AuthIdentity` 投影成授权上下文，并把统一授权错误映射为 HTTP 或 MCP 错误。

## Surfaces

### REST API

`secured_api` 继续作为大多数前端 REST/NDJSON 入口的保护层。缺少 `CurrentUser` 的 handler 需要接入 extractor，并按资源性质选择：

- Project-scoped：调用 `load_project_with_permission` 或更具体的 owner helper。
- Session-scoped：通过 session binding 解析 Project，再调用 `ensure_session_permission`。
- Backend-scoped：按 current user 的 backend owner/scope 或 ProjectBackendAccess 决定可见与可操作范围。
- Runtime invocation：`RuntimeActor::PlatformUser` 必须带当前 `user_id`。

### MCP

MCP 纳入本任务首批必修验收项。MCP transport 必须能从 authenticated request 中取得 request-scoped identity，并在进入具体 MCP service 前完成目标资源授权：

- `/mcp/story/{story_id}`：由 story 找到 project，要求 view/edit 取决于 tool 行为。
- `/mcp/task/{task_id}`：由 task 找到 story/project，要求 view/edit 取决于 tool 行为。
- `/mcp/workflow/{project_id}`：按 workflow tool 的读写语义要求 view/edit。
- `/mcp/relay`：通用 project/story mutation 工具需要在 tool 内或 dispatch 前使用同一 `AuthIdentity` 校验目标 project。

如果 rmcp service 不能自然携带 request extensions，优先引入明确的 request context carrier，而不是绕开 ProjectAuthorizationService。

### Terminal

Terminal cache 中的 terminal id 必须能回到 session id 和 backend id。所有 terminal 操作先解析 terminal/session，再用 `ensure_session_permission(current_user, session_id, View)` 校验。写入 input、resize、kill 属于操作行为，但权限与 spawn 保持一致，避免 viewer 无法使用自己有权查看的工作区终端。

### Backend

Backend 是用户本机 runtime 与 Project 工作区路由的边界。实现时区分两类入口：

- User/local runtime 入口：`owner_user_id` 与 `share_scope_id` 来自 current user。
- Project backend access 入口：使用 Project permission 与 `ProjectBackendAccess` 决定可访问 backend。

全局 Backend 管理入口在 enterprise 普通用户下需要按 owner/scope 收口；system/admin 能力由 Settings scope 一致的规则表达。目录浏览应优先使用 project-scoped browse 路径；setup browse 也必须带 current user actor。

Backend 全局入口的产品语义固定为：enterprise 普通用户只能看到和操作自己 owner 或 scope 内的 backend；admin 与 personal 模式拥有全局管理能力。这与 Settings system scope 的 admin 边界保持一致，避免普通用户枚举其它用户的本机 runtime 信息。

Backend owner/scope 判定由 application 层 BackendAuthorizationService 表达，route handler 不直接判断 `AuthMode`、`owner_user_id` 或 project scope。这样列表过滤、详情访问、管理操作和 Project backend access 附加 backend 时都调用同一语义入口。

### VFS

VFS API 的 Project/Workspace permission 校验已经在 handler 层完成，但下游仍需要 actor identity。所有 `vfs_service.list/read/stat` 与 `vfs_mutation_dispatcher.*` 调用应传入 `Some(&current_user)` 或等价 owned identity，使 relay fs / mount provider / audit 能读取同一身份。

Agent runtime tools 已从 `ExecutionSessionFrame.identity` 读取身份，本任务只需要防止前端/API 直接 VFS surface 与 file-picker 分支丢失身份。

### Identity Directory

`user_directory` 是 Project sharing subject lookup 的投影。middleware authenticate 成功后同步投影，可以让代理头、Bearer token、personal provider 与 OIDC login 获得一致行为。同步失败代表身份目录不可用，受保护请求应返回 service unavailable，避免后续授权对象处于不完整状态。

## Data And Migration

预期优先复用现有字段：

- `AuthIdentity` / `AuthGroup`
- `project_subject_grants`
- `BackendConfig.owner_user_id`
- backend share scope 字段
- `user_directory` users/groups 投影

如果 Backend 过滤需要新 repository 方法或索引，补充 migration 与 repository tests。若只使用现有列，不需要 schema 变更。

## Validation

需要新增或更新的测试层级：

- API middleware/provider mock：authenticate、authorize、identity projection。
- API route tests：terminal、backend、VFS surface、MCP 权限。
- Application/runtime tests：runtime actor user id、VFS mount context identity。
- Repository tests：backend owner/scope 查询或认领逻辑。
- Frontend tests/typecheck：若后端 DTO 或 API client contract 改动。

## Trade-Offs

MCP 是当前绕过 `/api` auth 的最高风险入口。本任务先收口 MCP transport 与 Project permission，再细化每个 tool 的 read/edit 权限，让身份模型在首轮实现中闭环。
