# REVIEW-004: local-runtime

## 范围

- `crates/agentdash-local/src`
- 重点：`tool_executor.rs`、`handlers/mod.rs`、`handlers/relay_mcp_servers.rs`、`extensions/host/permissions.rs`

## 实现级可修复问题

### LOCAL-IMPL-001: `permissions.rs` 文件名与职责不符

- 证据：`crates/agentdash-local/src/extensions/host/permissions.rs:22` `resolve_host_api()` 做 Host API 路由，`90-330` 实现 workspace/process/http/env/runtime API，`332-398` 才是真正权限裁决。
- 影响：权限裁决、参数解析、执行器调用和 HTTP/进程执行混在一个文件里，后续改任一 Host API 都容易误碰权限逻辑。
- 建议：拆成 `host_api.rs` 路由、`permission_guard.rs` 权限裁决、`workspace_api.rs`、`process_api.rs`、`http_api.rs`，`permissions.rs` 只保留声明权限判断。

### LOCAL-IMPL-002: Extension Host 重复构造 `ToolExecutor`

- 证据：`permissions.rs:97`、`113`、`128`、`151`、`186`、`233` 每个 Host API 都 `ToolExecutor::new(executor_workspace_roots(active))`；`466-473` 每次 clone roots 并补默认 root。
- 影响：workspace roots 装配逻辑散落在 API handler 内，和 `CommandHandler` 持有的 `tool_executor` 形成两套执行边界入口。
- 建议：在 `ActiveExtension` 或 Host API context 中预构造 `ToolExecutor` / `WorkspaceBoundary`，Host API 只消费同一个边界对象。

### LOCAL-IMPL-003: `process.exec` 绕开 `ToolExecutor` 自己管理进程

- 证据：`permissions.rs:207` 附近 `process.shell` 走 `ToolExecutor::shell_exec()`，但 `process.exec` 直接 `Command::new`、拼 args/env/cwd/stdout/stderr，并独立处理 timeout/output。
- 影响：shell 与 exec 的 cwd、timeout、env、输出截断和错误语义会自然分叉。
- 建议：抽一个 `ProcessExecutor`，让 `ToolExecutor` 和 Extension Host 共用；`process.shell`、`process.exec` 只表达 shell-string vs argv 两种输入形态。

### LOCAL-IMPL-004: 写路径解析函数有副作用

- 证据：`crates/agentdash-local/src/tool_executor.rs:735` `resolve_path_for_write_with_root()` 在 `748` 行 `create_dir_all(parent)`，调用方 `file_write()` 又在 `174-176` 行创建 parent。
- 影响：名为 resolve 的函数实际会改文件系统；`file_rename()` 调用该函数时也会提前创建目标目录，失败路径会留下目录。
- 建议：把“解析/校验目标路径”和“创建 parent 目录”分开；只有具体写入/rename 执行动作负责创建目录。

### LOCAL-IMPL-005: workspace root 校验重复 canonicalize 登记 roots

- 证据：`tool_executor.rs:87` `validate_workspace_root()` 每次调用都在 `110-112` 行对 `self.workspace_roots` 逐个 `std::fs::canonicalize(root)`。
- 影响：每次文件/搜索/shell 调用都会重复访问文件系统；如果登记 root 运行中被移动，错误表现还会随调用时机变化。
- 建议：`ToolExecutor::new()` 阶段 canonicalize 并保存 canonical roots；运行期校验只做 `starts_with`。

### LOCAL-IMPL-006: Relay 错误映射复制且丢失语义

- 证据：`crates/agentdash-local/src/handlers/tool_calls.rs:28` 等文件读写删除分支重复 `RelayError::io_error(e.to_string())`，shell/search 又用 `runtime_error(e.to_string())`。
- 影响：`PathNotAccessible`、`InvalidPath`、`Timeout` 都被压成字符串，调用侧无法稳定区分权限拒绝、参数错误和运行失败。
- 建议：增加 `impl From<ToolError> for RelayError` 或 `tool_error_to_relay_error()`，集中映射 `InvalidPath`、`PathNotAccessible`、`Timeout`、`Io`。

### LOCAL-IMPL-007: MCP server 解析静默吞错

- 证据：`crates/agentdash-local/src/handlers/relay_mcp_servers.rs:11` 非对象直接 `continue`，缺 name 默认空串，headers/env 内非法项通过 `filter_map` 丢弃，函数只返回 `Vec<SessionMcpServer>`。
- 影响：云端传入错误配置时只少一个 server 或少几个 header/env，prompt 启动仍继续，排查成本高。
- 建议：返回 `Result<Vec<SessionMcpServer>, RelayMcpServerParseError>` 或至少返回 `(servers, diagnostics)`，缺 name、非法 header/env 应成为结构化错误。

## 模块级 refactor 候选

这些问题未按当前口径进入 architecture backlog，但应在 local-runtime 模块修复中优先处理。

- `CommandHandler` 已成为本机 runtime 总线对象：`handlers/mod.rs:40` 持有 backend、workspace、tool、session、connector、MCP、terminal、materialization、extension artifact 等 15 个字段，`handle()` 集中 match 所有 relay command。
- `ToolExecutor` 职责过宽：同一 impl 覆盖 workspace root 校验、文件读写删改、shell 执行、文件列表和搜索。
- prompt MCP servers 仍是 raw JSON：`crates/agentdash-relay/src/protocol/prompt.rs:33` 使用 `Vec<serde_json::Value>`，local 侧再手写解析；这是跨 relay/local 的模块级候选，达到协议变更前不进 architecture backlog。
- Extension Host 暴露未接通 API：`extension.channel_invoke` 和 `runtime.invoke` 固定返回未接入，预研阶段应接通或从 surface 移除。
- env/process 权限边界不对称：`env.get` 与 `process.exec options.env` 分属不同权限表达。
- 搜索存在 rg 与 fallback 双实现链路，两套实现各自解释 ignore policy、context、truncation；预研期可考虑直接要求 rg 并删除 fallback。
