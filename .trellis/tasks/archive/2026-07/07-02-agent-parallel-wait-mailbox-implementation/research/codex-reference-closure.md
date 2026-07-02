# Research: codex-reference-closure

- Query: references/codex 中终端 exec、PowerShell 输出处理、spawn subagents、wait、mailbox/notification 闭环的参考实现。
- Scope: mixed
- Date: 2026-07-03

## Findings

### 结论

参考 Codex 闭合能力集，在 AgentDashboard 自有体系实现时，最有价值的不是复用 Codex 的协议名或对象形状，而是复用两个闭环能力模型：

1. 终端执行闭环：稳定的逻辑 `process_id` 串起启动、stdin、PTY、输出 delta、退出、关闭与最终结果；真实进程输出以 stdout/stderr/pty 字节流进入系统，UI/转录层再做文本投影。
2. 协作等待闭环：spawn/message/close 只负责改变 agent 或 mailbox 状态；wait 只等待 mailbox/activity/terminal 状态变化；结果通过 mailbox 或 notification 回到等待方，而不是把 wait 当成大结果传输通道。

PowerShell 输出方面，Codex 的参考实现是“真实 shell 进程输出字节流 + PowerShell 输出编码前缀”。它通过在 PowerShell 命令前追加 `[Console]::OutputEncoding=[System.Text.Encoding]::UTF8`，促使 PowerShell 控制台输出 UTF-8 字节；后续由 stdout/stderr/pty 字节流采集、UTF-8 边界切分和 lossy 文本投影处理。未发现 Codex 依赖 PowerShell 对象 JSON 序列化来传输终端输出。需要注意的是，PowerShell 对象进入控制台流之前仍由 PowerShell 自身格式化管线字符串化；Codex 处理的是格式化后的进程字节流，而不是 PowerShell 对象图。

AgentDashboard 已经有比 Codex in-memory input queue 更强的 AgentRun mailbox 事实源、receipt、claim lease、barrier、recovery 与 Backbone notification。建议把 Codex 的闭环语义映射为 AgentDashboard 自有能力：终端执行用 RuntimeSession/Backbone 事件承载，subagent/message/result 用 AgentRunMailbox envelope 和 `MailboxStateChanged` 通知承载。

### Files found

