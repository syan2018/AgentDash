# AgentDash Relay Protocol

Relay Protocol 是云端后端与本机后端之间的 WebSocket wire contract。云端持有业务数据、session 事件和 VFS 事实；本机只提供本机执行、文件系统、MCP 探测/调用和 extension host runtime 能力。

当前协议以 `crates/agentdash-relay/src/protocol.rs` 的 `RelayMessage` 为事实源。所有消息使用统一 envelope：

```json
{
  "type": "command.tool.file_read",
  "id": "cmd-1",
  "payload": {}
}
```

命令响应复用同一个 `id`，成功时返回 `payload`，失败时返回 `error`：

```json
{
  "type": "response.tool.file_read",
  "id": "cmd-1",
  "error": { "code": "IO_ERROR", "message": "file not found" }
}
```

## Connection

本机主动连接云端 WebSocket，并在连接建立后发送第一条 `register`：

```json
{
  "type": "register",
  "id": "reg-1",
  "payload": {
    "backend_id": "backend-1",
    "name": "dev-machine",
    "version": "0.1.0",
    "capabilities": {
      "executors": [
        { "id": "CODEX", "name": "Codex", "variants": [], "available": true }
      ],
      "supports_cancel": true,
      "supports_discover_options": true,
      "mcp_servers": [
        { "name": "repo-tools", "transport": "stdio" }
      ]
    },
    "workspace_roots": ["F:/Projects/AgentDash"]
  }
}
```

云端返回 `register_ack`。心跳使用 `ping` / `pong`，payload 分别携带 `server_time` / `client_time`。

`workspace_roots` 是本机上报的可访问目录集合；执行类命令的真实边界由每条命令的 `mount_root_ref` 决定。

## Prompt Runtime

`command.prompt` 用于云端启动本机第三方 agent session。当前 prompt 输入只通过 `prompt_blocks` 表达，执行边界通过 `mount_root_ref` 表达：

```json
{
  "type": "command.prompt",
  "id": "prompt-1",
  "payload": {
    "session_id": "session-1",
    "follow_up_session_id": null,
    "prompt_blocks": [
      { "type": "text", "text": "实现登录表单" }
    ],
    "mount_root_ref": "F:/Projects/AgentDash",
    "workspace_identity_kind": "git_repo",
    "workspace_identity_payload": { "remote_url": "git@example.com:org/repo.git" },
    "working_dir": "packages/app-web",
    "env": { "NODE_ENV": "development" },
    "executor_config": {
      "executor": "CODEX",
      "provider_id": "openai",
      "model_id": "gpt-5",
      "agent_id": "codex",
      "thinking_level": "medium",
      "permission_policy": "auto-edit"
    },
    "mcp_servers": []
  }
}
```

成功响应：

```json
{
  "type": "response.prompt",
  "id": "prompt-1",
  "payload": { "turn_id": "turn-1", "status": "started" }
}
```

运行中的用户 steering 使用 `command.steer`：

```json
{
  "type": "command.steer",
  "id": "steer-1",
  "payload": {
    "session_id": "session-1",
    "input": [{ "type": "text", "text": "先只改后端" }],
    "expected_turn_id": "turn-1"
  }
}
```

取消使用 `command.cancel`，payload 只包含 `session_id`。

本机执行输出使用 `event.session_notification` 上报，payload 是 `SessionNotificationPayload`：顶层 `session_id` 加 `agentdash-agent-protocol` 的 Codex app-server notification。执行状态变更使用 `event.session_state_changed`，携带 `session_id`、可选 `turn_id`、`state` 与可选 `message`。

## Tool Commands

`command.tool.*` 是云端运行时对本机文件系统和 shell 的执行入口。所有 tool command payload 都包含 `call_id` 和 `mount_root_ref`；`mount_root_ref` 是本次调用的本机执行边界，路径字段均按该边界解析。

### File Read

```json
{
  "type": "command.tool.file_read",
  "id": "read-1",
  "payload": {
    "call_id": "tool-1",
    "path": "src/main.rs",
    "mount_root_ref": "F:/Projects/AgentDash",
    "offset": 0,
    "limit": 120
  }
}
```

