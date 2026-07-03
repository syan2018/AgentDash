# Research: subagent-codex-reference

- Query: 检查 references/codex，提炼对 AgentDashboard 自有 waitable activity + exec 闭环有参考价值的能力模型；不接入 Codex runtime，不复制 Codex identity/domain。
- Scope: mixed
- Date: 2026-07-03

## Findings

### Files Found

- `.trellis/workflow.md`: Trellis 研究产物必须写入任务目录 `research/`，实现前读取相关 spec。
- `.trellis/spec/backend/workflow/activity-lifecycle.md`: AgentDashboard workflow lifecycle、runtime node、executor run ref、NodeStarted/terminal event 合约。
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`: 前端 run view 使用 `LifecycleRunView.orchestrations[]` 和 `RuntimeNodeView`，不能把 RuntimeSession 当生命周期主键。
- `.trellis/spec/backend/session/runtime-execution-state.md`: RuntimeSession 是 delivery/trace，不是 AgentRun 控制面；后台任务和 mailbox 通过应用层状态协调。
- `.trellis/spec/backend/runtime-gateway.md`: RuntimeGateway 的 action/permission 边界，包含 `process.exec`、`process.shell` 类能力。
- `.trellis/spec/backend/session/agentrun-mailbox.md`: mailbox/waiting item 模型，`exec_*` gate 投影为 `kind="exec"`，wait 返回应保持小而引用化。
- `.trellis/spec/cross-layer/backbone-protocol.md`: Backbone command output delta 与 ThreadItem command execution 的跨层事件约束。
- `references/codex/codex-rs/core/src/tools/handlers/unified_exec.rs`: Codex unified exec tool 参数入口和 shell/login/yield 默认值。
- `references/codex/codex-rs/core/src/tools/handlers/unified_exec/exec_command.rs`: `exec_command` handler，把模型工具调用转成 `UnifiedExecProcessManager::exec_command`。
- `references/codex/codex-rs/core/src/tools/handlers/unified_exec/write_stdin.rs`: `write_stdin` handler，写入 stdin 或空输入续读后台 process。
- `references/codex/codex-rs/core/src/unified_exec/mod.rs`: unified exec 的请求、进程 entry、yield clamp 基础结构。
- `references/codex/codex-rs/core/src/unified_exec/process_manager.rs`: 进程启动、续读、终止、状态枚举、进程列表的核心实现。
- `references/codex/codex-rs/core/src/tools/context.rs`: `ExecCommandToolOutput` 对模型可见 JSON/text 返回的序列化。
- `references/codex/codex-rs/core/tests/suite/unified_exec.rs`: running/completed command 的行为测试。
- `references/codex/codex-rs/core/src/tools/code_mode/wait_spec.rs`: code-mode `wait` 工具 schema。
- `references/codex/codex-rs/core/src/tools/code_mode/wait_handler.rs`: code cell wait/terminate handler。
- `references/codex/codex-rs/core/src/tools/handlers/sleep.rs`: 通用等待直到 timeout 或新输入唤醒的模式。
- `references/codex/codex-rs/core/src/tools/handlers/wait_for_environment.rs`: 等待外部环境 ready 的小型 wait 模式。
- `references/codex/codex-rs/core/src/session/input_queue.rs`: mailbox/steer 活动通知和 pending input drain 模型。
- `references/codex/codex-rs/core/src/tools/handlers/multi_agents/wait.rs`: 多 agent v1 等待最终状态，返回 status map 与 timeout。
- `references/codex/codex-rs/core/src/tools/handlers/multi_agents_v2/wait.rs`: 多 agent v2 等待 mailbox/steer 活动，返回 summary message 与 timeout。
- `references/codex/codex-rs/core/src/tools/handlers/multi_agents_spec.rs`: multi-agent spawn/wait/status output schema。
- `references/codex/codex-rs/core/src/tools/handlers/multi_agents/spawn.rs`: subagent spawn 返回小结果、事件化记录 agent 状态。
- `references/codex/codex-rs/agent-graph-store/src/store.rs`: parent/child thread spawn graph store 边界。
- `references/codex/codex-rs/agent-graph-store/src/types.rs`: thread spawn edge open/closed 状态。
- `references/codex/codex-rs/core/src/agent/status.rs`: agent status 由事件推导，final status 判定。
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/command_exec.rs`: app-server standalone `command/exec` 协议。
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/process.rs`: app-server raw process spawn/write/kill/output/exited 协议。
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/item.rs`: CommandExecution、CollabAgentToolCall、SubAgentActivity、TerminalInteraction、OutputDelta 的 item/event 模型。
- `crates/agentdash-contracts/src/runtime/workflow.rs`: AgentDashboard contract 中 waiting item、mailbox snapshot、executor run ref。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs`: waiting item contract projection。
- `crates/agentdash-domain/src/workflow/value_objects/orchestration.rs`: runtime node state、status、trace ref、journal fact。
- `crates/agentdash-infrastructure/src/function_runner.rs`: 当前 BashExec 同步执行模型。
- `crates/agentdash-application-workflow/src/orchestration/function_node_runner.rs`: Function/BashExec 节点执行和输出映射。
- `crates/agentdash-application-workflow/src/orchestration/executor_launcher.rs`: NodeStarted/NodeCompleted/NodeFailed/NodeBlocked 事件写入路径。

### Related Specs

- `.trellis/spec/backend/workflow/activity-lifecycle.md`: waitable exec 必须落在 `LifecycleRun -> OrchestrationInstance -> RuntimeNodeState`，节点坐标是 `orchestration_id + node_path + attempt`。
- `.trellis/spec/backend/session/runtime-execution-state.md`: `RuntimeSession` 只能作为 delivery/trace evidence，不应该成为 wait/exec 生命周期 owner。
- `.trellis/spec/backend/session/agentrun-mailbox.md`: `ConversationWaitingItemView.kind` 已预留 `exec`，wait 返回应小而稳定，结果通过 gate/mailbox/projection 引用读取。
- `.trellis/spec/backend/runtime-gateway.md`: process 类能力要走 RuntimeGateway 权限与 actor/context validation。
- `.trellis/spec/cross-layer/backbone-protocol.md`: command output delta 可以事件化，但大输出不能塞进 Backbone event body，应使用 bounded preview 与 output ref。
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`: 前端 primary run view 不能把 `RuntimeSession` id 当 lifecycle 主键。

