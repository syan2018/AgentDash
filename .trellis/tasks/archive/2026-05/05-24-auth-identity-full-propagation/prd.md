# Auth 身份完整接入修复

## Goal

把 AgentDash 的当前请求身份从认证入口完整贯穿到所有用户可触达的执行、文件、终端、Backend 与 MCP 能力面，确保同一份 `AuthIdentity` 成为 API 授权、Project grant、运行时 actor、VFS mount provider 与身份目录投影的共同依据。

这项工作的用户价值是：企业/多人模式下，用户只能看到和操作自己被授权的资源；本机 runtime、终端、MCP、VFS 文件操作都能可靠知道“是谁发起的”；Project 共享可以基于最新的用户和用户组 claim 正常工作。

## Confirmed Facts

- `/api` 下的 `secured_api` 已统一经过 `authenticate_request`，并通过 `CurrentUser` 暴露当前身份。
- Project、Story、Task、Session、Workspace、Settings 等主业务路由大多已经使用 `CurrentUser` 与 Project permission。
- Session prompt、Task start/continue 会把 `CurrentUser` 放入 `LaunchCommand`，最终进入 `ExecutionSessionFrame.identity`；Agent runtime 的 VFS tools 会读取 `context.session.identity`。
- MCP router 当前直接 merge 到根 router，未处于 `/api` 的统一认证中间件下，MCP server 方法也没有 request-scoped identity。
- Terminal spawn 校验了 session 权限，但 list/input/resize/kill 只凭 terminal id 操作全局 cache。
- Backend 全局管理与目录浏览接口缺少 `CurrentUser`，手工新增 backend 会写入 `owner_user_id: None`，setup runtime browse 使用 `RuntimeActor::PlatformUser { user_id: None }`。
- VFS surface、address entry、file-picker 路由校验了 Project/Workspace 权限，但调用 VFS service / mutation dispatcher 时传入 `None`，下游 mount provider 无法得到当前用户。
- 身份目录投影只在 login/OIDC 成功后写入；通过 middleware authenticate 得到的 personal/enterprise identity 不一定进入 `user_directory`。
- `AuthProvider::authorize()` 已在 trait 中定义，但当前请求链路没有调用。

## Requirements

- 认证中间件在成功 authenticate 后产出唯一 request identity，并在必要时同步身份目录投影。
- `AuthProvider::authorize()` 接入请求边界，使用稳定的 resource/action 表达当前入口的 provider 级粗粒度授权。
- MCP HTTP 能力面纳入同一身份模型：请求必须有当前用户，Project/Story/Task/Workflow 目标必须经过 Project permission。
- Terminal 的 list/input/resize/kill 必须像 spawn 一样基于当前用户校验 session 所属 Project 权限。
- Backend 管理、列表、删除、目录浏览与 runtime invocation 必须使用当前身份决定 owner/scope/actor。
- VFS surface、`/api/vfs/*`、file-picker 的 read/list/write/mutation 调用必须向 VFS service 或 mutation dispatcher 传递当前用户身份。
- 身份目录必须跟随成功认证的 `AuthIdentity` 更新 user/group 投影，使 Project sharing 的 subject lookup 与最新 claim 一致。
- 前端仍通过现有 token 注入与 `/api/me` 初始化当前用户；若后端 DTO 或 endpoint 契约变化，前端类型与调用点必须同步。
- 覆盖关键回归测试：跨用户 Project 权限、terminal 控制、backend actor、VFS identity 透传、MCP 权限、身份目录投影、provider authorize。

## Acceptance Criteria

- [ ] 已认证但无 Project 权限的用户不能通过 MCP 读取或修改对应 Project/Story/Task/Workflow 资源。
- [ ] 已认证但无 session 所属 Project 权限的用户不能 list/input/resize/kill 该 session 的 terminal。
- [ ] Backend 列表、详情、删除、目录浏览在 enterprise 普通用户下按用户/Project 授权收口；目录浏览 runtime actor 带 `user_id`。
- [ ] 手工新增或认领 local backend 时，`owner_user_id`、share scope 与当前用户一致。
- [ ] VFS surface、address entry、file-picker 的读写与 patch 路径向下游传递 `AuthIdentity`，测试中可观察到 mount operation context 的 user。
- [ ] middleware authenticate 成功后，`user_directory` 中可以查到当前用户及 claim groups；Project grant 给该用户/组不再依赖交互式 login 路径。
- [ ] `AuthProvider::authorize()` 返回 deny 时，请求得到 403，并覆盖至少一个 API route 测试。
- [ ] 现有 Session/Task launch identity 链路保持通过，Agent runtime tools 仍能收到 session identity。
- [ ] `cargo test` 覆盖相关 Rust crate，前端类型检查/测试通过。

## Boundaries

- 沿用现有 `AuthProvider`、`AuthIdentity`、Project grant 与 Settings scope 模型。
- 保留现有前端登录体验与 `/api/me` 当前用户初始化方式。
- 数据库字段优先复用现有 owner/scope/identity 结构；若实现中需要新索引或字段，必须补 migration。
- 本任务聚焦身份注入、授权收口与下游 actor 透传，不重写 SSO provider、VFS provider、SessionHub 或 relay protocol 的整体架构。

## Decisions

- MCP 纳入本任务首批必修验收项，至少完成统一认证、Project permission 与高风险写工具收口。
- Backend 全局入口在 enterprise 普通用户下必须按当前用户 owner/scope 收口；admin 与 personal 模式保留全局管理能力。
- 成功认证后的身份目录投影如果写入失败，受保护请求应阻断并返回 service unavailable，避免 Project sharing 与审计在不完整身份目录上继续运行。
