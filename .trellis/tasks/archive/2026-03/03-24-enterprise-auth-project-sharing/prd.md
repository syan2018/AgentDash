# 企业身份接入 + Project 私有共享授权重构

## Goal

为 AgentDash 设计一套**单企业实例优先，但保留个人模式**的身份与授权重构方案，使系统可以同时支持：

- 企业内部通过 SSO 接入用户身份
- 从 claim 中解析用户组并参与 Project 授权
- `Project` 默认私有，按用户 / 用户组手动共享
- `Story / Workspace / Task / Session / Workflow` 全部跟随 `Project` 权限
- 个人使用场景继续可用，不强制依赖企业 SSO
- 平台能力仍沿用插件化 / trait 化装配，不把企业认证硬编码进宿主

最终目标不是“给当前每张表都加一个 owner 字段”，而是收敛出一条**以 Project 为权限锚点**、兼容个人与企业两种运行模式、可由插件系统承载的长期正确模型。

## Why This Exists

- 项目最初按单用户个人工具推进，当前数据模型默认“谁能打到 API，谁就能操作”
- 现在需要迁入企业环境，必须解决身份来源、Project 共享、Session 可见性、Workspace 文件访问和实时流授权
- 同时该项目仍需支持个人模式，不能把整套平台绑死在企业 SSO 上
- 当前仓库已经存在 `AuthProvider` / `AgentDashPlugin` 等可插拔入口，适合作为企业模式与个人模式的统一承载层
- 现有实体层次已经较清晰，`Project -> Story / Workspace -> Task` 非常适合作为权限继承链的主轴

## Product Constraints

### C1: 单企业实例优先，不做多租户 SaaS

- 当前阶段目标是单企业实例部署
- 不引入复杂的 tenant / organization 多租户隔离模型
- 但内部模型不应阻断未来扩展

### C2: 个人模式必须继续存在

- 该项目需要同时支持个人使用
- 企业认证与个人认证在平台中都应体现为统一 trait / plugin 能力
- 不能在业务代码中散落“如果是企业模式就 XXX，否则个人模式就 YYY”的硬编码分叉

### C3: Project 是权限锚点

- `Project` 默认私有
- `Project` 可共享给指定用户和用户组
- `Story / Workspace / Task / Session / WorkflowRun` 不再各自定义独立 owner 体系
- 所有下游实体默认跟随 `Project` 权限继承

### C4: Session 跟随 Project，而不是走个人私有分支

- Session 不再单独设计“个人私有”权限模型
- 若用户需要私有化修改，应通过 clone Project 模板生成新的私有 Project
- 共享 Project 下的 Session 默认按 Project 角色控制可见性与可操作性

### C5: 用户组优先来自认证 claim

- 企业模式下，用户组以认证 claim 为第一来源
- 可允许宿主缓存 / 投影组信息，但不应把“本地维护组目录”作为第一真相源
- 用户与用户组的抽象应保持足够通用，以便个人模式和企业模式共用认证接口

### C6: 管理员旁路访问需要预留

- 允许系统管理员具备旁路查看 / 管理所有 Project 的能力
- 该能力必须显式建模，不能依赖“因为接口是全局的所以管理员自然都能看”

### C7: Project 模板通常允许被组织内用户看到并 clone

- 模板 Project 可以作为共享知识载体
- 用户私有需求通过 clone 模板创建新 Project 解决

## Current State Assessment

### 当前已有的有利条件

- `AuthProvider` 已存在于插件 API 中，认证入口方向正确
- `AgentDashPlugin` 已经能承载宿主装配扩展
- 核心业务模型围绕 `Project -> Story / Workspace -> Task` 展开，天然适合做权限继承
- Session 已经通过 `SessionBinding` 管理，具备统一收敛空间

### 当前最主要的问题

- HTTP 路由层没有真正挂载认证 / 授权链路
- `ProjectRepository::list_all()`、全局 `/sessions`、全局 `/events/stream` 等能力默认全局可见
- `settings`、`views`、`user_preferences`、`workflow_runs`、`session_bindings` 等横切数据缺少 scope
- `workspace-files` 在未指定 `workspace_id` 时仍可落到服务端 workspace root
- 事件流 `state_changes` 缺少 `project_id` / `actor_user_id`，后续难以做高效授权过滤
- 当前模型没有把“个人模式”和“企业模式”当作统一认证接口下的不同 provider 对待

## Core Principles

### P1: 认证来源可插拔，授权主轴统一

- 个人模式与企业模式都通过统一的 `AuthProvider` / 插件入口装配
- 不同模式只改变“身份从哪里来”，不改变“Project 是权限锚点”的业务规则

### P2: 权限字段与审计字段分离

