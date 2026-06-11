# Research: local-runtime follow-up executable plan

- Query: 基于 `reviews/004-local-runtime.md`、`research/local-runtime-executable-plan.md` 和当前已完成提交，重新梳理 local-runtime 剩余模块级问题，产出下一轮可执行批次。
- Scope: internal
- Date: 2026-06-11

## Findings

### Current Baseline

本轮按当前代码状态重新核对，以下已完成项不再重复规划：

- `ToolExecutor` root/write/search 边界已收敛：构造期 canonicalize roots，写路径解析无副作用，search 已只走 ripgrep。证据：`crates/agentdash-local/src/tool_executor.rs:25`, `crates/agentdash-local/src/tool_executor.rs:98`, `crates/agentdash-local/src/tool_executor.rs:126`, `crates/agentdash-local/src/tool_executor.rs:183`, `crates/agentdash-local/src/tool_executor.rs:430`, `crates/agentdash-local/src/tool_executor.rs:487`。
- `ToolError -> RelayError` 集中映射已提交，不在本轮展开。
- Relay MCP parser 已 fail-closed：`parse_relay_mcp_servers()` 返回 `Result`，`handle_prompt()` 在启动前返回 `INVALID_MESSAGE`。证据：`crates/agentdash-local/src/handlers/relay_mcp_servers.rs:80`, `crates/agentdash-local/src/handlers/prompt.rs:55`。
- Extension Host API split 与 `ActiveExtension.tool_executor` 复用已提交。证据：`crates/agentdash-local/src/extensions/host/mod.rs:1`, `crates/agentdash-local/src/extensions/host/process.rs:27`, `crates/agentdash-local/src/extensions/host/manager.rs:170`。

### Files Found

| Batch / Area | Files | One-line Description |
|---|---|---|
| Batch E: MCP prompt wire-shape alignment | `crates/agentdash-application/src/relay_connector.rs`; `crates/agentdash-local/src/handlers/relay_mcp_servers.rs`; optional tests in `crates/agentdash-api/src/workspace_resolution.rs` | 当前 cloud producer 发送 nested `SessionMcpServer` JSON，而 local parser 要求 flattened `type/url/command` JSON。 |
| Batch F: ProcessExecutor + env/process boundary | `crates/agentdash-local/src/tool_executor.rs`; new `crates/agentdash-local/src/process_executor.rs`; `crates/agentdash-local/src/lib.rs`; `crates/agentdash-local/src/extensions/host/process_api.rs`; `crates/agentdash-local/src/extensions/host/host_api.rs` tests | `process.shell` 走 `ToolExecutor::shell_exec()`，`process.exec` 仍直接 `Command::new()` 并单独处理 env/timeout/output。 |
| Batch G: Extension Host fallback API removal | `crates/agentdash-local/src/extensions/host/runner/context.mjs`; `crates/agentdash-local/src/extensions/host/host_api.rs`; `crates/agentdash-local/src/extensions/host/tests.rs` | runner 内已可路由预加载 action/channel，Rust host-api fallback 仍保留未接入占位。 |
| Batch H: SearchExecutor extraction | `crates/agentdash-local/src/tool_executor.rs`; new `crates/agentdash-local/src/search_executor.rs`; optional new `crates/agentdash-local/src/file_discovery_policy.rs`; `crates/agentdash-local/src/lib.rs` | search 行为已稳定，可从 `ToolExecutor` 中抽出搜索执行器和共享 discovery policy。 |
| Architecture: CommandHandler split | `crates/agentdash-local/src/handlers/*.rs`; `crates/agentdash-local/src/ws_client.rs`; likely runtime setup files | 真正拆总线对象会触及大部分 handler/service 边界，不适合用小 router 掩盖。 |
| Architecture: typed MCP prompt contract | `crates/agentdash-relay/src/protocol/prompt.rs`; `crates/agentdash-application-ports/src/backend_transport.rs`; `crates/agentdash-application/src/relay_connector.rs`; `crates/agentdash-api/src/workspace_resolution.rs`; `crates/agentdash-local/src/handlers/prompt.rs`; `crates/agentdash-local/src/handlers/relay_mcp_servers.rs` | 把 `mcp_servers: Vec<Value>` 改为 typed DTO 是 relay/application/local wire contract 变更。 |

