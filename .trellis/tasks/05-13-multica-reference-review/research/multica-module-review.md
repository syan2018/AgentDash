# multica 模块 review

## 基本信息

* 仓库：`https://github.com/multica-ai/multica`
* 本地路径：`references/multica`
* 分支：`main`
* 技术栈：Go 后端/CLI/daemon + PostgreSQL/sqlc + Next.js Web + Electron Desktop + pnpm/turbo monorepo。
* 关键定位：把 coding agent 作为 issue 看板中的一等协作者，提供任务分配、daemon 执行、技能沉淀、自动化和实时进度。

## 模块观察

### 1. 产品模型

multica 的产品模型围绕 Workspace / Issue / Agent / Runtime / Task / Skill / Autopilot 展开。最值得注意的是“多态行动者”模式：Issue 的 assignee、creator、comment author、subscriber 等均可为 member 或 agent，这让 agent 在 UI 和数据模型里自然成为团队成员，而不是隐藏在执行器后面的技术资源。

对 AgentDashboard 的启发：

* 当前 AgentDashboard 已经有 Story / Task / Session 分工，模型更偏编排系统；可以借鉴 multica 的“agent 作为可见 actor”表达，在看板、评论、活动流、订阅/通知中统一处理 human/agent/system actor。
* 如果后续要强化团队协作视角，建议把“谁发起、谁处理、谁被通知”从 session 执行层上提成跨模块通用 actor 表达。

需要谨慎：

* multica 的 Issue 是核心工作单元，Task 更像 agent execution run；AgentDashboard 当前 Task 是业务状态机与 Session 策略壳，两者语义不同，不能直接迁移命名。

### 2. 后端 API 与实时事件

Go 后端使用 Chi Router，按 public / daemon / protected / workspace-scoped 分组，daemon API 和用户 API 清晰分离。实时事件集中定义在 `server/pkg/protocol/events.go`，事件命名按领域前缀组织，例如 `issue:*`、`task:*`、`daemon:*`、`autopilot:*`。

对 AgentDashboard 的启发：

* 当前项目已有 BackboneEnvelope 和 session 流式事件体系，但业务域事件可以进一步学习 multica 的“领域前缀 + 用户可感知变化粒度”命名方式，用于 Story/Task/Workspace/Runtime/Capability 的非流式状态广播。
* daemon 专用 API 独立分组是个好模式：本机后端/云端 relay 的认证、heartbeat、claim、消息上报、GC-check 等最好保持独立边界，不与普通用户 API 混在一起。

需要谨慎：

* multica 的事件层偏传统 CRUD + WS invalidation；AgentDashboard 的 session/backbone 事件语义更细，不能降级为简单 CRUD 事件。

### 3. Agent Runtime 与 CLI Adapter

`server/pkg/agent/agent.go` 定义统一 Backend 接口，所有 provider 都实现 `Execute(prompt, opts) -> Session{Messages, Result}`。Claude、Codex、Copilot、Cursor、OpenCode、Hermes、Gemini、Pi、Kimi、Kiro 等差异都被各 adapter 吸收，统一输出 text/thinking/tool/status/error/log 与 token usage。

对 AgentDashboard 的启发：

* 当前 `AgentConnector` 已经比 multica 更泛化，分出 session frame / turn frame / capability state / tools / hook runtime。multica 的可借鉴点不是抽象本身，而是 provider adapter 的工程细节：每个 CLI 的 JSON/stream 协议、session_id 捕获、resume 失败识别、token usage 兜底、Windows 参数转义都有独立测试。
* 对 Codex/Claude/其它 CLI connector，可以建立类似“provider behavior matrix”：启动参数、输入方式、resume 字段、usage 来源、错误分类、Windows 注入风险。

需要谨慎：

* multica 的 Backend 只抽象“跑一个 prompt”，而 AgentDashboard 的 PiAgent/ACP/Hook/Capability 需要 turn 内热更新、工具审批、ContextFrame、MCP relay，不应把 connector 收窄到 multica 的形态。

### 4. Daemon 调度与运行时运维

multica daemon 本地轮询 claim task，同时使用 heartbeat 和 WebSocket wakeup；每个 runtime 有独立 poller/heartbeat，支持 runtime gone 恢复、孤儿任务恢复、取消轮询、并发 slot、task workdir 复用、session resume fallback、usage 上报、GC metadata。

对 AgentDashboard 的启发：

* 本项目云端/本机双后端已经有 relay 方向，multica 的 runtime 运维细节很值得借鉴：runtime deleted 后本机自恢复、heartbeat ack 与 HTTP heartbeat 互补、claim 前先拿并发 slot、防止 claimed task 堆积、in-flight shutdown 等待、任务取消轮询。
* AgentDashboard 后续完善本机 backend 时，可以把这些作为可靠性 checklist，而不是等问题出现再补。

需要谨慎：

* multica daemon 是任务 claim 模式，AgentDashboard 当前有云端下发命令和 PiAgent tool call 路由，控制流不同；可学“恢复机制”和“状态边界”，不必照搬 poll 模型。

### 5. 工作目录、仓库缓存与技能注入

multica 使用 bare repo cache + per-task git worktree，分支命名 `agent/<agent>/<task>`，并把 agent runtime 文件写入不同 provider 的原生位置：Claude 写 `CLAUDE.md`，Codex/Copilot/OpenCode 等写 `AGENTS.md`，Gemini 写 `GEMINI.md`。Skill 同时支持 workspace skill 和本地 skill，且处理同名冲突、复用目录刷新、旧技能清理。

对 AgentDashboard 的启发：

