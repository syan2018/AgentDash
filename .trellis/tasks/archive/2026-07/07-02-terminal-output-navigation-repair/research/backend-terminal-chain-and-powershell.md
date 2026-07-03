# Research: backend-terminal-chain-and-powershell

- Query: 后端终端执行、relay、terminal cache、PowerShell 对象输出问题。
- Scope: internal
- Date: 2026-07-02

## Findings

### 结论先行

1. 当前交互式终端执行层是 PTY 流，不是 PowerShell 对象/JSON 序列化链路。`spawn_terminal` 固定 `tty: true`，`spawn_session` 因此走 `codex_utils_pty::spawn_pty_process`；输出从 `stdout_rx` / `stderr_rx` 读取 `Vec<u8>` 后用 `String::from_utf8_lossy` 转为文本，再作为 relay `event.terminal.output` 和 Backbone `PlatformEvent::TerminalOutput` 注入前端。
2. 当前普通 shell/process 执行层也是捕获真实进程 stdout/stderr 字节流，不是对象序列化。`process_executor` 用 `tokio::process::Command.output()` 捕获 `stdout` / `stderr` 字节并 `String::from_utf8_lossy` 解码；Windows shell exec 只是包装 `powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command ...`，没有 `ConvertTo-Json`、`serde_json::Value` 或对象 mapper。
3. 当前仍暴露旧 session 形态终端端点：`GET/POST /sessions/{id}/terminals`、`POST /terminals/{id}/input`、`POST /terminals/{id}/resize`、`DELETE /terminals/{id}`。同时 session 本身也仍暴露多组 `/sessions/{id}/*` 诊断/trace/control 端点。本文不提出新增或保留任何对外 `/sessions/*` 终端端点方案。
4. terminal cache 当前只缓存终端状态元数据，不缓存输出正文。输出正文通过 session eventing 持久化为 Backbone Platform event，前端 `useSessionStream` 拦截该事件写入内存 `useTerminalStore.outputBuffers`，再由 xterm 增量写入。跳转后看不到历史输出的风险点更像是“输出事件是否已被当前页面/session stream 回放并写入 terminal store”，不是后端执行层把 PowerShell 对象吞掉。
5. `input` / `resize` / `kill` 的 HTTP handler 当前只要 relay `send_command` 返回任意响应就返回 204，没有检查 `ResponseTerminalInput/Resize/Kill.error`。这会掩盖本机 handler 返回的终端不存在、resize 失败、kill 失败等错误，属于链路可靠性修复点。
6. `pwd` / `Get-Location`、`dir` / `Get-ChildItem`、`Write-Output (Get-Location).Path` 在 PowerShell 下理论上都应通过 PTY 输出可见文本。若当前 UI 不显示，优先排查前端 terminal store 回放、tab active id 切换、session stream/backlog 分发，而不是 PowerShell 对象 JSON 化。

### Files found

