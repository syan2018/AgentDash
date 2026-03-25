# 实施分阶段说明

## Overview

本任务不是“补一个登录页”，而是一次围绕 `Project` 权限锚点的结构性重构。

建议总策略：

1. 先统一身份入口
2. 再建立 `Project grants`
3. 再把 Session / Event / Workspace Files / Workflow 等横切链路全部收口
4. 最后补前端分享与模板体验

这样可以避免先做 UI 或局部表字段，后续再因权限主轴变化返工。

## Phase 0: 目标模型冻结

### 目标

- 冻结权限主轴、角色模型、模式边界、模板策略

### 交付物

- `prd.md`
- 本文档
- 授权矩阵文档

### 关键决策

- 个人模式与企业模式统一走插件式认证
- `Project` 默认私有
- Session 跟随 Project
- 私有变体通过 clone 模板 Project

## Phase 1: 身份接入与模式框架

### 目标

- 把“当前用户是谁”打进所有 HTTP 请求链

### 后端任务

- 在宿主真正挂载认证中间件
- 增加 `CurrentUser` / `RequestIdentity` extractor
- 统一暴露：
  - `user_id`
  - `display_name`
  - `email`
  - `groups`
  - `is_admin`
  - `auth_mode`
- 新增 `/api/me`

### 插件任务

- 定义 / 收敛 `personal auth provider`
- 定义 / 收敛 `enterprise sso provider`
- 明确 provider 输出合同与 claim 解析边界

### 风险

- 若先在业务逻辑里硬编码企业 claim 字段，后续个人模式会很难回填

## Phase 2: Project 授权模型

### 目标

- 建立可复用的 Project 访问控制

### 数据层任务

- 新增：
  - `users`
  - `groups`
  - `group_memberships`
  - `project_subject_grants`
- 改造：
  - `projects`

### 应用层任务

- 增加 `ProjectAuthorizationService`
- 支持：
  - `can_view_project`
  - `can_edit_project`
  - `can_manage_project_sharing`
  - `can_admin_bypass`

### API 任务

- `GET /projects` 只返回当前用户可访问项目
- `POST /projects` 创建后自动授予创建者 `owner`
- 新增共享相关 API：
  - list grants
  - grant user
  - grant group
  - revoke grant

### 风险

- 如果仍保留 `list_all()` 为主要访问路径，后续非常容易遗留越权点

## Phase 3: 下游实体 project-scoped 化

### 目标

- 让所有权限判断都能低成本解析 `project_id`

### 数据层任务

- `stories` 增加审计字段
- `workspaces` 增加审计字段
- `tasks` 显式补 `project_id`
- `session_bindings` 显式补 `project_id`
- `workflow_runs` 显式补 `project_id`
- `state_changes` 显式补 `project_id`、`actor_user_id`

### 服务层任务

- 所有按 ID 读取的业务对象都能回溯或直取 `project_id`
- 所有状态变更都带 actor 审计信息

### 风险

- 如果继续依赖 `task -> story -> project` 多级回溯，后面 Session、Workflow、实时流会很难高效过滤

## Phase 4: 横切链路收口

### 目标

- 把最敏感的“能看见 / 能读文件 / 能看实时状态”的链路封住

### Session

- 下线或内部化全局 Session 枚举能力
- 所有 Session 详情访问都要求 project 授权
- `SessionBindingRepository` 优先支持 project-scoped 查询

### Event Stream

- 从全局 `/events/stream` 收敛为 project-scoped stream
- `state_changes` 查询按 `project_id` 过滤

### Workspace Files

- 强制要求 `workspace_id`
- 读取前先校验当前用户对该 Workspace 所属 Project 的权限
- 移除“未指定 workspace_id 读取服务端 workspace root”的兜底路径

### Workflow

- `workflow_runs`、`workflow_assignments`、phase session binding 均按 project 授权

### Settings

