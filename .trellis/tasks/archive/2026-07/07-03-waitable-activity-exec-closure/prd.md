# 补齐通用 waitable activity 与 exec 交互闭环

## Goal

在 AgentDashboard 自有 AgentRun、Runtime Tool、VFS/Relay exec、companion/subagent、human、LifecycleGate、mailbox 与 frontend projection 体系内，补齐一套 Agent 可直接使用的通用等待能力与 exec 交互闭环。

本父任务负责完整评估、总体设计、任务拆分和最终集成验收。实际实现拆成两个子任务：

- `07-03-exec-terminal-blocker-repair`：修复当前 exec 指令式终端交互、Windows PowerShell 环境提示和前端终端观察/跳转阻断。
- `07-03-waitable-activity-module`：补齐通用 waitable activity / wait module，并把 exec、companion/subagent、human、mailbox wake 接入同一等待路径。

目标不是接入 Codex runtime，也不是复制 Codex Thread / AgentPath / session API；目标是借鉴 `references/codex` 已闭合的能力模型，把能力落到 AgentDashboard 自己的 AgentRun control-plane、runtime tool catalog、mailbox、gate 和 projection 上。

## Confirmed Facts

- 当前任务在 `main` 上，base branch 是 `main`，父任务已经挂载两个子任务。参考仓库 `references/codex` 已更新到最新 `origin/main` 用于评估。
- Agent runtime tool surface 由 `SessionRuntimeToolComposer` 聚合 provider，并拒绝重复 tool name；工具在 launch preparation 阶段写入 `ExecutionTurnFrame.assembled_tools` 和 tool schema context。证据：`crates/agentdash-application/src/runtime_tools/provider.rs:60`、`crates/agentdash-application-runtime-session/src/session/tool_assembly.rs:22`。
- 当前 session bootstrap 只注册 VFS、workflow、collaboration、task、workspace module provider，没有独立 wait provider。证据：`crates/agentdash-api/src/bootstrap/session.rs:468`。
- 当前 VFS execute cluster 只把 `shell_exec` 暴露给 Agent，但 `shell_exec` 还没有 `operation=start|read|write|terminate|status` 的指令式终端交互模型。证据：`crates/agentdash-application-vfs/src/tools/factory.rs:116`。
- `shell_exec` 的 long-running result 已返回 `state`、`session_id`、`terminal_id`、`next_seq`；规划收束为只暴露 canonical `terminal_id`，底层本机 shell session 和 runtime trace refs 留在 terminal record 内部。证据：`crates/agentdash-application-vfs/src/tools/fs/shell.rs:393`、`crates/agentdash-application-vfs/src/tools/fs/shell.rs:473`。
- Relay/local 已有 shell read/input/terminate primitives：`ToolShellReadPayload`、`ToolShellInputPayload`、`ToolShellTerminatePayload`，local `ShellSessionManager` 支持 `read_session(wait_ms)`、`input_shell`、`terminate_shell`。证据：`crates/agentdash-relay/src/protocol/tool.rs:83`、`crates/agentdash-local/src/shell_session_manager.rs:327`。
- Windows OS shell 当前由 `powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command` 执行，并设置 UTF-8 output encoding。证据：`crates/agentdash-local/src/shell_session_manager.rs:798`。
- Environment ContextFrame 已经是 Windows shell/object-output 提示的正确位置；当前实现已有 PowerShell object output note，但还需要更明确声明 shell kind、PowerShell 命令组合规则、对象输出字符串化示例和 bash-only 语法不可用。证据：`crates/agentdash-application-runtime-session/src/session/environment_context_frame.rs:55`。
- companion/human 当前存在工具内部私有 gate polling：`wait=true` 会直接轮询 durable `LifecycleGate` payload，占用当前 tool call，不是通用 wait module。证据：`crates/agentdash-application/src/companion/tools.rs:243`、`crates/agentdash-application/src/companion/tools.rs:1066`、`crates/agentdash-application/src/companion/tools.rs:1312`。
- AgentRun mailbox 已经是 durable delivery authority：scheduler 从 runtime session 反查 AgentRun target、claim mailbox message、delivery launch/steer，并发 `MailboxStateChanged` notification。wait module 应观察和写入 mailbox envelope，不能绕过 scheduler 自己恢复 turn。证据：`crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:154`、`crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:229`。
- Frontend 已有 terminal stream projection 和 mailbox waiting item projection，但它们只是观察面，不提供 Agent 可调用的 wait/read/input/terminate 能力。证据：`packages/app-web/src/features/session/model/sessionPlatformEventDispatcher.ts:19`、`packages/app-web/src/features/session/model/useTerminalStore.ts:163`、`packages/app-web/src/features/agent-run-workspace/ui/MailboxMessageRow.tsx:62`。
- `ConversationWaitingItemView.kind` 已预留 `exec`；mailbox spec 也明确 waiting fact 由 gate/wait record 持有，mailbox message 只承载 wake/result envelope。证据：`crates/agentdash-contracts/src/runtime/workflow.rs:1151`、`.trellis/spec/backend/session/agentrun-mailbox.md`。
- 仓库当前仍有历史 `/sessions/*` 诊断/trace endpoint，但本任务不得新增或依赖任何 `/sessions/*` 作为 wait/exec/AgentRun 控制面。产品命令面必须使用 AgentRun workspace identity。
- `references/codex` 的可借鉴点是能力形状：running command 返回 handle、completed command 返回 exit code；wait 返回小摘要和 refs；multi-agent/mailbox 通过事件/状态投影闭合。不得复制其 runtime/session/connection-scoped process identity。
- 当前工作树已经直接使用 `codex-utils-pty` 和 `codex-utils-output-truncation`；它们可以作为 local shell backend 内部实现细节继续保留。`agentdash-local` 同时声明的裸 `portable-pty = "0.9"` 当前没有直接调用点，若实现阶段仍只经由 `codex-utils-pty` 使用 PTY，则应作为依赖清理项移除。
- Trellis channel 的 Codex provider worker 当前在本机启动失败，原因是 supervisor 解析成 `node.exe app-server` 并找不到 `D:\ABCTools_Dev\AgentDashboard\app-server`；本次改用 `multi_agent_v1` subagents 完成研究，失败记录保留为工具链风险。

