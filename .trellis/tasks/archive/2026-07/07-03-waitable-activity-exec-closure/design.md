# Technical Design

## Decision Summary

AgentDashboard 已经有底层 shell session、terminal projection、LifecycleGate、mailbox scheduler 和 frontend waiting projection。当前断点不是缺少所有底层能力，而是缺少一个 AgentDashboard-native 的终端活动根模型和 wait module，把这些能力通过 runtime tool catalog、wait service、mailbox wake 和 frontend projection 串成闭环。

设计结论：

- `RuntimeSession` 只作为 delivery/trace ref，不作为 wait/exec 产品控制面 owner。
- exec/terminal 只保留一个 Agent-facing continuation ref：`terminal_id`。
- 内部也以 `TerminalActivity` / terminal record 为根，backend ref、本机 shell session ref、runtime trace ref 都是这个 record 的私有字段或派生引用。
- exec 工具不拆平铺工具。`shell_exec` 保持单一工具面，通过 `operation=start|read|write|terminate|resize|status` 建模完整终端交互。
- wait module 对 exec 使用同一个 `terminal_id` 作为 activity/source ref，不再额外制造第二套 exec-specific handle。
- mailbox 继续作为 delivery authority；wait module 只登记/观察 activity，并在需要恢复 AgentRun 时写入 mailbox wake envelope。
- frontend 只投影 terminal/waiting/mailbox 状态；Agent 读取和等待必须走 runtime tools。
- `references/codex` 只提供能力参考，不提供可复制的 identity 或 lifecycle。

## Trellis Workflow

当前处于 Phase 1 planning。

执行顺序：

1. 父任务完成 PRD convergence、设计和实施计划。
2. 父任务创建两个子任务，并把实现范围拆开。
3. 用户 review 父任务与两个子任务的计划。
4. 只在用户确认后，对下一个实现子任务运行 `task.py start`。
5. 子任务按 Phase 2 实现、Phase 2.2 检查、Phase 3 spec update、Phase 3 commit。
6. 两个子任务都完成后，在父任务做集成检查并创建一份 PR。

## Architecture

### Parent Task Boundary

父任务不直接承载代码实现。它拥有：

- 总体目标和不可变约束；
- Codex reference 与当前项目链路评估；
- 子任务拆分；
- 跨子任务验收；
- 最终 PR 集成检查。

### Child 1: Exec And Terminal Blocker Repair

目标是让当前 exec 链路可用，同时保持工具面深而少：

- `shell_exec operation=start` 是 start + initial bounded read/yield。
- running result 返回 canonical `terminal_id`。
- `TerminalActivity` / terminal record 以 `terminal_id` 为主键，内部保存 owner、backend/local shell refs、state、exit_code、next_seq、truncation 和 terminal projection metadata。
- `shell_exec operation=read` 使用 `terminal_id`、`after_seq`、`wait_ms`、`max_bytes` 读取 bounded output。
- `shell_exec operation=write` 写 stdin，并返回随后的 bounded read。
- `shell_exec operation=write` 支持 `close_stdin`，对齐 Codex protocol 的 write-and-close 语义。
- `shell_exec operation=terminate` 幂等终止 running terminal。
- `shell_exec operation=resize` 调整 PTY terminal size。
- `shell_exec operation=status` 查询单 terminal state；未传 `terminal_id` 时可列出当前 owner scope 下的 background terminals。
- Windows Environment ContextFrame 明确 PowerShell shell contract。
- 前端修复 terminal output/state projection 和打开终端时的定位信息。

这里可以先不引入完整 durable wait table。对 exec 来说，后续 wait module 的 activity/source ref 应直接使用 `terminal_id`。

### Child 2: Waitable Activity Module

目标是统一等待所有并行来源，同时减少内部索引分叉：

```text
tool operation starts async source
  -> source root record exists
     - exec: TerminalActivity(terminal_id)
     - companion/human/subagent: LifecycleGate(gate_id)
     - mailbox: AgentRunMailboxMessage(message_id)
  -> WaitService observes source root
  -> WaitService.notify(...)
  -> wait tool returns summary/ref OR mailbox wake is written
  -> scheduler delivers mailbox envelope when AgentRun needs continuation
```

建议模型：

