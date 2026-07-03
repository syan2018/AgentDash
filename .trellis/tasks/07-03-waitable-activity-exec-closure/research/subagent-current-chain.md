# Research: subagent current chain

- Query: 映射 runtime tool、main:// exec/VFS、local shell session、relay terminal/process、companion/subagent/human wait、AgentRun mailbox、LifecycleGate、ContextFrame、前端 projection 的完整链路，支撑统一 wait module 设计。
- Scope: internal
- Date: 2026-07-03

## Findings

### 0. 任务与规范上下文

- 当前 Trellis active task 命令结果为 none：`python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)` / `Source: none`。本报告按用户显式给出的任务目录 `.trellis/tasks/07-03-waitable-activity-exec-closure/` 写入。
- 已阅读 `.trellis/workflow.md`，本次工作定位为只读研究 subagent，产物必须持久化在任务目录 `research/`。
- 相关规格：
  - `.trellis/spec/backend/index.md`：后端领域边界与 session/runtime/workflow 入口索引。
  - `.trellis/spec/backend/session/agentrun-mailbox.md`：AgentRun mailbox 是 durable delivery / wake surface，不是 wait fact store。
  - `.trellis/spec/backend/session/execution-context-frames.md`：ContextFrame 是运行时向 Agent 声明环境与工具事实的正式渠道。
  - `.trellis/spec/backend/session/runtime-execution-state.md`：runtime session state、turn boundary、mailbox wake 与终端状态变更关系。
  - `.trellis/spec/backend/vfs/architecture.md`：VFS bootstrap 只负责 VFS provider，session bootstrap 注入 runtime tool provider。
  - `.trellis/spec/backend/workflow/activity-lifecycle.md`：LifecycleGate 是等待/阻塞事实的 owner，projection 从 open gates 得到 waiting_items。
  - `.trellis/spec/frontend/workflow-activity-lifecycle.md`：前端只投影 mailbox/gate/terminal 事实，不应承载 Agent 命令构造语义。

### 1. Agent runtime tool catalog 在哪里组装，工具如何对 Agent 可见

结论：runtime tool catalog 的 composition root 在 session bootstrap。各领域 provider 产出 `AgentTool`，launch preparation 把工具实例和 tool schema 写入 `ExecutionTurnFrame`，connector 再把 assembled tools 暴露给模型/Agent。

关键文件：

- `crates/agentdash-api/src/bootstrap/session.rs:233`：构造 session runtime tool provider。
- `crates/agentdash-api/src/bootstrap/session.rs:288`：`RuntimeSessionBuilder` 注入 `.with_runtime_tool_provider(runtime_tool_provider)`。
- `crates/agentdash-api/src/bootstrap/session.rs:432`：`build_session_runtime_tool_composer` 是工具 catalog 组装入口。
- `crates/agentdash-api/src/bootstrap/session.rs:468` - `crates/agentdash-api/src/bootstrap/session.rs:473`：`SessionRuntimeToolComposer::new(vec![vfs_provider, workflow_provider, collaboration_provider, task_provider, workspace_module_provider])`。
- `crates/agentdash-application/src/runtime_tools/provider.rs:45` - `crates/agentdash-application/src/runtime_tools/provider.rs:69`：`SessionRuntimeToolComposer` 持有 providers 并循环调用 `provider.build_tools(context).await?`。
- `crates/agentdash-application/src/runtime_tools/provider.rs:70` - `crates/agentdash-application/src/runtime_tools/provider.rs:75`：重复 tool name 会报错，说明 catalog 是扁平命名空间。
- `crates/agentdash-spi/src/connector/mod.rs:933` - `crates/agentdash-spi/src/connector/mod.rs:934`：`RuntimeToolProvider` trait 定义 `build_tools(&self, context: &ExecutionContext)`。
- `crates/agentdash-application-runtime-session/src/session/runtime_builder.rs:73` - `crates/agentdash-application-runtime-session/src/session/runtime_builder.rs:74`：builder 接收 runtime tool provider。
- `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:121` - `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:123`：`assemble_tool_surface` 后设置 `context.turn.assembled_tools = assembled_tool_surface.tools` 与 `assembled_tool_schemas`。
- `crates/agentdash-spi/src/connector/mod.rs:235` - `crates/agentdash-spi/src/connector/mod.rs:244`：`ExecutionTurnFrame` 包含 `context_frames`、`context_delivery_plan`、`assembled_tools`。
- `crates/agentdash-spi/src/hooks/mod.rs:582` - `crates/agentdash-spi/src/hooks/mod.rs:584`：ContextFrame 可投递 `ToolSchemaDelta { added_tools }`。
- `crates/agentdash-spi/src/hooks/mod.rs:731` - `crates/agentdash-spi/src/hooks/mod.rs:734`：`RuntimeToolSchemaEntry` 包含 name、description、parameters_schema。
- `crates/agentdash-application-runtime-session/src/session/hub/runtime_context_transition.rs:51`：initial capability state frame 使用 tool schemas。
- `crates/agentdash-application-runtime-session/src/session/hub/runtime_context_transition.rs:578` - `crates/agentdash-application-runtime-session/src/session/hub/runtime_context_transition.rs:613`：测试断言 live context frame 包含 tool schema delta。
- `crates/agentdash-executor/src/connectors/pi_agent/connector.rs:674`、`crates/agentdash-executor/src/connectors/pi_agent/connector.rs:757`、`crates/agentdash-executor/src/connectors/pi_agent/connector.rs:803`：PiAgent connector 消费 `context.turn.assembled_tools`。

设计含义：

- 要让统一 wait module 对 Agent 可见，应新增 runtime tool provider 或扩展现有 VFS/runtime provider，让 wait tools 进入 `SessionRuntimeToolComposer`。
- 如果只在 relay/local 层新增 RPC 而不接入 runtime tool catalog，Agent 无法主动调用。

### 2. main:// / VFS exec 在哪里实现，running 时返回什么，session_id/handle 如何暴露

结论：`main://` 是 VFS mount URI/cwd 语义，`shell_exec` 是当前 Agent 可见的 VFS exec 工具。它通过 `VfsService::exec_with_policy` 分发到 mount provider；relay FS provider 再发 `CommandToolShellExec` 给 local。长命令 running 时返回 `state: running`，并把 `session_id`、`terminal_id`、`next_seq` 同时写入文本和 structured details，但当前没有更高层 Agent-visible handle 类型。