### Code Patterns

- `CommandHandler` 仍是本机 relay 总线对象，字段覆盖 backend/workspace/tool/session/connector/MCP/terminal/materialization/extension artifact 等依赖：`crates/agentdash-local/src/handlers/mod.rs:40`。`handle()` 继续集中 match 心跳、prompt、workspace、tool、MCP、extension、terminal 等命令：`crates/agentdash-local/src/handlers/mod.rs:112`。
- handler 实现已经按文件拆出，但仍都直接 `impl CommandHandler` 并读取 `self.*` 宽上下文：`crates/agentdash-local/src/handlers/prompt.rs:19`, `crates/agentdash-local/src/handlers/extension.rs:166`, `crates/agentdash-local/src/handlers/mcp_relay.rs:109`, `crates/agentdash-local/src/handlers/tool_calls.rs:196`。
- `ToolExecutor` 当前仍覆盖 file read/write/delete/rename/patch、shell、streaming shell、file list、search 和 path/discovery helpers：`crates/agentdash-local/src/tool_executor.rs:159`, `crates/agentdash-local/src/tool_executor.rs:183`, `crates/agentdash-local/src/tool_executor.rs:229`, `crates/agentdash-local/src/tool_executor.rs:263`, `crates/agentdash-local/src/tool_executor.rs:298`, `crates/agentdash-local/src/tool_executor.rs:397`, `crates/agentdash-local/src/tool_executor.rs:430`。
- `process.shell` 复用 `ToolExecutor::shell_exec()`，但 `process.exec` 直接 `tokio::process::Command::new()`、自行解析 args/env/cwd/timeout/output：`crates/agentdash-local/src/extensions/host/process_api.rs:34`, `crates/agentdash-local/src/extensions/host/process_api.rs:55`, `crates/agentdash-local/src/extensions/host/process_api.rs:81`, `crates/agentdash-local/src/extensions/host/process_api.rs:91`, `crates/agentdash-local/src/extensions/host/process_api.rs:98`。
- `process.exec` 对 `args` 使用 `filter_map(Value::as_str)`，非法参数会被静默丢弃；`options.env` 只在 exec 生效，shell 使用同一 SDK options 类型但当前忽略 env：`crates/agentdash-local/src/extensions/host/process_api.rs:65`, `packages/extension-sdk/src/index.ts:216`, `packages/extension-sdk/src/index.ts:231`。
- `env.get` 要求 `env.read` 或 `env.read:<NAME>`；`process.exec options.env` 当前只要求 `process.execute`：`crates/agentdash-local/src/extensions/host/host_api.rs:53`, `crates/agentdash-local/src/extensions/host/host_api.rs:57`, `crates/agentdash-local/src/extensions/host/process_api.rs:59`, `crates/agentdash-local/src/extensions/host/process_api.rs:91`。
- runner 内 `ctx.api.runtime.invoke()` 已先查找当前已加载 action，命中后直接 `invokeRegisteredAction()`；未命中才调用 Rust `runtime.invoke` host API：`crates/agentdash-local/src/extensions/host/runner/context.mjs:48`, `crates/agentdash-local/src/extensions/host/runner/context.mjs:49`, `crates/agentdash-local/src/extensions/host/runner/context.mjs:55`。
- runner 内 `ctx.api.channels.invoke/self/from` 已能路由当前 host 内注册 channel；未命中才调用 Rust `extension.channel_invoke` host API：`crates/agentdash-local/src/extensions/host/runner/context.mjs:111`, `crates/agentdash-local/src/extensions/host/runner/context.mjs:113`, `crates/agentdash-local/src/extensions/host/runner/context.mjs:117`, `crates/agentdash-local/src/extensions/host/runner/context.mjs:131`, `crates/agentdash-local/src/extensions/host/runner/context.mjs:135`, `crates/agentdash-local/src/extensions/host/runner/context.mjs:139`。
- Rust host API 对 `runtime.invoke` 与 `extension.channel_invoke` 仍固定返回占位错误：`crates/agentdash-local/src/extensions/host/host_api.rs:31`, `crates/agentdash-local/src/extensions/host/host_api.rs:32`, `crates/agentdash-local/src/extensions/host/host_api.rs:64`, `crates/agentdash-local/src/extensions/host/host_api.rs:77`。
- relay extension action payload 已带 `runtime_extensions`，local handler 会预激活这些 Project runtime hosts：`crates/agentdash-relay/src/protocol/extension_runtime.rs:46`, `crates/agentdash-local/src/handlers/extension.rs:31`, `crates/agentdash-local/src/handlers/extension.rs:193`。
- prompt MCP raw JSON 仍存在于 relay protocol 与 application transport port：`crates/agentdash-relay/src/protocol/prompt.rs:33`, `crates/agentdash-application-ports/src/backend_transport.rs:116`。
- 当前 producer 把 `SessionMcpServer` 直接 `serde_json::to_value()` 后塞入 relay payload：`crates/agentdash-application/src/relay_connector.rs:152`。但 `SessionMcpServer` 序列化形态是 `name + transport + uses_relay`，transport 在字段下嵌套：`crates/agentdash-spi/src/connector/mod.rs:510`, `crates/agentdash-spi/src/connector/mod.rs:512`, `crates/agentdash-spi/src/connector/mod.rs:513`。local parser 当前要求顶层 `type/url/command`：`crates/agentdash-local/src/handlers/relay_mcp_servers.rs:96`, `crates/agentdash-local/src/handlers/relay_mcp_servers.rs:98`, `crates/agentdash-local/src/handlers/relay_mcp_servers.rs:107`。