```text
WaitActivityView
  activity_ref: source natural id where possible
  owner_ref: run_id + agent_id + frame_id
  kind: exec | companion | subagent | human | mailbox | workflow
  source_ref: terminal_id | gate_id | mailbox_message_id | runtime_node_ref
  correlation_ref
  status: pending | running | completed | failed | cancelled | timed_out | lost
  preview
  result_refs
  cursor
  created_at / updated_at / resolved_at
```

`WaitActivityView` 可以是 source root records 的统一投影，不要求所有来源都复制一行新的 durable record。只有缺少稳定 source root 的未来来源才需要 wait module 自己 mint activity id。

`WaitService` 负责：

- observe/register source roots；
- update terminal/result state；
- wait for one or many activities with timeout；
- project bounded summaries；
- bridge source adapters；
- emit notifications to in-process waiters；
- create mailbox wake envelope when continuation is required。

Source adapters：

- Exec adapter reads status from `TerminalActivity(terminal_id)` and relay/local session control.
- LifecycleGate adapter wraps companion/subagent/human gates and replaces private polling.
- Mailbox adapter observes pending/completed mailbox wake messages and `MailboxStateChanged`.
- Future workflow adapter can observe runtime node blocked/completed state.

### Agent Tool Surface

最小 Agent-facing tools：

- `shell_exec`: a single instruction-style terminal tool with `operation=start|read|write|terminate|resize|status`.
- `wait`: wait for activity refs or kinds in the current AgentRun scope.

`shell_exec operation=start`:

```json
{
  "operation": "start",
  "cwd": "main://",
  "command": "pnpm test",
  "yield_time_ms": 10000,
  "max_output_bytes": 6000,
  "tty": false
}
```

`shell_exec` continuation operations use the same public `terminal_id`:

```json
{
  "operation": "read",
  "terminal_id": "term_...",
  "after_seq": 12,
  "wait_ms": 1000,
  "max_output_bytes": 6000
}
```

`operation=write` accepts `data` and optional `close_stdin`; `operation=resize` accepts terminal columns/rows for PTY-backed terminals.

`wait` returns:

```json
{
  "status": "ready | timed_out | cancelled | not_found",
  "timed_out": false,
  "items": [
    {
      "activity_ref": "term_...",
      "kind": "exec",
      "status": "running | completed | failed",
      "source_ref": "term_...",
      "correlation_ref": "...",
      "preview": "...",
      "result_refs": {},
      "next": {
        "tool": "shell_exec",
        "operation": "read",
        "terminal_id": "term_...",
        "cursor": "..."
      }
    }
  ]
}
```

`wait` 不返回完整 stdout/stderr 或 companion 正文。

## Data Flow

### Exec Running

```text
Agent calls shell_exec operation=start
  -> VfsRuntimeToolProvider exposes ShellExecTool
  -> VfsService.exec_with_policy
  -> RelayFS CommandToolShellExec
  -> local ShellSessionManager.start_shell
  -> result running: terminal_id + next_seq
  -> TerminalActivity records owner/backend/local shell refs under terminal_id
  -> wait projection can expose kind=exec using the same terminal_id
  -> tool result returns terminal_id + preview
```

Continuation:

```text
Agent calls shell_exec operation=read|write|terminate|status
  -> resolve TerminalActivity by terminal_id
  -> validate owner/current AgentRun permission
  -> call relay/local ToolShellRead/Input/Terminate
  -> update terminal state/cursor/exit_code
  -> return bounded chunks/status
```

### Companion / Human

```text
companion dispatch or human request
  -> LifecycleGateResolver opens gate
  -> WaitService observes gate_id as activity_ref/source_ref
  -> wait=true uses WaitService.wait(...)
  -> wait=false returns gate-backed activity_ref
  -> companion_respond / user response resolves gate
  -> LifecycleGateAdapter updates wait projection
  -> MailboxWakeAdapter writes deduped result envelope when parent AgentRun should continue
```

### Mailbox Wake

```text
source result ready
  -> MailboxWakeAdapter creates AgentRunMailboxMessage with stable source identity
  -> AgentRunMailboxService.schedule(...)
  -> scheduler claim/delivery remains authority
  -> Backbone MailboxStateChanged informs frontend projection
  -> wait tool can observe activity/mailbox readiness
```

## Frontend Boundary

Frontend keeps two projection responsibilities:

- terminal output/state from Backbone `terminal_output` and `terminal_state_changed`;
- mailbox/waiting activity rows from AgentRun workspace snapshot.