- `crates/agentdash-api/src/routes/terminals.rs`: 终端 HTTP API；list/spawn/input/resize/kill 入口，负责权限、target 解析、terminal cache 预注册和 relay command 投递。
- `crates/agentdash-api/src/agent_run_runtime_surface.rs`: terminal spawn 的 runtime surface target 解析入口，从 runtime session 反查当前 AgentRun surface 和 backend anchor。
- `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs`: 从 VFS + runtime backend anchor 解析 terminal launch backend 和 mount root ref。
- `crates/agentdash-api/src/relay/registry.rs`: 云端 backend registry；维护在线 backend、pending request、session sink，并提供 `send_command` request/response matching。
- `crates/agentdash-api/src/relay/ws_handler.rs`: relay WebSocket 消息处理；terminal output/state event 转成 Backbone Platform event，backend disconnect 转 lost。
- `crates/agentdash-relay/src/protocol.rs`: relay 顶层 wire enum；定义 `command.terminal.*`、`response.terminal.*`、`event.terminal.*` envelope。
- `crates/agentdash-relay/src/protocol/terminal.rs`: terminal payload DTO；spawn/input/resize/kill/output/state payload shape。
- `crates/agentdash-local/src/handlers/mod.rs`: 本机 relay command router；把 `CommandTerminal*` 分发到 terminal handler。
- `crates/agentdash-local/src/handlers/terminal.rs`: 本机 terminal domain handler；校验 workspace root，调用 `ShellSessionManager` spawn/input/resize/kill。
- `crates/agentdash-local/src/shell_session_manager.rs`: 本机 shell/terminal session manager；PTY/pipe spawn、input、resize、terminate、stdout/stderr 读取、relay live event 发送。
- `crates/agentdash-local/src/process_executor.rs`: 普通 process/shell exec 层；捕获 stdout/stderr 字节并解码为字符串。
- `crates/agentdash-application-runtime-session/src/session/terminal_cache.rs`: 云端 terminal cache；仅缓存 terminal 状态元数据。
- `crates/agentdash-agent-protocol/src/backbone/platform.rs`: Backbone PlatformEvent 定义；terminal output/state 是平台事件，不进入聊天 item。
- `packages/app-web/src/features/session/model/useSessionStream.ts`: 前端 session stream 入口；terminal platform event 被拦截写入 terminal store。
- `packages/app-web/src/features/session/model/sessionPlatformEventDispatcher.ts`: 前端 platform event dispatcher；把 terminal output/state 写入 `useTerminalStore`。
- `packages/app-web/src/features/session/model/useTerminalStore.ts`: 前端 terminal store；内存保存状态和有界输出 buffer。
- `packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx`: xterm tab；spawn terminal、发送 input/resize、从 terminal store 增量写入 xterm。
- `packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx`: 普通 command output promote 到 terminal tab 的前端路径。
- `packages/app-web/src/types/terminal.ts`: 前端 terminal 类型定义。

### Code patterns and evidence

#### API 到 runtime target

- `terminals::router` 暴露旧 session 形态 terminal list/spawn：`/sessions/{id}/terminals`，以及 `/terminals/{id}/input`、`/terminals/{id}/resize`、`/terminals/{id}` kill。证据：`crates/agentdash-api/src/routes/terminals.rs:35`-`47`。
- list 先 `ensure_session_permission(... ProjectPermission::View)`，再读取 `terminal_cache.list_terminals(&session_id)`。证据：`crates/agentdash-api/src/routes/terminals.rs:18`-`32`。
- spawn 先通过 `resolve_terminal_launch_target_for_api` 解析目标 backend/mount，再检查 backend online。证据：`crates/agentdash-api/src/routes/terminals.rs:49`-`66`。
- spawn 构造 `TerminalSpawnPayload { terminal_id, session_id, mount_root_ref, cwd, shell, cols, rows }`，先预注册 terminal cache，再 `backend_registry.send_command(CommandTerminalSpawn)`。证据：`crates/agentdash-api/src/routes/terminals.rs:68`-`97`。
- spawn 成功只返回 `{ terminal_id, process_id }`；失败移除预注册 cache。证据：`crates/agentdash-api/src/routes/terminals.rs:99`-`133`。
- `resolve_terminal_launch_target_for_api` 先确认 runtime session 存在，再通过 `runtime_surface_query.current_runtime_surface_with_backend(session_id, "terminal_spawn")` 读取 current runtime surface，并校验 Project view 权限。证据：`crates/agentdash-api/src/agent_run_runtime_surface.rs:125`-`148`。
- `terminal_launch_target_from_vfs` 使用 backend anchor root_ref 匹配 VFS mount，要求 mount provider 是 `PROVIDER_RELAY_FS`，并取 `backend_anchor.backend_id()` 与 mount `root_ref`。证据：`crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:544`-`585`。

#### API input / resize / kill

