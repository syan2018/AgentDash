# 实际失败 Run 数据库取证

## 取证对象

- PostgreSQL：本地 dev embedded PostgreSQL，端口 `8964`，数据库 `agentdash_api`。
- AgentRun：`59644f0c-b708-4d29-a237-d8ee1c1a0b57`。
- Agent：`d6517f27-a45a-4d9e-aa9b-dd7688b3abfe`。
- Runtime thread：`thread-59644f0c-b708-4d29-a237-d8ee1c1a0b57-d6517f27-a45a-4d9e-aa9b-dd7688b3abfe`。
- Runtime binding：`binding-59644f0c-b708-4d29-a237-d8ee1c1a0b57-d6517f27-a45a-4d9e-aa9b-dd7688b3abfe`，driver generation `3`。
- AgentFrame：`77567b63-3727-4eac-8463-4fb5acde812a`，revision `1`。
- 第二轮 operation：`agentrun-59644f0c-b708-4d29-a237-d8ee1c1a0b57-d6517f27-a45a-4d9e-aa9b-dd7688b3abfe-mailbox-8baa7c69-7030-4b61-be38-c7d3df67fbc4`。

以下结论来自 `agent_runtime_event`、`agent_runtime_operation`、`agent_runtime_outbox`、`agent_runtime_tool_call`、`agent_runtime_surface_snapshot` 与 `agent_frames` 的同一实际 run，不是 fixture 推测。

## 失败时间线

### 1. 第一轮已经完成 Turn，但 operation 未终结

第一轮 journal sequence `1..22`：

- sequence `1`：`OperationAccepted(ThreadStart)`；
- sequence `4`：canonical `TurnStarted`；
- sequence `5`：`UserInputSubmitted`；
- sequence `14..15`：Native mapper 又创建一个 internal `AgentMessage`，正文竟是用户输入“你好，能听见我说话不？”；
- sequence `17..18`：真正的 assistant `AgentMessage`；
- sequence `21`：`TurnTerminal(Completed)`；
- sequence `22`：`turn_completed` presentation。

但 `agent_runtime_operation` 中第一轮 operation 在第二轮开始前仍为 active，没有 `OperationTerminal(Succeeded)`。当前 Native driver 只产生 `TurnTerminal`，没有产生 operation terminal；Runtime gateway 也不会自动把 driver turn terminal 推导成 operation terminal。

### 2. 同一个工具调用由两个 producer 使用两套 ID 发布

第二轮输入“尝试随便调用几组工具？”触发 `mounts_list`、`task_read`、`workspace_module_list`。数据库出现两套完整 presentation lifecycle：

| 逻辑工具 | Native vendor stream ID | ToolBroker ID |
|---|---|---|
| `mounts_list` | `turn_001:tool_001` | `native-runtime-tool-native-turn-...-call_...` |
| `task_read` | `turn_001:tool_002` | `native-runtime-tool-native-turn-...-call_...` |
| `workspace_module_list` | `turn_001:tool_003` | `native-runtime-tool-native-turn-...-call_...` |

具体顺序：

- sequence `29..31`：Native vendor presentation 首次发布三个 `item_started`；
- sequence `32..37`：ToolBroker 写 canonical internal item，并用自己的 runtime item ID 再发布三个 `item_started`；
- sequence `41..46`：ToolBroker 用 runtime item ID 发布三个 `item_completed`；
- sequence `49..51`：Native vendor presentation 再次发布同一批 `turn_001:tool_00N` 的 `item_started`；
- sequence `52/55/58`：Native vendor presentation 用 `turn_001:tool_00N` 发布 `item_completed`。

当前 production business surface 中这 16 个工具的 `presentation_emitter` 全部是 `tool_broker`，但 `DriverToolDefinition` 丢失该字段，Native mapper 无从抑制 vendor lifecycle。前端 reducer 按 inner `item.id` 合并，因而必然得到两个 card，无法靠前端修正。

### 3. canonical internal item 把 User 与 ToolResult 错当 Agent 消息

Native mapper 对所有 `MessageStart/MessageEnd` 建立 internal `AgentMessage`，只对“无 canonical content 的 assistant tool-only message”做了特判，没有按角色过滤：

- 第一轮 sequence `14..15` 把用户正文保存为 `AgentMessage`；
- 第二轮 sequence `27..28` 再次把用户正文“尝试随便调用几组工具？”保存为 `AgentMessage`；
- sequence `53..60` 把三个序列化 tool result/error JSON 保存为新的 `AgentMessage` item。

