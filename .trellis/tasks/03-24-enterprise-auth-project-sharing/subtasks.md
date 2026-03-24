# 可执行子任务拆分

## 使用方式

这份子任务清单的目标不是做“概念归类”，而是让后续开发可以直接拿一项开工。

建议推进顺序：

1. 先做身份合同与中间件
2. 再做 Project grants
3. 再改 project-scoped 数据与横切链路
4. 最后补前端、模板与回归

---

## 1. `identity-contract-and-modes`

### 目标

- 收敛个人模式与企业模式统一认证合同

### 主要产出

- 明确 `AuthProvider` 需要输出的最小身份字段
- 明确 `auth_mode = personal | enterprise`
- 明确 `is_admin` / `admin_bypass` 语义
- 明确 groups 从 claim 进入系统的标准形态

### 涉及文件

- `crates/agentdash-plugin-api/src/auth.rs`
- `crates/agentdash-plugin-api/src/plugin.rs`
- `.trellis/tasks/03-24-plugin-api-architecture/prd.md`

---

## 2. `request-identity-middleware`

### 目标

- 宿主真正把认证接进请求链

### 主要产出

- 认证中间件
- `CurrentUser` / `RequestIdentity` extractor
- 未认证 / 无权限统一错误响应

### 涉及文件

- `crates/agentdash-api/src/routes.rs`
- `crates/agentdash-api/src/app_state.rs`
- `crates/agentdash-api/src/rpc.rs`

---

## 3. `me-endpoint-and-frontend-bootstrap`

### 目标

- 让前端启动时就拿到当前用户上下文

### 主要产出

- `/api/me`
- 前端 current user store 或等价启动流程
- 统一 API client 身份初始化策略

### 涉及文件

- `crates/agentdash-api/src/routes/`
- `frontend/src/api/client.ts`
- `frontend/src/stores/`

---

## 4. `user-group-claim-projection-schema`

### 目标

- 建立用户、组、组成员关系以及 claim 投影落地机制

### 主要产出

- `users`
- `groups`
- `group_memberships`
- claim 同步 / 更新策略

### 涉及文件

- `crates/agentdash-domain/`
- `crates/agentdash-infrastructure/src/persistence/sqlite/`

---

## 5. `project-grants-schema-and-repo`

### 目标

- 建立 Project 共享权限的持久化模型

### 主要产出

- `project_subject_grants`
- `projects` 审计与模板字段
- 仓储接口与 SQLite 实现

### 涉及文件

- `crates/agentdash-domain/src/project/`
- `crates/agentdash-infrastructure/src/persistence/sqlite/project_repository.rs`

---

## 6. `project-authorization-service`

### 目标

- 建立统一权限判定服务，避免散落 if/else

### 主要产出

- `can_view_project`
- `can_edit_project`
- `can_manage_project_sharing`
- `can_admin_bypass`

### 涉及文件

- `crates/agentdash-application/`
- `crates/agentdash-api/src/`

---

## 7. `project-route-authorization-and-sharing-api`

### 目标

- 收口 Project CRUD，并补齐分享管理 API

### 主要产出

- `/projects` 只返回当前用户可访问项目
- 共享用户 / 共享用户组 / 撤销共享 API
- Project 创建默认授予创建者 `owner`

### 涉及文件

- `crates/agentdash-api/src/routes/projects.rs`

---

## 8. `project-scoped-entity-schema-refactor`

### 目标

- 把横切实体改成显式 project-scoped

### 主要产出

- `tasks.project_id`
- `session_bindings.project_id`
- `workflow_runs.project_id`
- `state_changes.project_id`
- 关键审计字段

### 涉及文件

- `crates/agentdash-domain/src/task/`
- `crates/agentdash-domain/src/session_binding/`
- `crates/agentdash-domain/src/workflow/`
- `crates/agentdash-infrastructure/src/persistence/sqlite/`

---

## 9. `story-task-workspace-route-authorization`

### 目标

- 让 Story / Task / Workspace API 全部走 Project 授权

### 主要产出

- 所有路由先解析 project_id 再判权
- 读写接口统一收口

### 涉及文件

- `crates/agentdash-api/src/routes/stories.rs`
- `crates/agentdash-api/src/routes/workspaces.rs`
- `crates/agentdash-api/src/routes/task_execution.rs`

---

## 10. `session-scope-closure`

### 目标

- 去掉全局 Session 泄露面

### 主要产出

- Session 详情与列表全部 project-scoped
- SessionBinding 查询按 Project 收口
- 全局 `/sessions` 降级或内部化