- input 通过 `load_terminal_for_user` 从 terminal cache 找 terminal，并校验对应 session view 权限；随后发送 `CommandTerminalInput { terminal_id, data }` 到 cached `backend_id`。证据：`crates/agentdash-api/src/routes/terminals.rs:136`-`169`、`245`-`263`。
- resize 同样从 terminal cache 路由到 cached `backend_id`，发送 `CommandTerminalResize { terminal_id, cols, rows }`。证据：`crates/agentdash-api/src/routes/terminals.rs:172`-`207`。
- kill 发送 `CommandTerminalKill { terminal_id, signal: None }`。证据：`crates/agentdash-api/src/routes/terminals.rs:209`-`243`。
- 风险：input/resize/kill 的 match 分支是 `Ok(_) => 204`，没有校验本机响应是否是对应 `ResponseTerminal*` 且 `error == None`。证据：`crates/agentdash-api/src/routes/terminals.rs:149`-`168`、`186`-`205`、`222`-`240`。

#### Relay request/response/protocol

- relay protocol 顶层定义 terminal command：`command.terminal.spawn/input/resize/kill`。证据：`crates/agentdash-relay/src/protocol.rs:541`-`564`。
- relay protocol 顶层定义 terminal response：`response.terminal.spawn/input/resize/kill`。证据：`crates/agentdash-relay/src/protocol.rs:566`-`600`。
- relay protocol 顶层定义 terminal async event：`event.terminal.output` 和 `event.terminal.state_changed`。证据：`crates/agentdash-relay/src/protocol.rs:603`-`614`。
- terminal payload 是纯字符串/数字 DTO：`TerminalInputPayload.data: String`、`TerminalOutputPayload.data: String`、`TerminalStateChangedPayload.state/exit_code/message`。证据：`crates/agentdash-relay/src/protocol/terminal.rs:7`-`106`。
- `BackendRegistry::send_command` 把 msg id 放入 `pending`，通过 backend sender 发送，再等待 oneshot response。证据：`crates/agentdash-api/src/relay/registry.rs:158`-`166`、`372`-`407`。
- relay ws handler 对 response message 调用 `backend_registry.resolve_response(&msg)`，按 message id 唤醒 pending request。证据：`crates/agentdash-api/src/relay/ws_handler.rs:359`-`368`。
- terminal response 类型被纳入 pending response message 集合。证据：`crates/agentdash-api/src/relay/ws_handler.rs:647`-`650`。

#### Local runtime / handler / execution

- local `CommandRouter` 把 `CommandTerminalSpawn/Input/Resize/Kill` 分发到 `TerminalCommandHandler`。证据：`crates/agentdash-local/src/handlers/mod.rs:268`-`280`。
- terminal handler spawn 先 `tool_executor.validate_workspace_root(&payload.mount_root_ref)`，再调用 `shell_sessions.spawn_terminal(&payload, &workspace_root)`。证据：`crates/agentdash-local/src/handlers/terminal.rs:46`-`83`。
- terminal handler input 复用 shell session input，把 `payload.terminal_id` 当 `ToolShellInputPayload.session_id`，`wait_ms=Some(0)`。证据：`crates/agentdash-local/src/handlers/terminal.rs:85`-`113`。
- terminal handler resize 调用 `shell_sessions.resize_terminal(&payload)`。证据：`crates/agentdash-local/src/handlers/terminal.rs:115`-`134`。
- terminal handler kill 调用 `shell_sessions.terminate_shell(ToolShellTerminatePayload { session_id: terminal_id })` 并映射 kill 状态。证据：`crates/agentdash-local/src/handlers/terminal.rs:136`-`162`。
- `ShellSessionManager` 引入 `spawn_pipe_process` 和 `spawn_pty_process`，terminal 是同一 session manager 内的一种 `terminal_id: Some(...)` session。证据：`crates/agentdash-local/src/shell_session_manager.rs:10`-`13`、`36`-`63`。
- `spawn_terminal` 解析 cwd，默认 shell 为 `default_shell()`，设置 `tty: true`、`args: Vec::new()`、`terminal_id: Some(...)`。证据：`crates/agentdash-local/src/shell_session_manager.rs:297`-`325`。
- `spawn_session` 在 `spec.tty` 为 true 时调用 `spawn_pty_process`，否则才调用 `spawn_pipe_process`。证据：`crates/agentdash-local/src/shell_session_manager.rs:480`-`505`。
- Windows 默认交互 shell 是 `powershell.exe`，没有附加 `-Command` 或 JSON 化参数。证据：`crates/agentdash-local/src/shell_session_manager.rs:827`-`833`。
- 普通 non-terminal shell command 在 Windows 下才包装为 `powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command ...`，并设置 UTF-8 输出编码。证据：`crates/agentdash-local/src/shell_session_manager.rs:798`-`815`、`crates/agentdash-local/src/process_executor.rs:126`-`143`。
- terminal resize 直接调用 `ProcessHandle.resize(TerminalSize { rows, cols })`。证据：`crates/agentdash-local/src/shell_session_manager.rs:463`-`477`。
- terminal running/exited/killed state 通过 `EventTerminalStateChanged` 发回云端。证据：`crates/agentdash-local/src/shell_session_manager.rs:544`-`554`、`642`-`682`、`430`-`454`。