`offset` 与 `limit` 是当前模型的可选范围读取参数；省略表示读取全文。二进制读取复用相同 payload，消息类型为 `command.tool.file_read_binary`，响应为 base64 bytes、MIME type 和 size。

### File Mutation

写入、删除、重命名和 patch 分别使用：

```json
{ "type": "command.tool.file_write", "id": "write-1", "payload": { "call_id": "tool-2", "path": "src/lib.rs", "content": "text", "mount_root_ref": "F:/Projects/AgentDash" } }
{ "type": "command.tool.file_delete", "id": "delete-1", "payload": { "call_id": "tool-3", "path": "src/old.rs", "mount_root_ref": "F:/Projects/AgentDash" } }
{ "type": "command.tool.file_rename", "id": "rename-1", "payload": { "call_id": "tool-4", "from_path": "src/a.rs", "to_path": "src/b.rs", "mount_root_ref": "F:/Projects/AgentDash" } }
{ "type": "command.tool.apply_patch", "id": "patch-1", "payload": { "call_id": "tool-5", "patch": "*** Begin Patch\n*** End Patch\n", "mount_root_ref": "F:/Projects/AgentDash" } }
```

Patch 响应返回 `added`、`modified`、`deleted` 路径列表。

### File List And Search

```json
{
  "type": "command.tool.file_list",
  "id": "list-1",
  "payload": {
    "call_id": "tool-6",
    "path": "src",
    "mount_root_ref": "F:/Projects/AgentDash",
    "pattern": "*.rs",
    "recursive": true
  }
}
```

```json
{
  "type": "command.tool.search",
  "id": "search-1",
  "payload": {
    "call_id": "tool-7",
    "mount_root_ref": "F:/Projects/AgentDash",
    "query": "needle",
    "path": "src",
    "is_regex": false,
    "include_glob": "*.rs",
    "max_results": 50,
    "context_lines": 0,
    "case_sensitive": true,
    "multiline": false,
    "before_lines": 0,
    "after_lines": 0
  }
}
```

Search 响应返回 `hits` 和 `truncated`。每个 hit 包含 `path`、`line_number`、`content`、`context_before`、`context_after`。

### Shell Exec

```json
{
  "type": "command.tool.shell_exec",
  "id": "shell-1",
  "payload": {
    "call_id": "tool-8",
    "command": "pnpm test",
    "mount_root_ref": "F:/Projects/AgentDash",
    "cwd": "packages/app-web",
    "timeout_ms": 30000
  }
}
```

`cwd` 可省略；提供时必须仍位于 `mount_root_ref` 边界内。执行期间本机可发送 `event.tool.shell_output`，payload 包含 `call_id`、`delta` 和 `stream`（`stdout` / `stderr`）。最终响应包含 `exit_code`、`stdout`、`stderr`。

## VFS Materialization

`command.vfs.materialize` 将云端 VFS 资源物化到本机 session cache 或 working copy：

```json
{
  "type": "command.vfs.materialize",
  "id": "mat-1",
  "payload": {
    "session_id": "session-1",
    "turn_id": "turn-1",
    "tool_call_id": "tool-1",
    "plan_id": "plan-1",
    "plan_kind": "skill_resource_set",
    "source_uri": "skill-assets://skills/reviewer/scripts/check.sh",
    "root_uri": "skill-assets://skills/reviewer",
    "mount_id": "skill-assets",
    "provider": "skill_asset_fs",
    "primary_relative_path": "scripts/check.sh",
    "target_kind": "file",
    "access_mode": "read_only",
    "entries": [
      {
        "relative_path": "scripts/check.sh",
        "content": { "encoding": "utf8_text", "text": "echo ok\n" },
        "digest": "sha256:test",
        "size_bytes": 8,
        "mime_hint": "text/x-shellscript",
        "executable_hint": true
      }
    ],
    "cache_scope": "session",
    "ttl_ms": 60000
  }
}
```

响应返回 `source_uri`、`local_root_path`、`primary_local_path`、可选 `primary_local_url`、`access_mode`、`manifest_digest`、`total_size_bytes`、`entry_count`、`dirty` 与 `cache_hit`。

## MCP Relay

MCP relay 支持一次性 transport probe、本机 server tool 列表、tool 调用和关闭连接：