### 涉及文件

- `crates/agentdash-api/src/routes/acp_sessions.rs`
- `crates/agentdash-api/src/routes/project_sessions.rs`
- `crates/agentdash-api/src/routes/story_sessions.rs`

---

## 11. `project-scoped-event-stream`

### 目标

- 把实时流改成项目级可授权模型

### 主要产出

- project-scoped event stream
- `state_changes.project_id`
- `state_changes.actor_user_id`

### 涉及文件

- `crates/agentdash-api/src/stream.rs`
- `crates/agentdash-infrastructure/src/persistence/sqlite/story_repository.rs`

### 当前状态

- 已完成
- `/events/stream`、`/events/stream/ndjson`、`/events/since/{id}` 现在都必须带 `project_id`
- 后端已按 Project 权限和 project-scoped `state_changes` 查询返回事件
- 前端 `eventStore` 已改为按当前选中 Project 建连与切换重连

---

## 12. `workspace-files-hardening`

### 目标

- 封住文件读取的越权路径

### 主要产出

- 强制要求 `workspace_id`
- Workspace 所属 Project 授权校验
- 移除服务端 root fallback

### 涉及文件

- `crates/agentdash-api/src/routes/workspace_files.rs`

### 当前状态

- 已完成
- 后端已强制要求 `workspace_id`
- 已移除读取服务端 root 的 fallback
- `SessionChatView` 及其上层场景已显式透传 `workspaceId`
- 当前会话若没有工作空间上下文，会禁用 `@` 文件引用而不是继续走旁路

---

## 13. `settings-scope-refactor`

### 目标

- 把全局 settings 改造成多层 scope

### 主要产出

- system settings
- user preferences
- project settings
- 前后端读写路径调整

### 涉及文件

- `crates/agentdash-domain/src/settings.rs`
- `crates/agentdash-infrastructure/src/persistence/sqlite/settings_repository.rs`
- `crates/agentdash-api/src/routes/settings.rs`
- `frontend/src/stores/settingsStore.ts`

### 当前状态

- 已完成
- settings 仓储已改为显式 `system / user / project` scope
- 旧平铺 settings 表会在启动时自动迁移到 `system` scope
- enterprise 模式下 `system` scope 仅管理员允许访问
- 前端设置页已支持 scope 切换，并为 user/project scope 提供通用 JSON 键值编辑入口

---

## 14. `frontend-sharing-and-permission-state`

### 目标

- 前端正确理解当前用户、Project 权限和无权限状态

### 主要产出

- 当前用户启动流程
- Project 分享 UI
- Session 列表按 Project 授权展示
- 403/404 区分

### 涉及文件

- `frontend/src/stores/projectStore.ts`
- `frontend/src/stores/storyStore.ts`
- `frontend/src/stores/sessionHistoryStore.ts`
- `frontend/src/pages/`

### 当前状态

- 已完成
- `Project` DTO 已带 access summary，前端能区分 `owner/editor/viewer`、模板可见来源和管理员旁路
- `projectStore` 已补 grants 读取/授予/撤销与 clone 能力
- `ProjectDetailDrawer` 已补共享管理面板、目录查询接入与权限态展示

---

## 15. `project-template-clone-flow`

### 目标

- 把模板 clone 作为正式私有化路径

### 主要产出

- 模板 Project 标记
- clone API
- 前端模板入口
- clone 后默认只授予创建者 owner

### 涉及文件

- `crates/agentdash-api/src/routes/projects.rs`
- `frontend/src/features/project/`

### 当前状态

- 已完成
- 已新增 `POST /api/projects/{id}/clone`
- clone 会复制项目基础配置与 workflow assignments，并把默认 workspace 清空
- clone 不复制 grants、workspaces、stories、tasks、sessions，符合“模板 -> 私有副本”路径

---

## 16. `authorization-regression-and-deploy-docs`

### 目标

- 收尾所有回归与部署说明，避免“功能做完但团队不敢用”

### 主要产出

- 个人模式 / 企业模式测试矩阵
- 管理员旁路测试
- Session / Event / Files 越权测试
- 部署文档与接入说明

### 涉及文件

- `tests/`
- `.trellis/spec/`
- `docs/`

### 当前状态

- 已完成
- 已新增 settings scope 仓储迁移测试和 system scope 权限测试
- 已新增部署与运维说明文档 `docs/enterprise-auth-project-sharing.md`
- 文档已覆盖认证模式、Project 共享、模板 clone、settings scope 和回归检查清单