- `created_by_user_id` / `updated_by_user_id` / `actor_user_id` 用于审计
- `project grants` / `role` 才用于访问控制
- 禁止用“创建者”偷代“唯一可访问者”

### P3: Project 级授权优先于下游细粒度 ACL

- 短期只做 `owner / editor / viewer / admin-bypass`
- 不在第一阶段为 Story / Task / Session 单独引入复杂 ACL

### P4: 先封住全局泄露面，再扩展共享体验

- 先解决全局 Session、全局事件流、Workspace 文件访问、Settings 全局暴露等问题
- 再补前端分享入口与模板 clone 体验

### P5: 平台内核不硬编码企业约束

- 企业 SSO、claim 组解析、个人模式 mock / local auth 都通过插件承载
- 宿主只感知统一的 `CurrentUser / CurrentGroups / CurrentRoles`

## Target Capability Model

### 1. 身份模式

系统支持至少两类认证模式：

- `personal`
  - 单用户 / 少量用户的个人部署
  - 可通过本地 provider、静态配置、轻量登录方式生成身份
- `enterprise`
  - 企业 SSO 登录
  - 从 token / claim 中提取 `user_id`、显示名、邮箱、groups、admin 标识

二者都通过统一 `AuthProvider` 实现，不在业务层分裂。

### 2. 角色模型

Project 级建议先收敛为：

- `owner`
  - 可管理 Project 设置、分享、模板属性、删除 Project
- `editor`
  - 可创建/修改 Story、Task、Session、WorkflowRun、Workspace 绑定等
- `viewer`
  - 可查看 Project 内容、Session、WorkflowRun、实时状态，不可改
- `admin_bypass`
  - 系统管理员旁路能力，独立于 Project grants

### 3. Project 共享模型

- `Project` 创建后默认只授予创建者 `owner`
- 可显式共享给：
  - 指定用户
  - 指定用户组
- 共享记录需要区分 subject type：`user` / `group`

### 4. Session 模型

- Project 级 Project Agent Session：跟随 Project 权限
- Story 级 Companion Session：跟随 Story 所属 Project 权限
- Task Execution Session：跟随 Task 所属 Project 权限
- 不再鼓励“未绑定全局 Session”作为用户主工作流

### 5. 模板与私有化模型

- 模板 Project 可被组织内可见并 clone
- clone 产生新的普通 Project，默认私有
- 私有需求通过 clone Project 模板解决，而不是在共享 Project 下派生私有 Session

## Data Model Requirements

### R1: 用户与组模型

至少需要新增：

- `users`
- `groups`
- `group_memberships`

说明：

- 企业模式下组关系优先来自 claim 投影
- 允许将 claim 投影落地缓存，但不应要求人工维护完整组目录

### R2: Project grants 模型

需要新增独立授权表，例如：

- `project_subject_grants`
  - `project_id`
  - `subject_type` (`user` / `group`)
  - `subject_id`
  - `role` (`owner` / `editor` / `viewer`)
  - `granted_by_user_id`
  - `created_at`

### R3: Project 审计字段

`projects` 至少新增：

- `created_by_user_id`
- `updated_by_user_id`
- `visibility`（当前阶段可收敛为 `private` + `template_visible` 等有限枚举）
- `is_template`
- `cloned_from_project_id`

### R4: 下游实体 project-scoped 化

以下实体都必须能快速解析到 `project_id`，且不依赖深层回溯：

- `stories`
- `workspaces`
- `tasks`
- `session_bindings`
- `workflow_runs`
- `state_changes`

其中建议显式补充：

- `tasks.project_id`
- `session_bindings.project_id`
- `workflow_runs.project_id`
- `state_changes.project_id`

### R5: 审计字段补齐

关键业务表应逐步补齐：

- `created_by_user_id`
- `updated_by_user_id`
- `actor_user_id`（事件 / 状态变更）

## API / Service Requirements

### R6: 路由必须先取身份，再判权

所有业务路由都必须遵循：

```text
提取 CurrentUser
  -> 加载目标实体 / 解析 project_id
  -> 调用 ProjectAuthorizationService
  -> 执行业务逻辑
```

### R7: 全局列举接口必须收口

以下接口不能继续保持全局可见：

- `/projects`
- `/sessions`
- `/events/stream`
- `/events/since/{id}`
- `/workspace-files`
- `/settings`
- `/workflow-runs/targets/...`

### R8: Session 访问必须 project-scoped

- Session 列表与详情应通过 `project / story / task` 作用域访问
- `/sessions/{id}` 这类全局直达接口若保留，必须在内部做 owner -> project 授权映射

### R9: Settings 必须分 scope

至少分成：