### 1. Codex exec/shell/start/read/wait/write/terminate/status Anchors

Codex 的模型工具侧核心不是把 `start/read/wait/status` 拆成独立模型工具，而是把 `exec_command` 做成“start + 初次 read/yield”，把 `write_stdin` 做成“write 或空输入 poll/read”。terminate/status 存在于 manager 和 app-server protocol 层，但并非完全以同名模型工具暴露。

- `exec_command` 参数在 `references/codex/codex-rs/core/src/tools/handlers/unified_exec.rs:27` 定义，包含 `cmd`、`shell`、`login`、`tty`、`yield_time_ms`、`max_output_tokens`、sandbox/approval 参数。
- `exec_command` 默认 yield 是 10 秒，`write_stdin` 默认 yield 是 250ms：`references/codex/codex-rs/core/src/tools/handlers/unified_exec.rs:60`。
- shell/login/direct command 解析在 `references/codex/codex-rs/core/src/tools/handlers/unified_exec.rs:97`，zsh fork shell 场景拒绝自定义 shell 在 `references/codex/codex-rs/core/src/tools/handlers/unified_exec.rs:125`。
- `ExecCommandRequest` 结构在 `references/codex/codex-rs/core/src/unified_exec/mod.rs:91`，关键字段有 command、shell_type、process_id、yield_time_ms、max_output_tokens、cwd、environment、shell_mode、tty、permissions。
- `WriteStdinRequest` 结构在 `references/codex/codex-rs/core/src/unified_exec/mod.rs:112`，字段是 `process_id`、`input`、`yield_time_ms`、`max_output_tokens`、`truncation_policy`。
- process entry 记录 call id、process id、cwd、hook command、tty、network approval、session weak ref、last_used：`references/codex/codex-rs/core/src/unified_exec/mod.rs:155`。
- yield clamp 在 `references/codex/codex-rs/core/src/unified_exec/mod.rs:168`；Windows 有 floor，超过上限会 clamp。
- `exec_command` handler 读取 payload 并构造 context：`references/codex/codex-rs/core/src/tools/handlers/unified_exec/exec_command.rs:121`。
- `exec_command` handler 分配 process id、解析 command/shell、抽取 tty/yield/max/sandbox 参数：`references/codex/codex-rs/core/src/tools/handlers/unified_exec/exec_command.rs:231`。
- apply_patch 会被 special-case 截获并直接返回终态输出，不进入长期 process：`references/codex/codex-rs/core/src/tools/handlers/unified_exec/exec_command.rs:314`。
- handler 调用 `manager.exec_command(ExecCommandRequest { ... })` 在 `references/codex/codex-rs/core/src/tools/handlers/unified_exec/exec_command.rs:342`。
- sandbox denial 返回终态，`process_id: None`、`exit_code: Some(output.exit_code)`，不能再 `write_stdin` 续读：`references/codex/codex-rs/core/src/tools/handlers/unified_exec/exec_command.rs:369`。
- process manager 启动 sandboxed session、注册 network denial termination、创建 transcript、发 `ExecCommandBegin`：`references/codex/codex-rs/core/src/unified_exec/process_manager.rs:408`。
- 启动后先开启 output stream，并在初始等待前持久化 live session，避免 turn end/interrupt 丢进程：`references/codex/codex-rs/core/src/unified_exec/process_manager.rs:450`。
- 初始 exec 收集输出直到 deadline 或 pause state：`references/codex/codex-rs/core/src/unified_exec/process_manager.rs:478`。
- 初始等待后刷新 process state；alive 返回 `process_id Some`，exited 返回 `exit_code`：`references/codex/codex-rs/core/src/unified_exec/process_manager.rs:554`。
- 短命令会发 end event、释放 process id、返回无 live process：`references/codex/codex-rs/core/src/unified_exec/process_manager.rs:580`。
- `ExecCommandToolOutput` 返回字段在 `references/codex/codex-rs/core/src/unified_exec/process_manager.rs:624` 组装。
- `write_stdin` 入口在 `references/codex/codex-rs/core/src/unified_exec/process_manager.rs:641`；非空输入写 TTY，非 TTY 只支持 interrupt，空输入是 background poll。
- `write_stdin` 续读输出直到 deadline：`references/codex/codex-rs/core/src/unified_exec/process_manager.rs:705`。
- `write_stdin` 后刷新状态；alive 继续返回 process id，exited 返回 exit code：`references/codex/codex-rs/core/src/unified_exec/process_manager.rs:745`。
- exited process 从 process store 移除，alive process 保留 call id 与 process id：`references/codex/codex-rs/core/src/unified_exec/process_manager.rs:796`。
- terminate-all 在 `references/codex/codex-rs/core/src/unified_exec/process_manager.rs:1379`。
- list background processes/status surface 在 `references/codex/codex-rs/core/src/unified_exec/process_manager.rs:1397`，返回 item id、process id、command、cwd。
- terminate single process 在 `references/codex/codex-rs/core/src/unified_exec/process_manager.rs:1416`，返回 bool 并移除 process store entry。
- `ProcessStatus` 有 `Alive`、`Exited`、`Unknown`：`references/codex/codex-rs/core/src/unified_exec/process_manager.rs:1451`。
- 模型工具 `write_stdin` schema 把 process id 暴露成 `session_id`，训练语义上是同一个续读/写入句柄：`references/codex/codex-rs/core/src/tools/handlers/unified_exec/write_stdin.rs:20`。
- `write_stdin` handler 调用 unified manager 在 `references/codex/codex-rs/core/src/tools/handlers/unified_exec/write_stdin.rs:69`。
- 非空写入或 live process 空 poll 会发 `TerminalInteraction`：`references/codex/codex-rs/core/src/tools/handlers/unified_exec/write_stdin.rs:85`。
- `write_stdin` 没有 PreToolUse，完成时才可能对原始 exec command 发 PostToolUse：`references/codex/codex-rs/core/src/tools/handlers/unified_exec/write_stdin.rs:110`。

