# 企业身份接入、Project 共享与 Settings Scope 说明

## 目标

这次重构的目标不是“给项目补一个登录态”，而是把 AgentDash 的多用户/企业化边界收敛成可长期演进的结构：

- personal / enterprise 两种认证模式统一走 `AuthProvider` trait
- `Project` 成为唯一权限锚点
- `Project` 默认私有，可共享给用户 / 用户组
- `Session`、`Task`、`Workspace`、`WorkflowRun`、`Event Stream` 全部跟随 `Project`
- 私有化需求通过 clone 模板 `Project` 解决
- `settings` 拆成 system / user / project 三层 scope

---

## 认证模式

### personal

- 适用于个人开发、自用场景
- 当前用户默认可管理 system scope 设置
- 仍然走统一认证中间件与 `/api/me`，而不是绕开认证链

### enterprise

- 适用于企业 SSO/代理头/令牌接入
- 当前请求身份通过 `AuthProvider::authenticate()` 注入
- 用户组优先来自 claim 投影
- `is_admin=true` 的身份具备管理员旁路

---

## Project 权限模型

### 角色

- `owner`
  - 可查看
  - 可编辑
  - 可管理共享
- `editor`
  - 可查看
  - 可编辑
  - 不可管理共享
- `viewer`
  - 仅可查看

### 额外可见性来源

- `template_visible`
  - 仅对模板 `Project` 生效
  - 无 grant 的用户也可查看模板
  - 但不能因此获得编辑或共享管理权限
- `admin_bypass`
  - 管理员视作拥有完整权限

### 共享对象

- 用户
- 用户组

授权持久化在 `project_subject_grants` 中，`Project` 至少保留一个 owner。

---

## 模板与 Clone 语义

### 模板条件

- `is_template = true`
- 若希望未授权用户可浏览模板，则再设置 `visibility = template_visible`

### Clone 规则

`POST /api/projects/{id}/clone`

clone 时：

- 会复制 `Project` 基础信息和 `config`
- 会复制 `workflow assignments`
- 会设置 `cloned_from_project_id = source.id`
- 新项目固定为：
  - `visibility = private`
  - `is_template = false`
- 不复制：
  - source project grants
  - workspaces
  - stories
  - tasks
  - sessions
  - workflow runs

这条路径的设计目的，是把“企业标准模板”与“个人私有副本”明确分开。

---

## Settings Scope

### scope 列表

- `system`
  - 宿主级配置
  - 例如默认 executor、LLM provider API key、全局 Pi Agent system prompt
- `user`
  - 当前用户私有偏好
  - 不应影响其他用户
- `project`
  - 当前 `Project` 局部配置
  - 适合项目级协作策略或局部覆盖项

### API

- `GET /api/settings?scope=system|user|project&project_id=...`
- `PUT /api/settings?scope=system|user|project&project_id=...`
- `DELETE /api/settings/{key}?scope=system|user|project&project_id=...`

### 权限约束

- `system`
  - personal 模式允许
  - enterprise 模式仅管理员允许
- `user`
  - 永远绑定当前登录用户自身
- `project`
  - 读取要求具备该 `Project` 的 view 权限
  - 写入/删除要求具备 edit 权限

---

## 前端行为

### Project 侧

- Project 列表显示当前用户对每个 `Project` 的 access summary
- `ProjectDetailDrawer` 提供：
  - 共享管理
  - 模板策略
  - clone 私有副本
  - 当前权限态展示

### Settings 侧

- 设置页可切换：
  - system
  - 我的设置
  - 当前项目
- system scope 仍承载原有宿主配置表单
- user / project scope 提供通用 JSON 键值编辑能力

---

## 部署与接入要点

### 必需环境变量

- `AGENTDASH_AUTH_MODE`
  - `personal`
  - `enterprise`

### personal 模式

- 若未额外配置企业认证插件，可直接使用内建 personal provider

### enterprise 模式

- 必须确保有可用的 `AuthProvider`
- Provider 应输出：
  - `user_id`
  - `subject`
  - `display_name`
  - `email`
  - `groups`
  - `is_admin`
  - `provider`

### 首次升级旧 settings 数据

- 旧版平铺 `settings` 表会在启动时自动迁移到 scoped schema
- 旧数据默认落到 `system` scope

---

## 回归检查清单

### 认证与 Project

- personal 模式可正常启动并访问 `/api/me`
- enterprise 模式普通用户只能看到自己可访问的 `Project`
- enterprise 管理员具备 admin bypass
- 模板 `Project` 在 `template_visible` 下可被未授权用户查看
- 非 owner 不能管理 `Project` 共享

### 模板与 Clone

- 模板 `Project` 可成功 clone 为私有 `Project`
- clone 后新项目不继承源 grants
- clone 后新项目不继承源 workspaces/stories/tasks/sessions
- clone 后保留 `workflow assignments`

### Settings

- system scope 在 personal 模式可读写
- system scope 在 enterprise 普通用户下返回 403
- user scope 只落到当前用户自己的作用域
- project scope 在无 `project_id` 时返回 400
- project scope 在无 edit 权限时拒绝写入

---

## 当前实现边界

当前已完成“结构正确”的主干闭环，但并未做以下扩展：

- 更细粒度的 system settings RBAC
- user/project 内建表单的产品化分类
- 审计日志面板
- SSO claim 到更复杂组织结构的映射规则

这些都可以在现有结构上继续演进，不需要再推翻本轮权限主轴。