- system settings
- user preferences
- project-scoped settings

不能继续使用单张全局 `key -> value` 表承接所有配置。

### R10: Workspace 文件访问必须绑定 Workspace 权限

- 前端文件引用与文件读取必须通过 `workspace_id`
- 禁止在业务 API 中保留“未指定 workspace_id 就读取服务端 workspace root”的路径

## Frontend Requirements

### R11: 前端必须引入当前用户上下文

- 启动时加载当前用户信息
- 所有 Project 列表和详情都以“当前用户可访问范围”为准

### R12: 增加 Project 分享与模板入口

- Project 成员/共享管理 UI
- 模板标记与 clone UI
- 明确“共享 Project”与“我基于模板创建的新 Project”的区别

### R13: 无权限态必须显式展示

- 区分 `404` 与 `403`
- 不允许因无权限而静默显示空白页面

## Plugin & Mode Integration Requirements

### R14: 认证能力与模式选择必须插件化

- 个人模式 provider 与企业模式 provider 都走插件注册
- 宿主只依赖统一 `AuthProvider`
- 后续如需新增 GitHub OAuth、OIDC、Local Dev Auth，不应触碰核心业务权限模型

### R15: 与插件架构收敛任务保持一致

参考 `.trellis/tasks/03-24-plugin-api-architecture/prd.md`：

- 认证必须是稳定外部契约的一部分
- 宿主仍遵循“先注册、后构建”的启动模型
- 企业私有认证实现通过 enterprise plugin 追加，而不是复制宿主装配逻辑

## Phased Delivery

| 阶段 | 内容 | 结果 |
|---|---|---|
| Phase 0 | 明确目标模型与权限矩阵 | 不再在 owner/user_id 上摇摆 |
| Phase 1 | 接入统一 `CurrentUser` + provider 模式 | 企业模式与个人模式进入统一认证框架 |
| Phase 2 | 建立 `users/groups/project grants` | Project 共享模型成型 |
| Phase 3 | 下游实体补 `project_id` / 审计字段 | 权限继承链落地 |
| Phase 4 | 收口 Session / 事件流 / 文件访问 / Settings | 封住核心泄露面 |
| Phase 5 | 前端分享、模板 clone、无权限态 | 企业使用体验闭环 |
| Phase 6 | 测试、回归、管理员旁路、文档沉淀 | 进入稳定实施态 |

## Acceptance Criteria

- [ ] PRD 明确个人模式与企业模式都走统一认证 trait / plugin 模型
- [ ] PRD 明确用户组优先来自 claim，而不是本地手工主目录
- [ ] PRD 明确 `Project` 默认私有、可共享给用户和用户组
- [ ] PRD 明确 `Story / Workspace / Task / Session / Workflow` 跟随 `Project` 权限
- [ ] PRD 明确私有需求通过 clone Project 模板解决，而不是私有 Session
- [ ] PRD 明确系统管理员拥有显式的旁路能力
- [ ] PRD 明确模板通常允许被组织内用户查看并 clone
- [ ] PRD 明确需收口全局 Session / 全局事件流 / Workspace root 文件读取
- [ ] PRD 明确 `settings` 需要分 scope，而不是继续使用全局 key-value
- [ ] PRD 明确关键横切表需要补 `project_id` 与审计字段

## Out of Scope

- 多租户 SaaS 组织隔离
- 复杂细粒度资源 ACL（如 Story 单独授权、Task 单独授权）
- 插件市场 / 外部插件动态加载
- 第一阶段就实现完整审计后台
- 第一阶段就支持所有可能的认证 provider

## Open Questions

- 个人模式 provider 的最简实现形式是本地静态用户、轻量登录，还是 dev-only bypass
- 模板 Project 的组织内可见范围是否还需要分“所有人可见”与“仅特定组可见”
- claim 组信息是否需要持久化快照用于离线审计

## Related Files

- `crates/agentdash-plugin-api/src/auth.rs`
- `crates/agentdash-plugin-api/src/plugin.rs`
- `crates/agentdash-api/src/app_state.rs`
- `crates/agentdash-api/src/routes.rs`
- `crates/agentdash-api/src/routes/projects.rs`
- `crates/agentdash-api/src/routes/acp_sessions.rs`
- `crates/agentdash-api/src/stream.rs`
- `crates/agentdash-api/src/routes/workspace_files.rs`
- `crates/agentdash-api/src/routes/workflows.rs`
- `crates/agentdash-infrastructure/src/persistence/sqlite/`
- `frontend/src/api/client.ts`
- `frontend/src/stores/projectStore.ts`
- `frontend/src/stores/storyStore.ts`
- `.trellis/tasks/03-24-plugin-api-architecture/prd.md`