app-server protocol 层提供更显式的 standalone command/process start/write/terminate/status-by-notification 能力：

- `command/exec` 是 server sandbox 下 standalone command，最终 response 等进程结束后发送，且在所有 outputDelta notification 之后：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/command_exec.rs:21`。
- `CommandExecParams` 包含 command、connection-scoped `process_id`、tty、stream stdin/stdout/stderr、output cap、disable_timeout、timeout_ms、cwd/env/size、sandbox/permission：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/command_exec.rs:30`。
- `CommandExecResponse` 是 completed-only final result：`exit_code`、`stdout`、`stderr`；启用 stream 时 stdout/stderr 为空：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/command_exec.rs:111`。
- `command/exec/write` 参数是 process id、base64 stdin delta、close stdin：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/command_exec.rs:128`。
- `command/exec/terminate` 参数是 process id：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/command_exec.rs:151`。
- output delta notification 绑定 process id、stdout/stderr stream、base64 delta、cap_reached；连接关闭会终止对应 process：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/command_exec.rs:185`。
- `process/spawn` 启动无 Codex sandbox 的 raw process，response 只表示 started/registered，输出和退出走 notification：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/process.rs:19`。
- `ProcessSpawnParams` 使用 client-supplied `process_handle`，包含 cwd、tty、stream、output cap、timeout/env/size：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/process.rs:28`。
- raw process 的 stdin write、kill、output delta、exited notification 分别在 `references/codex/codex-rs/app-server-protocol/src/protocol/v2/process.rs:96`、`references/codex/codex-rs/app-server-protocol/src/protocol/v2/process.rs:118`、`references/codex/codex-rs/app-server-protocol/src/protocol/v2/process.rs:150`、`references/codex/codex-rs/app-server-protocol/src/protocol/v2/process.rs:177`。

### 2. Codex Wait / Async Tool / Background Task Anchors

Codex 有三类可借鉴 wait：针对 code cell 的 `wait`，针对环境 ready 的 wait，针对 multi-agent/mailbox/steer 活动的 wait。共同点是 wait 返回小结构，主要表达“是否有活动/是否完成/是否 timeout”，不把大结果塞入 wait 返回。