- 把全局 `settings` 拆成：
  - system
  - user
  - project

### 当前收口进展（2026-03-24）

- `workspace-files` 已强制要求 `workspace_id`，并在后端通过 `workspace.project_id -> ProjectPermission::View` 校验；未指定 `workspace_id` 读取服务端 root 的 fallback 已移除。
- `SessionChatView` 已补齐 `workspaceId` 透传，`Task / Story / Project Agent / SessionPage` 四条主要会话链路都已按上下文解析工作空间；若当前会话无可用工作空间，会显式关闭 `@` 文件引用能力而不是走越权兜底。
- `/events/stream`、`/events/stream/ndjson`、`/events/since/{id}` 已改为必须携带 `project_id`，并在后端按 `ProjectPermission::View` + project-scoped `state_changes` 查询返回实时事件。
- 前端事件流连接已改为随 `currentProjectId` 建连与重连，切换项目时会重置连接游标，不再保留全局共享事件流。

## Phase 5: 前端企业化体验

### 目标

- 前端显式理解“我是谁、我能看哪些 Project、我能把 Project 分享给谁”

### 任务

- 启动时加载 `/api/me`
- Project 列表改为按可访问范围展示
- 新增分享管理 UI
- 新增模板 clone 入口
- Session 页面按 Project 授权展示
- 补 403 页面与权限不足提示

### 风险

- 若仍沿用“空数组代表没数据”而不是区分 403/404，用户会误以为数据丢失

### 当前进展（2026-03-24）

- 已新增 `GET /api/directory/users` 与 `GET /api/directory/groups`，前端不再需要手输 `user_id/group_id` 才能管理共享。
- `ProjectResponse` / `ProjectDetailResponse` 已附带 access summary，前端可直接展示当前用户的 `role`、`can_edit`、`can_manage_sharing`、模板可见来源和管理员旁路状态。
- `ProjectDetailDrawer` 已新增“共享管理”和“模板策略”面板，支持给用户 / 用户组授予与撤销 `owner/editor/viewer` 授权，并按权限切换为只读或可管理视图。
- 已新增模板 clone 流程：`POST /api/projects/{id}/clone` 会复制项目基础配置与 workflow assignments，清空默认 workspace，且不会复制源 Project grants/workspaces/stories/tasks/sessions。

## Phase 6: 测试与发布策略

### 后端测试

- 认证模式切换测试
- Project grant 授权测试
- group claim 生效测试
- admin bypass 测试
- Session / Event / Workspace Files 越权测试

### 前端测试

- 登录态初始化
- 分享后可见性变化
- 无权限跳转
- 模板 clone 主流程

### 发布策略

- 优先以 feature flag 或独立分支完成大部分 schema 与服务层改造
- 在权限链未打通前，不建议零碎上线 UI

### 当前进展（2026-03-24）

- 已为 `settings` 仓储增加 scoped schema 与旧平铺表自动迁移测试。
- 已增加 `system scope` 权限测试，明确 personal 可访问、enterprise 非 admin 拒绝、enterprise admin 允许。
- 已补充运维/开发说明文档 [docs/enterprise-auth-project-sharing.md](../../docs/enterprise-auth-project-sharing.md)，覆盖 auth mode、Project 共享、模板 clone、settings scope 与回归清单。

## 建议拆分的子任务

### Backend / Identity

- 认证中间件与 `CurrentUser`
- personal provider
- enterprise sso provider

### Backend / Authorization

- users/groups/grants schema
- ProjectAuthorizationService
- project sharing routes

### Backend / Scope Refactor

- tasks/session_bindings/workflow_runs/state_changes 补 `project_id`
- settings scope 重构
- event stream project 化
- workspace files 授权化

### Frontend

- current user bootstrap
- project sharing UI
- template clone UI
- 403 / no-access UX

### QA / Docs

- 授权矩阵测试
- 个人模式 / 企业模式部署文档
- 插件模式说明文档