main-reference 的 presentation mapper 只在 `AgentMessage::Assistant` 上生成 assistant/reasoning presentation；用户输入由 application submission producer 负责，tool result 由 tool lifecycle 负责。新 Runtime 的 internal projection也必须保持同一语义分工。

### 4. 工具执行对象冻结了错误的 bootstrap context

`AgentBusinessSurfaceSource::load` 在 provision 阶段用：

- `session.turn_id = surface-bootstrap-<frame_id>`；
- `ExecutionTurnFrame { capability_state, ..Default::default() }`，因此 `hook_runtime=None`；

调用全部 production `RuntimeToolProvider::build_tools`，随后把这些捕获了错误 context 的 `DynAgentTool` 冻结进 `CompiledAgentRunToolBinding.tools`。真实 `AgentFrameHookRuntime(session_id=canonical runtime thread)` 直到 compiler 后段才创建，只放进 registry，从未回填到已构建工具。

数据库中的实际结果与源码完全对应：

- `mounts_list` completed；
- `task_read` failed：`当前 session 缺少 hook runtime，无法定位 Task scope`；
- `workspace_module_list` failed：`runtime surface query missing anchor: component=workspace_module_visibility, session_id=surface-bootstrap-77567b63-3727-4eac-8463-4fb5acde812a`。

所以当前只是工具 schema/目录注入成功，不是 executable runtime context 注入成功。VFS 的部分纯操作偶然可用；Task、Workspace Module、Companion、Wait、workflow 等依赖 session/Hook/scope 的工具均可能携带假 thread/turn 或缺失 scope。

### 5. 展示 projector 错误升级为 driver critical violation

当前 Native presentation projector 对 `fs_read/fs_grep/fs_glob` 的展示字段做 required-field 强校验。实际 `fs_glob` event 缺少 `pattern` 时返回：

`native tool fs_glob cannot be projected as fs_glob: typed tool arguments require a string pattern field`

main-reference 的同一 mapper 对展示使用缺省空字符串/动态工具 fallback，真正的参数合法性由 tool schema 与 executor 裁决。展示失败不应取消已开始的 Agent loop，更不应改变工具执行和 turn 的业务结果。

### 6. 已产生副作用的命令被 outbox 当作未接受重投

第二轮 outbox row：

- `attempt_count=2`；
- 第一次 dispatch 已写入 tool call、presentation 和 internal items；
- Native `DriverDispatchReceipt` 只在整个 `run_turn()` 完成后写入内存 map；
- mapper/projector 中途报错时，driver 调用 `agent.abort()` 并返回 `DriverError`；
- outbox 对除 `DriverError::Lost` 外的所有错误统一 `release`，因此重投整个 `TurnStart`。

第二次 dispatch 重新从 `...-item-1` 编号，sequence `61` 命中 `item ...-item-1 was already started` critical protocol violation。随后：

- sequence `62`：active turn `Lost`；
- sequence `63/64`：第一轮与第二轮两个仍 active 的 operation 一起 `Lost`；
- sequence `65`：再次写 terminal 导致 `duplicate_terminal`。

日志中的 `Agent run aborted` 与 `Cancelled` 是此链路的结果：presentation mapper 报错后 Native 主动 abort Agent Core，不是 provider 自发中止，也不是正常工具轮的业务语义。

## 必须建立的数据库不变量

1. 一个 accepted operation 恰好一个 `OperationTerminal`；正常 `TurnTerminal(Completed)` 后不能遗留 active operation。
2. 一个逻辑工具调用只有一个 presentation producer、一个 presentation item ID 和一套 start/update/terminal lifecycle。
3. canonical internal item 不重复记录 application-owned user submission，也不把 tool result 伪装为 agent message。
4. driver receipt 在命令被 Agent Core 接受后即可确定；接受后的 stream/tool/presentation 错误不得令 outbox 重投整条命令。
5. outbox ack 只依赖 delivery acceptance，不等待 business terminal；pre-acceptance retry 与 post-acceptance terminal recovery严格分离。
6. 工具 invocation 使用 canonical runtime thread/turn/binding/Hook scope，任何 bootstrap identity 只允许用于纯编译，不得进入 executable handle。
7. 实际 DB 断言必须覆盖 operation、outbox、tool_call、internal item、presentation 与 terminal application effect，不再只断言 API 有终态事件。