- `references/codex/codex-rs/exec-server-protocol/src/protocol.rs` - exec-server 协议定义，包含 `process_id`、PTY、stdin、stdout/stderr/pty delta、exited/closed 终态。
- `references/codex/codex-rs/exec-server-protocol/src/process_id.rs` - `ProcessId` 是透明字符串包装，代表连接/会话范围内的逻辑进程句柄。
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/command_exec.rs` - app-server `command/exec` 协议定义，说明 `process_id`、streaming、PTY、write、terminate 和最终响应。
- `references/codex/codex-rs/app-server-protocol/src/protocol/common.rs` - `command/exec/outputDelta` notification 定义，输出 delta 以 base64 编码字节块发送。
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/item.rs` - terminal interaction、command execution delta、collab tool status、subagent activity item 定义。
- `references/codex/codex-rs/app-server-protocol/src/protocol/event_mapping.rs` - core 事件到 app-server item/notification 的映射，包括 exec begin/delta/end、terminal interaction、CollabWaitingBegin/End、SubAgentActivity。
- `references/codex/codex-rs/core/src/tools/handlers/unified_exec.rs` - unified exec 工具参数，包括 shell、PTY、yield、token、sandbox。
- `references/codex/codex-rs/core/src/tools/handlers/unified_exec/exec_command.rs` - unified `exec_command` handler，分配 `process_id` 并交给 unified exec manager。
- `references/codex/codex-rs/core/src/tools/handlers/unified_exec/write_stdin.rs` - `write_stdin` handler，将模型侧 `session_id` 映射回进程句柄并发出 terminal interaction。
- `references/codex/codex-rs/core/src/tools/runtimes/unified_exec.rs` - unified exec runtime，执行前处理 sandbox、shell 与 PowerShell UTF-8 前缀。
- `references/codex/codex-rs/core/src/unified_exec/process_manager.rs` - unified exec process manager，构造 exec-server 参数、环境、逻辑 process id 与进程句柄。
- `references/codex/codex-rs/core/src/unified_exec/process.rs` - unified exec 进程状态和输出 snapshot，保留字节并在文本投影时 lossy decode。
- `references/codex/codex-rs/core/src/unified_exec/async_watcher.rs` - 异步输出 watcher，按 UTF-8 边界切分字节并发输出 delta。
- `references/codex/codex-rs/core/src/exec.rs` - legacy/raw exec 输出最终聚合，stdout/stderr/aggregated output 从字节 lossy decode。
- `references/codex/codex-rs/shell-command/src/powershell.rs` - PowerShell UTF-8 输出前缀与测试。
- `references/codex/codex-rs/core/src/shell.rs` - shell 参数派生，PowerShell 使用 `-Command` 执行脚本。
- `references/codex/codex-rs/core/src/tools/handlers/multi_agents_v2/spawn.rs` - v2 spawn agent，创建 communication context 并发出 `SubAgentActivity::Started`。
- `references/codex/codex-rs/core/src/tools/handlers/multi_agents/send_input.rs` - v1 send input，发出 interaction begin/end 并调用 agent control。
- `references/codex/codex-rs/core/src/tools/handlers/multi_agents_v2/message_tool.rs` - v2 send/followup message 共用逻辑，向目标 agent 发送 inter-agent communication 并发出 interacted activity。
- `references/codex/codex-rs/core/src/tools/handlers/multi_agents_v2/wait.rs` - v2 wait，等待 mailbox activity、steer 或 timeout，并发出 CollabWaitingBegin/End。
- `references/codex/codex-rs/core/src/tools/handlers/multi_agents/wait.rs` - v1 wait，等待指定 agent 进入 final status，并可返回状态摘要。
- `references/codex/codex-rs/core/src/tools/handlers/multi_agents/close_agent.rs` - close agent，发出 close begin/end 并返回 previous status。
- `references/codex/codex-rs/core/src/agent/control.rs` - root-scoped agent control，负责 spawn/send/input/communication/interrupt/close 和 completion watcher。
- `references/codex/codex-rs/core/src/agent/control/spawn.rs` - spawn agent 内部流程，包含 v2 initial inter-agent communication。
- `references/codex/codex-rs/core/src/session/input_queue.rs` - Codex mailbox/steer input queue，负责 pending mailbox、activity watch、drain 与 trigger turn。
- `references/codex/codex-rs/core/src/session/handlers.rs` - session 接收 inter-agent communication，入队 mailbox 并按 `trigger_turn` 启动 pending work。
- `references/codex/codex-rs/core/src/session/inject.rs` - idle extension work 与 trigger-turn mailbox 的协调。
- `references/codex/codex-rs/core/src/session/turn.rs` - turn 开始前 drain pending input，并可被 mailbox mail preempt。
- `references/codex/codex-rs/core/src/tasks/lifecycle.rs` - idle lifecycle 避免在 trigger-turn mailbox pending 时发 idle。
- `references/codex/codex-rs/core/src/context/subagent_notification.rs` - legacy subagent notification 上下文片段格式。
- `references/codex/codex-rs/core/src/session_prefix.rs` - subagent completion notification 文本格式。
- `.trellis/workflow.md` - 本项目 Trellis workflow 与 task/research 工作约束。
- `.trellis/spec/backend/session/agentrun-mailbox.md` - AgentRun mailbox 作为事实源、调度队列与恢复投影的规范。
- `.trellis/spec/backend/session/runtime-execution-state.md` - RuntimeSession 执行态与 AgentRun mailbox 调度关系规范。
- `.trellis/spec/backend/session/streaming-protocol.md` - session streaming/backbone 协议相关规范。
- `.trellis/spec/backend/session/architecture.md` - session subsystem 架构边界，包含 Codex-compatible turn 与 AgentRun envelope 扩展。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` - 前后端 DTO 与 composer-submit/mailbox contract。
- `crates/agentdash-domain/src/agent_run_mailbox/mod.rs` - AgentDashboard mailbox domain 模型、source identity、delivery、barrier 与 repository contract。
- `crates/agentdash-contracts/src/agent/run_mailbox.rs` - AgentRun mailbox DTO、composer submit request、command response 与 outcome。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/mod.rs` - mailbox application service 边界与导出。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs` - mailbox scheduler、barrier drain、claim/consume 与 delegate drain。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs` - Runtime turn boundary delegate 与 `MailboxStateChanged` notification 发出点。
- `crates/agentdash-api/src/agent_run_mailbox.rs` - terminal callback 对 AgentRunTurnBoundary 调度和失败/中断暂停的适配。