#### Output / cache / eventing / frontend

- `insert_spawned_session` 为 stdout/stderr 各起一个 task，从 `stdout_rx` / `stderr_rx` 收 `bytes` 后调用 `push_output`。证据：`crates/agentdash-local/src/shell_session_manager.rs:565`-`583`。
- `push_output` 是关键证据：`let data = String::from_utf8_lossy(&bytes).to_string();`，terminal stdout 被标为 `ShellOutputStream::Pty`，然后写入 retained buffer 和 live output budget。证据：`crates/agentdash-local/src/shell_session_manager.rs:593`-`616`。
- terminal live output 作为 `RelayMessage::EventTerminalOutput { payload: TerminalOutputPayload { terminal_id, data, truncation } }` 发出。证据：`crates/agentdash-local/src/shell_session_manager.rs:629`-`638`。
- 云端收到 `EventTerminalOutput` 后 bounded，再构造 `BackboneEvent::Platform(PlatformEvent::TerminalOutput { terminal_id, data })`，通过 `session_eventing.inject_notification` 注入 runtime session。证据：`crates/agentdash-api/src/relay/ws_handler.rs:474`-`515`。
- 云端收到 `EventTerminalStateChanged` 后先更新 `terminal_cache.update_state`，再注入 `PlatformEvent::TerminalStateChanged`。证据：`crates/agentdash-api/src/relay/ws_handler.rs:524`-`570`。
- backend disconnect 会把该 backend 下 running/starting terminal 标记为 lost，并注入 terminal state event。证据：`crates/agentdash-api/src/relay/ws_handler.rs:293`-`340`、`crates/agentdash-application-runtime-session/src/session/terminal_cache.rs:115`-`130`。
- terminal cache 只保存 `terminal_id/session_id/backend_id/state/exit_code/process_id/created_at/exited_at`，无 output 字段。证据：`crates/agentdash-application-runtime-session/src/session/terminal_cache.rs:17`-`31`。
- `PlatformEvent::TerminalOutput` 注释明确“路由到前端 xterm.js，不作为 chat entry 展示”。证据：`crates/agentdash-agent-protocol/src/backbone/platform.rs:36`-`47`。
- 前端 generated contract 中 terminal output/state 是 `PlatformEvent` union 分支。证据：`packages/app-web/src/generated/backbone-protocol.ts:271`。
- `useSessionStream` 收到事件后先调用 `dispatchSessionPlatformEvent`，terminal event 被直接转给 TerminalStore，不进入 React state reducer，避免 StrictMode 双写。证据：`packages/app-web/src/features/session/model/useSessionStream.ts:132`-`136`。
- `dispatchSessionPlatformEvent` 对 `terminal_output` 调 `useTerminalStore.getState().appendOutput(terminal_id, data)`；对 state changed 调 `updateTerminalState`。证据：`packages/app-web/src/features/session/model/sessionPlatformEventDispatcher.ts:14`-`42`。
- session reducer 也显式不把 terminal output/state 放进聊天 display entries。证据：`packages/app-web/src/features/session/model/sessionStreamReducer.ts:582`-`591`。
- `useTerminalStore` 只在前端内存维护 `outputBuffers` 和 `outputBufferBaseOffsets`，上限 256 KiB。证据：`packages/app-web/src/features/session/model/useTerminalStore.ts:4`-`24`、`64`-`84`。
- terminal tab 的唯一 xterm 写入路径是 `store outputBuffer -> useEffect -> term.write(pending)`。证据：`packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:53`-`59`、`133`-`149`。
- terminal tab 新建时 POST 当前旧 endpoint `/sessions/{sessionId}/terminals`，成功后在前端 store 注册 terminal，并把 tab uri 更新为 `terminal://{terminal_id}`。证据：`packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:212`-`259`。
- terminal tab input/resize 分别 POST `/terminals/{id}/input` 和 `/terminals/{id}/resize`。证据：`packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:92`-`110`。
- 搜索结果显示前端当前没有调用 `GET /sessions/{id}/terminals` 的生产代码；只有 spawn POST、input/resize POST、store 注册和 command output promote。证据：`rg -n "listTerminals|getTerminalsForSession|/terminals|registerTerminal\\(" packages/app-web/src`，生产命中集中在 `terminal-tab.tsx`、`CommandExecutionCardBody.tsx` 和 store。