工具定义与 VFS 分发：

- `crates/agentdash-application-vfs/src/tools/factory.rs:15` - `crates/agentdash-application-vfs/src/tools/factory.rs:18`：`VfsToolFactory` 持有 `VfsService`、materialization service、shell output registry。
- `crates/agentdash-application-vfs/src/tools/factory.rs:46`：`build_tools` 入口。
- `crates/agentdash-application-vfs/src/tools/factory.rs:116` - `crates/agentdash-application-vfs/src/tools/factory.rs:124`：Execute cluster / `shell_exec` capability 下构造 `ShellExecTool`。
- `crates/agentdash-application-vfs/src/tools/factory.rs:127` - `crates/agentdash-application-vfs/src/tools/factory.rs:128`：materialization context 注入 `session_id`、`turn_id`。
- `crates/agentdash-application-vfs/src/tools/factory.rs:143` - `crates/agentdash-application-vfs/src/tools/factory.rs:149`：工具 input 包含 shared_vfs、overlay、identity、session_id、turn_id、flow。
- `crates/agentdash-application-vfs/src/tools/fs/shell.rs:31` - `crates/agentdash-application-vfs/src/tools/fs/shell.rs:37`：`ShellExecTool` 字段包含 `session_id`、`turn_id`。
- `crates/agentdash-application-vfs/src/tools/fs/shell.rs:88` - `crates/agentdash-application-vfs/src/tools/fs/shell.rs:95`：参数包含 `cwd`、`command`、`timeout_secs`；`cwd` 文档说明 `mount_id://relative/path`，省略或 `platform://` 表示 platform shell。
- `crates/agentdash-application-vfs/src/tools/fs/shell.rs:98` - `crates/agentdash-application-vfs/src/tools/fs/shell.rs:119`：`AgentTool for ShellExecTool`，tool name 为 `shell_exec`，description 明确 long-running commands return a background session after initial yield。
- `crates/agentdash-application-vfs/src/tools/fs/shell.rs:119` - `crates/agentdash-application-vfs/src/tools/fs/shell.rs:131`：解析参数并处理 platform shell / VFS shell。
- `crates/agentdash-application-vfs/src/tools/fs/shell.rs:269` - `crates/agentdash-application-vfs/src/tools/fs/shell.rs:281`：调用 `self.service.exec_with_policy(... ExecRequest { mount_id, cwd, command, timeout_ms, streaming_call_id })`。
- `crates/agentdash-application-vfs/src/tools/fs/shell.rs:502` - `crates/agentdash-application-vfs/src/tools/fs/shell.rs:512`：拒绝未 materialize 的 VFS URI 出现在 host shell command 中；`main://` 只能作为 VFS cwd/mount 或被 materialization 改写后执行。
- `crates/agentdash-application-vfs/src/materialization.rs:48` - `crates/agentdash-application-vfs/src/materialization.rs:54`：shell command materialization 会改写命令与 exec cwd。
- `crates/agentdash-application-vfs/src/materialization.rs:78` - `crates/agentdash-application-vfs/src/materialization.rs:90`：解析 exec mount 并用 access policy 校验 Exec 权限。
- `crates/agentdash-application-vfs/src/service.rs:1144` - `crates/agentdash-application-vfs/src/service.rs:1153`：`VfsService::exec` / `exec_with_policy`。
- `crates/agentdash-application-vfs/src/service.rs:1154` - `crates/agentdash-application-vfs/src/service.rs:1164`：按 `MountCapability::Exec` 解析 provider 并规范化 dispatch path。
- `crates/agentdash-spi/src/platform/mount.rs:314` - `crates/agentdash-spi/src/platform/mount.rs:323`：`ExecRequest` 字段为 `mount_id`、`cwd`、`command`、`timeout_ms`、`streaming_call_id`。
- `crates/agentdash-spi/src/platform/mount.rs:326` - `crates/agentdash-spi/src/platform/mount.rs:334`：`ExecResult` 字段包含 `state`、`exit_code`、`stdout`、`stderr`、`pty`、`session_id`、`terminal_id`、`next_seq`、truncation。

relay/local dispatch：

- `crates/agentdash-api/src/mount_providers/relay_fs.rs:592` - `crates/agentdash-api/src/mount_providers/relay_fs.rs:597`：relay FS mount provider 实现 `exec`。
- `crates/agentdash-api/src/mount_providers/relay_fs.rs:612` - `crates/agentdash-api/src/mount_providers/relay_fs.rs:629`：发送 `RelayMessage::CommandToolShellExec`，payload 带 `call_id`、`command`、`mount_root_ref`、`cwd`、`timeout_ms`、`yield_time_ms`、`tty`。
- `crates/agentdash-api/src/mount_providers/relay_fs.rs:635` - `crates/agentdash-api/src/mount_providers/relay_fs.rs:645`：收到 `ResponseToolShellExec` 后映射为 `ExecResult`，把 `payload.session_id` 暴露为 `session_id`。
- `crates/agentdash-relay/src/protocol.rs:173`：relay protocol command `CommandToolShellExec`。
- `crates/agentdash-relay/src/protocol.rs:347`：relay protocol response `ResponseToolShellExec`。
- `crates/agentdash-relay/src/protocol/tool.rs:55`：`ToolShellExecPayload` 定义。
- `crates/agentdash-relay/src/protocol/tool.rs:297`：`ToolShellExecResponse` 定义。
- `crates/agentdash-relay/src/protocol/tool.rs:202`：`ToolShellSessionState` 定义。
- `crates/agentdash-relay/src/protocol.rs:1189` - `crates/agentdash-relay/src/protocol.rs:1204`：测试覆盖 running response 携带 `session_id`、`terminal_id`、`state: Running`、`exit_code: None`、chunks、`next_seq`。

running 返回格式：