### Codex 参考证据

#### Unified exec / command exec

- `ExecParams.process_id` 是“Client-chosen logical process handle scoped to this connection/session”，不是 OS pid；同一协议中还带有 `argv`、`cwd`、`env`、`tty`、`pipe_stdin` 字段，说明 process id 是客户端可控的逻辑句柄，PTY/stdin 是执行参数的一部分。证据：`references/codex/codex-rs/exec-server-protocol/src/protocol.rs:118`、`references/codex/codex-rs/exec-server-protocol/src/protocol.rs:122`、`references/codex/codex-rs/exec-server-protocol/src/protocol.rs:127`、`references/codex/codex-rs/exec-server-protocol/src/protocol.rs:130`。
- `ProcessId` 在 exec-server protocol 中是 `serde(transparent)` 的字符串包装，只有 `new/as_str/into_inner` 等逻辑句柄行为。证据：`references/codex/codex-rs/exec-server-protocol/src/process_id.rs:8`、`references/codex/codex-rs/exec-server-protocol/src/process_id.rs:12`。
- exec-server read/write 使用同一 `process_id`。read 支持 `after_seq`、`max_bytes`、`wait_ms`；write 支持 `chunk` 和可选 `write_id`。证据：`references/codex/codex-rs/exec-server-protocol/src/protocol.rs:165`、`references/codex/codex-rs/exec-server-protocol/src/protocol.rs:196`。
- stdout/stderr/pty 是协议级 stream 枚举；输出 delta 携带 `process_id`、`seq`、`stream`、`chunk`；终态分为 exited 和 closed，其中 exited 带 `exit_code` 与 `sandbox_denied`。证据：`references/codex/codex-rs/exec-server-protocol/src/protocol.rs:510`、`references/codex/codex-rs/exec-server-protocol/src/protocol.rs:515`、`references/codex/codex-rs/exec-server-protocol/src/protocol.rs:524`、`references/codex/codex-rs/exec-server-protocol/src/protocol.rs:534`。
- app-server `command/exec` 明确要求 final response 延迟到进程退出且所有 `outputDelta` notification 已发出；可由客户端提供 connection-scoped `processId`，且 streaming stdin/stdout/stderr、PTY、follow-up write/resize/terminate 都依赖该 id。证据：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/command_exec.rs:21`、`references/codex/codex-rs/app-server-protocol/src/protocol/v2/command_exec.rs:30`、`references/codex/codex-rs/app-server-protocol/src/protocol/v2/command_exec.rs:35`。
- app-server `tty` 会隐含 `streamStdin` 和 `streamStdoutStderr`；被 stream 的 stdout/stderr 不会再复制到最终 response 中。证据：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/command_exec.rs:41`、`references/codex/codex-rs/app-server-protocol/src/protocol/v2/command_exec.rs:47`、`references/codex/codex-rs/app-server-protocol/src/protocol/v2/command_exec.rs:115`。
- `command/exec/write` 使用 `process_id` 写 stdin 字节或关闭 stdin，`command/exec/terminate` 使用同一 `process_id` 终止进程。证据：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/command_exec.rs:128`、`references/codex/codex-rs/app-server-protocol/src/protocol/v2/command_exec.rs:151`。
- app-server notification `command/exec/outputDelta` 以 base64 编码 stdout/stderr 字节块；tool item 侧 `item/commandExecution/outputDelta` 是 item 维度的命令输出 delta。证据：`references/codex/codex-rs/app-server-protocol/src/protocol/common.rs:1638`、`references/codex/codex-rs/app-server-protocol/src/protocol/common.rs:1643`。
- item mapping 中，`ExecCommandOutputDelta` 将 `chunk` 字节以 `String::from_utf8_lossy` 投影为文本 delta，再发 `CommandExecutionOutputDeltaNotification`；terminal stdin interaction 会带 `process_id` 与 stdin。证据：`references/codex/codex-rs/app-server-protocol/src/protocol/event_mapping.rs:433`、`references/codex/codex-rs/app-server-protocol/src/protocol/event_mapping.rs:445`。
- unified `exec_command` handler 通过 `session.services.unified_exec_manager` 工作，并在执行前 `allocate_process_id()`。证据：`references/codex/codex-rs/core/src/tools/handlers/unified_exec/exec_command.rs:129`、`references/codex/codex-rs/core/src/tools/handlers/unified_exec/exec_command.rs:231`。
- `write_stdin` 对外参数名仍是模型熟悉的 `session_id: i32`，但内部把它作为 `process_id` 传给 unified exec manager；非空 stdin 会发 `TerminalInteraction` 事件。证据：`references/codex/codex-rs/core/src/tools/handlers/unified_exec/write_stdin.rs:20`、`references/codex/codex-rs/core/src/tools/handlers/unified_exec/write_stdin.rs:69`、`references/codex/codex-rs/core/src/tools/handlers/unified_exec/write_stdin.rs:85`。
- unified exec process manager 构造执行环境时强制 `LANG/LC_CTYPE/LC_ALL=C.UTF-8` 等环境；对 exec-server 的实际 process id，会在 sandbox retry 时组合 `{process_id}-{uuid}`，否则使用数字 `process_id` 字符串，说明外部逻辑 id 与底层执行器进程 id 可以分离。证据：`references/codex/codex-rs/core/src/unified_exec/process_manager.rs:69`、`references/codex/codex-rs/core/src/unified_exec/process_manager.rs:171`。
- output snapshot 保留输出字节，最终文本通过 `String::from_utf8_lossy` 生成。证据：`references/codex/codex-rs/core/src/unified_exec/process.rs:256`、`references/codex/codex-rs/core/src/unified_exec/process.rs:278`。
- async watcher 先追加 pending byte buffer，再切出有效 UTF-8 前缀并发 `ExecCommandOutputDeltaEvent`；如果没有完整有效 UTF-8 前缀，也会发出单字节保证流前进。证据：`references/codex/codex-rs/core/src/unified_exec/async_watcher.rs:159`、`references/codex/codex-rs/core/src/unified_exec/async_watcher.rs:290`。
- legacy/raw exec 最终结果同样从原始 stdout/stderr/aggregated output 字节 lossy decode。证据：`references/codex/codex-rs/core/src/exec.rs:765`。

#### PowerShell 输出处理

- PowerShell shell 参数派生为 shell path、可选 `-NoProfile`、`-Command`、脚本文本。证据：`references/codex/codex-rs/core/src/shell.rs:20`。
- unified exec runtime 在 PowerShell shell 类型下调用 `prefix_powershell_script_with_utf8(&command)`，执行前把 UTF-8 输出前缀注入脚本。证据：`references/codex/codex-rs/core/src/tools/runtimes/unified_exec.rs:52`、`references/codex/codex-rs/core/src/tools/runtimes/unified_exec.rs:380`。
- UTF-8 输出前缀内容是 `try { [Console]::OutputEncoding=[System.Text.Encoding]::UTF8 } catch {}`；注入函数会从 `pwsh -Command ...` 或 `powershell.exe -Command ...` 形式提取脚本文本并避免重复注入。证据：`references/codex/codex-rs/shell-command/src/powershell.rs:11`、`references/codex/codex-rs/shell-command/src/powershell.rs:15`、`references/codex/codex-rs/shell-command/src/powershell.rs:35`。
- 测试覆盖“会添加前缀”和“不会重复添加前缀”。证据：`references/codex/codex-rs/shell-command/src/powershell.rs:209`、`references/codex/codex-rs/shell-command/src/powershell.rs:228`。
- 未发现 PowerShell 输出被 Codex 转为对象 JSON 再传输的路径；相反，证据链显示输出来源是 shell 进程 stdout/stderr/pty 字节，随后按 UTF-8 边界切分或 lossy decode。关键证据：`references/codex/codex-rs/exec-server-protocol/src/protocol.rs:174`、`references/codex/codex-rs/core/src/unified_exec/async_watcher.rs:159`、`references/codex/codex-rs/core/src/unified_exec/process.rs:278`、`references/codex/codex-rs/app-server-protocol/src/protocol/common.rs:1638`。

#### spawn/send/wait/close 与 mailbox/notification 闭环

- 协作 tool item status 有 `InProgress/Completed/Failed`；核心协作 tool 枚举包含 `SpawnAgent`、`SendInput`、`ResumeAgent`、`Wait`、`CloseAgent`；SubAgentActivity 有 `Started/Interacted/Interrupted`。证据：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/item.rs:1134`、`references/codex/codex-rs/app-server-protocol/src/protocol/v2/item.rs:1143`、`references/codex/codex-rs/app-server-protocol/src/protocol/v2/item.rs:1165`。
- `SubAgentActivity` 被映射为 `ThreadItem::SubAgentActivity` 并立即 `ItemCompleted`；`CollabWaitingBegin` 被映射为 Wait tool call started；`CollabWaitingEnd` 根据状态映射为 Completed 或 Failed。证据：`references/codex/codex-rs/app-server-protocol/src/protocol/event_mapping.rs:181`、`references/codex/codex-rs/app-server-protocol/src/protocol/event_mapping.rs:195`、`references/codex/codex-rs/app-server-protocol/src/protocol/event_mapping.rs:219`。
- v2 spawn 构造 `AgentCommunicationContext::new(Spawn, session.thread_id)`，通过 `spawn_agent_with_communication` 启动 agent，并发出 `SubAgentActivityEvent { kind: Started }`。证据：`references/codex/codex-rs/core/src/tools/handlers/multi_agents_v2/spawn.rs:96`、`references/codex/codex-rs/core/src/tools/handlers/multi_agents_v2/spawn.rs:114`、`references/codex/codex-rs/core/src/tools/handlers/multi_agents_v2/spawn.rs:143`。
- v1 send_input 会先发 `CollabAgentInteractionBeginEvent`，调用 `agent_control.send_input`，读取目标状态后发 `CollabAgentInteractionEndEvent` 并返回 `submission_id`。证据：`references/codex/codex-rs/core/src/tools/handlers/multi_agents/send_input.rs:69`、`references/codex/codex-rs/core/src/tools/handlers/multi_agents/send_input.rs:82`、`references/codex/codex-rs/core/src/tools/handlers/multi_agents/send_input.rs:87`。
- v2 send_message/followup_task 共用 message tool。`QueueOnly` 只入队，`TriggerTurn` 会触发目标 turn；发送后发 `SubAgentActivityEvent { kind: Interacted }`。证据：`references/codex/codex-rs/core/src/tools/handlers/multi_agents_v2/message_tool.rs:13`、`references/codex/codex-rs/core/src/tools/handlers/multi_agents_v2/message_tool.rs:111`、`references/codex/codex-rs/core/src/tools/handlers/multi_agents_v2/message_tool.rs:118`。
- v2 wait 订阅 `session.input_queue.subscribe_activity`，先发 `CollabWaitingBeginEvent`，等待 mailbox activity、steered input 或 timeout，再发 `CollabWaitingEndEvent`；返回只包含 `message` 和 `timed_out`，不携带 payload。证据：`references/codex/codex-rs/core/src/tools/handlers/multi_agents_v2/wait.rs:69`、`references/codex/codex-rs/core/src/tools/handlers/multi_agents_v2/wait.rs:78`、`references/codex/codex-rs/core/src/tools/handlers/multi_agents_v2/wait.rs:92`、`references/codex/codex-rs/core/src/tools/handlers/multi_agents_v2/wait.rs:126`。
- v1 wait 以 target agents 为等待对象，订阅每个目标状态，等待 final status 或 timeout，并返回 status map。证据：`references/codex/codex-rs/core/src/tools/handlers/multi_agents/wait.rs:99`、`references/codex/codex-rs/core/src/tools/handlers/multi_agents/wait.rs:113`、`references/codex/codex-rs/core/src/tools/handlers/multi_agents/wait.rs:151`、`references/codex/codex-rs/core/src/tools/handlers/multi_agents/wait.rs:183`。
- multi-agent spec 明确 v1 wait 会等待 final status，completed status 可能包含 final message；v2 wait 等 mailbox update，包括 queued messages/final-status notifications，并且不返回内容，只返回摘要。证据：`references/codex/codex-rs/core/src/tools/handlers/multi_agents_spec.rs:236`、`references/codex/codex-rs/core/src/tools/handlers/multi_agents_spec.rs:252`、`references/codex/codex-rs/core/src/tools/handlers/multi_agents_spec.rs:485`。
- close_agent 发 close begin，订阅/读取 previous status，调用 `agent_control.close_agent`，再发 close end，并返回 previous status。证据：`references/codex/codex-rs/core/src/tools/handlers/multi_agents/close_agent.rs:46`、`references/codex/codex-rs/core/src/tools/handlers/multi_agents/close_agent.rs:58`、`references/codex/codex-rs/core/src/tools/handlers/multi_agents/close_agent.rs:88`、`references/codex/codex-rs/core/src/tools/handlers/multi_agents/close_agent.rs:92`。
- AgentControl 是 root-scoped control-plane handle，共享于 root 和 subagents；`send_input` 先做 capacity check 再 `state.send_op`；`send_inter_agent_communication` 可按 `trigger_turn` 保障执行容量。证据：`references/codex/codex-rs/core/src/agent/control.rs:88`、`references/codex/codex-rs/core/src/agent/control.rs:140`、`references/codex/codex-rs/core/src/agent/control.rs:153`、`references/codex/codex-rs/core/src/agent/control.rs:178`。
- completion watcher 订阅 child status 到 final；v2 把结果作为 `InterAgentCommunication` 送回 parent，`trigger_turn=false`，context kind 是 `Result`；v1 则注入 legacy user message。证据：`references/codex/codex-rs/core/src/agent/control.rs:454`、`references/codex/codex-rs/core/src/agent/control.rs:531`、`references/codex/codex-rs/core/src/agent/control.rs:533`。
- Codex input queue 有 `TurnInput::InterAgentCommunication`，`InputQueueActivity::Mailbox/Steer`，`mailbox_pending_mails`；`enqueue_mailbox_communication` 入队后 `send_replace(InputQueueActivity::Mailbox)` 唤醒 wait。证据：`references/codex/codex-rs/core/src/session/input_queue.rs:12`、`references/codex/codex-rs/core/src/session/input_queue.rs:22`、`references/codex/codex-rs/core/src/session/input_queue.rs:34`、`references/codex/codex-rs/core/src/session/input_queue.rs:72`。
- `subscribe_activity` 会返回 receiver 和当前 pending activity，且 pending steer 优先于 mailbox；`get_pending_input` 在当前 turn 接受 mailbox delivery 时 drain mailbox，并追加到 pending input。证据：`references/codex/codex-rs/core/src/session/input_queue.rs:49`、`references/codex/codex-rs/core/src/session/input_queue.rs:197`。
- session 接收 inter-agent communication 时先 enqueue mailbox，再发 receive log；如果 `trigger_turn` 为 true，会尝试启动 pending work。证据：`references/codex/codex-rs/core/src/session/handlers.rs:277`。
- turn 会在下一次模型请求前 drain pending input；assistant commentary/reasoning 阶段可被 mailbox mail preempt。证据：`references/codex/codex-rs/core/src/session/turn.rs:217`、`references/codex/codex-rs/core/src/session/turn.rs:2037`。
- subagent legacy notification 是 user-role contextual fragment，包含 `<subagent_notification>` 标记和 `{ agent_path, status }` JSON body；completion message 中 completed 状态包含 final message，errored 状态包含截断错误与 next action。证据：`references/codex/codex-rs/core/src/context/subagent_notification.rs:20`、`references/codex/codex-rs/core/src/session_prefix.rs:20`、`references/codex/codex-rs/core/src/session_prefix.rs:27`。