#### PowerShell 对象输出定位

- 交互式 terminal：Windows 默认 shell 是 `powershell.exe`，通过 PTY 启动；输出按字节读取并 `from_utf8_lossy`。证据：`crates/agentdash-local/src/shell_session_manager.rs:297`-`325`、`480`-`505`、`593`-`638`、`827`-`833`。
- 普通 process/shell exec：`run_output_command` 设置 `stdout(Stdio::piped()).stderr(Stdio::piped())`，调用 `command.output()`，再解码 stdout/stderr 字节。证据：`crates/agentdash-local/src/process_executor.rs:101`-`118`、`153`-`155`。
- 普通 Windows shell exec 只设置 PowerShell UTF-8 输出编码并执行原始 command；没有 `ConvertTo-Json`、对象 mapper 或非字符串序列化。证据：`crates/agentdash-local/src/process_executor.rs:126`-`143`。
- relay terminal payload 的 `data` 字段本身是 `String`，协议没有携带对象形态。证据：`crates/agentdash-relay/src/protocol/terminal.rs:72`-`87`。
- 未发现 terminal 路径上把 PowerShell 输出当对象、JSON 或 serde value 序列化的生产代码。`serde_json` 在此链路主要用于 relay/Backbone/HTTP envelope，而不是 PowerShell 输出正文。

### Current exposed old session-shaped endpoints

- 仍暴露 terminal 相关旧 session endpoint：`GET /sessions/{id}/terminals`、`POST /sessions/{id}/terminals`。证据：`crates/agentdash-api/src/routes/terminals.rs:35`-`40`。
- terminal input/resize/kill 当前不在 `/sessions/*` 下，但仍是裸 terminal id API：`POST /terminals/{id}/input`、`POST /terminals/{id}/resize`、`DELETE /terminals/{id}`。证据：`crates/agentdash-api/src/routes/terminals.rs:41`-`46`。
- 仍暴露多组 session trace/diagnostic/control endpoints：`/sessions/{id}`、`/sessions/{id}/runtime-control`、`/sessions/{id}/meta`、`/sessions/{id}/state`、`/sessions/{id}/events`、`/sessions/{id}/context/projection`、`/sessions/{id}/lineage`、`/sessions/{id}/fork`、`/sessions/{id}/projection/rollback`、`/sessions/{id}/context/audit`、`/sessions/{id}/tool-approvals/*`、`/sessions/{id}/stream/ndjson`。证据：`crates/agentdash-api/src/routes/sessions.rs:86`-`137`。
- `routes.rs` 把 `sessions::router()` 和 `terminals::router()` 都 merge 到 secured API。证据：`crates/agentdash-api/src/routes.rs:75`-`110`。

### Risk