### Immediate Implementation Batches

#### Batch E: MCP prompt wire-shape alignment

- Parallel / dependency: 可与 Batch F/G 并行；不依赖 Batch H。不要在本批做 full typed contract。
- Write scope:
  - `crates/agentdash-application/src/relay_connector.rs`
  - `crates/agentdash-local/src/handlers/relay_mcp_servers.rs`
  - optional: `crates/agentdash-api/src/workspace_resolution.rs` tests if API relay serialization has coverage.
- Core changes:
  - 新增 `session_mcp_server_to_relay_prompt_value()` 或等价 helper，把 `SessionMcpServer` 转成当前 local parser 接受的 flattened JSON：`{ name, type, url|command, headers|args|env }`。
  - `RelayAgentConnector::prompt()` 不再直接 `serde_json::to_value(server)`。
  - 更新 `relay_prompt_payload_passes_full_mcp_and_projects_working_dir`，断言 `mcp_servers[0].type == "stdio"`、`command` 在顶层、没有 `transport` 嵌套。
  - 保留 `parse_relay_mcp_servers()` fail-closed；新增测试覆盖 application 发送形态可被 local parser 解析。
- Risk:
  - 这是 wire-shape 修复，不是 typed contract；raw JSON 仍存在。
  - 如果 cloud 其它 producer 已发送 flattened shape，本批与其一致；如果已有 producer 发送 nested `SessionMcpServer`，本批会阻断该错误继续向 local 传播。
- Verification commands:
  - `cargo test -p agentdash-application relay_prompt_payload_passes_full_mcp_and_projects_working_dir`
  - `cargo test -p agentdash-local relay_mcp_servers`
  - `cargo check -p agentdash-application`
  - `cargo check -p agentdash-local`

#### Batch F: ProcessExecutor shared implementation and explicit env option policy

- Parallel / dependency: 可与 Batch E/G 并行；会改 `tool_executor.rs`，建议与 Batch H 顺序执行。
- Write scope:
  - new `crates/agentdash-local/src/process_executor.rs`
  - `crates/agentdash-local/src/lib.rs`
  - `crates/agentdash-local/src/tool_executor.rs`
  - `crates/agentdash-local/src/extensions/host/process_api.rs`
  - `crates/agentdash-local/src/extensions/host/host_api.rs` tests