* 当前项目正在做 VFS/skill-assets/materialization，multica 的经验提醒我们：agent-facing 指导文件和技能文件最好按 provider 原生发现机制落位，而不是只靠 prompt 文本。
* workdir 复用时要同步刷新 skills，并清理被移除或被 workspace skill 覆盖的用户 skill，避免旧上下文污染新任务。
* git cache 层要显式处理 default branch 变化、fetch refspec、branch collision、用户 hook 保护、agent 注入文件的 `.git/info/exclude`。

需要谨慎：

* AgentDashboard 的 VFS/mount 目标更强，不能退回到“物理 worktree 即全部上下文”；但 provider 原生配置文件可以作为 VFS materialization 的一个输出面。

### 6. 前端架构

multica 前端把共享业务能力放在 `packages/core`，跨 Web/Desktop 复用视图放在 `packages/views`，`apps/web` 和 `apps/desktop` 作为宿主。服务器状态使用 TanStack Query，客户端临时状态使用 Zustand；WebSocket 事件通过局部 cache patch + invalidate 混合处理。

对 AgentDashboard 的启发：

* 当前前端已有 `features/stores/pages`，但随着桌面端和 Web 管理一体化推进，可以考虑借鉴“core 查询/状态 + views 复用 + app 宿主适配”的边界，把平台差异收在 adapter，而不是让页面组件感知太多环境差异。
* multica 的纯函数 derivation 值得学习，例如 agent presence、activity sparkline、runtime health 等都从 raw server data 派生，便于测试和复用。

需要谨慎：

* AgentDashboard 的数据流高度依赖 session streaming、BackboneEnvelope 和 hook/capability 可视化，不能仅用 TanStack Query CRUD 模式覆盖。

### 7. 桌面端与本机 daemon 体验

Electron 桌面端负责 CLI bootstrap、profile 隔离、daemon auto-start/stop、health polling、日志 tail、版本 mismatch 安全重启、用户切换 PAT 轮换。Desktop 使用专属 `desktop-<host>` profile，避免污染用户手工配置的 CLI profile。

对 AgentDashboard 的启发：

* 本机后端管理 UI 可以借鉴“独立 profile + health endpoint + 日志流 + 安全重启”的组合。
* 版本不一致时，不在有 active task 时强杀 daemon，而是标记 pending restart，等任务清空后再重启，这是本机执行器升级体验的好范式。

需要谨慎：

* 本项目当前要求 `pnpm dev` 统一拉起云端/本机/前端，Rust 本机后端不可热重载；开发期管理策略和最终桌面端守护策略要区分。

### 8. Skill 与 Autopilot

Skill 是 workspace 级实体，支持 skill_file 和 agent_skill 关联；Autopilot 支持 schedule/webhook/api 触发，生成 issue 或 run-only 任务。两者把“可复用能力”和“自动触发”提升为产品对象，而不是只存在于本地目录或 cron 脚本里。

对 AgentDashboard 的启发：

* 当前 plugin/skill/VFS 体系可以学习其“Skill 作为可分配、可导入、可审计的工作区资产”视角。
* Workflow lifecycle 与 Routine trigger 可以参考 Autopilot 的触发器/运行记录模型，把定时/外部 webhook/API 触发统一为可追踪 run。

需要谨慎：

* multica Skill 更像文本/文件注入；AgentDashboard 的 plugin API、MCP、VFS、capability state 更强，Skill 不应吞掉 Plugin/Capability 的职责。

## 值得优先学习的清单

1. Agent actor 一等公民：member/agent/system 统一建模，贯穿 assignee、creator、comment、activity、subscriber。
2. Daemon 可靠性细节：runtime gone 自恢复、claim 前并发 slot、取消轮询、orphan recovery、WS heartbeat 与 HTTP heartbeat 互补。
3. Provider adapter 测试矩阵：session id 捕获、resume fallback、usage 解析、Windows shell/编码差异。
4. Provider 原生指导文件注入：按 CLI 读取习惯写 `AGENTS.md`/`CLAUDE.md`/`GEMINI.md`，把长指令从 per-turn prompt 中移出。
5. Skill 文件复用/冲突/清理策略：workspace skill 优先级、用户本地 skill seed、复用 workdir 时刷新。
6. 前端 core/views/app 三层复用：为未来 Web/Desktop 一体化降低重复。
7. Desktop daemon profile 隔离：桌面专属 profile、PAT 轮换、日志 tail、版本安全重启。
8. 产品级自动化对象：Autopilot 的 trigger/run 模型可为 Routine/Workflow 外部触发提供参考。

## 不建议直接照搬

* 不建议把 AgentDashboard 的 Story/Task/Session 改成 multica 的 Issue/Task 语义；两者领域目标不同。
* 不建议把当前 Connector/Capability/Hook 抽象降级为单一 prompt runner；AgentDashboard 当前能力面更适合长会话和动态上下文。
* 不建议照搬 daemon poll-only 模式；本项目已有云端/本机 relay 与 PiAgent tool routing，需要保留主动下发和工具转发路径。
* 不建议照搬前端 CRUD/query 结构来处理 session stream；只能借鉴 cache patch、纯派生和跨宿主复用方式。

## 后续可展开专题

* 专题一：Codex/Claude provider adapter 行为矩阵，对照 `agentdash-executor` 的 connector 实现补测试。
* 专题二：VFS materialization 输出 provider 原生指导文件和 skills 目录的设计。
* 专题三：本机 backend health / log / restart / version 管理 UI。
* 专题四：Story/Task/Session 中 actor/activity/subscriber/inbox 的统一建模。