- 输出正文没有 backend terminal cache；只有 session eventing 持久化 + 前端内存 terminal store。若用户跳转到 terminal tab 时当前页面没有消费过相关 session backlog，xterm 没有可回放文本。
- `GET /sessions/{id}/terminals` 目前未被前端生产代码消费，旧终端状态查询无法自动补齐前端 store；即使消费它，也只能补状态，补不了输出正文。
- `useSessionStream` 会把 terminal platform event 从聊天 reducer 前拦截，这是正确的双写防护；但修复时要保证 backlog/历史事件也经过同一 dispatcher，否则“实时看到、跳转看不到”会复现。
- terminal output 经过多层有界化：local `LiveOutputEventBudget`、relay `TerminalOutputPayload::bounded`、前端 256 KiB buffer。验收时要区分“完全无输出”和“输出被截断但有 truncation notice”。
- API input/resize/kill 不检查 response error，会让终端不存在、resize 失败、kill 失败表现成前端 204 成功，导致 UI 误以为交互链路正常。
- PowerShell 对象命令如 `Get-Location`、`Get-ChildItem` 依赖 PowerShell host formatting；当前交互 terminal 是 PTY，理论上应触发正常文本格式化。如果只有这些对象命令无输出，而 `Write-Output (Get-Location).Path` 有输出，应继续验证 PTY host/ANSI/formatting 行为；如果三者都无输出，则更可能是 terminal output event/store/xterm 写入链路问题。
- 当前 Trellis `task.py current --source` 指向 `.trellis/tasks/07-02-agent-parallel-wait-mailbox-implementation`，与用户指定的 `.trellis/tasks/07-02-terminal-output-navigation-repair` 不一致。本文件按用户显式路径写入。

### 建议实现切片

#### 属于本次“修复终端输出展示与跳转链路”

1. 先修前端 terminal output 回放路径：确保打开/跳转到 `terminal://{id}` 时，已有 session backlog 中的 `PlatformEvent::TerminalOutput` 已经通过 `dispatchSessionPlatformEvent` 写入 `useTerminalStore`，并且 `TerminalView` active id 切换后从 `outputBaseOffset=0` 或当前 retained base 正确回放到 xterm。
2. 保持 terminal output 不进入聊天列表，但保证所有 session stream 历史事件、实时事件、重连事件走同一个 terminal dispatcher。关键验收点是“先产生输出，再打开 terminal tab”仍可见。
3. 修 API terminal command response handling：input/resize/kill 应校验返回的 relay response 类型与 `error` 字段；本机 handler 返回 error 时 HTTP 不应 204。
4. 修 terminal state 与 tab 状态同步：state changed 如果先于前端 store 注册或 tab 打开到达，应避免丢失后续状态；必要时让 terminal tab 从已有 session event/store 重建 state，而不是只在 spawn 成功时前端本地注册。
5. 针对 PowerShell 对象输出做最小执行层验证：在 Windows terminal PTY 内发送 `pwd\r`、`Get-Location\r`、`dir\r`、`Get-ChildItem\r`、`Write-Output (Get-Location).Path\r`，断言后端 `event.terminal.output` 至少包含可见 cwd/path 或 directory table 文本，并最终写入 xterm buffer。

#### 应交给并行能力任务

1. 对外 API 形态治理：当前确实仍有 `/sessions/{id}/terminals` 旧 session 形态端点；是否迁移到 AgentRun/workspace-scoped command contract、如何兼容/移除旧 route，属于 API/产品能力边界任务，不放入本次输出展示与跳转修复。
2. terminal output 长期持久化/检索模型：如果要提供独立于 session stream 的 terminal scrollback/read API，需要设计存储、裁剪、权限与 AgentRun workspace 归属；这超出最小展示修复。
3. Windows PowerShell host 体验增强：如切换 PowerShell 版本、profile 策略、prompt/encoding/ANSI 细节、对象格式化宽度策略，应作为 Windows terminal capability 任务处理，除非本次验收证明 PTY 流本身没有任何对象命令输出。
4. 普通 `process_executor` / tool shell 的 PowerShell formatting 策略：它是 pipe stdout/stderr 执行面，不是交互 terminal；若工具卡片里的 PowerShell 对象展示另有问题，应由 tool shell 能力任务处理。

### 建议测试

#### 后端/local runtime 最小测试