### 可借鉴能力模型

#### 终端执行能力模型

- 对外公开逻辑 `process_id`，不要把它等同 OS pid。该 id 应串联 start/write/resize/terminate/read/output delta/exited/closed。
- PTY 是执行模式，不只是 UI 标记。TTY 下 stdin/stdout/stderr streaming 行为会变化，最终 response 要明确 streamed output 与 buffered output 的去重边界。
- 输出模型保留 stream identity：stdout、stderr、pty。底层事件可保存 bytes + seq；timeline/UI 可以另有文本投影。
- 事件顺序应闭合：process started -> output deltas -> terminal interaction -> exited -> closed -> final response/terminal item completed。app-server 参考实现要求 final response 在所有 output delta 发完之后返回。
- PowerShell 准备阶段设置 UTF-8 console output encoding；采集阶段按真实 stdout/stderr/pty 字节处理；展示阶段再 decode。这样能避免“中文经过宿主 shell inline 管道或对象序列化后变 `?`”的类别问题。

#### 协作 agent 能力模型

- spawn 返回稳定 agent reference/path/nickname，同时发 activity started；消息发送和等待要解耦。
- send/followup message 只改变目标 agent/mailbox 状态，并发 activity interacted；是否触发目标 turn 是 delivery mode/trigger turn 问题。
- wait 是“等待活动发生”的工具，不是“搬运结果”的工具。v2 wait 的方向更适合 AgentDashboard：返回摘要、timeout 与可查询引用，由 UI 或后续工具查询 mailbox 内容。
- close/cancel 应记录 previous status，并发 begin/end notification，让 timeline 与状态机闭合。
- subagent completion/result 应由 completion watcher 或 terminal callback 转为 mailbox envelope，再发 notification 唤醒等待方；结果内容的权威来源应是 durable mailbox/message record。

