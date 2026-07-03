# 修复 exec 续读与终端观察阻断

## Goal

修复当前 `main://` / VFS real OS shell exec 的 Agent 可用性阻断：长命令返回 running 后，Agent 必须能通过同一个 `shell_exec` 工具继续读取输出、等待短窗口、写 stdin、终止、查询状态；Windows PowerShell 环境提示必须足够明确；前端必须能观察 running terminal output/state 并打开对应终端。

这是父任务 `07-03-waitable-activity-exec-closure` 的第一阶段子任务。

## Requirements

1. `shell_exec` 必须保持单一工具面，通过 `operation=start|read|write|terminate|resize|status` 覆盖完整终端交互。
2. `shell_exec operation=start` running result 必须返回唯一 canonical continuation ref：`terminal_id`。Agent 不需要同时记 `session_id` 和 `terminal_id`。
3. continuation operations 必须复用现有 relay/local shell read/input/terminate primitives，而不是重新发明 shell process manager。
4. read/status 需要返回 bounded chunks、`next_seq`、truncation、state、exit_code。
5. write/input 需要支持交互进程 stdin 和 close stdin，并返回写入后的短窗口 read 结果。
6. terminate 必须对 running/terminal/unknown session 给出幂等、typed result。
7. PTY terminal 必须支持 resize operation；非 PTY terminal 返回 typed unsupported/status。
8. Windows Environment ContextFrame 必须明确真实 OS shell 是 PowerShell，提示 PowerShell 语法、对象输出字符串化和 dedicated file tools 优先策略。
9. 前端必须能稳定接收 terminal output/state projection，并能从 `terminal_id` 或 waiting/projection refs 打开对应 terminal。
10. `codex-utils-pty` 作为 local shell backend 内部 process/PTY helper 继续使用；Codex process handle 不跨出 local shell/session-control 边界。
11. 若实现阶段仍没有直接 `portable_pty::` 调用，应移除 `agentdash-local` 的裸 `portable-pty` 依赖，保持 PTY 选型只有一条封装路径。
12. 不新增或依赖 `/sessions/*` 作为本功能控制面；历史诊断 endpoint 不作为实现路径。
13. 本子任务只修当前阻断和预留 activity/ref 形状，不承载完整 common wait module。

## Acceptance Criteria

- [ ] Agent tool catalog 中 exec 能力仍表现为单一 `shell_exec` 工具，不新增 read/write/terminate/status 顶层平铺工具。
- [ ] 长命令验收：`shell_exec operation=start` 返回 running `terminal_id`；`operation=read` 能拿到增量输出；完成后 `operation=status/read` 能拿到 exit code。
- [ ] 交互命令验收：启动可写 stdin 的进程，`operation=write` 后 `operation=read` 能得到后续输出。
- [ ] stdin close 验收：`operation=write` 可携带 close stdin，进程可收到 EOF 并退出。
- [ ] PTY resize 验收：PTY-backed terminal 可 resize；非 PTY 返回 typed unsupported/status。
- [ ] terminate 验收：`operation=terminate` 可终止 running 进程，重复 terminate 或已结束进程返回明确状态。
- [ ] `shell_exec` result details 中包含唯一 public `terminal_id`、cursor/next_seq 和 truncation summary；底层本机 shell session ref 不作为 Agent continuation contract。
- [ ] Windows ContextFrame 测试覆盖 PowerShell shell kind、对象输出、`Write-Output`、PowerShell 语法提示和非 Windows 不出现该提示。
- [ ] 前端 terminal projection 测试覆盖 output/state event 幂等投影和打开对应 terminal。
- [ ] 依赖清理验收：`agentdash-local` 不保留未使用的裸 `portable-pty` 依赖；`codex-utils-pty` 仍只作为 local backend 内部实现细节。
- [ ] 代码搜索确认本子任务未新增 `/sessions/*` 控制 endpoint，也未把 RuntimeSession 当作 AgentRun command owner。

## Evidence

- VFS 当前只暴露 `shell_exec`：`crates/agentdash-application-vfs/src/tools/factory.rs:116`。
- Running result 现有 `session_id` / `terminal_id` / `next_seq`，规划收束为单一 public `terminal_id`：`crates/agentdash-application-vfs/src/tools/fs/shell.rs:473`。
- Relay/local 已有 read/input/terminate primitives：`crates/agentdash-relay/src/protocol/tool.rs:83`、`crates/agentdash-local/src/shell_session_manager.rs:327`。
- Windows shell wrapper 使用 PowerShell：`crates/agentdash-local/src/shell_session_manager.rs:798`。
- Environment ContextFrame 是 Windows guidance 的正式渠道：`.trellis/spec/backend/session/execution-context-frames.md`。
- Codex crate 复用和冗余 PTY 依赖清理结论：`.trellis/tasks/07-03-waitable-activity-exec-closure/research/codex-crate-reuse.md`。
