# Shell 工具扩展与 Codex 行为对齐

## Goal

将 AgentDashboard 的 shell 工具从“一次请求等待命令结束”的同步模型，扩展为与 Codex terminal / unified exec 行为对齐的可等待、可输入、可轮询、可保留输出的执行模型。长命令应在初始等待窗口后继续后台运行，并通过稳定的进程/终端句柄继续交互，避免 relay pending request 超时后引发后续工具调用级联失败。

本任务同时覆盖项目 Codex Rust 依赖升级：当前工作区只直接依赖 `codex-app-server-protocol`，版本为 `rust-v0.133.0`；远端稳定 tag 已确认可升级目标为 `rust-v0.140.0`。

## Requirements

- `shell_exec` 的用户可见行为与 Codex `exec_command` 对齐：一次工具调用只等待一个有限的 `yield_time_ms` 初始窗口；命令仍在运行时返回稳定句柄、部分输出和运行状态。
- 后续工具调用可按句柄继续等待输出、轮询状态、写入 stdin、终止进程；空输入用于等待/轮询，非空输入用于真实交互。
- 长命令生命周期由本机 runtime 的进程/终端管理器持有，relay pending request 只覆盖单次 RPC 响应，不再承担进程生命周期。
- 输出流保留实时增量事件、可轮询快照和最终状态事件三条路径；进程退出后的尾部输出可被可靠读取。
- 输出缓存具备上限，保留稳定头部和尾部，向模型/前端报告截断事实，避免长输出无限累积。
- 现有交互式 terminal 和 shell 工具共享统一的本机 process/session 抽象，前端 terminal 面板可以查看由 shell 工具启动的后台进程。
- Codex 依赖升级到最新稳定 Rust tag，并评估可直接引用的公开 crate；对 Codex `core` 内部私有模块只移植设计和必要代码，不把内部路径当成稳定依赖。
- 不引入兼容性分支；协议、DTO、前端 generated types 和测试按当前最正确形态一次性收束。

## Acceptance Criteria

- [ ] `Cargo.toml` 中 Codex git 依赖升级到最新稳定 tag，`Cargo.lock` 与编译结果一致。
- [ ] relay 协议新增或调整后的 shell 执行响应能表达 `running`、`completed`、`failed`、`timed_out` / `killed` 等状态，以及 `terminal_id` 或 `process_id`、`next_seq`、截断信息和输出片段。
- [ ] 长命令超过默认初始等待窗口后不会返回 relay pending timeout；后续可通过 wait/read 工具继续获得输出和最终 exit code。
- [ ] `shell_wait` / `shell_input` / `shell_terminate` 或等价能力可供模型和服务端调用，空输入 poll 与真实 stdin 写入语义清晰。
- [ ] 本机 runtime 对后台 shell session 有进程上限、退出后短期保留、后端断连状态标记和清理策略。
- [ ] 前端 command execution card 能继续展示流式输出，并可打开 terminal 面板查看同一后台会话。
- [ ] 针对长命令、尾部输出、stdin、超大输出截断、进程退出后 read、relay 断连/丢失状态有聚焦测试覆盖。
- [ ] `cargo check` 覆盖受影响 Rust crate；前端协议类型生成和受影响 UI 测试通过。

## Confirmed Facts

- 当前 `agentdash-local` 的 `ProcessExecutor::shell_exec_streaming` 以 pipe 方式执行命令，按 `timeout_ms` 等到 stdout/stderr 读取结束后返回最终 `ProcessOutput`；超时会 kill 子进程并返回 `ToolError::Timeout`。
- 当前 `agentdash-local` 的 `TerminalManager` 以 PTY 管理交互式终端，已有 spawn/input/resize/kill、输出事件和状态事件，但没有 retained output read API，也没有与 `shell_exec` 共享抽象。
- 当前 `agentdash-api` 的 `BackendRegistry::pending` 是 msg_id 到 oneshot 的单次请求表；已提交的 stopgap 只是让 shell_exec pending timeout 覆盖 process timeout 外加 grace。
- 当前前端 `useTerminalStore` 以 `terminal_id` 累积输出字符串，command card 可将命令输出 promote 到 terminal tab，但 promote 后不是同一个真实进程。
- Codex `core::unified_exec` 的 process manager、head-tail buffer、yield/wait 逻辑在 `codex-core` 内部是 `pub(crate)`，不能作为公开稳定模块直接依赖。
- Codex 提供公开 crate `codex-exec-server`、`codex-utils-pty`、`codex-utils-output-truncation` 等；其中 `codex-exec-server` 暴露 `ExecParams`、`ReadParams`、`WriteParams`、`TerminateParams` 和 process read/write/terminate 协议类型。
- 用户已确认复用策略：Codex 的沙盒防护、approval、session orchestration 等重实现不纳入本项目目标；优先复用基础 process/PTY/输出截断等低耦合细节。

## Out of Scope

- 本任务不重做非 shell 文件工具、MCP relay、Agent provider 选择和权限治理模型。
- 本任务不保留旧 shell 协议兼容分支；相关 DTO、generated types 和调用方按新协议同步更新。
- Codex 沙盒防护、approval policy、网络权限审批和 Codex session orchestration 不作为本项目 shell 工具扩展的一部分。

## Open Questions

- 当前无阻塞性产品问题；下一步需要 review 规划并决定是否启动实现。