#### mailbox / notification 能力模型

- Codex 的 in-memory mailbox 有两个关键语义值得迁移：一是 pending activity 订阅能唤醒 wait；二是 mailbox delivery 与 steer delivery 都进入 turn input，但 steer 优先。
- AgentDashboard 应把这些语义落在既有 AgentRunMailbox：durable envelope、source identity、source dedup、delivery/barrier/drain mode、claim lease、attempt/recovery。
- notification 触发点应在 durable state change 之后，例如现有 `MailboxStateChanged`。这样等待方收到通知后可以通过查询得到一致状态，而不是依赖内存消息体。
- wait 应支持“已有 pending mailbox 立即完成”和“未来 mailbox state change 唤醒”两种路径，并把 timeout/steered/closed 区分为不同结果。

### 不可迁移部分

- 不应迁移 Codex `InputQueue` 作为 AgentDashboard 权威事实源。Codex queue 是 session 内存结构；AgentDashboard 规范已经要求 AgentRun mailbox 是 envelope/domain/repository 事实源，并支持 receipt、lease、barrier 与 recovery。
- 不应迁移 v1 wait 的“直接返回 completed final message”作为主模型。AgentDashboard 更适合 v2 风格：wait 返回摘要和引用，内容通过 mailbox/trace 查询。
- 不应暴露 Codex ThreadId、AgentPath、SessionSource 作为 AgentDashboard domain identity。AgentDashboard 应继续使用 LifecycleRun/LifecycleAgent/AgentFrame/RuntimeSession/AgentRunThread/AgentRunTurn 等自有锚点。
- 不应把 `String::from_utf8_lossy` 当作唯一存储形态。Codex 在多个 UI/event 映射层做 lossy 文本投影，但 terminal exec 能力若要可靠，应尽量保留 raw bytes 或至少保留可追踪的 byte delta/seq。
- 不应把 PowerShell 输出建模为对象 JSON。Codex 的证据链显示它依赖真实 shell 进程字节流和 UTF-8 输出前缀，PowerShell 对象格式化发生在 shell 内部，不是 AgentDashboard 协议层对象序列化。
- 不应绕过 AgentRun mailbox 让 route/workspace snapshot/project start/frame construction 直接分支 launch/steer。本地规范已明确 composer-submit -> command receipt -> mailbox envelope -> scheduler -> outcome 是主路径。