- Core changes:
  - 抽 `ProcessExecutor`，统一 cwd resolve、shell command wrapping、argv exec、timeout、stdout/stderr decode、exit code、输出截断前原始结果。
  - `ToolExecutor::shell_exec()` 和 `shell_exec_streaming()` 委托 `ProcessExecutor`，保留现有 public 方法供 relay tool call 使用。
  - `process.shell` 与 `process.exec` 都消费同一个 `ProcessExecutor`；API 层只解析 shell string vs argv 两种输入。
  - `args` 与 `options.env` 改为 fail-closed typed parsing：非字符串 arg/env value 返回 host API 参数错误，不再静默丢弃。
  - `options.env` 对 shell/exec 保持一致；若传入 env overlay，要求当前 action/channel method 除 `process.execute` 外也声明 `env.read` 或 `env.read:<KEY>`。这复用既有 permission family，不新增公共 permission key。
- Risk:
  - Windows shell wrapper 必须保留 UTF-8 output prelude：`crates/agentdash-local/src/tool_executor.rs:911`。
  - 当前 child process 默认继承宿主环境；本批只收敛显式 `options.env` 的权限表达，不把 `process.execute` 改造成完整环境沙箱。
  - timeout 行为要保持 extension host 当前返回 `{ timed_out: true }`，relay tool shell 仍返回 `ToolError::Timeout`。
- Verification commands:
  - `cargo test -p agentdash-local process_host_apis`
  - `cargo test -p agentdash-local shell_exec`
  - `cargo test -p agentdash-local built_in_host_apis_use_action_permissions_and_workspace_boundary`
  - `cargo check -p agentdash-local`

#### Batch G: Remove unsupported Rust host-api fallback for runtime.invoke/channel_invoke

- Parallel / dependency: 可与 Batch E/F 并行；不依赖 Batch H。
- Write scope:
  - `crates/agentdash-local/src/extensions/host/runner/context.mjs`
  - `crates/agentdash-local/src/extensions/host/host_api.rs`
  - `crates/agentdash-local/src/extensions/host/tests.rs`
- Core changes:
  - 保留 `ctx.api.runtime.invoke()` 与 `ctx.api.channels.*` public SDK surface，因为 runner 已能调用当前 host 内预加载 action/channel。
  - runner 未命中本机已加载 action/channel 时直接抛出 diagnostic error，例如 `runtime action is not loaded in current extension host: <key>`，不再发送 Rust `runtime.invoke` / `extension.channel_invoke` host API request。
  - 删除 `host_api.rs` 中 `runtime.invoke` 和 `extension.channel_invoke` match arms 及占位测试。
  - 保留/强化现有测试：`runtime_invoke_calls_loaded_extension_action`、`runtime_invoke_requires_cross_extension_permission`、`protocol_channel_registers_and_self_invokes`、`dependency_alias_invokes_provider_channel_in_same_host`。
- Risk:
  - 若未来需要跨 backend 或未预加载 Project registry fallback，这不是 local host_api 小修，应另做 architecture 设计。
  - `packages/extension-sdk/src/index.ts` 的 `createNoopApi()` 仍是非 host runtime fallback，不应误删。
- Verification commands:
  - `cargo test -p agentdash-local runtime_invoke`
  - `cargo test -p agentdash-local protocol_channel`
  - `cargo test -p agentdash-local host_api`
  - `cargo check -p agentdash-local`

#### Batch H: Extract SearchExecutor and shared file discovery policy

- Parallel / dependency: 建议在 Batch F 后执行，避免同时改 `tool_executor.rs`。
- Write scope:
  - `crates/agentdash-local/src/tool_executor.rs`
  - new `crates/agentdash-local/src/search_executor.rs`
  - optional new `crates/agentdash-local/src/file_discovery_policy.rs`
  - `crates/agentdash-local/src/lib.rs`
- Core changes:
  - 将 ripgrep detection、ripgrep arg policy、JSON match parsing、search timeout 和 search tests 从 `tool_executor.rs` 移到 `SearchExecutor`。
  - `ToolExecutor::search()` 只做 workspace root validation 后委托 `SearchExecutor`。
  - 把 `FileDiscoveryPolicy` 抽到共享 helper，供 `file_list` 与 `SearchExecutor` 共用，避免 search/file-list 对 ignore/noise policy 再次分叉。
  - 不在本批拆 file read/write/patch；这些路径 helper 与 list/discovery 仍高度耦合，等 Process/Search 抽离后再评估。
- Risk:
  - 纯结构迁移，但测试移动容易漏 `#[cfg(test)]` 可见性。
  - ripgrep unavailable 行为必须保持当前 fail-fast。