- `crates/agentdash-application-vfs/src/tools/fs/shell.rs:312` - `crates/agentdash-application-vfs/src/tools/fs/shell.rs:319`：`shell_exec` 返回文本来自 `shell_exec_result_text`，并包含 details。
- `crates/agentdash-application-vfs/src/tools/fs/shell.rs:393` - `crates/agentdash-application-vfs/src/tools/fs/shell.rs:470`：文本输出包含 `command`、`executed_command`、`cwd`、`state`、`exit_code`、`session_id`、`terminal_id`、stdout/stderr。
- `crates/agentdash-application-vfs/src/tools/fs/shell.rs:473` - `crates/agentdash-application-vfs/src/tools/fs/shell.rs:494`：details JSON 包含 `{ type: "shell_exec", original_command, executed_command, state, exit_code, session_id, terminal_id, next_seq, truncated, ... }`。

风险：

- `session_id` 目前只是 shell/local session id，不是统一 wait handle；Agent 能看到 id，但没有 schema 说明它能用什么工具继续操作这个 id。
- `terminal_id` 同时作为前端 terminal projection key；不要直接把前端 terminal id 当作 wait activity owner。

### 3. 当前是否有 Agent 可见的 read/wait/input/terminate/status；如果没有，断点在哪里

结论：没有完整 Agent-visible read/wait/input/terminate/status。系统内部已经有 relay protocol 和 local primitive，但未进入 runtime tool catalog。断点在 `VfsToolFactory` 只注册 `shell_exec`，以及 `SessionRuntimeToolComposer` 没有 wait/session follow-up provider。

Agent-visible 侧：

- `crates/agentdash-application-vfs/src/tools/factory.rs:116` - `crates/agentdash-application-vfs/src/tools/factory.rs:136`：execute cluster 只 push `ShellExecTool`，没有 push `shell_read`、`shell_wait`、`shell_input`、`shell_terminate`、`shell_status`。
- `crates/agentdash-api/src/bootstrap/session.rs:468` - `crates/agentdash-api/src/bootstrap/session.rs:473`：composer 的 provider 列表没有独立 wait provider。

内部 primitive 侧：

- `crates/agentdash-relay/src/protocol.rs:179`：relay command `CommandToolShellRead`。
- `crates/agentdash-relay/src/protocol.rs:185`：relay command `CommandToolShellInput`。
- `crates/agentdash-relay/src/protocol.rs:191`：relay command `CommandToolShellTerminate`。
- `crates/agentdash-relay/src/protocol.rs:356`：relay response `ResponseToolShellRead`。
- `crates/agentdash-relay/src/protocol/tool.rs:83`：`ToolShellReadPayload`。
- `crates/agentdash-relay/src/protocol/tool.rs:95`：`ToolShellInputPayload`。
- `crates/agentdash-relay/src/protocol/tool.rs:107`：`ToolShellTerminatePayload`。
- `crates/agentdash-relay/src/protocol/tool.rs:319`：`ToolShellReadResponse`。
- `crates/agentdash-local/src/shell_session_manager.rs:327` - `crates/agentdash-local/src/shell_session_manager.rs:333`：local `read_session(session_id, after_seq, wait_ms, max_bytes)`。
- `crates/agentdash-local/src/shell_session_manager.rs:365`：local `input_shell(payload)`。
- `crates/agentdash-local/src/shell_session_manager.rs:407` - `crates/agentdash-local/src/shell_session_manager.rs:410`：local `terminate_shell(payload)`。
- `crates/agentdash-local/src/handlers/tool_calls.rs:236`、`crates/agentdash-local/src/handlers/tool_calls.rs:263`、`crates/agentdash-local/src/handlers/tool_calls.rs:291`、`crates/agentdash-local/src/handlers/tool_calls.rs:310`：local WS tool call handler 接收 exec/read/input/terminate。

断点：

- Agent-visible tool surface 停在 `shell_exec`。`shell_exec` 的 running result 给出了 `session_id`/`next_seq`，但 follow-up read/wait/input/terminate/status 没有被注册为 runtime tools。
- relay/local 层已有 session continuation 能力，但它是 API/local 内部协议，不是 Agent contract。

### 4. local shell session 如何保存输出、状态、exit code，是否已有内部 read/wait/input/terminate primitive

结论：local shell session 以 `ShellSessionManager` 内存表保存 process handle、state、exit_code、retained output buffer、notify 与时间戳。已有内部 read/input/terminate primitive，并通过 `read_session(... wait_ms ...)` 支持有限 wait/read 合并语义。

状态与输出保存：

- `crates/agentdash-local/src/shell_session_manager.rs:24` - `crates/agentdash-local/src/shell_session_manager.rs:29`：`ShellSessionManager` 持有 session table、`ToolExecutor`、event sender。
- `crates/agentdash-local/src/shell_session_manager.rs:36` - `crates/agentdash-local/src/shell_session_manager.rs:47`：`ShellSession` 字段包括 `session_id`、`call_id`、`terminal_id`、`state`、`exit_code`、`handle: Arc<ProcessHandle>`、retained buffer、live output budget、notify、timestamps。
- `crates/agentdash-local/src/shell_session_manager.rs:146` - `crates/agentdash-local/src/shell_session_manager.rs:153`：retained output buffer 保存 head/tail、omitted bytes/chunks、`next_seq`。
- `crates/agentdash-local/src/shell_session_manager.rs:164` - `crates/agentdash-local/src/shell_session_manager.rs:167`：push 输出时分配 seq 并递增 `next_seq`。
- `crates/agentdash-local/src/shell_session_manager.rs:522` - `crates/agentdash-local/src/shell_session_manager.rs:529`：新 session 初始 `state: Running`、`exit_code: None`。
- `crates/agentdash-local/src/shell_session_manager.rs:544` - `crates/agentdash-local/src/shell_session_manager.rs:552`：启动后 emit terminal running state event。
- `crates/agentdash-local/src/shell_session_manager.rs:644` - `crates/agentdash-local/src/shell_session_manager.rs:663`：process exit 时更新 `state` 为 completed/failed，写入 `exit_code`、completed_at，并 notify。
- `crates/agentdash-local/src/shell_session_manager.rs:664` - `crates/agentdash-local/src/shell_session_manager.rs:672`：exit 后 emit terminal `Exited` state event。
- `crates/agentdash-local/src/shell_session_manager.rs:689` - `crates/agentdash-local/src/shell_session_manager.rs:706`：timeout 后 request terminate，state 变 `TimedOut`，terminal state 为 `Killed`。
- `crates/agentdash-local/src/shell_session_manager.rs:761` - `crates/agentdash-local/src/shell_session_manager.rs:768`：`shell_read_snapshot` 返回 state、exit_code、chunks、next_seq、truncation。
- `crates/agentdash-local/src/shell_session_manager.rs:786` - `crates/agentdash-local/src/shell_session_manager.rs:795`：terminal shell states 为 Completed/Failed/TimedOut/Killed/Lost/Closed。

