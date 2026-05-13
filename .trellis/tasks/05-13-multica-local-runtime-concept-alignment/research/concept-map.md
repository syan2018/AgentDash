# 概念映射：AgentDash ↔ multica

## 核心映射表

| 领域概念 | AgentDash | multica | 关键差异 |
| --- | --- | --- | --- |
| 租户/团队边界 | Project + grants；Workspace 是项目下逻辑工作区 | Workspace 是团队/租户边界 | multica Workspace 更接近 SaaS team；AgentDash Project 更接近产品工作容器 |
| 代码工作区 | WorkspaceBinding、VFS mount、relay fs、inline fs | workspace repos、repo cache、per-task worktree | AgentDash 抽象虚拟文件系统；multica 偏物理 repo/worktree |
| 工作需求 | Story | Issue | AgentDash Story 承载用户价值和上下文；multica Issue 是协作与执行核心 |
| 最小执行单元 | Task + SessionBinding | agent_task_queue | AgentDash Task 是业务实体；multica task queue 更像执行 attempt |
| 会话 | SessionHub / ACP session / Backbone events | agent CLI session_id + task messages | AgentDash 会话更通用；multica 会话信息附在 task run 上 |
| Agent | Agent + ProjectAgentLink + executor config | Agent profile + runtime binding + skills | multica Agent 是团队成员感更强，出现在 assignee/comment/activity 中 |
| 本机运行时 | BackendConfig / online backend / local backend | agent_runtime / daemon | multica runtime 生命周期更完整，AgentDash relay transport 更强 |
| 执行器 | AgentConnector / Pi Agent / vibe-kanban / ACP | pkg/agent.Backend adapters | AgentDash 抽象更深；multica CLI provider 适配细节值得学 |
| 技能 | Skill Asset、built-in skill、CODEX_HOME、context dimension | skill、skill_file、agent_skill、local skill report | multica skill 更产品化为 workspace asset；AgentDash skill 与 VFS/MCP/plugin 更强耦合 |
| 自动化 | Routine | Autopilot | 两者可对齐 trigger/run/history/failure governance |
| 实时状态 | SSE stream、session stream、relay event | WS Hub、scope rooms、protocol events | multica 业务事件粒度更清晰；AgentDash session event 更细 |
| 通知 | 当前散落在 session/task/status UI | Inbox + Activity + Subscriber | multica 协作反馈闭环更成熟 |
| 前端状态 | Zustand stores + services + SSE | TanStack Query server state + Zustand client state + WS sync | multica 的 server state 纪律值得学习 |
| 桌面端 | 规划中 | Electron app + daemon manager | multica 已有 desktop 管本机 daemon 的产品形态 |

## 需要特别避免的命名误读

- multica 的 `Task` 不是 AgentDash 的 `Task`。前者是 agent execution run / queue item，后者是业务拆解出来的工作项。
- AgentDash 的 `Workspace` 不是 multica 的 `Workspace`。前者偏代码工作区和物理/逻辑绑定，后者偏团队租户。
- multica 的 daemon/runtime 不等同于 AgentDash 的 executor。daemon 是本机常驻任务执行进程，runtime 是服务端登记的可执行资源；AgentDash local backend 同时承担工具、终端、MCP、可选 SessionHub。
- multica 的 Skill 不等同于 AgentDash Plugin。Skill 是可注入文本/文件资产；Plugin 是扩展平台能力的 API。

## 值得学习的概念升级

1. **Agent 作为 actor**  
   multica 在 assignee、creator、comment author、subscriber、activity 中统一 member/agent/system。AgentDash 可以在 Story/Task/Session/Activity 中引入统一 actor 视角。

2. **业务工作项与执行尝试分离**  
   multica Issue 与 agent_task_queue 分离清晰，便于记录多次执行、重试、失败原因和 task message。AgentDash 可保持 Task 语义，但补一个 execution attempt / run 投影。该投影不应成为新的执行事实源，Backbone/session event 仍是事实源。

3. **Runtime 是可运营资源**  
   multica runtime 有 status、last_seen、visibility、timezone、models、local skills、usage rollup。AgentDash online backend 当前更像连接表，可升级为可运营的 Runtime/Backend Health 资源。

4. **协作反馈闭环**  
   Comment、Activity、Inbox、Subscriber 让用户不必盯 session 流。AgentDash 可把当前 session/task 状态变化沉淀成可订阅通知。

5. **自动化治理闭环**  
   multica Autopilot 的 admission skip、run history、failure-rate auto-pause 值得 Routine 学习。AgentDash Workflow/Lifecycle 不应被简化为 Autopilot；Routine 需要先把 completed/failed 与真实 session terminal 对齐。

5. **Desktop 是 local 能力控制台**  
   multica desktop 不只是 web wrapper，还负责 CLI bootstrap、daemon health、日志、版本重启和运行时列表。AgentDash 后续 desktop 应以 local backend 控制台为核心卖点之一。

## 暂不适配或需要改写

- 不把 AgentDash 的 Workflow/Lifecycle 简化成 multica Autopilot；AgentDash 的 DAG/Hook 能力更强。
- 不用 multica 的 physical worktree 覆盖 VFS；只学习 workdir 生命周期、provider 原生文件注入、GC。
- 不把 AgentDash session stream 替换为 CRUD WS event；可以新增业务事件层，但 session/backbone 仍是执行事实源。
- 不照搬 multica 的 sqlc 数据行模型；AgentDash 保持 domain/repository 分层，但可以学习其 SQL 查询覆盖面和 rollup 思路。
