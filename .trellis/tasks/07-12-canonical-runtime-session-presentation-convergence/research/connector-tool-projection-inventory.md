# Connector 与 Tool Protocol Projection 初始审计

## 已确认退化点

- `agentdash-integration-codex/src/mapping.rs::item_content`只识别user/agent/reasoning/plan/dynamic/MCP少数类型，unknown item走`AgentMessage { text: item.to_string() }`。
- `agentdash-integration-native-agent/src/tool.rs`把工具统一投影为`RuntimeItemContent::ToolCall`，terminal经generic Tool Broker result收敛。
- `agentdash-agent-runtime/src/tool_broker.rs`当前conversation journal只保存tool name、arguments和generic JSON output，无法区分command/file/MCP/fs/Companion等presentation family。
- `agentdash-integration-remote-runtime`是Runtime Wire proxy，应保持typed event原样穿透，只替换本地placement/generation坐标。
- `AgentToolResult`当前包含`content/is_error/details: JsonValue`；`details.kind`承载多个产品工具的结构信息，但没有owner-declared conversation projector。

## 旧行为基线

使用以下Git对象做行为oracle：

```powershell
git show af21f9d7c^:crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs
git show af21f9d7c^:crates/agentdash-executor/src/connectors/codex_bridge.rs
```

旧Native mapper明确投影：Agent message/reasoning delta与final、item started/updated/completed、command output delta、shell exec、fs read/grep/glob、context compaction、usage、approval与typed error/platform facts。

## 必须完成的动态Inventory

W3执行时不得只使用静态名单。必须从最终Business Surface/Tool Catalog枚举所有`ToolContribution`，为每项记录：owner crate、runtime name、capability key、tool path、allowed channel、projector family、call/update/result fixture、frontend renderer。任何缺失projector的contribution在surface compile阶段失败。

## 初始Family清单

- Codex standard ThreadItem families
- Native Agent message/reasoning/provider events
- command/shell
- file write/edit/apply patch
- fs read/grep/glob
- MCP
- explicit dynamic tool
- Workspace Module/Canvas/VFS
- Companion/collaboration
- Task/Wait/Lifecycle product tools
- context compaction、usage、error、approval/user input
- Remote Runtime/Relay typed pass-through

该清单是起点，不替代运行时catalog inventory。

## W3 最终投影矩阵

### Driver inventory

| Driver | Profile / conformance evidence |
| --- | --- |
| Codex | generated owned ThreadItem strict transcode；message/reasoning/plan/command/file/MCP delta、usage、error、compaction与全部interaction；unknown method/item拒绝 |
| Native Agent | message/reasoning、provider status、usage、typed error、tool progress/approval/terminal；Agent Core不存在Plan事件，descriptor不声明该family；context compaction由Runtime-owned activation链承担 |
| Remote Runtime / Relay | RuntimeWire typed envelope原样透传；只转换source/canonical placement坐标；保留transient generation/sequence与terminal exactly-once |

Host admission验证每个driver profile至少声明message、reasoning、command、file change、MCP、context、typed interaction、transient identity、usage/error fidelity和正extension revision。

### Production Tool Catalog inventory

| Owner / tool family | Protocol projector |
| --- | --- |
| Application VFS `shell_exec` | `Command / ShellExec` |
| Application VFS `fs_read` / `fs_grep` / `fs_glob` | 对应AgentDash typed FS item |
| Application VFS `apply_patch` | Codex `FileChange` |
| Application VFS `mounts_list` | AgentDash `Vfs` |
| MCP direct / relay | Codex `McpToolCall` |
| Workspace Module list/describe/operate/invoke/present/unavailable | AgentDash `WorkspaceModule` |
| Companion request/respond | AgentDash `Companion` |
| Task read/write | AgentDash `Task` |
| Wait activity | AgentDash `Wait` |
| Lifecycle complete node | AgentDash `LifecycleComplete` |
| Runtime Gateway action | AgentDash `RuntimeAction` |
| Explicit dynamic / enterprise fixture | Codex `DynamicToolCall`，仅显式声明时可用 |

最终AgentFrame tool assembly从owner的`AgentTool::protocol_projector`读取descriptor；缺失descriptor在Business Surface admission失败。ToolBroker持久化descriptor，call/update/result/error使用同一family，不按tool name推断。ToolContribution/ToolBroker journal是tool started/update/terminal presentation的唯一owner：首次accept使用owner projector提交canonical ItemStarted，重复accept只校验同一canonical identity与payload；Native callback只携带显式分离的canonical/source坐标，不预投影第二份ItemStarted。Owner update经ToolExecutionRequest回到ToolBroker并发布为带canonical turn/item/binding generation的Runtime transient `ToolProgress`。

### Production owner chain evidence

| Owner | Call / update / terminal evidence |
| --- | --- |
| `ShellExecTool` | `Command` descriptor按typed operation分支：start依据真实cwd投影Platform/MountExec；read/write/status/resize/terminate投影`TerminalControl`并保留terminal identity、input/size、state/output/result。真实platform `pwd`经Registry与terminal projector验证 |
| `FsApplyPatchTool` | owner复用真实patch parser/normalized targets，把multi-file patch切为逐entry diff；add/update+move/delete分别保留path/kind/move_path且diff不混入其它文件。真实Registry失败路径和completed owner result验证started/completed/failed |
| Tool update callback | executor连续3次callback进入Broker/Managed Runtime；store按binding generation + turn在锁内分配sequence `1,2,3`与唯一event id，cursor replay只返回未消费update |
| Remote Runtime | ordered inbound queue保证前序DriverEvent完成projection后才结算dispatch response；HostPort callback保持并发处理，disconnect先提交authoritative `BindingLost`再关闭pending correlation |
