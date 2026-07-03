# 修复终端输出展示与跳转链路

## Goal

修复现有终端从本机执行、relay、Backbone、前端 store 到 workspace terminal tab 的展示与跳转闭环。用户看到的症状不是简单“没输出”，而是终端输出链路在历史回放、tab 跳转、命令输出 promotion 和 PowerShell 对象命令验收上存在断点。

本任务只修现有终端可见性与可靠性问题，不实现 Agent 并行等待/mailbox 能力。并行等待能力由 `.trellis/tasks/07-02-agent-parallel-wait-mailbox-implementation` 单独承接。

## Confirmed Facts

- 后端终端执行层走真实进程 PTY/stdout/stderr 字节流。交互式 terminal 使用 `spawn_pty_process`，输出从 bytes 解码为文本后进入 relay `event.terminal.output` 和 Backbone `PlatformEvent::TerminalOutput`。
- 未发现本项目终端执行层把 PowerShell 输出作为对象、JSON 或 `serde_json::Value` 序列化。PowerShell 对象命令应由 PowerShell host 自身格式化成控制台文本，再由 PTY/stdout 字节流采集。
- Codex 参考实现同样以真实 stdout/stderr/PTY 字节流为输出边界，并在 PowerShell command preparation 阶段加 UTF-8 console output 前缀；它不依赖 PowerShell 对象 JSON 序列化。
- 前端 live terminal event 会被 `dispatchSessionPlatformEvent` 拦截写入 `useTerminalStore`，避免 reducer 双写。
- 前端 history hydrate 只跑 `reduceStreamState`，而 reducer 明确过滤 terminal platform event；因此刷新页面、打开历史会话或先产生输出再打开 terminal tab 时，terminal store 可能没有历史输出。
- 命令执行卡片当前把输出复制到 `promote-*` synthetic terminal，并打开 terminal tab。该 tab 看起来像真实交互终端，但没有真实后端进程，输入/resize 语义会误导用户。
- 前端 terminal tab 新建和后端 terminal route 仍存在旧 Session 形态入口。实现方案不得新增、依赖或强化这类外露入口；若必须触及 spawn/list contract，应迁移到 AgentRun/workspace runtime surface 所属的命令面。
- `ContextFrame(kind="environment")` 已在 connector startup context 中作为 system/session policy 投递，适合承载 Windows-only shell 操作提示。该提示服务 Agent 操作策略，不替代终端输出链路修复。

## Requirements

1. 终端 live output、history output、state changed 必须进入同一 terminal store 投影，且 terminal output 继续不进入聊天 feed。
2. 打开或跳转到既有 terminal tab 时，已持久化到 session event backlog 的 terminal output 必须可回放到 xterm；state changed 不能因为 terminal 尚未前端注册而丢失。
3. 命令执行卡片不得把静态输出伪装成真实可交互终端。静态输出 replay 应以只读能力表达，真实 terminal 输入/resize 只对后端存在的 interactive terminal 启用。
4. workspace panel 跳转必须走页面/WorkspacePanel 的 open-and-expand 能力，而不是只直接写全局 tab store，保证用户点击后能看到目标 tab。
5. terminal input、resize、kill 的 API handler 必须校验 relay response 类型和错误字段；本机返回失败时不能以 HTTP 204 表示成功。
6. PowerShell 对象输出必须被作为终端验收项验证：`pwd` / `Get-Location`、`dir` / `Get-ChildItem`、`Write-Output (Get-Location).Path` 都应在 terminal tab 中产生可见文本。
7. 不新增对外旧 Session 形态终端入口；前端新代码不得直接拼装旧 Session 形态 terminal path。
8. 不把 PowerShell 修复做成前端字符串拼接或对象 JSON 转换。若验收失败，应在执行准备或 PTY/pipe 字节流边界修复。
9. Windows 环境下，Environment ContextFrame 应提示 Agent：PowerShell 部分命令返回对象，若通过非交互工具或脚本需要稳定文本输出，应显式选择字符串字段、`Write-Output` 文本或专用文件工具；交互终端仍以真实 PTY/stdout 字节流为准。

## Acceptance Criteria

- [ ] history hydrate 后，`PlatformEvent::TerminalOutput` 顺序写入 `useTerminalStore`，对应 terminal tab 可以回放历史输出。
- [ ] live terminal event 仍只写一次 terminal store，不进入聊天 display entries，也不因 StrictMode/reducer 重放产生重复输出。
- [ ] `terminal_state_changed` 能为未预先注册的 terminal 建立可显示状态，或通过等价 projection 保证 terminal tab 状态不丢。
- [ ] 命令输出 promotion 打开的 tab 是只读 replay 或明确的输出查看器，不向不存在的后端 terminal 发送 input/resize。
- [ ] 点击命令卡片查看输出/终端会展开 workspace panel 并激活目标 tab。
- [ ] terminal input、resize、kill 在 relay/local 返回错误时向前端返回稳定错误，不再静默 204。
- [ ] Windows PowerShell terminal 验收覆盖对象输出命令，证明输出来自真实 PTY/stdout 字节流并在前端可见。
- [ ] Windows 环境下 Environment ContextFrame 包含 PowerShell 文本输出提示；非 Windows 环境不出现该提示。
- [ ] 代码搜索确认本任务没有新增旧 Session 形态终端入口或新的前端直接拼路径调用。

## Out Of Scope

- Agent 并行等待、subagent wait、exec wait、mailbox result wake-up 的能力补齐。
- 全项目旧 Session API 总清理。若终端修复必须触及终端 spawn/list，会在本任务内收束终端入口；其他资源入口不在本任务中扩大处理。
- 独立 terminal scrollback 存储模型。当前最小修复以 session event backlog + terminal store hydrate 闭合展示。

## Research

- `.trellis/tasks/07-02-terminal-output-navigation-repair/research/backend-terminal-chain-and-powershell.md`
- `.trellis/tasks/07-02-terminal-output-navigation-repair/research/frontend-terminal-display-navigation.md`