- Verification commands:
  - `cargo test -p agentdash-local tool_executor`
  - `cargo test -p agentdash-local search_requires_ripgrep_when_unavailable`
  - `cargo check -p agentdash-local`

### Architecture Backlog

Only the following meet the strict backlog threshold.

#### ARCH-LR-001: CommandHandler service split

- Evidence: `CommandHandler` fields remain broad (`crates/agentdash-local/src/handlers/mod.rs:40`), and `handle()` is still the central relay command match (`crates/agentdash-local/src/handlers/mod.rs:112`).
- Impact: prompt/session forwarder, tool calls, MCP relay, terminal, materialization, extension artifact activation, connector discovery.
- Direction: design a real command service boundary such as `PromptCommandService`, `ToolCommandService`, `ExtensionCommandService`, `TerminalCommandService`, with a small router consuming typed handler services.
- Reason for backlog: a meaningful split touches most files under `handlers/`, `ws_client.rs`, constructor/config plumbing, and tests; a small router wrapper would only move match arms without reducing dependency width.

#### ARCH-LR-002: Full typed MCP prompt contract

- Evidence: relay protocol and application transport still use `Vec<serde_json::Value>` for prompt MCP servers: `crates/agentdash-relay/src/protocol/prompt.rs:34`, `crates/agentdash-application-ports/src/backend_transport.rs:120`.
- Impact: application connector, API backend registry relay transport, local prompt parser, relay protocol tests, any external backend speaking `command.prompt`.
- Direction: introduce an explicit relay prompt MCP DTO with stable serde shape, then make application/local consume that type instead of raw JSON.
- Reason for backlog: this changes a public cross-crate relay/application/local contract. Batch E should fix the current wire-shape mismatch without claiming full typed contract ownership.

#### ARCH-LR-003: Full process/env sandbox and permission contract

- Evidence: `env.get` has `env.read[:NAME]` checks, while process execution currently inherits host process environment by default through `tokio::process::Command`: `crates/agentdash-local/src/extensions/host/host_api.rs:53`, `crates/agentdash-local/src/extensions/host/process_api.rs:85`.
- Impact: extension manifest permission keys, process execution defaults, Node/PowerShell command discovery, docs/examples, possibly SDK type docs.
- Direction: if the product requires hard environment isolation, define whether process execution starts with `env_clear()`, what minimal base env is injected, and whether a new permission family such as env pass/inject is needed.
- Reason for backlog: hard sandbox semantics are a public extension permission contract and runtime behavior change. Batch F should only align explicit `options.env` handling inside the module.

### Non-Deferred Review Items

- `process.exec` vs `process.shell` is not architecture. They already live in local-runtime, share workspace boundary inputs, and can converge through Batch F.
- `runtime.invoke` / `extension.channel_invoke` should not be dismissed as "not connected" wholesale. In-runner invocation for preloaded actions/channels is connected; only Rust host-api fallback stubs remain, and Batch G should remove them instead of preserving dead public surface.
- prompt MCP fail-closed is done, but the current producer/parser wire-shape mismatch is non-deferred because it can make valid session MCP declarations fail prompt startup. Full typed contract is backlog; Batch E is the immediate shape repair.
- `ToolExecutor` shell/search extraction is module-level work, not architecture. Shell belongs with Batch F; search can be Batch H. File executor extraction is not a next-round priority until process/search are out, but it should not be filed as architecture if later pursued.
- `CommandHandler` should enter architecture backlog only for a real service split. A cosmetic router/context wrapper does not meet the user-value threshold and should not consume a batch.

### External References

- No external web references were used. This research relies on repository code, task review/fix artifacts, and Trellis specs.

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

- `python ./.trellis/scripts/task.py current --source` returned no active task. The file was written only because the user explicitly provided `.trellis/tasks/06-11-review-refactor-quality-sweep` and the exact output path.
- No source code, specs, review files, fix records, or git state were modified.
- No cargo tests were run during this research turn; validation commands above are proposed for implement/check agents.
- I did not trace every external producer of `CommandPromptPayload.mcp_servers`; the evidence here covers the in-repo application relay connector path and local parser path.
