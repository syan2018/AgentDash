# Design: relay protocol 当前模型收敛

## Scope

本任务只收敛 relay wire protocol 的当前事实模型：`agentdash-relay` 的协议类型、crate 内 serde 测试，以及 `docs/relay-protocol.md`。调用方盘点显示当前 API 构造路径已经使用 `mount_root_ref`，本机 handler 也以 `mount_root_ref` 做执行边界校验；本任务不移动 application ports、extension manifest、frontend workspace tab 或 legacy identity 链路。

## Current Model

- `command.prompt` 以 `prompt_blocks` 表达用户输入块，以 `mount_root_ref` 表达本轮执行的工作区边界。
- `command.tool.*` 以 `call_id` 关联工具调用，以 `mount_root_ref` 约束本机文件和 shell 执行边界。
- VFS 物化使用 `command.vfs.materialize`，由云端给出 source/root URI、mount、provider、访问模式、entry 内容与 cache scope，本机返回本地物化路径和 manifest 信息。
- MCP relay 使用 `command.mcp_probe_transport`、`command.mcp_list_tools`、`command.mcp_call_tool`、`command.mcp_close`。
- Extension runtime 使用 `command.extension_action_invoke` 与 `command.extension_channel_invoke`，携带 extension identity、package artifact、workspace projection、trace 与 invocation id。

## Decisions

- Prompt/tool payload 拒绝未知字段，原因是 relay 是云端与本机的共享 wire contract，旧字段进入时应在反序列化边界失败，而不是被忽略后产生不确定执行。
- Tool search/list 的执行选项作为当前 payload 的显式字段传输，缺失时反序列化失败，原因是这些字段会影响本机搜索/遍历语义。
- `offset`/`limit`、`cwd`、`timeout_ms`、`pattern`、`path`、`include_glob` 等保留为 `Option`，原因是它们表达当前业务上的可选输入，而不是旧协议默认。
- 文档以当前 crate 类型为准，删除旧 `prompt`、`workspace_root`、`workspace_files.*` 叙述。