- code-mode `wait` schema 在 `references/codex/codex-rs/core/src/tools/code_mode/wait_spec.rs:6`，参数是 `cell_id`、`yield_time_ms`、`max_tokens`、`terminate`。
- `wait_handler` 参数解析在 `references/codex/codex-rs/core/src/tools/code_mode/wait_handler.rs:23`。
- `terminate=true` 时调用 `code_mode_service.terminate(cell_id)`，否则调用 `wait(WaitRequest { cell_id, yield_time_ms })`：`references/codex/codex-rs/core/src/tools/code_mode/wait_handler.rs:82`。
- live cell 返回 non-yield 后会记录 ended 并完成 dispatch：`references/codex/codex-rs/core/src/tools/code_mode/wait_handler.rs:99`。
- code-mode wait 不发 PreToolUse/PostToolUse，因为它是已有 code cell 的 runtime control，不是新工具执行：`references/codex/codex-rs/core/src/tools/code_mode/wait_handler.rs:135`。
- `sleep` 等待会在新输入到达时提前结束：`references/codex/codex-rs/core/src/tools/handlers/sleep.rs:47`。
- `sleep` 通过 `input_queue.subscribe_activity` 监听活动：`references/codex/codex-rs/core/src/tools/handlers/sleep.rs:91`。
- `sleep` 用 `tokio::select!` 在 timer 和 activity_rx.changed() 之间竞争：`references/codex/codex-rs/core/src/tools/handlers/sleep.rs:105`。
- `wait_for_environment` 检查 ready，否则等待 `environment.wait_until_ready().await`：`references/codex/codex-rs/core/src/tools/handlers/wait_for_environment.rs:72`。
- `wait_for_environment` 返回小 JSON `{ environment_id, status: "ready" }`：`references/codex/codex-rs/core/src/tools/handlers/wait_for_environment.rs:97`。
- `InputQueueActivity` 只有 `Mailbox` 与 `Steer` 两类：`references/codex/codex-rs/core/src/session/input_queue.rs:22`。
- input queue 的 watch sender 和 mailbox pending queue 在 `references/codex/codex-rs/core/src/session/input_queue.rs:34`。
- `subscribe_activity` 会返回 receiver，也会检查当前是否已有 pending steer/mailbox：`references/codex/codex-rs/core/src/session/input_queue.rs:49`。
- enqueue mailbox communication 会写 pending queue 并 notify `Mailbox`：`references/codex/codex-rs/core/src/session/input_queue.rs:72`。
- pending user input/steer 会 notify `Steer`：`references/codex/codex-rs/core/src/session/input_queue.rs:165`。
- pending input 只在当前 turn 接受 mailbox delivery 时 drain mailbox：`references/codex/codex-rs/core/src/session/input_queue.rs:197`。
- multi-agent v1 wait 验证 timeout、clamp min/max：`references/codex/codex-rs/core/src/tools/handlers/multi_agents/wait.rs:89`。
- v1 wait 发 `CollabWaitingBeginEvent`：`references/codex/codex-rs/core/src/tools/handlers/multi_agents/wait.rs:99`。
- v1 wait 订阅 agent statuses，捕获初始 final/not found/error：`references/codex/codex-rs/core/src/tools/handlers/multi_agents/wait.rs:113`。
- v1 wait 用 deadline 等 final status；一个 final 到达后会 drain 其他 immediately ready results：`references/codex/codex-rs/core/src/tools/handlers/multi_agents/wait.rs:151`。
- v1 wait 返回 `WaitAgentResult { status: HashMap<String, AgentStatus>, timed_out }`：`references/codex/codex-rs/core/src/tools/handlers/multi_agents/wait.rs:183`。
- v1 wait 发 `CollabWaitingEndEvent`：`references/codex/codex-rs/core/src/tools/handlers/multi_agents/wait.rs:199`。
- `wait_for_final_status` 循环 watch receiver 直到 final 或 channel closed：`references/codex/codex-rs/core/src/tools/handlers/multi_agents/wait.rs:254`。
- multi-agent v2 wait 验证 timeout 并取 config min/max/default：`references/codex/codex-rs/core/src/tools/handlers/multi_agents_v2/wait.rs:50`。
- v2 wait 订阅 input queue activity：`references/codex/codex-rs/core/src/tools/handlers/multi_agents_v2/wait.rs:69`。
- v2 wait 将 activity/deadline 映射为结果：`references/codex/codex-rs/core/src/tools/handlers/multi_agents_v2/wait.rs:92`。
- v2 wait 返回 `WaitAgentResult { message, timed_out }`，message 是 completed/interrupted/timed out 级别摘要：`references/codex/codex-rs/core/src/tools/handlers/multi_agents_v2/wait.rs:126`。
- v2 wait 的 outcome 是 `MailboxActivity`、`Steered`、`TimedOut`：`references/codex/codex-rs/core/src/tools/handlers/multi_agents_v2/wait.rs:164`。
- wait spec v1 描述等待 final status，timeout 时 status 为空：`references/codex/codex-rs/core/src/tools/handlers/multi_agents_spec.rs:236`。
- wait spec v2 描述等待 mailbox update/queued message/final status notification，或被 steered input 提前结束；不返回内容，只返回 summary/timeout：`references/codex/codex-rs/core/src/tools/handlers/multi_agents_spec.rs:252`。
- wait output schema v1 必填 `status` 和 `timed_out`：`references/codex/codex-rs/core/src/tools/handlers/multi_agents_spec.rs:466`。
- wait output schema v2 必填 `message` 和 `timed_out`：`references/codex/codex-rs/core/src/tools/handlers/multi_agents_spec.rs:485`。

