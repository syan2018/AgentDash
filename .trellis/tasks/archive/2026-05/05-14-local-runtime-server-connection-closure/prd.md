# 本机端与服务器连接闭环重构规划

## Goal

把 AgentDash Desktop 从“手动配置 local runtime 的壳”推进为真正可用的本机端：用户在本地 app 选择或使用目标 server 后，桌面端应自动确保 server 侧存在属于这台机器/当前用户的 local backend，拿到稳定的 `backend_id + relay token + ws endpoint`，启动内置 `agentdash-local` runtime，并在 UI 中清楚呈现本机运行状态、连接目标、能力、根目录与诊断信息。

同时，Desktop 不再把 Dashboard 作为独立本地视图存在。Web dashboard 应作为桌面端的主应用体验复用同一套前端组件与样式；本机 runtime 管理能力进入 desktop app 的设置界面下，作为只在 desktop 环境出现的 Local/Runtime 专属面板。

## Requirements

1. 桌面端启动闭环
   - Tauri app 启动内置 `agentdash-api` 后，必须能基于当前目标 server 自动完成 local backend 的 ensure/claim。
   - 本机 runtime 不再依赖用户手工填写 `cloud_url`、`token`、`backend_id` 三元组。
   - `backend_id` 必须稳定，按 server profile、当前用户、设备身份生成或由 server upsert 返回；禁止继续使用随机 UUID 作为默认注册身份。
   - local runtime 使用 server 返回的 token 连接 `/ws/backend`，注册 payload 的 `backend_id` 必须与 token 绑定的 server row 一致。

2. 目标 server 与桌面 profile
   - Desktop 支持用户设置目标 server，且本机 runtime 绑定到当前目标 server。
   - 不同 server target 必须隔离本地配置、设备身份、runtime profile 与 token，避免切换 server 后误连旧 token。
   - 本地配置可以保存 device/profile 元数据，但 server 侧 backend row 与 runtime health 是权威状态。

3. Server API
   - 增加一个面向登录用户的本机 runtime ensure API，负责在 server DB 中创建或更新 local backend 并返回运行时连接凭证。
   - ensure API 必须复用已存在 `backends.owner_user_id` 与 `runtime_health.profile_id` 语义，必要时增加迁移补足 device/profile 字段或唯一约束。
   - token 由 server 生成/轮转；前端与 Tauri 不自行发明 relay token。

4. UI 信息架构
   - 移除 Desktop 顶层 `Runtime / Dashboard` 双入口结构。
   - Desktop 首屏应是统一样式后的 web app 主体验。
   - 本机 runtime 面板放入 Settings 中，作为 desktop-only 面板展示连接目标、启动状态、server 注册状态、能力、根目录、最近日志、启动/停止/重启和 token/profile 重置等操作。
   - Web 端不能出现 desktop-only local 控件。

5. 前端样式与工程化
   - 延续前序 task 的结论：`@agentdash/ui/styles.css` 是唯一共享样式入口，Desktop 与 Web 共用 `@agentdash/ui` 组件、tokens 与 Tailwind/shadcn 语义。
   - 本次重构不得新增一套 desktop-only CSS 体系；如缺组件，优先移动/抽取现有 web 组件到共享包。
   - 对现有 Dashboard UI 以移动和组件抽取为主，避免复制重写。

6. 可观测与恢复
   - UI 同时呈现本地 IPC 状态与 server runtime health，server 状态仍为权威，local 状态用于桌面端即时反馈。
   - 断线、server 切换、token 失效、backend 被删除、重复在线实例等状态必须有明确错误与恢复路径。
   - local runtime 的日志与状态需要可在设置页诊断，不污染主 dashboard 视图。

## Acceptance Criteria

- [ ] 新增设计文档明确 server API、Tauri 管理器、local runtime、UI 设置页、样式工程化和迁移策略。
- [ ] 新增实施计划按阶段拆分，可在后续重构分支上分阶段提交。
- [ ] Server 侧有 ensure/claim 本机 backend 的 API 设计，覆盖稳定身份、token 生成、权限、唯一性与迁移。
- [ ] Tauri 侧有自动启动闭环设计，覆盖目标 server profile、配置隔离、启动/停止/重启、状态融合和错误恢复。
- [ ] Frontend 侧有信息架构设计，明确 Dashboard 不再作为独立 desktop 视图，Local 面板进入 Settings。
- [ ] 参考 `references/multica` 的本地端实现完成差异分析，并转化为 AgentDash 可执行决策，而不是照搬 multica 结构。
- [ ] 风险清单覆盖身份/token 泄漏、重复连接、server 切换、embedded API 生命周期、数据库迁移、测试和 UI 回归。

## Notes

- 当前项目处于预研期，不保留旧手动配置路径作为兼容方案。实现时可以提供“高级诊断/重置”，但默认体验必须是一条完整自动闭环。
- 本 task 在 `codex/local-app-runtime-integration` 长程分支上推进；该分支承接先前桌面样式工程化工作，并继续完成 local app 与 server 的完整闭环。
