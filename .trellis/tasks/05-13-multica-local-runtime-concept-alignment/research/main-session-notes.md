# 主会话研究记录

## 当前目标

本任务当前处于 planning 阶段，目标是为后续完整学习 `references/multica` 建立导航层：

- 概念对应：AgentDash 的 Project / Workspace / Story / Task / Session / Workflow / Lifecycle / Routine / Skill Asset / Canvas / Local Backend / Relay / VFS 等，分别与 multica 的 Workspace / Project / Issue / Agent / Runtime / Task Queue / Task Message / Skill / Autopilot / Inbox / Activity / Daemon / Desktop 等如何对应。
- 目录对应：AgentDash 各 Rust crate、前端目录、脚本/文档，与 multica 的 `server/`、`apps/`、`packages/`、`docs/`、`scripts/` 如何对应。
- 学习评估：哪些机制值得吸收，哪些需要按 AgentDash 架构改写，哪些不应照搬。
- 任务转化：将研究结论拆成后续正式 Trellis 任务候选。

## 已确认事实

- AgentDash 当前采用 Rust/Axum cloud backend + `agentdash-local` 本机后端 + Relay WebSocket + SessionHub + VFS + Pi Agent Loop + Vite 前端。
- multica 采用 Go server + CLI/daemon + PostgreSQL/sqlc + Next web + Electron desktop + `packages/core/views/ui` 共享包结构。
- AgentDash 的强项在底层抽象：VFS、Hook Runtime、Lifecycle DAG、SessionHub、Pi Agent Loop、Backbone/session event。
- multica 的强项在产品闭环与运行时运维：Issue/Comment/Inbox/Activity、Agent 作为团队成员、daemon runtime 注册/心跳/恢复、task queue/task message、desktop 管 daemon 的体验。
- 用户已恢复原始整体 review 到 `research/multica-module-review.md`，主会话不得覆盖。

## 初步概念对应

| 维度 | AgentDash | multica | 初步判断 |
| --- | --- | --- | --- |
| 顶层组织 | Project | Workspace | 名称不同；AgentDash Project 更像产品/业务容器，multica Workspace 更像团队租户边界 |
| 代码工作区 | Workspace + Binding + VFS mount | Workspace repo / repo cache / worktree | AgentDash 更虚拟化，multica 更物理 worktree 化 |
| 工作单元 | Story / Task | Issue | AgentDash 分 Story/Task，multica 以 Issue 为协作核心 |
| 执行尝试 | Session / SessionBinding / task execution | agent_task_queue / task_message | multica 对执行 run 的状态、日志、恢复更产品化 |
| 本机进程 | agentdash-local | multica daemon | 同类问题不同解法；需要重点对比 |
| 本机在线实体 | BackendRegistry / backend config | agent_runtime | multica runtime 状态机更完整 |
| 执行器抽象 | AgentConnector / Pi Agent / ACP / vibe-kanban | pkg/agent.Backend | AgentDash 抽象更强，multica provider adapter 工程细节更成熟 |
| 自动化 | Routine | Autopilot | 可对齐 trigger/run/failure governance |
| 技能资产 | Skill Asset / built-in skills / CODEX_HOME | Skill / skill_file / agent_skill / local skills | AgentDash 更平台化，multica 更贴近 provider 文件注入 |
| 实时事件 | SSE / session stream / relay events | WS Hub / protocol events | multica 的业务事件命名和前端 cache sync 值得学 |
| 前端复用 | frontend monolith with features/stores/services | apps/web + apps/desktop + packages/core/views/ui | multica 的跨 web/desktop 边界值得学 |

## 目前最值得学习的机制候选

1. Runtime 生命周期：heartbeat、last_seen、offline sweeper、runtime gone recovery、orphan task recovery。
2. Task execution 可观测性：task queue 与 task message 分离，工具调用/thinking/text/error 序列化存储。
3. Desktop 管理本机能力：daemon profile 隔离、health polling、日志 tail、安全重启、版本 mismatch 策略。
4. 前端 server state 纪律：TanStack Query query key + WS granular patch/invalidate。
5. 协作产品闭环：agent/member/system actor、comment、subscriber、activity、inbox。
6. 自动化失败治理：Autopilot run history、skip/failure reason、失败率自动暂停。
7. Provider 原生文件注入：`AGENTS.md`/`CLAUDE.md`/`GEMINI.md` 和 per-task skill materialization。

## 不应直接照搬的方向

- 不应把 AgentDash 的 Story/Task/Session 直接改成 multica Issue/TaskQueue 语义。
- 不应用 multica 的 Go handler/sqlc 结构替代 Rust 分层 crate 与 repository trait。
- 不应把 AgentDash 的 AgentConnector/Hook/Capability/Pi Agent Loop 降级为单次 prompt runner。
- 不应让物理 worktree 管理绕过 VFS；可学习 workdir 生命周期，但应落在 VFS/materialization 设计内。

## 只读 subagent 研究结论

- `research_cloud_data_events`：确认业务事件协议、runtime health、ExecutionAttempt、Inbox/Activity/Subscriber 是云端最值得学习的方向；强调 event bus/listener/fanout 分层和 runtime SQL sweeper 防竞态细节。
- `research_local_daemon`：确认 AgentDash 不应改成 poll claim，但应学习 runtime_gone、slot-before-claim、mid-flight session pinning、poisoned session、workdir GC meta、provider adapter 测试矩阵。
- `research_frontend_desktop`：确认 AgentDash 当前无正式 desktop/Tauri 包；multica 的 desktop local daemon manager、profile/token/version 策略、IPC bridge、React Query server-state 纪律值得学习。
- `research_product_automation`：确认 Activity Timeline、Actor、ExecutionAttempt、Routine admission skip/failure governance、Local Skill inventory/import 是产品层重要候选；提醒 `RoutineExecution.completed` 当前不等于 Agent terminal。

主会话已将共识沉淀到 `subagent-feature-synthesis.md`，并补充 `cloud-capability-map.md`、`local-daemon-comparison.md`、`desktop-local-integration.md`、`learning-backlog.md`、`concept-map.md`、`directory-map.md`。