### 3. Running Command vs Completed Command Return Structure, Continuation, Timeout

Codex unified exec 的返回结构由 `ExecCommandToolOutput` 和 `code_mode_result` 决定。

- `ExecCommandToolOutput` 字段包括 `event_call_id`、`chunk_id`、`wall_time`、`raw_output`、`truncation_policy`、`max_output_tokens`、`process_id`、`exit_code`、`original_token_count`、`hook_command`：`references/codex/codex-rs/core/src/tools/context.rs:310`。
- JSON 返回包含可选 `chunk_id`、`wall_time_seconds`、可选 `exit_code`、可选 `session_id`、`original_token_count`、`output`：`references/codex/codex-rs/core/src/tools/context.rs:369`。
- 文本返回会显示 `Chunk ID`、`Wall time`、`Process exited with code`、`Process running with session ID`、`Original token count`、`Output`：`references/codex/codex-rs/core/src/tools/context.rs:412`。
- process 仍在运行时 PostToolUse response 会被抑制，避免把未完成工具当终态处理：`references/codex/codex-rs/core/src/tools/context.rs:359`。
- completed process 测试断言 `process_id.is_none()`、`exit_code=0`，且 `chunk_id` 是 6 位 hex、wall time 非负：`references/codex/codex-rs/core/tests/suite/unified_exec.rs:1406`。
- 交互式运行测试断言 initial exec 返回 process id 且没有 exit code；后续 `write_stdin` 在运行中复用 process id 且没有 exit code；EOF 后返回 no process id 与 `exit_code=0`：`references/codex/codex-rs/core/tests/suite/unified_exec.rs:1874`。
- 长进程在 turn completion 后仍保持 alive，shutdown 才终止：`references/codex/codex-rs/core/tests/suite/unified_exec.rs:2368`。
- yield clamp 行为测试覆盖 Windows floor、max clamp、非 Windows min clamp：`references/codex/codex-rs/core/src/unified_exec/process_manager_tests.rs:213`。

结论：

- Running command 返回：`session_id/process_id` present，`exit_code` absent，`output` 是当前 yield 窗口内已收集的 bounded output。模型可用同一个 `session_id` 调 `write_stdin`，空 `chars` 表示“继续读取/等待一点”。
- Completed command 返回：`exit_code` present，`session_id/process_id` absent，process 已从 store 移除。之后不能再用 `write_stdin` 续读同一 process。
- 输出续读不是显式 cursor/offset，而是 manager 持有 process output buffer/transcript，每次 tool call drain 新增输出并返回 bounded preview；事件流另有 output delta。
- `yield_time_ms` 是“工具调用愿意等多久再把控制权还给模型”，不是进程 execution timeout。进程 timeout 是 app-server `CommandExecParams.timeout_ms`/`disable_timeout` 或 raw `ProcessSpawnParams.timeout_ms` 这一类独立参数：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/command_exec.rs:69`、`references/codex/codex-rs/app-server-protocol/src/protocol/v2/process.rs:64`。
- app-server `command/exec` 的 final response 是 completed-only；如果启用 streaming，stdout/stderr 在 final response 里为空，输出从 `outputDelta` notification 获得：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/command_exec.rs:111`。
- app-server raw `process/spawn` 的 status 是 notification-driven：spawn response 只表示 registered，退出由 `process/exited` notification 给出 exit code 和最终 stdout/stderr：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/process.rs:90`、`references/codex/codex-rs/app-server-protocol/src/protocol/v2/process.rs:177`。

### 4. Subagent / Parallel Work / Mailbox / Thread / Event Return Model

Codex multi-agent 返回模型可参考的是“控制面返回小结果、状态由事件投影、消息由 mailbox 传递”，不是它的 Thread/AgentPath/session identity。

- spawn agent 后保存 `new_thread_id`、metadata、status：`references/codex/codex-rs/core/src/tools/handlers/multi_agents/spawn.rs:139`。
- spawn end event 包含 agent metadata/status 并结束工具调用：`references/codex/codex-rs/core/src/tools/handlers/multi_agents/spawn.rs:180`。
- `SpawnAgentResult` 只返回 `agent_id` 和 `nickname`：`references/codex/codex-rs/core/src/tools/handlers/multi_agents/spawn.rs:206`。
- `agent_status_output_schema` 支持 `Pending`、`Running`、`Interrupted`、`Shutdown`、`NotFound`、`completed`、`errored` 等状态形态：`references/codex/codex-rs/core/src/tools/handlers/multi_agents_spec.rs:327`。
- event-derived status：`TurnStarted -> Running`，`TurnComplete -> Completed(last_agent_message)`，interrupted/budget -> Interrupted，errors -> Errored，shutdown -> Shutdown：`references/codex/codex-rs/core/src/agent/status.rs:6`。
- final status 排除 PendingInit/Running/Interrupted：`references/codex/codex-rs/core/src/agent/status.rs:23`。
- agent graph store 是 storage-neutral parent/child topology 边界，要求 stable ordering：`references/codex/codex-rs/agent-graph-store/src/store.rs:13`。
- graph store upsert parent/child/status edge：`references/codex/codex-rs/agent-graph-store/src/store.rs:22`。
- graph store 支持 list direct children 和 breadth-first descendants：`references/codex/codex-rs/agent-graph-store/src/store.rs:43`、`references/codex/codex-rs/agent-graph-store/src/store.rs:49`。
- thread spawn edge status 只有 `Open`/`Closed`：`references/codex/codex-rs/agent-graph-store/src/types.rs:4`。
- `ThreadItem::CommandExecution` 聚合 command、cwd、process_id、source、status、actions、aggregated_output、exit_code、duration_ms：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/item.rs:261`。
- `ThreadItem::CollabAgentToolCall` 聚合 tool、status、sender/receiver thread ids、prompt、model、reasoning_effort、agents_states：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/item.rs:327`。
- `ThreadItem::SubAgentActivity` 包含 kind、agent_thread_id、agent_path：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/item.rs:350`。
- `CommandExecutionStatus` 是 InProgress/Completed/Failed/Declined：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/item.rs:995`。
- `CollabAgentToolCallStatus` 是 InProgress/Completed/Failed：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/item.rs:1137`。
- `SubAgentActivityKind` 是 Started/Interacted/Interrupted：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/item.rs:1168`。
- `CollabAgentState` 带 agent_id、thread_id、agent_path、status，completed/errored 可带 message：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/item.rs:1187`。
- `TerminalInteractionNotification` 带 thread_id、turn_id、item_id、process_id、stdin：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/item.rs:1396`。
- `CommandExecutionOutputDeltaNotification` 带 thread_id、turn_id、item_id、delta：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/item.rs:1408`。