```json
{
  "type": "command.mcp_probe_transport",
  "id": "mcp-probe-1",
  "payload": {
    "transport": {
      "type": "stdio",
      "command": "npx",
      "args": ["@modelcontextprotocol/server-filesystem"],
      "env": [],
      "cwd": "F:/Projects/AgentDash"
    }
  }
}
```

```json
{
  "type": "command.mcp_list_tools",
  "id": "mcp-list-1",
  "payload": {
    "server": {
      "name": "repo-tools",
      "transport": {
        "type": "http",
        "url": "http://127.0.0.1:7357/mcp",
        "headers": [{ "name": "x-workspace", "value": "agentdash" }]
      }
    }
  }
}
```

```json
{
  "type": "command.mcp_call_tool",
  "id": "mcp-call-1",
  "payload": {
    "server": {
      "name": "repo-tools",
      "transport": {
        "type": "http",
        "url": "http://127.0.0.1:7357/mcp",
        "headers": [{ "name": "x-workspace", "value": "agentdash" }]
      }
    },
    "tool_name": "read_file",
    "arguments": { "path": "README.md" }
  }
}
```

```json
{ "type": "command.mcp_close", "id": "mcp-close-1", "payload": { "server_name": "repo-tools" } }
```

`command.mcp_list_tools` 与 `command.mcp_call_tool` 的 server payload 是完整 `McpServerRelay { name, transport }`，原因是云端发送的是已经解析 runtime binding 的执行面 MCP server，本机不能只靠静态 server name 还原 transport。`McpTransportConfigRelay` 支持 `http`、`sse` 和 `stdio`。HTTP/SSE transport 携带 `url` 与 headers；stdio transport 携带 `command`、`args`、`env` 和可选 `cwd`。Tool info 使用 `name`、`description`、`parameters_schema`。

## Extension Runtime

Extension runtime 命令由云端调用本机 TypeScript Extension Host。

Action invoke：

```json
{
  "type": "command.extension_action_invoke",
  "id": "ext-action-1",
  "payload": {
    "extension_key": "local-hello",
    "extension_id": "local-hello",
    "action_key": "local-hello.profile",
    "project_id": "project-1",
    "session_id": "session-1",
    "input": { "verbose": true },
    "package_artifact": {
      "artifact_id": "artifact-1",
      "archive_digest": "sha256:0123456789abcdef"
    },
    "runtime_extensions": [],
    "workspace": {
      "mount_id": "main",
      "root_ref": "F:/Projects/AgentDash"
    },
    "trace_id": "trace-1",
    "invocation_id": "invoke-1"
  }
}
```

Protocol invoke：

```json
{
  "type": "command.extension_protocol_invoke",
  "id": "ext-protocol-1",
  "payload": {
    "provider_extension_key": "protocol-demo",
    "provider_extension_id": "protocol-demo",
    "protocol_key": "protocol-demo.api",
    "method": "echo",
    "project_id": "project-1",
    "session_id": "session-1",
    "input": { "text": "hello" },
    "package_artifact": {
      "artifact_id": "artifact-1",
      "archive_digest": "sha256:0123456789abcdef"
    },
    "consumer": {
      "kind": "extension_panel",
      "extension_key": "protocol-demo",
      "extension_id": "protocol-demo",
      "dependency_alias": "self"
    },
    "workspace": {
      "mount_id": "main",
      "root_ref": "F:/Projects/AgentDash"
    },
    "trace_id": "trace-1",
    "invocation_id": "invoke-2"
  }
}
```

Action response returns `extension_key`、`extension_id`、`action_key`、`output`、`metadata`。Protocol response returns provider identity、`protocol_key`、`method`、`output`、`metadata`。

## Discovery And Workspace Detection

Executor discovery uses `command.discover` and `command.discover_options`; streamed option patches use `event.discover_options_patch`.

Workspace setup capabilities are:

- `command.workspace_detect`：probe a local directory and return workspace metadata.
- `command.workspace_detect_git`：probe Git metadata.
- `command.browse_directory`：browse local directories for setup UI.

These commands are setup/discovery surfaces. Runtime file access uses VFS mounts and `command.tool.*`.

## Serialization Contract

Prompt and tool command payloads reject unknown fields. Current required fields must be present at deserialization time. Optional fields express operation-specific choices such as range reads, shell timeouts, scoped search paths, and extension workspace projection.