- `agentdash-local` 层新增或扩展 `ShellSessionManager` Windows-gated integration test：spawn terminal（`tty: true`），写入 `Write-Output (Get-Location).Path\r`，等待 `EventTerminalOutput`，断言 payload `data` 含 cwd 文本。
- 同一层 Windows-gated test 覆盖对象输出命令：`pwd\r` 或 `Get-Location\r` 至少输出 `Path`/cwd；`dir\r` 或 `Get-ChildItem\r` 至少输出当前目录条目或 PowerShell table/list 文本。
- relay/API handler test：local terminal input/resize/kill 返回 `ResponseTerminal* { error: Some(...) }` 时，HTTP handler 不返回 204；类型不匹配 response 返回稳定错误。
- ws handler unit test：`EventTerminalOutput` 注入 `PlatformEvent::TerminalOutput`，并保留 data；truncation 时追加 `[terminal output truncated: omitted_bytes=...]`。

#### 前端最小测试

- `useSessionStream`/dispatcher test：历史/backlog terminal output event 被 `dispatchSessionPlatformEvent` 写入 `useTerminalStore.outputBuffers`，且不进入 display entries。
- `TerminalView` test：先向 store 写入 terminal output，再渲染 `terminal://{id}` tab，xterm write 收到已有 output；active id 从 `new` 切到 real id 后不丢第一段输出。
- terminal store test：多段输出 append 后 base offset 与 retained buffer 正确，截断情况下仍显示最新可见文本。

#### 端到端验收

- Windows PowerShell 终端打开后依次输入：
  - `pwd`
  - `Get-Location`
  - `dir`
  - `Get-ChildItem`
  - `Write-Output (Get-Location).Path`
- 每条命令都应在 terminal tab 中出现可见文本输出；其中 `pwd`/`Get-Location` 至少能看到路径，`dir`/`Get-ChildItem` 至少能看到目录列表或对象格式化表格。
- 验收“跳转链路”：在输出产生后关闭/切走 terminal tab，再通过对应 `terminal://{id}` 跳转打开，仍能看到已接收的输出 buffer。
- 验收“不进聊天流”：同一 terminal output 不应作为普通 chat display entry 出现在 session feed 中。

### Related specs

- `.trellis/workflow.md`: 研究产物必须持久化到 task `research/`，Phase 1.2 research 约定。
- `.trellis/spec/backend/index.md`: 后端 spec 入口。
- `.trellis/spec/backend/runtime-gateway.md`: runtime action、session control 和 RuntimeGateway 边界。
- `.trellis/spec/cross-layer/desktop-local-runtime.md`: local relay command routing、terminal handler domain、后台 spawn 与 PTY 边界。
- `.trellis/spec/cross-layer/backbone-protocol.md`: BackboneEnvelope/PlatformEvent/session stream/frontend consumption 合同。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`: Rust contract -> generated TS -> frontend service/reducer 的合同。

### External references / versions

- `codex-utils-pty` 来自 OpenAI Codex git tag `rust-v0.140.0`。证据：`Cargo.toml:112`。
- `codex-utils-pty` 依赖 `portable-pty`。证据：`Cargo.lock:2026`-`2035`、`Cargo.lock:7072`。
- 前端 terminal 使用 xterm：`@xterm/xterm ^6.0.0`、`@xterm/addon-fit ^0.11.0`、`@xterm/addon-web-links ^0.12.0`。证据：`packages/app-web/package.json:47`-`49`、`pnpm-lock.yaml:1475`-`1481`。

## Caveats / Not Found

- 未运行生产代码、未启动 dev server、未做 Windows PowerShell PTY 实机验收；本文件是只读代码调研。
- 未使用 git 命令确认 origin/main；本 researcher 约束禁止任何 git operation。本工作树按用户描述视为当前 origin/main 基线。
- 未发现 terminal 执行层把 PowerShell 对象序列化为 JSON/object 的代码；发现的是 PTY/pipe 字节流捕获和字符串化。
- 未发现前端生产代码消费 `GET /sessions/{id}/terminals` 来恢复终端列表；也未发现 backend terminal cache 保存输出正文。
- 未提出新增或保留任何对外 `/sessions/*` 终端端点方案；当前暴露情况只作为现状证据记录。