AgentDashboard 已有相近承载点：

- `ConversationWaitingItemView` 字段是 `wait_id`、`gate_id`、`kind`、`source_ref`、`correlation_ref`、`status`、`source_label`、`preview`、`created_at`、`resolved_at`：`crates/agentdash-contracts/src/runtime/workflow.rs:1151`。
- `ConversationMailboxSnapshotView` 包含 `waiting_items`：`crates/agentdash-contracts/src/runtime/workflow.rs:1176`。
- waiting item API projection 在 `crates/agentdash-api/src/routes/lifecycle_agents.rs:1704`，字段拷贝函数在 `crates/agentdash-api/src/routes/lifecycle_agents.rs:1735`。
- `ExecutorRunRef` 当前有 `RuntimeSession`、`FunctionRun`、`HumanDecision`：`crates/agentdash-contracts/src/runtime/workflow.rs:611`。
- `RuntimeNodeState` 包含 `status`、`attempt`、`outputs`、`executor_run_ref`、`children`、timestamps、error、trace refs：`crates/agentdash-domain/src/workflow/value_objects/orchestration.rs:353`。
- `RuntimeNodeStatus` 有 Pending/Ready/Claiming/Running/Blocked/Completed/Failed/Cancelled/Skipped：`crates/agentdash-domain/src/workflow/value_objects/orchestration.rs:383`。
- `RuntimeTraceRef` 是 trace 引用，不是 ownership identity：`crates/agentdash-domain/src/workflow/value_objects/orchestration.rs:415`。
- `NodeStarted`、`NodeCompleted`、`NodeFailed` journal fact 在 `crates/agentdash-domain/src/workflow/value_objects/orchestration.rs:527`。
- Function runner 在 launch 时先写 `NodeStarted` + `ExecutorRunRef::FunctionRun`，再执行：`crates/agentdash-application-workflow/src/orchestration/executor_launcher.rs:245`。
- terminal path 写 `NodeCompleted`/`NodeFailed`：`crates/agentdash-application-workflow/src/orchestration/executor_launcher.rs:266`。
- blocked path 写 `NodeBlocked`：`crates/agentdash-application-workflow/src/orchestration/executor_launcher.rs:335`。
- 当前 BashExec 是同步 `.output()`，返回 exit_code/stdout/stderr/success：`crates/agentdash-infrastructure/src/function_runner.rs:65`。
- BashExec non-success 被映射成 `bash_exec_nonzero` 并带 exit_code/stdout/stderr detail：`crates/agentdash-application-workflow/src/orchestration/function_node_runner.rs:88`。
- BashExec outputs 映射为 success/exit_code/stdout/stderr：`crates/agentdash-application-workflow/src/orchestration/function_node_runner.rs:177`。

可参考部分：

- 控制工具返回小对象：spawn 只返回 id/name，wait 只返回 status/message/timed_out。
- 状态从事件或 watch channel 推导，而不是每个调用自己拼状态真相。
- output delta、terminal interaction、command execution item 分离：大输出走 delta/ref，item/status 只保留聚合摘要。
- parent/child work graph 只需要 edge 与 status，查询需要稳定排序。
- mailbox/steer 活动可以作为 wait 的统一唤醒源，wait 不负责消费所有内容，只告诉调用者“该读了/被打断了/超时了”。