已有 primitive：

- `crates/agentdash-local/src/shell_session_manager.rs:248` - `crates/agentdash-local/src/shell_session_manager.rs:251`：`start_shell(payload)`。
- `crates/agentdash-local/src/shell_session_manager.rs:282` - `crates/agentdash-local/src/shell_session_manager.rs:293`：start response 返回 `session_id`、`terminal_id`、state、exit_code、stdout/stderr、chunks、`next_seq`、truncation。
- `crates/agentdash-local/src/shell_session_manager.rs:327` - `crates/agentdash-local/src/shell_session_manager.rs:333`：`read_session` 支持 `after_seq`、`wait_ms`、`max_bytes`。
- `crates/agentdash-local/src/shell_session_manager.rs:365`：`input_shell(payload)`。
- `crates/agentdash-local/src/shell_session_manager.rs:407` - `crates/agentdash-local/src/shell_session_manager.rs:459`：`terminate_shell`，unknown session 返回 Closed；running 会 request terminate、设置 Killed、notify 并 emit state changed。
- `crates/agentdash-local/src/shell_session_manager.rs:892` - `crates/agentdash-local/src/shell_session_manager.rs:909`：测试 helper `wait_until_terminal` 循环调用 `read_session(... wait_ms=5000)`。
- `crates/agentdash-local/src/shell_session_manager.rs:915` - `crates/agentdash-local/src/shell_session_manager.rs:958`：测试覆盖 long command start running，随后 read completed、exit_code 为 0。
- `crates/agentdash-local/src/shell_session_manager.rs:961` - `crates/agentdash-local/src/shell_session_manager.rs:982`：测试覆盖 stdin input 与 output read。

relay terminal / process projection：

- `crates/agentdash-local/src/ws_client.rs:376` - `crates/agentdash-local/src/ws_client.rs:384`：background command handler spawn，shell exec 不阻塞 WS client 主循环。
- `crates/agentdash-local/src/ws_client.rs:597` - `crates/agentdash-local/src/ws_client.rs:606`：测试 `shell_exec_is_handled_in_background`。
- `crates/agentdash-agent-protocol/src/backbone/platform.rs:37`：Backbone `TerminalOutput { terminal_id, data }`。
- `crates/agentdash-agent-protocol/src/backbone/platform.rs:40` - `crates/agentdash-agent-protocol/src/backbone/platform.rs:46`：Backbone `TerminalStateChanged { terminal_id, state, exit_code, message }`。
- `crates/agentdash-api/src/relay/ws_handler.rs:461` - `crates/agentdash-api/src/relay/ws_handler.rs:465`：`EventToolShellOutput` route 到 `shell_output_registry`。
- `crates/agentdash-api/src/relay/ws_handler.rs:474` - `crates/agentdash-api/src/relay/ws_handler.rs:499`：`EventTerminalOutput` 转为 Backbone `PlatformEvent::TerminalOutput`。
- `crates/agentdash-api/src/relay/ws_handler.rs:524` - `crates/agentdash-api/src/relay/ws_handler.rs:560`：`EventTerminalStateChanged` 更新 terminal cache 并发 Backbone `TerminalStateChanged`。
- `crates/agentdash-relay/src/shell_output_registry.rs:8` - `crates/agentdash-relay/src/shell_output_registry.rs:12`：注释说明 `ShellExecTool` 注册 call_id channel，WS handler route streaming output，tool callback 消费更新。
- `crates/agentdash-relay/src/shell_output_registry.rs:25` - `crates/agentdash-relay/src/shell_output_registry.rs:35`：register/unregister/route。

风险：

- local shell session 目前在 local backend 进程内存中；统一 wait module 如果承诺 durable wait，需要定义 local backend 重启后的 `lost`/recoverability 语义。
- retained output 是有界 buffer，wait/read API 必须显式暴露 truncation 与 next_seq，不能假设可以回放完整 stdout。

### 5. companion/subagent/human 的 wait=true 当前在哪里私有轮询，gate 如何创建/resolve

结论：companion/subagent/human wait 目前在 `companion/tools.rs` 内私有轮询 `LifecycleGateRepository`，不是统一 wait 模块。gate 创建在 `LifecycleGateResolver::open_companion_gate`，resolve 在 companion response / human response 路径。

私有轮询：

- `crates/agentdash-application/src/companion/tools.rs:237` - `crates/agentdash-application/src/companion/tools.rs:248`：`CompanionGateWaitOutcome` 与 `wait_for_lifecycle_gate_resolution(...)` 定义。
- `crates/agentdash-application/src/companion/tools.rs:250` - `crates/agentdash-application/src/companion/tools.rs:274`：poll loop：读取 gate，若 resolved 返回 payload，deadline 到则 timeout，否则 sleep poll interval 或 cancel。
- `crates/agentdash-application/src/companion/tools.rs:1066` - `crates/agentdash-application/src/companion/tools.rs:1072`：companion `wait=true` 取得 `dispatch_result.gate_ref` 后调用 `poll_gate_until_resolved`。
- `crates/agentdash-application/src/companion/tools.rs:1073` - `crates/agentdash-application/src/companion/tools.rs:1078`：timeout 返回文本包含 `status: timed_out`、gate_ref、child ids。
- `crates/agentdash-application/src/companion/tools.rs:1105` - `crates/agentdash-application/src/companion/tools.rs:1112`：resolved wait 返回 status、summary、bounded preview、gate_ref。
- `crates/agentdash-application/src/companion/tools.rs:1190` - `crates/agentdash-application/src/companion/tools.rs:1201`：`poll_gate_until_resolved` 委托到私有 `wait_for_lifecycle_gate_resolution`。
- `crates/agentdash-application/src/companion/tools.rs:1427` - `crates/agentdash-application/src/companion/tools.rs:1432`：human `wait=true` 也调用同一私有 wait function。
- `crates/agentdash-application/src/companion/tools.rs:1463` - `crates/agentdash-application/src/companion/tools.rs:1470`：human resolved wait 返回 status/summary/gate_ref。

gate 创建：