Needed additions:

- waiting item can represent exec activity with `terminal_id` as source/activity ref;
- terminal jump uses AgentRun workspace refs and the canonical terminal id;
- UI action affordances can open terminal or show status, but cannot become the Agent wait/read authority.

## PowerShell Handling

PowerShell guidance belongs in Environment ContextFrame because it influences Agent command construction before tools are called.

Windows rendered text should state:

- real OS shell is PowerShell;
- use PowerShell command separators and conditionals, not bash-only `&&` / `|| true`;
- some PowerShell commands return objects;
- for stable text output use explicit string fields, `Write-Output`, `ForEach-Object`, `Out-String` when appropriate, or dedicated VFS tools;
- examples: `Write-Output (Get-Location).Path` and `Get-ChildItem | ForEach-Object { Write-Output $_.FullName }`.

This is not a frontend hint and not an OS shell mutation.

## Reference To Codex

Codex operation semantic set has three layers:

- Model tool layer:
  - `exec_command`: start command, wait/yield for initial output, return `session_id` only if still running, return `exit_code` only when completed.
  - `write_stdin`: continue an existing process; non-empty `chars` writes stdin, empty `chars` is poll/read/wait; completion can produce the original exec final result.
- Manager layer:
  - process store / terminal entry;
  - start / write-or-poll / status alive-exited-unknown / list background processes / terminate one / terminate all;
  - bounded output, yield timeout and execution timeout are separate.
- Protocol layer:
  - `command/exec`, `command/exec/write`, `command/exec/terminate`, `command/exec/resize`, `command/exec/outputDelta`;
  - raw `process/spawn`, `process/writeStdin`, `process/kill`, `process/resizePty`, `process/outputDelta`, `process/exited`;
  - streamed output is notification/delta, final response carries terminal exit state and does not duplicate streamed bytes.

Borrow:

- running command returns one continuation handle and no exit code;
- completed command returns exit code and no live handle;
- yield/wait timeout differs from execution timeout;
- terminal interaction is modeled as continuing an existing command/terminal, not as many unrelated tools;
- stdin close, PTY resize, output delta and exited notification are first-class protocol semantics;
- wait returns small status/summary/ref;
- mailbox/steer/activity events wake waiters.

Do not borrow:

- Codex Thread/AgentPath/session identity;
- connection-scoped app-server process semantics;
- external `/sessions/*` control surface;
- Codex runtime dependency.

## Crate Reuse

Current worktree already uses selected Codex crates. The implementation should treat this as a constrained reuse decision, not as permission to import Codex runtime wholesale.

Keep direct use:

- `codex-utils-pty` inside `agentdash-local` for pipe/PTY spawn, stdin write, stdin close, resize, terminate and exit observation.
- `codex-utils-output-truncation` narrowly inside retained shell output buffering while AgentDashboard relay DTOs remain the public truncation contract.

Keep existing project usage but do not expand for terminal control:

- `codex-app-server-protocol` is already used for Backbone/thread DTOs. Its command/process DTOs are reference material only for this exec/wait design because their process handles are Codex connection-scoped, not AgentRun-owned.

Reference only:

- `codex-exec-server-protocol`: useful `process/start|read|write|signal|terminate|output|exited|closed` vocabulary, but too tied to Codex sandbox/path/network/process id semantics for direct adoption.
- `codex-core`: useful unified exec manager behavior, not a dependency target.
- `codex-agent-graph-store`, `codex-code-mode-protocol`, `codex-terminal-detection`: no direct use for this task.

Cleanup note:

- `agentdash-local` currently declares direct `portable-pty = "0.9"` without direct `portable_pty::` usage. If `codex-utils-pty` remains the wrapper, the direct dependency can be removed during implementation cleanup.

Full evaluation: `research/codex-crate-reuse.md`.

## Risks

- Durable exec recovery after local backend restart needs explicit `lost` semantics; first implementation can mark in-memory lost terminals as terminal/lost rather than pretending they are recoverable.
- `shell_exec` schema must stay understandable despite multiple operations.
- Companion wait migration must preserve existing gate resolution and mailbox dedup semantics.
- Frontend generated contracts need synchronized Rust/TS updates if waiting item shape changes.
- Trellis channel Codex provider worker currently fails locally; use `multi_agent_v1` for implementation/check dispatch until that tool path is repaired separately.
