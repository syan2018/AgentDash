# AgentRun 分叉工作台与 Session API 收口

## Goal

把用户可见的继续对话入口收束到 AgentRun 工作台：当用户在不属于自己控制的 AgentRun 上继续发言时，系统创建当前用户自己的 forked AgentRun，并把该输入投递到新工作台；同时允许用户在自己的或任何可见 AgentRun 上从某个会话轮次显式 fork 出探索分支。RuntimeSession / Session 只作为内部 trace、projection、runtime delivery 事实，不再作为产品 API、路由、文案或用户决策对象外露。

本任务同时评估并规划 Session API 收口，因为 AgentRun fork 产品化依赖同一个边界：Session 负责内部 runtime trace，AgentRun 负责业务归属、权限、工作台导航、mailbox 和用户交互。

## Requirements

### Confirmed Facts

- AgentRun workspace 是用户可见执行工作台。前端规范已声明 `RuntimeSession` trace view 不作为业务执行归属事实源：`.trellis/spec/frontend/architecture.md`。
- RuntimeSession fork 底层能力已经存在：`crates/agentdash-application-runtime-session/src/session/branching.rs` 的 `SessionBranchingService::fork_session` 会创建 child session、写 `SessionLineageRelationKind::Fork`、提交 child initial projection。
- 当前 `/sessions/{id}/fork` 只返回 child session / lineage / projection 信息，没有创建 `LifecycleRun`、`LifecycleAgent`、`AgentFrame`、`RuntimeSessionExecutionAnchor` 或 AgentRun mailbox envelope，因此不能成为用户继续工作的产品入口。
- AgentRun composer 是当前用户输入权威入口：`crates/agentdash-api/src/routes/lifecycle_agents.rs` 的 `/agent-runs/{run_id}/agents/{agent_id}/composer-submit` 先解析 AgentRun、校验 Project 权限、执行 command policy，再写 AgentRun mailbox。
- AgentRun workspace 已有一跳 lineage 展示和跳转，但现有 `agent_lineages` 是同一 Run 内的 agent 控制树：`agent_lineages.run_id` + `parent_agent_id` + `child_agent_id`。跨 Run fork 不应复用该表来伪装同 Run child agent，需要独立的 AgentRun fork lineage 表达。
- `LifecycleRun` / `LifecycleAgent` 当前没有 user owner / initiator 字段；现有权限边界主要是 Project 权限。
- 当前 Project role 有 `owner` / `editor` / `viewer`，但 fork 方案应移除“viewer 只能看不能对话”的产品语义：进入项目的成员默认可以使用 AgentRun 工作台、创建自己的 AgentRun / forked AgentRun；更高权限只用于配置 Project 和 Project 层资产。
- 后端 session routes 存在权限不一致风险：`delete_session`、tool approval routes 会调用 `ensure_session_permission`，但 `fork_session`、`get_session_lineage`、`rollback_session_projection` 当前没有先做同类检查。
- 前端 `packages/app-web/src/services/session.ts` 仍直接暴露 `fetchSessionMeta`、`fetchSessionEvents`、`forkSession`、`fetchSessionLineage`、`rollbackSessionProjection` 等 `/sessions/*` 服务函数；`features/session/*` 仍大量使用 `Session` 命名作为 runtime feed / chat component 内部实现。
- 现有 spec `backend/session/session-lineage-projection.md` 仍把 `POST /sessions/{id}/fork`、`GET /sessions/{id}/lineage`、`POST /sessions/{id}/projection/rollback` 写成 HTTP surface；这与“Session 完全是内部业务”的新目标冲突，需要更新为内部 service / diagnostic capability 描述。
- 现有 `SessionEntry` / `SessionMessageCard` / `ToolCallCardShell` 已经以 runtime feed item 渲染对话和工具项，前端具备给消息/轮次补 hover action toolbar 的挂载点；`MailboxMessageRow` 已有 hover 操作区，可作为交互密度参考。

### Product Requirements

- 用户可见交互入口必须以 AgentRun 为单位表达。用户继续对话、导航、列表、父子关系、命令状态和错误提示都应围绕 AgentRun，而不是 Session。
- 当当前用户在无控制权的 AgentRun 上提交输入时，系统应创建当前用户自己的 forked AgentRun，并将该输入作为新 AgentRun 的第一条/下一条 mailbox message 投递。
- 加入 Project 的普通成员应允许对话。用户是否能继续输入不应由“只读/可编辑项目资产”决定，而应由“是否控制当前 AgentRun”决定；不能原地写入时 fork 到自己名下。
- 用户应能在自己的 AgentRun 上显式 fork。显式 fork 不表示权限隔离，而是探索不同后续路线。
- 每个稳定会话轮次下方应提供轻量 action toolbar，至少包含复制内容和从此处 fork 两个动作。按钮使用图标+tooltip，不用大块文字按钮；默认低干扰，hover/focus 时清晰可用。
- “从此处 fork”应以该轮次的稳定 message/turn boundary 作为 fork point，创建新 AgentRun 并导航到新工作台。原 AgentRun 保持不变。
- “复制内容”应只复制当前大轮会话里最后一小轮 agent 回复的可读消息内容，避免用户为了转述或留档手动选中最终答案。
- 原 AgentRun 在 fork submit 后保持不变；后续用户输入应进入 forked AgentRun。
- forked AgentRun 应保留可理解的跨 Run 父子关系：child 工作台能跳回 parent，parent 工作台能看到可见 child fork；同 Run 内 subagent 控制树继续由 `agent_lineages` 表达。
- forked AgentRun 应有明确的当前用户归属事实。该事实应在 domain / persistence 层表达，而不是由标题、Session metadata 或 mailbox source 推断。
- Project 权限应从 `View/Edit/ManageSharing` 收敛为 `Use/Configure/ManageSharing` 语义：`Use` 包含查看、运行、fork 和继续自己的 AgentRun；`Configure` 才允许修改 Project / ProjectAgent / VFS / backend access / workflow / MCP preset / skill asset 等 Project 层资产。
- Session / RuntimeSession 相关命名允许存在于内部模块、内部 DTO、内部持久化和 runtime trace plumbing；产品 API、前端页面路径、用户文案、任务描述和对外契约应使用 AgentRun / runtime trace 等业务语义。
- Session API 收口后，仍需保留 AgentRun workspace 所需的事件流、projection、tool approval、runtime control 等能力，但入口应由 AgentRun API 或内部 backend service 间接提供。