- `crates/agentdash-application/src/companion/tools.rs:1376` - `crates/agentdash-application/src/companion/tools.rs:1384`：human request gate metadata 包含 `session_id`、`turn_id`、`request_type`；`wait=true` 时 gate_kind 为 `companion_wait`，否则为 `companion_human_request`。
- `crates/agentdash-application/src/companion/tools.rs:1386` - `crates/agentdash-application/src/companion/tools.rs:1388`：通过 `LifecycleGateResolver::open_companion_gate(OpenCompanionGateCommand { ... })` 创建 gate。
- `crates/agentdash-application-workflow/src/gate/resolver.rs:72` - `crates/agentdash-application-workflow/src/gate/resolver.rs:83`：`open_companion_gate` 要求 `gate_kind` 以 `companion_` 开头，并创建 `LifecycleGate::open(...)`。
- `crates/agentdash-application-workflow/src/gate/resolver.rs:96` - `crates/agentdash-application-workflow/src/gate/resolver.rs:105`：workflow human gate 使用 `WORKFLOW_HUMAN_GATE_KIND`。
- `crates/agentdash-application-workflow/src/gate/resolver.rs:200` - `crates/agentdash-application-workflow/src/gate/resolver.rs:209`：打开 parent request gate。

gate resolve：

- `crates/agentdash-application-workflow/src/gate/resolver.rs:154` - `crates/agentdash-application-workflow/src/gate/resolver.rs:160`：`respond_human` 读取 open gate metadata/request_type。
- `crates/agentdash-application/src/companion/tools.rs:1977` - `crates/agentdash-application/src/companion/tools.rs:1984`：respond companion request 时 resolve LifecycleGate，details 标记 `mode: "resolve_gate"`、`gate_id`。
- `crates/agentdash-domain/src/workflow/repository.rs:118` - `crates/agentdash-domain/src/workflow/repository.rs:122`：`LifecycleGateRepository` trait 提供 `create`、`get`、`list_open_for_agent`、`update`。

风险：

- wait=true 的 tool implementation 自己 poll gate，exec wait/read 又在 local shell manager。等待语义分散，超时、取消、预览截断、wake/dedupe 很容易不一致。
- `LifecycleGate` 已经是 companion/human/subagent wait fact 的 source of truth；统一 wait module 应复用 gate repository，而不是把这些 wait 状态迁移进 mailbox。

### 6. AgentRun mailbox 如何存储/dedupe/deliver/notify，和 wait module 的关系

结论：AgentRun mailbox 是 durable delivery queue 和 wake surface。它存储消息 envelope、payload preview、claim/lease、delivery policy 与 source identity；dedupe 通过 `source_dedup_key`；delivery 由 scheduler claim 并 dispatch；notify 通过 Backbone `MailboxStateChanged`。wait module 不应把 mailbox 当状态表，应在 wait resolve 时写入 deduped wake/result envelope。

领域模型与 repository：

- `crates/agentdash-domain/src/agent_run_mailbox/mod.rs:353` - `crates/agentdash-domain/src/agent_run_mailbox/mod.rs:374`：`AgentRunMailboxMessage` 字段包含 run_id、agent_id、runtime_session_id、origin/source/delivery/barrier/drain_mode/status/priority/order_key/source_dedup_key、turn refs、claim token/lease、payload/preview/attempts。
- `crates/agentdash-domain/src/agent_run_mailbox/mod.rs:414` - `crates/agentdash-domain/src/agent_run_mailbox/mod.rs:422`：`AgentRunMailboxState` 保存 paused/pause reason 等状态。
- `crates/agentdash-domain/src/agent_run_mailbox/mod.rs:438` - `crates/agentdash-domain/src/agent_run_mailbox/mod.rs:503`：`AgentRunMailboxRepository` 方法包含 `create_message`、`create_message_idempotent`、`get_message`、`list_messages`、`claim_next`、`recover_expired_consuming`、`mark_message_status`、`update_message_policy`、`delete_message`、`cleanup_user_payload`、`pause_state`、`resume_state`。

Postgres 存储与 dedupe：

- `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:73`：表字段包含 `source_dedup_key`。
- `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:154` - `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:155`：`run_id, agent_id, source_dedup_key` 的 partial index。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:37` - `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:46`：`find_by_source_dedup`。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:100` - `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:101`：SQL column list 包含 source identity、source_dedup_key、claim_token、claim_expires_at、payload、preview。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:120` - `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:149`：insert mailbox message，绑定 `source_dedup_key`。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:167` - `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:187`：`create_message_idempotent` 先按 dedup key 查找，冲突后再查回已有消息。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:224` - `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:269`：`claim_next`。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:272` - `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:307`：`recover_expired_consuming`。

delivery / scheduling / notify：

- `crates/agentdash-application-agentrun/src/agent_run/mailbox/mod.rs:68` - `crates/agentdash-application-agentrun/src/agent_run/mailbox/mod.rs:75`：`AgentRunMailboxService` 持有 repos 与 runtime ports。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/commands.rs:140` - `crates/agentdash-application-agentrun/src/agent_run/mailbox/commands.rs:141`：`stable_source_dedup_key` 优先从 source identity 推导，否则用显式 dedup key。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/delivery.rs:95` - `crates/agentdash-application-agentrun/src/agent_run/mailbox/delivery.rs:98`：`accept_intake_message_for_target` 入口。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/delivery.rs:128`：计算 `source_dedup_key`。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/delivery.rs:238` - `crates/agentdash-application-agentrun/src/agent_run/mailbox/delivery.rs:248`：调用 `create_message_idempotent` 创建 intake message。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/delivery.rs:371` - `crates/agentdash-application-agentrun/src/agent_run/mailbox/delivery.rs:387`：另一条 source identity dedup create path。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:19` - `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:25`：`schedule(...)`。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:154` - `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:160`：从 runtime_session resolve control-plane target。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:230` - `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:236`：claim starts 并调用 repository `claim_next`。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:209` - `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:214`：Agent loop turn boundary schedule 后，如果有 outcomes 则 emit mailbox state changed。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:229` - `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:236`：发 Backbone `PlatformEvent::MailboxStateChanged { reason }`。
- `crates/agentdash-agent-protocol/src/backbone/platform.rs:50`：`MailboxStateChanged { reason }` 事件。
- `crates/agentdash-api/src/agent_run_mailbox.rs:68` - `crates/agentdash-api/src/agent_run_mailbox.rs:72`：terminal callback schedules `AgentRunTurnBoundary`。
- `crates/agentdash-api/src/agent_run_mailbox.rs:100`：failed terminal fallback 会 pause mailbox。
- `crates/agentdash-api/src/agent_run_mailbox.rs:118`：interrupted terminal fallback 会 pause mailbox。