### 5. 可借鉴与不得复制

可借鉴：

- `exec_command = start + first bounded read/yield` 的交互闭环。启动后立即返回一段输出；若仍运行，返回稳定句柄供后续 read/write/wait/terminate。
- running/completed 二态返回：running 有 handle、无 exit_code；completed 有 exit_code、无 live handle。
- 区分 wait timeout 与 execution timeout。wait/yield timeout 只把控制权还给调用者，process 继续；execution timeout 才终止或失败 process。
- wait 返回小对象：`timed_out`、status/message、result refs/preview，不直接返回完整产物。
- 输出采用 bounded preview + delta/ref，避免把大 stdout/stderr 塞进 event 或 mailbox。
- 事件化 terminal interaction 与 output delta，便于 UI 和 runtime trace 解耦。
- 状态由 journal/event/watch projection 生成，终态应幂等可恢复。

不得复制到 AgentDashboard：

- 不复制 Codex Thread 作为产品领域主键。AgentDashboard 已有 AgentRun/LifecycleRun/OrchestrationInstance/RuntimeNodeState，spec 要求 runtime node 坐标使用 `orchestration_id + node_path + attempt`。
- 不复制 Codex AgentPath 作为 agent identity。AgentDashboard 应使用自己的 AgentRun、agent id、Lifecycle node coordinate 与 mailbox/gate。
- 不复制 Codex session API/runtime dependency，包括外部 `/sessions/*`、`~/.codex/sessions`、Codex app-server Thread/Turn lifecycle。
- 不把 app-server `process_id`/`process_handle` 的 connection-scoped 语义照搬为 durable exec id。Codex app-server 明确 output delta 是 connection scoped，originating connection 关闭会终止 process：`references/codex/codex-rs/app-server-protocol/src/protocol/v2/command_exec.rs:195`。
- 不把 `RuntimeSession` 当 exec/wait owner。AgentDashboard spec 明确 RuntimeSession 是 delivery/trace evidence，product control-plane 应落在 AgentRun/Lifecycle。
- 不复制 Codex 的模型训练命名 `session_id`。AgentDashboard 工具面应使用 `exec_id`、`wait_id`、`run_id`、`node coordinate` 等自有语义。

### 6. AgentDashboard Minimal Tool Surface Recommendation

建议最小闭环分成一个通用 `wait` 与一组 durable exec tools。核心原则是：`wait_id/exec_id` 属于 AgentDashboard application/runtime domain，process PID 或 transport session 只是后端私有细节。

#### 通用 wait

建议工具：

- `wait`

建议参数：

- `wait_ids?: string[]`
- `kinds?: ("exec" | "workflow" | "subagent" | "human" | "mailbox")[]`
- `timeout_ms: number`
- `max_items?: number`
- `include?: "status" | "summary"`

建议返回：

```json
{
  "status": "ready | timed_out | cancelled | not_found",
  "timed_out": false,
  "items": [
    {
      "wait_id": "wait_...",
      "kind": "exec",
      "status": "running | blocked | completed | failed | cancelled",
      "source_ref": "...",
      "correlation_ref": "...",
      "preview": "...",
      "result_refs": ["..."]
    }
  ],
  "next_poll_after_ms": 250
}
```

实现建议：

- wait 监听 durable gate/activity/mailbox projection，而不是监听 websocket connection 或 RuntimeSession。
- wait 可被 exec terminal、human gate resolve、subagent mailbox、workflow node blocked/completed 共同唤醒。
- wait 的返回只做 summary/ref；完整 stdout/stderr、artifact、agent message 由 `exec_read`、mailbox read 或 lifecycle VFS/ref 读取。
- wait timeout 只表示“这次等待没有新活动”，不能隐式 cancel 后台工作。

#### Exec tools

建议工具：

- `exec_start`
- `exec_read`
- `exec_wait`
- `exec_write`
- `exec_terminate`
- `exec_status`

`exec_start` 建议：

```json
{
  "command": ["pnpm", "test"],
  "cwd": "...",
  "env": {},
  "tty": false,
  "owner": {
    "agent_run_id": "...",
    "orchestration_id": "...",
    "node_path": "...",
    "attempt": 1
  },
  "yield_ms": 10000,
  "max_tokens": 6000,
  "execution_timeout_ms": null
}
```

返回：

```json
{
  "exec_id": "exec_...",
  "status": "running | completed | failed",
  "exit_code": null,
  "chunk_id": "000001",
  "cursor": "out_...",
  "output_preview": "...",
  "output_ref": "vfs://...",
  "started_at": "...",
  "completed_at": null
}
```

`exec_read` 建议：

```json
{
  "exec_id": "exec_...",
  "cursor": "out_...",
  "max_tokens": 6000
}
```

返回：

```json
{
  "exec_id": "exec_...",
  "status": "running | completed | failed | cancelled",
  "exit_code": null,
  "cursor": "out_next_...",
  "stdout": "...",
  "stderr": "...",
  "combined_preview": "...",
  "truncated": false,
  "output_ref": "vfs://..."
}
```