### Technical Requirements

- 新增 AgentRun fork application service，原子编排 RuntimeSession fork、AgentRun 控制面 materialization、跨 Run fork lineage 写入、mailbox envelope 创建和 scheduler 投递。
- 新增或扩展 domain/persistence 字段以表达 `created_by_user_id` / `initiator_user_id` / `forked_by_user_id` 等用户归属事实，并处理数据库 migration。
- 移除 Project `viewer` 作为“只能看”的长期产品角色；可迁移为 `member`，保留 `editor` / `owner` 作为配置和共享管理能力。
- 新增 AgentRun 级 fork / fork-submit API，支持显式 `fork_point_ref` 和可选 initial input；或让 composer-submit 在 command policy 中返回 fork target。API 响应需要能让前端导航到新 AgentRun。
- AgentRun workspace projection 或 runtime feed model 需要暴露前端 action toolbar 所需的稳定 fork boundary：至少能把某个 UI 轮次映射到后端可校验的 `MessageRef` / turn boundary。
- 将裸 `/sessions/{id}/fork`、`/sessions/{id}/lineage`、`/sessions/{id}/projection/rollback` 从产品可调用面收口。过渡期间若保留内部诊断 route，必须接入 Project 权限检查，并且前端产品流不能直接调用。
- 将前端 service 层的 `/sessions/*` 直接调用迁移到 AgentRun scoped service，保留内部文件名/组件名重命名计划，避免一次性大规模无意义重命名影响评审。
- 更新 `.trellis/spec/` 中关于 session HTTP surface 的描述，记录新的边界原因：Session 是 runtime trace/projection 内部事实，AgentRun 是用户可见控制面。

### Out Of Scope

- 不实现跨项目 fork。
- 不继承父 Run 的用户授权事实、tool approval、临时 grant 或 provider live private state；fork 继承模型上下文和可恢复 trace provenance，授权事实由新 AgentRun 按当前用户重新建立。
- 不做兼容旧 `/sessions/*` 产品入口的长期回退方案。预研期目标是把事实源收束到正确模型。

## Acceptance Criteria

- [ ] 当前用户对其它用户 AgentRun 提交输入时，后端创建新的 current-user forked AgentRun，返回新 `run_id` / `agent_id`，并把输入投递到新 Run mailbox。
- [ ] 当前用户可在自己的 AgentRun 上从某个稳定轮次显式 fork，新 Run 继承该轮次之前的模型上下文，父 Run 不变。
- [ ] 每个已稳定的会话轮次下方展示复制和 fork action；复制按钮能把该大轮里最后一小轮 agent 回复的可读消息内容写入剪贴板，fork 按钮能创建并导航到 forked AgentRun。
- [ ] fork 按钮只在后端可解析的稳定 boundary 上启用；运行中或半工具调用边界不可 fork 时给出明确不可用状态。
- [ ] 父 AgentRun 不接收 fork submit 的用户输入；父 RuntimeSession event stream 和 mailbox 不被该操作追加用户消息。
- [ ] forked AgentRun 有持久化用户归属事实，并在 workspace projection 中可见。
- [ ] forked AgentRun 与 parent AgentRun 有跨 Run fork lineage 关系；父子工作台均可通过 AgentRun ref 跳转，同 Run subagent 控制树不被混用。
- [ ] Session fork/projection/lineage route 不再作为产品 API 暴露给前端业务流；必要内部 route 均通过 `ensure_session_permission` 或等价 Project 权限检查。
- [ ] 前端用户可见文案、路由和服务入口不再把 Session 作为可操作业务对象；内部 runtime feed 允许继续使用 session id 作为实现细节。
- [ ] `.trellis/spec/` 中 session lineage / startup / frontend architecture 相关描述更新为 AgentRun 外露、Session 内部的边界模型。
- [ ] 覆盖后端 application/API tests：fork submit 幂等、权限、父 Run 不变、child mailbox delivery、lineage 写入、Session route 权限。
- [ ] 覆盖前端 tests：composer submit 收到 fork redirect 后导航到新 AgentRun，Session service 直连入口不再被产品组件调用。