### 对两个本地任务的建议

当前 task `prd.md` 仍是 TBD，未找到已定稿的两个子任务说明；以下按本调研主题自然拆为两个本地实现任务建议。

1. 终端 exec / PowerShell 输出闭环任务：
   - 在 AgentDashboard 自有 RuntimeSession/Backbone 体系内建模 terminal command execution：逻辑 `process_id`、PTY flag、stdin write、resize/terminate、stdout/stderr/pty byte delta、exited/closed/final result。
   - PowerShell UTF-8 前缀应放在 command preparation/runtime adapter 边界，且仅作为执行前缀，不改变用户命令的结果模型。
   - 输出存储建议保留 stream + seq + bytes 或等价可恢复表示；UI timeline 另行生成文本 delta。
   - 测试重点：中文 PowerShell 输出、stdout/stderr 分流、PTY stdin、output delta 与 final response 顺序、streamed output 不在 final buffered output 重复、无对象 JSON 序列化路径。

2. 并行 subagent wait / mailbox notification 闭环任务：
   - 以现有 `AgentRunMailboxService`、scheduler、runtime adapter、command receipt 为主干；spawn/send/result/close 都落为有 source identity/correlation/source dedup 的 mailbox 或状态事实。
   - wait 工具等待 durable mailbox state 或 runtime terminal state 变化，返回摘要、timeout、message refs，不直接传输大段结果内容。
   - completion watcher/terminal callback 把 subagent final status/result 转为 AgentRun mailbox envelope，再发 `MailboxStateChanged`，等待方通过 query 刷新。
   - 测试重点：已有 queued mailbox 立即唤醒 wait、未来 mailbox notification 唤醒 wait、timeout 不吞状态、result delivery 重启后可恢复、close/cancel 记录 previous status、parent resume 不绕过 mailbox。