companion wake dedupe：

- `crates/agentdash-application/src/companion/tools.rs:227`：`companion_wake_source_dedup_key`。
- `crates/agentdash-application/src/companion/tools.rs:327` - `crates/agentdash-application/src/companion/tools.rs:341`：companion wake 使用 source_dedup_key。
- `crates/agentdash-application/src/companion/tools.rs:479` - `crates/agentdash-application/src/companion/tools.rs:507`：wake message input 包含 `source_dedup_key` 并写入 mailbox command。

wait module 关系：

- mailbox 应接收 wait resolve 后的 result/wake envelope，例如 companion result、human response、exec completion wake。
- wait module 应保存或读取 wait fact：exec session state 来自 local shell/session adapter，companion/human/subagent 来自 `LifecycleGate`，activity registry 记录 owner 与 handle mapping。
- mailbox payload 应保持 bounded preview/result pointer；大 stdout、长 companion result 不应塞进 mailbox 作为唯一结果通道。

### 7. ContextFrame / Environment 应在哪里声明 Windows PowerShell shell contract

结论：Windows PowerShell shell contract 应在 backend Environment ContextFrame 中声明。当前已有对应实现，位置正确。前端只能渲染环境 frame，不能成为 Agent shell contract 的来源。

关键文件：

- `crates/agentdash-application-runtime-session/src/session/environment_context_frame.rs:7` - `crates/agentdash-application-runtime-session/src/session/environment_context_frame.rs:13`：`EnvironmentFrameInput` 包含 date_utc、platform、arch、model_id、executor、working_directory。
- `crates/agentdash-application-runtime-session/src/session/environment_context_frame.rs:17` - `crates/agentdash-application-runtime-session/src/session/environment_context_frame.rs:20`：`build_environment_context_frame`。
- `crates/agentdash-application-runtime-session/src/session/environment_context_frame.rs:55`：`WINDOWS_POWERSHELL_TEXT_OUTPUT_NOTE`，说明 Windows PowerShell output 有对象管道差异，需选择 string fields、`Write-Output` 或 dedicated file tools；interactive terminals 仍依赖真实 PTY/stdout bytes。
- `crates/agentdash-application-runtime-session/src/session/environment_context_frame.rs:62` - `crates/agentdash-application-runtime-session/src/session/environment_context_frame.rs:63`：frame kind 是 `environment`。
- `crates/agentdash-application-runtime-session/src/session/environment_context_frame.rs:82` - `crates/agentdash-application-runtime-session/src/session/environment_context_frame.rs:91`：summary 在 Windows 下包含 PowerShell text output note。
- `crates/agentdash-application-runtime-session/src/session/environment_context_frame.rs:103` - `crates/agentdash-application-runtime-session/src/session/environment_context_frame.rs:120`：rendered text 生成 `## Environment` 与 shell guidance。
- `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:187`：launch preparation 创建 environment context frame。
- `crates/agentdash-spi/src/hooks/mod.rs:457`、`crates/agentdash-spi/src/hooks/mod.rs:469`、`crates/agentdash-spi/src/hooks/mod.rs:484`、`crates/agentdash-spi/src/hooks/mod.rs:499`、`crates/agentdash-spi/src/hooks/mod.rs:519`：environment frame 分类为 SessionPolicy、order 15、SessionDigest、System channel、label Environment。
- `crates/agentdash-spi/src/hooks/mod.rs:672` - `crates/agentdash-spi/src/hooks/mod.rs:684`：`ContextFrameSection::Environment` schema。
- `crates/agentdash-application-runtime-session/src/session/context_usage_projection.rs:776`：context usage projection 处理 `ContextFrameSection::Environment`。
- `packages/app-web/src/features/session/model/contextFrame.ts:924` - `packages/app-web/src/features/session/model/contextFrame.ts:925`：前端对 environment frame 做 badge 投影。

建议：

- wait module / shell tools 的 Windows contract 仍应通过 `Environment` 或 tool schema description 暴露给 Agent。
- 前端可以展示该 frame，但不应影响 Agent command construction，也不应持有“PowerShell 应如何输出文本”的业务规则。

### 8. 前端 projection 已有什么、缺什么

结论：前端已有 terminal stream projection、mailbox snapshot、waiting_items 渲染与 mailbox state changed 通知链路；缺 Agent-facing wait handle 操作模型、exec wait item 的 action/status/read 投影，以及统一 wait activity 的 generated contract。

已有 terminal projection：

- `packages/app-web/src/generated/backbone-protocol.ts:271`：`PlatformEvent` union 包含 `terminal_output`、`terminal_state_changed`、`mailbox_state_changed`。
- `packages/app-web/src/features/session/model/sessionPlatformEventDispatcher.ts:19` - `packages/app-web/src/features/session/model/sessionPlatformEventDispatcher.ts:27`：`terminal_output` 投递到 `useTerminalStore.projectOutputEvent(session_id, event_seq, terminal_id, data)`。
- `packages/app-web/src/features/session/model/sessionPlatformEventDispatcher.ts:31` - `packages/app-web/src/features/session/model/sessionPlatformEventDispatcher.ts:44`：`terminal_state_changed` 校验 state 并投递到 `projectStateEvent(... exit_code)`。
- `packages/app-web/src/features/session/model/useTerminalStore.ts:26` - `packages/app-web/src/features/session/model/useTerminalStore.ts:35`：terminal store 以 `session_id -> terminal_id -> TerminalInfo` 与 bounded output buffer 存储投影。
- `packages/app-web/src/features/session/model/useTerminalStore.ts:46` - `packages/app-web/src/features/session/model/useTerminalStore.ts:58`：store 暴露 project output/state 方法。
- `packages/app-web/src/features/session/model/useTerminalStore.ts:163` - `packages/app-web/src/features/session/model/useTerminalStore.ts:170`：output event 按 `session_id/event_seq` 幂等投影。
- `packages/app-web/src/features/session/model/useTerminalStore.ts:173` - `packages/app-web/src/features/session/model/useTerminalStore.ts:178`：state event 幂等投影。
- `packages/app-web/src/features/session/model/useTerminalStore.ts:213` - `packages/app-web/src/features/session/model/useTerminalStore.ts:222`：读取 output getters。