## Requirements

1. 父任务必须把当前缺口拆成两个可独立验收的实现任务：阻断修复与能力补全。
2. exec 阻断修复必须保持单一 `shell_exec` 工具面，通过 `operation=start|read|write|terminate|status` 覆盖读取、等待短窗口、写 stdin、终止、查询最终 exit status。
3. exec/terminal continuation 的公开和内部主轴都应收束到 `terminal_id` / terminal record；本机 shell session、backend refs、runtime trace refs 作为 terminal record 的私有字段或派生引用，不再形成并列链路。
4. Windows 环境提示必须通过 ContextFrame Environment 进入 Agent 可见上下文，并说明当前真实 shell 是 PowerShell、使用 PowerShell 语法、对象输出要显式字符串化、专用文件工具优先用于探查。
5. 前端修复只作为 observation repair：恢复 terminal output projection、running terminal state 和终端打开/跳转；前端不得成为 Agent wait/read 的事实源。
6. waitable activity module 必须提供统一 activity owner 与 wait service，表达 exec、companion/subagent、human、mailbox/runtime wake 等来源。
7. Agent tool catalog 必须出现通用 `wait` 工具。它可以等待已有 pending activity、未来 activity、timeout、completed/failed/cancelled，并返回 bounded summary、status、source refs 和下一步读取方式。
8. companion/subagent/human 等待必须迁入 wait module；`wait=true` 可保留为语法糖，但内部只能复用统一 wait service，不再保留各工具私有等待协议作为最终形态。
9. wait 返回不搬运大结果正文。大 stdout/stderr、companion result、artifact、mailbox payload 保留在对应 buffer/repository/projection；wait 返回 preview、refs、cursor 和读取建议。
10. mailbox/scheduler 继续作为 AgentRun delivery authority。wait module 可以创建/解析 wait activity 和写入 wake envelope，但不得直接绕过 scheduler 启动或恢复 turn。
11. 需要形成 exec completion/failure/cancel、companion result、human response、mailbox wake 的幂等 source/dedup/ref 策略。
12. Codex crate 复用必须保持收口：`codex-utils-pty` / output truncation 仅作为本机后端内部 helper；Codex exec/app-server protocol 只借操作语义，不作为 AgentDashboard terminal/wait activity 公共协议。
13. 实现阶段必须清理与选型冲突或冗余的依赖声明；若无直接 `portable_pty::` 调用，应移除 `agentdash-local` 的裸 `portable-pty` 依赖。
14. 不新增任何外部 `/sessions/*` endpoint，不把 RuntimeSession 重新提升为 workspace command owner，不引入 Codex runtime dependency。

## Acceptance Criteria

- [ ] 父任务产物包含 `prd.md`、`design.md`、`implement.md`，并引用 subagent research 与相关 specs。
- [ ] 两个子任务存在且范围清晰：`07-03-exec-terminal-blocker-repair` 与 `07-03-waitable-activity-module`。
- [ ] `implement.jsonl` 与 `check.jsonl` 使用真实 spec/research entries，不保留 seed-only manifest。
- [ ] 方案明确回答 waitable activity owner、terminal activity owner、wake envelope、mailbox interaction、scheduler trigger、ContextFrame 和 frontend projection 边界。
- [ ] 阻断修复子任务验收覆盖 running exec 通过 `shell_exec operation=read|write|terminate|status` 续接、Windows PowerShell ContextFrame、前端 terminal projection/jump。
- [ ] 能力补全子任务验收覆盖通用 wait tool、exec activity、companion/subagent/human gate adapter、mailbox wake adapter、timeout/cancel/completed/failed 状态。
- [ ] 实现检查确认 Codex crate 复用未扩大到 terminal/wait 公共协议，并清理未使用的直接 PTY 依赖。
- [ ] 最终实现不得新增 `/sessions/*` 控制面，不得把 Codex Thread/AgentPath/session identity 接进 AgentDashboard。
- [ ] 最终 PR 在 `main` 基线分阶段提交，提交信息使用 `type(scope): 中文信息` 并在备注列出分点更新。

## Out Of Scope

- 全面删除历史 `/sessions/*` 诊断/trace endpoint。当前任务只禁止新增或依赖它们作为新能力控制面。
- 复制 Codex runtime、Thread、AgentPath、session API、Codex app-server lifecycle。
- 无限持久化 terminal scrollback。大输出通过 bounded buffer、cursor、artifact/ref 读取。
- 重做整个 AgentRun workspace UI。前端只补与 Agent tool 闭环对应的必要 projection 和跳转。

## Research Artifacts

- `.trellis/tasks/07-03-waitable-activity-exec-closure/research/subagent-codex-reference.md`
- `.trellis/tasks/07-03-waitable-activity-exec-closure/research/subagent-current-chain.md`
- `.trellis/tasks/07-03-waitable-activity-exec-closure/research/main-evidence.md`
- `.trellis/tasks/07-03-waitable-activity-exec-closure/research/codex-crate-reuse.md`
- `.trellis/tasks/07-03-waitable-activity-exec-closure/research/trellis-channel-subagent-attempt.md`