### Related specs

- `.trellis/spec/backend/session/agentrun-mailbox.md` - AgentRun mailbox 是 unified message intake、scheduling queue 与 recovery projection；composer-submit 进入 receipt/envelope/scheduler/outcome；source identity 是开放命名空间；runtime adapter 只承担 turn boundary delegate。
- `.trellis/spec/backend/session/runtime-execution-state.md` - AgentRun workspace command text input 必须经过 composer-submit 与 durable mailbox；route/workspace snapshot/project start/frame construction 不直接分支 launch/steer。
- `.trellis/spec/backend/session/streaming-protocol.md` - Backbone/session streaming 是前端订阅 runtime 变化的协议层，适合作为 terminal delta 与 mailbox state notification 投影。
- `.trellis/spec/backend/session/architecture.md` - Session subsystem 拥有 RuntimeSession trace；AgentRun mailbox 映射到 Codex-compatible turn/start turn/steer 或 AgentDash envelope extension。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` - AgentRun composer submit 使用 generated DTO，后端 command receipt -> envelope -> scheduler；前端不手写状态分支。

### External references

- 未访问外部网络文档。本次调研仅基于仓库内 `references/codex` 快照、本项目 `.trellis/spec/` 与必要本地代码。

## Caveats / Not Found

- `references/codex` 是本地快照，本文件没有确认其对应的上游 Codex commit/tag 或最新官方文档版本。
- Codex v2 的 close agent handler 未在本次必要路径中找到；close 证据来自 v1 `close_agent.rs`，但 app-server item/event mapping 已覆盖 CloseAgent begin/end 的 UI 投影。
- Codex output 有两层需要区分：`command/exec/outputDelta` 协议层是 base64 bytes；tool item timeline 的 `CommandExecutionOutputDeltaNotification` 是从 bytes lossy decode 后的文本 delta。
- PowerShell “对象输出问题”的结论是协议层不依赖对象 JSON 序列化；但 PowerShell 自身仍会在进程内部把对象格式化成控制台文本，再由 stdout/stderr 字节流传出。
- “两个本地任务”的建议是按用户给定调研主题推导出的实现拆分；当前 active task 的 `prd.md` 尚未提供明确子任务名称或验收条款。