已有 mailbox / waiting item projection：

- `packages/app-web/src/generated/workflow-contracts.ts:199`：`ConversationMailboxSnapshotView` 包含 `waiting_items`。
- `packages/app-web/src/generated/workflow-contracts.ts:205`：`ConversationWaitingItemView` 字段为 `wait_id`、`gate_id`、`kind`、`source_ref`、`correlation_ref`、`status`、`source_label`、`preview`、`created_at`、`resolved_at`。
- `crates/agentdash-contracts/src/runtime/workflow.rs:1151`：Rust contract `ConversationWaitingItemView`。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1714` - `crates/agentdash-api/src/routes/lifecycle_agents.rs:1716`：API snapshot 映射 `waiting_items`。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1737` - `crates/agentdash-api/src/routes/lifecycle_agents.rs:1738`：构造 `ConversationWaitingItemView`。
- `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:198`：conversation mailbox model 包含 `waiting_items`。
- `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:617`：snapshot 使用 `input.open_wait_items`。
- `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:1445` - `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:1515`：测试覆盖 open companion/human gates projected as waiting items。
- `packages/app-web/src/features/agent-run-workspace/ui/MailboxMessageRow.tsx:46`：读取 `mailbox?.waiting_items ?? []`。
- `packages/app-web/src/features/agent-run-workspace/ui/MailboxMessageRow.tsx:64` - `packages/app-web/src/features/agent-run-workspace/ui/MailboxMessageRow.tsx:69`：渲染 Waiting section。
- `packages/app-web/src/features/agent-run-workspace/ui/MailboxMessageRow.tsx:298` - `packages/app-web/src/features/agent-run-workspace/ui/MailboxMessageRow.tsx:303`：`WaitingItemRow` 渲染 kind/source/preview/status/time。
- `packages/app-web/src/features/agent-run-workspace/ui/SessionStatusBar.tsx:79`：status bar 读取 waiting items。
- `packages/app-web/src/features/agent-run-workspace/model/conversationCommandState.ts:333`：conversation command state 复制 mailbox waiting_items。
- `packages/app-web/src/features/agent-run-workspace/model/conversationCommandState.test.ts:278` - `packages/app-web/src/features/agent-run-workspace/model/conversationCommandState.test.ts:294`：测试等待项透传。

缺口：

- 没有 `WaitActivityView` / `ExecHandleView` generated contract；现有 waiting item 只有 gate-oriented `gate_id`，对 exec handle 的 `session_id`、`terminal_id`、`next_seq`、actions 不够表达。
- terminal store 是 UI projection，不是 Agent 可调用的 wait/read model；它不能替代 runtime wait tool。
- waiting item 行目前展示为状态/preview，缺少 read/wait/input/terminate/status 操作意图，也缺少 exec output unread count / terminal linkage。
- `mailbox_state_changed` 已能触发状态刷新，但 wait module 还需要定义何时发 mailbox wake、何时只发 terminal projection。

### 9. 推荐 wait module / exec handle / activity owner 边界、MVP 切片和必测项

推荐边界：

- `wait module`：统一对 Agent 暴露等待、读取、输入、终止、状态查询的 runtime tool/provider；内部通过 adapter 接 local shell session、LifecycleGate、mailbox wake。
- `ActivityOwner`：描述 wait activity 归属，建议包含 `runtime_session_id`、`agent_run_id`、`agent_id`、`turn_id`、`protocol_turn_id` 或 `context_frame_id`，以及 owner kind：`exec`、`companion`、`subagent`、`human`、`workflow`。
- `ExecHandle`：Agent 可见的稳定 handle，包装现有 `session_id`、`terminal_id`、`next_seq`、mount/cwd/command summary、created_at、state。不要暴露 `ProcessHandle`；不要把 `terminal_id` 单独当 wait handle。
- `LifecycleGateAdapter`：`companion/subagent/human` wait 的 source of truth 继续是 `LifecycleGateRepository`；wait module 只提供统一 read/wait/status facade，替代 `companion/tools.rs` 内私有 polling。
- `MailboxWakeAdapter`：wait resolve 后如需恢复 AgentRun，写入 deduped mailbox envelope。mailbox 只负责 delivery/wake/dedupe，不保存 wait activity 的完整状态和大结果。
- `TerminalProjectionAdapter`：terminal output/state events 继续给前端；Agent read 使用 wait/exec read primitive，不能从前端 terminal store 反读。
- `ContextFrame`：environment 与 tool schema 继续声明 Agent 可用能力；Windows PowerShell contract 放在 environment frame 或 wait/shell tool schema。

MVP 切片：

1. Exec follow-up runtime tools：在 runtime tool catalog 中新增 wait/exec provider，最小暴露 `wait.status`、`wait.read`、`wait.wait`、`wait.input`、`wait.terminate`，底层复用 local/relay `ToolShellRead/Input/Terminate` 与 `read_session(wait_ms)`。
2. `shell_exec` running result handle 化：保留现有 text/details 字段，同时新增稳定 `activity_id` 或 `exec_handle` details；details 保留 `session_id`、`terminal_id`、`next_seq` 以兼容前端/调试。
3. Gate wait adapter：把 `companion/tools.rs` 的 `wait_for_lifecycle_gate_resolution` 抽到 wait module，companion/human/subagent `wait=true` 复用统一 timeout/cancel/result preview。
4. Mailbox wake adapter：wait activity terminal/resolved 时按 owner/source identity 写入 `create_message_idempotent`，使用 stable dedup key，payload 只放 bounded preview 与 handle/result pointer。
5. Frontend projection：扩展 contracts 支持 exec wait item 或 activity item；waiting row 显示 exec/source/status/preview，terminal linkage 由 `terminal_id` 找 output，actions 单独走 API/tool intent。

必测项：

