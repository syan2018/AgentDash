# Research: local-runtime 模块明确收敛方案

- Query: 基于 `reviews/004-local-runtime.md`，研究 `crates/agentdash-local/src` 的可执行模块级修复批次，并区分直接 implement、延后实现与架构项。
- Scope: internal
- Date: 2026-06-11

## Findings

### 依据与约束

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)`；本文件按用户本轮明确指定的路径写入，未推断其它任务目录。
- 本次只做 research，未修改源码，未运行 cargo 测试。
- `.trellis/spec/backend/error-handling.md` 要求错误语义在层边界保留，不能先 `.to_string()` 抹平后再由上层解析字符串；这直接支持 `ToolError -> RelayError` 集中映射。
- `.trellis/spec/cross-layer/desktop-local-runtime.md` 约束 Local TS Extension Host 的 workspace/process Host API 使用 relay payload 中的 session workspace root，原因是插件执行目录必须跟随本次 session 的工作区事实。
- `.trellis/spec/backend/quality-guidelines.md` 要求 Rust 代码使用结构化错误、clippy/rustfmt 基线；本轮修复宜保持包级 `cargo check/test` 可验证。
- `.trellis/spec/backend/runtime-gateway.md` 说明 extension action 经 relay/local backend 落到 TS Extension Host；`runtime.invoke` / `extension.channel_invoke` 接通属于 runtime/extension registry 语义，不应混入本轮小修。

### Files Found

- `crates/agentdash-local/src/tool_executor.rs` - 本机 file/shell/search 工具执行器，包含 workspace root 校验、写路径解析、rg/fallback 搜索和相关单测。
- `crates/agentdash-local/src/handlers/tool_calls.rs` - relay tool call handler，将 `ToolExecutor` 结果包装为 `RelayMessage`。
- `crates/agentdash-local/src/handlers/relay_mcp_servers.rs` - relay prompt 中 raw JSON MCP server 配置解析。
- `crates/agentdash-local/src/handlers/prompt.rs` - prompt command handler，调用 MCP parser 并组装 `LaunchCommand`。
- `crates/agentdash-local/src/handlers/mod.rs` - `CommandHandler` 总线对象和 relay message 分发入口。
- `crates/agentdash-local/src/extensions/host/permissions.rs` - 当前同时承担 Host API 路由、workspace/process/http/env/runtime API 和权限裁决。
- `crates/agentdash-local/src/extensions/host/process.rs` - Node extension host 进程协议处理，收到 `host_api_request` 后调用 `resolve_host_api()`。
- `crates/agentdash-local/src/extensions/host/manager.rs` - 激活 extension 时构造 `ActiveExtension`。
- `crates/agentdash-local/src/extensions/host/mod.rs` - extension host 模块声明与公开类型。
- `crates/agentdash-local/src/runtime.rs` - 本机 runtime 组装入口，当前在 `LocalRuntimeConfig::new()` 与 `canonicalize_workspace_roots()` 处预处理 workspace roots。
- `crates/agentdash-relay/src/error.rs` - `RelayError` 和 `RelayErrorCode` 定义，已有 `Forbidden`、`InvalidMessage`、`Timeout` 等机器可读错误码。
- `crates/agentdash-relay/src/protocol/prompt.rs` - `CommandPromptPayload.mcp_servers` 仍是 `Vec<serde_json::Value>`，属于跨层 raw JSON contract。

### Code Patterns

- `ToolExecutor` 只保存 `workspace_roots: Vec<PathBuf>`，构造函数不 canonicalize；`validate_workspace_root()` 每次运行时 canonicalize mount root，再对每个登记 root 重复 `std::fs::canonicalize(root)`：`crates/agentdash-local/src/tool_executor.rs:25`, `crates/agentdash-local/src/tool_executor.rs:83`, `crates/agentdash-local/src/tool_executor.rs:88`, `crates/agentdash-local/src/tool_executor.rs:110`。
- runtime 启动配置已在外层做过 roots canonicalization，但使用 `unwrap_or_else` 保留失败路径并继续：`crates/agentdash-local/src/runtime.rs:49`, `crates/agentdash-local/src/runtime.rs:373`。
- `resolve_path_for_write_with_root()` 命名是解析函数，但会创建 parent 目录；`file_write()` 与 `file_rename()` 随后也创建 parent，失败路径可能留下目录：`crates/agentdash-local/src/tool_executor.rs:165`, `crates/agentdash-local/src/tool_executor.rs:174`, `crates/agentdash-local/src/tool_executor.rs:188`, `crates/agentdash-local/src/tool_executor.rs:204`, `crates/agentdash-local/src/tool_executor.rs:735`, `crates/agentdash-local/src/tool_executor.rs:748`。
- search 当前先探测 `rg`，没有则走 fallback；两条链路各自实现 ignore/context/truncation 逻辑和测试：`crates/agentdash-local/src/tool_executor.rs:412`, `crates/agentdash-local/src/tool_executor.rs:429`, `crates/agentdash-local/src/tool_executor.rs:433`, `crates/agentdash-local/src/tool_executor.rs:581`, `crates/agentdash-local/src/tool_executor.rs:928`, `crates/agentdash-local/src/tool_executor.rs:1281`。
- `handlers/tool_calls.rs` 在每个分支里重复把 `ToolError` 压成 `RelayError::io_error(e.to_string())` 或 `runtime_error(e.to_string())`：`crates/agentdash-local/src/handlers/tool_calls.rs:28`, `crates/agentdash-local/src/handlers/tool_calls.rs:85`, `crates/agentdash-local/src/handlers/tool_calls.rs:223`, `crates/agentdash-local/src/handlers/tool_calls.rs:291`。
- `RelayErrorCode` 已具备可用于集中映射的 `Forbidden`、`InvalidMessage`、`IoError`、`RuntimeError`、`Timeout`：`crates/agentdash-relay/src/error.rs:36`, `crates/agentdash-relay/src/error.rs:40`, `crates/agentdash-relay/src/error.rs:44`, `crates/agentdash-relay/src/error.rs:48`, `crates/agentdash-relay/src/error.rs:67`。
- MCP parser 当前返回 `Vec<SessionMcpServer>`，非对象、缺 type、非法 header/env/arg 都可能被 `continue` 或 `filter_map` 静默丢弃；`prompt.rs` 无错误处理入口，直接把解析结果交给 launch：`crates/agentdash-local/src/handlers/relay_mcp_servers.rs:11`, `crates/agentdash-local/src/handlers/relay_mcp_servers.rs:15`, `crates/agentdash-local/src/handlers/relay_mcp_servers.rs:20`, `crates/agentdash-local/src/handlers/relay_mcp_servers.rs:49`, `crates/agentdash-local/src/handlers/relay_mcp_servers.rs:95`, `crates/agentdash-local/src/handlers/prompt.rs:134`。
- `permissions.rs` 顶部 `resolve_host_api()` 路由所有 host API；同文件中 workspace API、process API、http API 与权限裁决混在一起：`crates/agentdash-local/src/extensions/host/permissions.rs:22`, `crates/agentdash-local/src/extensions/host/permissions.rs:90`, `crates/agentdash-local/src/extensions/host/permissions.rs:170`, `crates/agentdash-local/src/extensions/host/permissions.rs:271`, `crates/agentdash-local/src/extensions/host/permissions.rs:332`。
- Host API 每个 workspace/process handler 重新 `ToolExecutor::new(executor_workspace_roots(active))`；roots helper 每次 clone roots 并补 default root：`crates/agentdash-local/src/extensions/host/permissions.rs:97`, `crates/agentdash-local/src/extensions/host/permissions.rs:113`, `crates/agentdash-local/src/extensions/host/permissions.rs:128`, `crates/agentdash-local/src/extensions/host/permissions.rs:151`, `crates/agentdash-local/src/extensions/host/permissions.rs:186`, `crates/agentdash-local/src/extensions/host/permissions.rs:233`, `crates/agentdash-local/src/extensions/host/permissions.rs:466`。
- `process.shell` 走 `ToolExecutor::shell_exec()`，`process.exec` 自己 `Command::new()`、处理 args/env/cwd/timeout/output，语义会自然分叉：`crates/agentdash-local/src/extensions/host/permissions.rs:186`, `crates/agentdash-local/src/extensions/host/permissions.rs:207`, `crates/agentdash-local/src/extensions/host/permissions.rs:237`, `crates/agentdash-local/src/extensions/host/permissions.rs:250`。
- `ActiveExtension` 当前只保存原始 workspace roots；激活时由 manager 直接复制 activation roots：`crates/agentdash-local/src/extensions/host/process.rs:26`, `crates/agentdash-local/src/extensions/host/process.rs:30`, `crates/agentdash-local/src/extensions/host/manager.rs:180`, `crates/agentdash-local/src/extensions/host/manager.rs:184`。
- `CommandHandler` 是本机 relay 总线对象，字段和 match 分发都较大，但拆分会触及多 handler 面：`crates/agentdash-local/src/handlers/mod.rs:40`, `crates/agentdash-local/src/handlers/mod.rs:112`, `crates/agentdash-local/src/handlers/mod.rs:153`。

### 可立即实施的并行批次

#### Batch A: ToolExecutor 边界与搜索收敛

- 写入范围：只写 `crates/agentdash-local/src/tool_executor.rs`。
- 可并行性：与 Batch B/C/D 无文件冲突；后续 Batch E 需等本批完成。
- 核心改法：
  - 在 `ToolExecutor` 内增加边界状态，例如 `workspace_roots_configured: bool` + `canonical_workspace_roots: Vec<PathBuf>`，`new()` 阶段 canonicalize + dedupe 登记 roots。
  - 运行期 `validate_workspace_root()` 只 canonicalize 本次 mount root，然后对已保存 canonical roots 做 `starts_with`；如果构造时 roots 非空但全不可用，不应退化成“空 roots 允许任意 mount root”。
  - 拆分写路径解析：`resolve_path_for_write_with_root()` 不再创建目录；改为只 normalize + 校验目标 parent 边界。`file_write()` / `file_rename()` 作为实际写动作负责 `create_dir_all(parent)`。
  - 删除 fallback search 链路，`search()` 找不到 `rg` 时返回结构化 `ToolError`；保留/收敛 `ripgrep_policy_args()` 作为唯一搜索策略事实源。
  - 删除 `FallbackCollector`、`fallback_search()` 和 fallback 专用测试；新增 `resolve_path_for_write_does_not_create_parent`、`registered_roots_are_canonicalized_once`、`search_requires_ripgrep_when_unavailable` 或等价测试。
- 风险：
  - search 会从“无 rg 时仍能慢速搜索”变成“无 rg 直接失败”；符合预研期不保留 fallback 的项目口径，但会把 `rg` 变成本地开发/运行前置条件。
  - roots 构造不应因无效登记路径导致边界打开；这是实现时最重要的安全点。
  - 写路径校验要考虑 parent 不存在时的边界判断，不能为了去掉副作用而允许 `..` 或 symlink 逃逸。
- 验证命令：
  - `rg --version`
  - `cargo test -p agentdash-local tool_executor`
  - `cargo check -p agentdash-local`
- 结论：直接 implement。

#### Batch B: ToolError 到 RelayError 集中映射

- 写入范围：只写 `crates/agentdash-local/src/handlers/tool_calls.rs`。
- 可并行性：与 Batch A/C/D 无文件冲突；如果 Batch A 新增 `ToolError` 变体，合并时补一个 match arm 即可。
- 核心改法：
  - 在 `tool_calls.rs` 内新增单一 `tool_error_to_relay_error(error: ToolError) -> RelayError`，所有 tool call handler 使用该函数。
  - 建议映射：
    - `ToolError::PathNotAccessible(_)` -> `RelayError::new(RelayErrorCode::Forbidden, ...)`
    - `ToolError::InvalidPath(_)` -> `RelayError::invalid_message(...)`
    - `ToolError::Timeout(_)` -> `RelayError::timeout(...)`
    - `ToolError::Io(_)` -> `RelayError::io_error(...)`
    - `ToolError::PatchApply(_)` -> `RelayError::runtime_error(...)`
    - Batch A 若新增缺少 `rg` 之类 runtime dependency 变体，可映射为 `RuntimeError` 或 `ExecutorUnavailable`，但要集中在同一函数。
  - 添加小单测覆盖 `PathNotAccessible`、`InvalidPath`、`Timeout` 三类机器码。
- 风险：
  - relay 客户端如果此前只看 message 文本不会受影响；如果已有地方隐式假设 file path 问题都是 `IO_ERROR`，本批会暴露真实错误码，这是预期收敛。
- 验证命令：
  - `cargo test -p agentdash-local tool_error_to_relay_error`
  - `cargo check -p agentdash-local`
- 结论：直接 implement。

#### Batch C: MCP server 解析 fail-closed

- 写入范围：
  - `crates/agentdash-local/src/handlers/relay_mcp_servers.rs`
  - `crates/agentdash-local/src/handlers/prompt.rs`
- 可并行性：与 Batch A/B/D 无文件冲突。
- 核心改法：
  - `parse_relay_mcp_servers(raw)` 改为 `Result<Vec<SessionMcpServer>, RelayMcpServerParseError>`。
  - `RelayMcpServerParseError` 用 `thiserror::Error`，错误消息包含 index/server name/字段名。
  - 缺 name 或 blank name、非对象 entry、缺/未知 type、http/sse 缺 URL、stdio 缺 command、headers/env/args 中非法 item 都返回错误，不再 `continue` 或 `filter_map`。
  - `prompt.rs` 在组装 `LaunchCommand` 前处理 Err，返回 `ResponsePrompt` + `RelayError::invalid_message("mcp_servers 配置非法: ...")`。
  - 更新现有 `relay_mcp_servers_require_explicit_type`、`relay_mcp_servers_reject_unknown_type` 测试为 `expect_err`，新增非法 header/env/arg 测试。
- 风险：
  - 云端若仍发送部分坏 MCP 配置，prompt 会 fail-fast，不再静默少启一个 server；这是本批目的。
  - 只改 local parser，不改 `CommandPromptPayload.mcp_servers: Vec<Value>`，因此不是跨层 contract 变更。
- 验证命令：
  - `cargo test -p agentdash-local relay_mcp_servers`
  - `cargo test -p agentdash-local handle_prompt`
  - `cargo check -p agentdash-local`
- 结论：直接 implement。

#### Batch D: Extension Host API 文件拆分与 ToolExecutor 复用

- 写入范围：
  - `crates/agentdash-local/src/extensions/host/mod.rs`
  - `crates/agentdash-local/src/extensions/host/process.rs`
  - `crates/agentdash-local/src/extensions/host/manager.rs`
  - `crates/agentdash-local/src/extensions/host/host_api.rs`（新增）
  - `crates/agentdash-local/src/extensions/host/permission_guard.rs`（新增）
  - `crates/agentdash-local/src/extensions/host/workspace_api.rs`（新增）
  - `crates/agentdash-local/src/extensions/host/process_api.rs`（新增）
  - `crates/agentdash-local/src/extensions/host/http_api.rs`（新增）
  - `crates/agentdash-local/src/extensions/host/permissions.rs`（删除或缩减为迁移后空壳；推荐删除模块声明，避免名称继续误导）
- 可并行性：与 Batch A/B/C 无文件冲突，前提是 Batch A 保持 `ToolExecutor::new(Vec<PathBuf>) -> Self` 签名稳定。
- 核心改法：
  - `host_api.rs` 只保留 `resolve_host_api()` 路由和通用参数 helper。
  - `permission_guard.rs` 放 `require_declared_permission()`、`action_permission_denial_message()`。
  - `workspace_api.rs` 放 workspace read/write/list/stat。
  - `process_api.rs` 放 process.shell/process.exec 的当前行为实现。
  - `http_api.rs` 放 http.fetch 与 header parsing。
  - `ActiveExtension` 增加预构造 `tool_executor: ToolExecutor`；`manager.rs` 激活时用 `activation.workspace_roots + default_workspace_root` 构造一次。
  - workspace/process API 不再调用 `ToolExecutor::new(executor_workspace_roots(active))`，统一消费 `active.tool_executor`。
- 风险：
  - 主要是 Rust module visibility 和测试 import 调整，行为应保持不变。
  - 如果同时推进 Batch A 且修改 `ToolExecutor::new` 签名，本批会产生冲突；建议 Batch A 不改签名，或先合并 A 再做 D。
- 验证命令：
  - `cargo test -p agentdash-local host_api`
  - `cargo test -p agentdash-local workspace_host_apis`
  - `cargo test -p agentdash-local process_host_apis`
  - `cargo check -p agentdash-local`
- 结论：直接 implement。

### 应延后但仍属于模块级 implement 的批次

#### Batch E: process.exec 与 shell 共享进程执行器

- 写入范围（建议在 Batch A/D 之后）：
  - `crates/agentdash-local/src/tool_executor.rs`
  - `crates/agentdash-local/src/extensions/host/process_api.rs`
  - 可选新增 `crates/agentdash-local/src/process_executor.rs` 或 `crates/agentdash-local/src/tool_executor/process.rs`
- 延后原因：
  - 与 Batch A 都会改 `tool_executor.rs`，与 Batch D 都会改 process Host API 文件；并行会冲突。
  - 当前最小收益是避免 shell/exec 语义继续分叉，但不阻塞 root/write/error/MCP/permissions 的快速收敛。
- 核心改法：
  - 抽 `ProcessExecutor`，统一 cwd resolve、timeout、stdout/stderr 解码、exit_code、输出截断前原始结果。
  - `ToolExecutor::shell_exec()` / `shell_exec_streaming()` 和 extension `process.shell` / `process.exec` 共用同一执行边界；API 层只表达 shell string vs argv 两种输入。
  - `process.exec options.env` 的解析先保持当前语义；env/process 权限对称性另列架构项，不混入本批。
- 风险：
  - streaming shell 与 non-streaming exec 的抽象边界要谨慎，避免为了共用而拉大一次改动。
  - Windows shell 包装逻辑在 `shell_command()` 中有 UTF-8 输出设置，迁移时必须保留。
- 验证命令：
  - `cargo test -p agentdash-local process_host_apis`
  - `cargo test -p agentdash-local resolve_shell_cwd`
  - `cargo check -p agentdash-local`
- 结论：延后到 Batch A/D 完成后直接 implement，不列架构。

### 暂不 implement 的架构项

仅以下问题符合“预计超过 10 个文件或改变跨层 contract”的架构项口径：

- Relay MCP servers typed contract：`CommandPromptPayload.mcp_servers` 当前是 `Vec<serde_json::Value>`（`crates/agentdash-relay/src/protocol/prompt.rs:34`）。若要把 MCP server DTO 上移到 relay 协议并要求云端 producer 发送 typed payload，会改变 cloud/local wire contract，需列架构 backlog。Batch C 只做 local fail-closed parser，不属于架构项。
- `CommandHandler` 总线对象拆分：`CommandHandler` 当前持有 backend/workspace/tool/session/connector/MCP/terminal/materialization/extension artifact 等多字段（`crates/agentdash-local/src/handlers/mod.rs:40`），`handle()` match 覆盖所有 relay command（`crates/agentdash-local/src/handlers/mod.rs:112`）。真正拆成多 service/dispatcher 会触及大部分 handler 文件，预计超过 10 文件，应列架构 backlog。
- Extension `runtime.invoke` / `extension.channel_invoke` 接通：当前 `runtime.invoke` 固定返回未预加载，`extension.channel_invoke` 固定未接入 registry（`crates/agentdash-local/src/extensions/host/permissions.rs:72`, `crates/agentdash-local/src/extensions/host/permissions.rs:40`）。接通需要 Project extension registry、runner invocation depth、RuntimeGateway surface 与 relay payload 一起设计，属于跨层 contract。
- env/process 权限语义对称性：`env.get` 使用 `env.read[:name]`，`process.exec options.env` 目前归在 `process.execute` 下（`crates/agentdash-local/src/extensions/host/permissions.rs:61`, `crates/agentdash-local/src/extensions/host/permissions.rs:243`）。如果要调整 manifest 权限表达，应进入 extension permission contract 设计；本轮只保持现状。

### Direct Implement Queue

推荐主控按以下方式派发：

1. 可并行派发 Batch A、Batch B、Batch C、Batch D。
2. Batch A/D 合并后，再派发 Batch E。
3. 架构项只写入 `architecture-backlog.md`，不要阻塞上述直接 implement。

### External References

- 未使用外部网页资料；本研究仅依赖仓库内代码、review 与 Trellis specs。
- 相关 crate/version 来自 `crates/agentdash-local/Cargo.toml`：`ignore = "0.4"`、`globset = "0.4"`、`regex.workspace = true`、`rmcp = "1.2"`、`reqwest = "0.13.2"`、`tempfile = "3.18"`。
- search 收敛后的外部运行前置为 `rg`/ripgrep binary；当前代码通过 Windows `where` 或 Unix `which` 探测：`crates/agentdash-local/src/tool_executor.rs:457`。

### Related Specs

- `.trellis/spec/cross-layer/desktop-local-runtime.md`
- `.trellis/spec/backend/error-handling.md`
- `.trellis/spec/backend/quality-guidelines.md`
- `.trellis/spec/backend/runtime-gateway.md`
- `.trellis/spec/backend/permission/architecture.md`
- `.trellis/spec/backend/permission/policy-engine.md`
- `.trellis/spec/guides/code-reuse-thinking-guide.md`
- `.trellis/spec/guides/cross-layer-thinking-guide.md`

## Caveats / Not Found

- `task.py current --source` 未能解析当前任务；由于用户明确给出任务路径和目标文件，本研究按该显式路径落盘。
- 未追踪 cloud 侧 `CommandPromptPayload.mcp_servers` producer；因此 Batch C 只建议 local fail-closed，不建议直接改 relay typed contract。
- 未运行 cargo 验证；验证命令已按批次列出，应由 implement/check agent 在改码后执行。
- `rg/fallback` 收敛会让 ripgrep 成为实际运行前置条件；这是按预研期“不保留 fallback”口径得出的模块级结论，不是兼容性方案。
