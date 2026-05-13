# 本地连接与运行时：AgentDash local ↔ multica daemon

## 概念对应

| 维度 | AgentDash | multica |
| --- | --- | --- |
| 本机进程 | `agentdash-local` | `multica daemon` |
| 云端连接 | `/ws/backend` relay WebSocket | daemon HTTP API + daemon WS wakeup/heartbeat |
| 在线资源 | BackendRegistry 中的 ConnectedBackend | `agent_runtime` rows |
| 能力上报 | executors、supports_cancel、MCP servers、accessible roots | provider、runtime mode、device info、models、local skills、repos/settings |
| 执行路径 | 云端下发 relay command，本机执行工具/session/terminal/MCP | daemon claim queued task，构造 workdir，调用 CLI agent，上报 task messages/result |
| 会话恢复 | SessionHub persistence + interrupted recovery | task session_id/work_dir pinning + orphan recovery + resume fallback |
| 文件系统 | accessible roots + VFS/relay fs/materialization | repo cache + per-task worktree + env root GC |

## 连接握手与注册

AgentDash：

- `crates/agentdash-local/src/ws_client.rs` 连接云端，首条发送 `RelayMessage::Register`。
- `crates/agentdash-api/src/relay/ws_handler.rs` 校验 token 绑定 backend，拒绝 backend_id mismatch 或重复在线。
- `crates/agentdash-api/src/relay/registry.rs` 保存 online backend、pending request、per-session sink。

multica：

- CLI/daemon 由 `references/multica/server/cmd/multica/cmd_daemon.go`、`server/internal/daemon/daemon.go` 管理。
- daemon 启动后检测 CLI provider，向 server 注册 runtimes。
- `agent_runtime` 通过 `(workspace_id, daemon_id, provider)` upsert，服务端记录 status/last_seen/device/metadata。

可学习点：AgentDash 当前注册模型更像“连接在线表”，multica runtime 更像“可运营资源”。后续可把 backend/runtime 的 status、last_seen、owner、capabilities、health、version、logs 统一建模。

## 心跳、活性与恢复

AgentDash：

- relay 连接断开时 `BackendRegistry.unregister` 删除 online backend。
- 断连时会把相关 terminal 标记为 lost 并注入 session event。
- SessionHub 有 `recover_interrupted_sessions`，将上次 running 标记 interrupted。

multica：

- daemon 有 heartbeat loop、WS heartbeat ack freshness、runtime gone recovery。
- server 有 stale runtime sweeper、offline runtime fail task、runtime liveness store。
- daemon 启动和 runtime gone 后会 RecoverOrphans，失败任务走统一 retry/rollback/事件广播路径。

可学习点：

- `last_seen_at + status + sweeper` 比单纯连接表更适合跨进程/跨节点。
- runtime 被服务端删除后，本机应能收敛并重新注册。
- 启动恢复不只标记 session interrupted，还要处理执行队列、关联任务、用户可见状态。
- multica 的 `runtime_gone` 不是简单重连，而是本机收到服务端 runtime 消失信号后做去抖、收敛、重新注册；AgentDash 若引入该协议，需要明确 backend token/config 与 runtime row 的关系。

## 任务领取与执行

AgentDash：

- 云端主动向 backend 发 relay command，等待 response。
- 本机可选 SessionHub 启动第三方 executor，也可执行工具/终端/MCP。
- 控制流适合 VFS/tool routing 和 live session。

multica：

- daemon 按 runtime claim task，claim 前有并发 slot、empty claim cache、wakeup。
- 执行时构造 prompt/runtime brief/workdir/env，启动 provider backend。
- `executeAndDrain` 将 text/thinking/tool_use/tool_result/error 批量上报到 `task_message`。
- 完成/失败时上报 usage、session_id、work_dir、failure_reason。

可学习点：

- 即便 AgentDash 不改成 poll claim，也可引入“执行 attempt + message log”的投影。
- provider adapter 应捕获 session_id/usage/error classification，并独立测试。
- cancellation/status poll、drain timeout、poisoned output classification 值得进入执行器 checklist。
- multica “先拿本机 execution slot 再 claim task”的经验应转译为 AgentDash 的 backend capacity / dispatch 前检查，避免云端已派发但本机长期排队。
- `PinTaskSession` 在执行中途拿到 provider session id 时立即持久化，而非等 complete/fail；AgentDash 已有 `ExecutorSessionBound` 事件，应继续以事件驱动方式提升 crash 恢复成功率。
- `poisoned.go` 与 resume lookup 的坏上下文排除规则值得学习，用来避免 iteration limit、invalid request、agent fallback 等失败不断被自动恢复。

## 工作目录与 GC

AgentDash：

- VFS 是主抽象，local backend 暴露 accessible roots。
- materialization 正在成为跨 mount 写入物理工作区的桥。

multica：

- `repocache` 管 bare repo cache 和 worktree。
- `execenv` 写 provider 原生配置文件、skills、Codex home。
- GC 支持 completed/cancelled task 全量清理、orphan cleanup、artifact-only cleanup。

可学习点：

- AgentDash 不应绕过 VFS，但 VFS materialization 可学习 env root lifecycle、artifact cleanup、provider 原生文件布局。
- provider 指导文件不一定都放 prompt；可按 CLI 原生发现机制写入 `AGENTS.md`、`CLAUDE.md`、`GEMINI.md`、`CODEX_HOME`。
- `execenv` 的 active root 防护与 GC meta 可用于 AgentDash materialized workdir：为 session/task/tool_call 记录 owner、ttl、last_access，并避免执行中目录被清理。

## 后续正式任务候选

1. `feat(runtime): backend/runtime health 状态机与 last_seen sweeper`
2. `feat(runtime): local backend 断连恢复与 orphan session/task reconciler`
3. `feat(task): execution attempt 与 task message 持久化`
4. `feat(vfs): materialized workdir 生命周期与 artifact GC`
5. `test(executor): provider adapter 行为矩阵与 session/usage/error 分类测试`
6. `feat(runtime): backend capacity / execution slot 与 dispatch 前检查`