`exec_wait` 建议：

```json
{
  "exec_id": "exec_...",
  "cursor": "out_...",
  "timeout_ms": 10000,
  "max_tokens": 6000
}
```

返回与 `exec_read` 相同，并增加：

```json
{
  "timed_out": false
}
```

`exec_write` 建议：

```json
{
  "exec_id": "exec_...",
  "stdin": "...",
  "close_stdin": false
}
```

返回：

```json
{
  "exec_id": "exec_...",
  "accepted": true,
  "status": "running"
}
```

`exec_terminate` 建议：

```json
{
  "exec_id": "exec_..."
}
```

返回：

```json
{
  "exec_id": "exec_...",
  "previous_status": "running",
  "status": "cancelled | completed | failed",
  "exit_code": null
}
```

`exec_status` 建议：

```json
{
  "exec_id": "exec_..."
}
```

返回：

```json
{
  "exec_id": "exec_...",
  "status": "running | completed | failed | cancelled",
  "exit_code": null,
  "started_at": "...",
  "completed_at": null,
  "cwd": "...",
  "command_preview": "pnpm test",
  "latest_cursor": "out_...",
  "output_refs": ["vfs://..."],
  "owner_coordinate": {
    "agent_run_id": "...",
    "orchestration_id": "...",
    "node_path": "...",
    "attempt": 1
  }
}
```

与 AgentDashboard 现有模型的闭合方式：

- `exec_start` 对 workflow node 应写 `NodeStarted`，并挂接一个 future `ExecutorRunRef::FunctionRun` 或更准确的 `ExecutorRunRef::ExecRun/EffectInvocation`。当前 `FunctionRun` 已有先 started 后 terminal 的路径可参考：`crates/agentdash-application-workflow/src/orchestration/executor_launcher.rs:245`。
- `exec_start` 如果很快完成，可以一次性写 terminal event；如果仍运行，创建 durable exec activity/gate，并投影到 `ConversationWaitingItemView.kind="exec"`。
- stdout/stderr 使用 cursor/ref 读取，不能仅依赖内存 drain buffer。Codex 的 implicit drain 适合单 runtime loop；AgentDashboard 要跨浏览器刷新、后端重启、agent resume，应使用 durable cursor。
- `exec_wait` 与通用 `wait` 不重复：`exec_wait` 是单 exec 的“等输出或终态并读一段”；通用 `wait` 是多活动聚合唤醒。
- `exec_write` 只允许写支持 stdin/tty 的 running exec；对非交互 exec 返回 typed error，不隐式吞掉。
- `exec_terminate` 应幂等：已 terminal 返回 current terminal status，不报错破坏闭环。
- terminal 事件必须同时更新 exec status、runtime node status、mailbox/gate wake；否则 agent 会出现“命令完成但等待项不醒”的断链。

### Conclusions

Codex 对 AgentDashboard 最有价值的不是 runtime/session 体系，而是三个能力模型：

1. Command lifecycle：启动后先读一次，running 返回 handle，completed 返回 exit code；后续 read/write/wait/terminate/status 都围绕同一个 durable handle 闭合。
2. Wait lifecycle：wait 是活动唤醒器，不是大结果传输通道；它返回 timeout/status/summary/ref，真实内容通过对应 reader 或 projection 获取。
3. Event/projection lifecycle：output delta、terminal interaction、agent status、mailbox activity 都应事件化，并投影成 UI/agent 可读的小结构。

AgentDashboard 应把这些思想落到自己的 AgentRun/Lifecycle/RuntimeNode/mailbox/gate 上，而不是接入或仿造 Codex Thread/Session/AgentPath。

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回当前 task 为 none；本研究按用户显式给出的 `.trellis/tasks/07-03-waitable-activity-exec-closure` 写入，未猜测其他目录。
- 未访问外部网络文档；“External references” 仅为仓库内 `references/codex` 本地快照与 AgentDashboard `.trellis/spec/`。如果需要精确对应 Codex 上游 commit，需要另行记录 `references/codex` 的来源版本。
- Codex model tool 层没有完整同名 `exec_start/read/wait/terminate/status` 工具面；相关能力分散在 `exec_command`、`write_stdin`、code-mode `wait`、process manager、app-server protocol 中。本报告按能力拆解，不表示 Codex 原样暴露了这些工具。
- app-server protocol 的 `process_id/process_handle` 是 connection-scoped，且连接关闭可能终止 process；这对 AgentDashboard durable waitable activity 是风险点，只能参考事件/返回形状，不能复制生命周期语义。
- 当前 AgentDashboard `BashExec` 是同步 `.output()`；升级为 waitable exec 会涉及 durable exec store、cursor、terminal materialization、cancel/terminate、mailbox wake、权限校验，不是只改 contract 字段即可完成。
- 需要在后续设计中明确 execution timeout 与 wait/yield timeout 的不同错误码和 UI 文案，否则 agent 可能把“等待超时”误判为“命令失败”。