- runtime catalog：新增 wait tools 出现在 `SessionRuntimeToolComposer` 输出与 `ToolSchemaDelta`，重复 name 被拒。
- `shell_exec` running：返回 `state: running`、`session_id`、`terminal_id`、`next_seq`、`activity_id/exec_handle`；completed 命令不错误创建 long-running wait item。
- `wait.read`：`after_seq` 幂等，返回 retained chunks、`next_seq`、truncation；buffer 截断时明确 omitted 信息。
- `wait.wait`：running -> completed、timeout 不终止进程、cancel 能退出等待；terminal state 包含 exit_code。
- `wait.input`：stdin 写入交互进程并可通过 read 读回输出；closed/completed session 返回明确错误或 terminal status。
- `wait.terminate`：running 可终止；重复 terminate 幂等；unknown/lost session 状态清晰。
- LifecycleGate：companion wait resolved、human wait resolved、subagent result、timeout/cancel 都走统一 wait path；`request_type` 正确投影为 human/subagent。
- Mailbox：source_dedup_key 重放不重复入队；claim/lease/mark status 保持现有语义；resolved wake 不塞大 payload。
- Frontend：generated contracts 更新后 waiting item/action 渲染稳定；terminal store 仍按 `session_id/event_seq` 幂等；`mailbox_state_changed` 能触发 waiting item 刷新。

### 文件发现清单

- `crates/agentdash-api/src/bootstrap/session.rs`：session bootstrap 与 runtime tool provider composition root。
- `crates/agentdash-application/src/runtime_tools/provider.rs`：`SessionRuntimeToolComposer` 聚合 runtime tool providers。
- `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs`：launch 前组装 tool surface 与 context frames。
- `crates/agentdash-spi/src/connector/mod.rs`：`ExecutionTurnFrame`、`RuntimeToolProvider`、assembled tools contract。
- `crates/agentdash-spi/src/hooks/mod.rs`：ContextFrame section、ToolSchemaDelta、Environment section contract。
- `crates/agentdash-executor/src/connectors/pi_agent/connector.rs`：connector 消费 assembled tools。
- `crates/agentdash-application-vfs/src/tools/factory.rs`：VFS runtime tools 注册，当前 execute cluster 只暴露 `shell_exec`。
- `crates/agentdash-application-vfs/src/tools/fs/shell.rs`：Agent-visible `shell_exec` implementation 与 running result formatting。
- `crates/agentdash-application-vfs/src/materialization.rs`：shell command / cwd materialization 与 access policy。
- `crates/agentdash-application-vfs/src/service.rs`：VFS exec dispatch。
- `crates/agentdash-spi/src/platform/mount.rs`：`ExecRequest` / `ExecResult` platform mount contract。
- `crates/agentdash-api/src/mount_providers/relay_fs.rs`：VFS exec 到 relay/local shell 的 bridge。
- `crates/agentdash-relay/src/protocol.rs`：relay shell exec/read/input/terminate command/response message。
- `crates/agentdash-relay/src/protocol/tool.rs`：relay shell payload/response/session state DTO。
- `crates/agentdash-local/src/shell_session_manager.rs`：local shell session lifecycle、output buffer、read/input/terminate primitive。
- `crates/agentdash-local/src/handlers/tool_calls.rs`：local WS tool call dispatch 到 shell session manager。
- `crates/agentdash-local/src/ws_client.rs`：local relay command background handling。
- `crates/agentdash-agent-protocol/src/backbone/platform.rs`：terminal/mailbox platform events。
- `crates/agentdash-api/src/relay/ws_handler.rs`：relay event 到 shell output registry、terminal cache、Backbone event 的 projection。
- `crates/agentdash-relay/src/shell_output_registry.rs`：shell exec streaming output route。
- `crates/agentdash-application/src/companion/tools.rs`：companion/human wait=true 私有 polling、gate resolve、mailbox wake dedupe。
- `crates/agentdash-application-workflow/src/gate/resolver.rs`：LifecycleGate 创建与 human response resolve。
- `crates/agentdash-domain/src/workflow/repository.rs`：LifecycleGate repository contract。
- `crates/agentdash-domain/src/agent_run_mailbox/mod.rs`：AgentRun mailbox domain model 与 repository trait。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs`：mailbox Postgres 存储、dedupe、claim、recover。
- `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql`：mailbox 表与 `source_dedup_key` index。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/*.rs`：mailbox delivery、scheduler、runtime adapter。
- `crates/agentdash-api/src/agent_run_mailbox.rs`：terminal callback 到 mailbox scheduling / pause fallback。
- `crates/agentdash-application-runtime-session/src/session/environment_context_frame.rs`：Environment ContextFrame 与 Windows PowerShell contract。
- `crates/agentdash-application-runtime-session/src/session/context_usage_projection.rs`：ContextFrame usage projection。
- `packages/app-web/src/generated/backbone-protocol.ts`：前端 Backbone event generated contract。
- `packages/app-web/src/generated/workflow-contracts.ts`：前端 mailbox/waiting item generated contract。
- `packages/app-web/src/features/session/model/sessionPlatformEventDispatcher.ts`：terminal Backbone event 分发。
- `packages/app-web/src/features/session/model/useTerminalStore.ts`：terminal output/state projection store。
- `packages/app-web/src/features/agent-run-workspace/ui/MailboxMessageRow.tsx`：waiting items UI。
- `packages/app-web/src/features/agent-run-workspace/ui/SessionStatusBar.tsx`：workspace status bar waiting count。
- `packages/app-web/src/features/agent-run-workspace/model/conversationCommandState.ts`：conversation command state 携带 waiting_items。

## Caveats / Not Found

- 未发现 Agent-visible 的 `shell_read`、`shell_wait`、`shell_input`、`shell_terminate`、`shell_status` 或通用 wait runtime tool。现有 read/input/terminate 均停留在 relay/local internal protocol。
- 未发现统一 `WaitActivity` / `ExecHandle` domain model；当前 `session_id` 是 local shell session id，`terminal_id` 是 UI terminal projection key，`gate_id` 是 LifecycleGate key，三者尚未被统一抽象。
- 未发现 exec wait facts 的 durable storage。local shell session 由 local backend 内存持有，进程重启后的恢复/丢失语义需要新设计明确。
- 未发现前端对 exec handle 的 action contract；现有 waiting item 主要来自 open gates，UI 只展示 status/preview。
- 本报告未进行外部资料检索；判断基于仓库代码与 `.trellis/spec/`。
