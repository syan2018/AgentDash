# Work Item 03: VFS 与 Local guard rails 收束

## Goal

补齐 VFS/runtime tool/local relay 中修改范围可控的 guard rails，减少后续 provider 和 local command 扩展时重新产生路径冗余。

## Source Issues

- `adversarial-review.md` Issue 22。
- `adversarial-review.md` Issue 24。
- `adversarial-review.md` Issue 25。
- `adversarial-review.md` Issue 26。
- `research/09-vfs-runtime-tool-surface.md`。
- `research/10-local-runtime-relay-surface.md`。

## Requirements

- Runtime tool composition root 检查 callable tool name 唯一性。
- Schema dedupe 与 callable tool guard 使用一致的 source/name 语义。
- `ToolExecutor` 与 `ProcessExecutor` 共用 workspace root validation guard。
- Local relay command scheduling 由 handler/dispatch plan 声明，不由 `ws_client` 中央 enum allowlist 判断。
- Builtin VFS skill discovery 传递 launch identity，与 dynamic VFS-first discovery 使用一致 identity 语义。

## Evidence

- `crates/agentdash-application/src/runtime_tools/provider.rs:64` 直接 extend provider tools。
- `crates/agentdash-application-runtime-session/src/session/tool_assembly.rs:75` / `:86` 只 dedupe schemas。
- `crates/agentdash-agent/src/agent_loop/tool_call.rs:351` 按第一个同名 tool 查找。
- `crates/agentdash-local/src/tool_executor.rs:20` / `:80` 与 `process_executor.rs:23` / `:38` 重复 root validation。
- `crates/agentdash-local/src/ws_client.rs:373` / `:498` 中央判断 background command。
- `crates/agentdash-application-skill/src/skill/loader.rs:120` / `:195` / `:295` 内建 loader read/list 传 `None` identity。

## Suggested Implementation Shape

### Tool name guard

- 在 `SessionRuntimeToolComposer` 或 `assemble_tool_surface_for_execution_context` 统一检查 callable tool name。
- 重复时返回包含 provider/source/tool name 的诊断错误。
- 保持 MCP schema/tool path dedupe 与 runtime tool name guard 的语义一致。

### Workspace root guard

- 在 `agentdash-local` 增加 `WorkspaceRootGuard`。
- `ToolExecutor` / `ProcessExecutor` / terminal 或 extension process 调用处复用它。
- 保持现有 configured roots 与 canonical roots 行为。

### Handler-declared scheduling

- domain handler 或 router 返回 `CommandDispatchPlan` / `ExecutionMode`。
- `ws_client` 只执行 dispatch plan。
- shell/terminal 维持当前 background/ordering 语义。

### Builtin skill identity

- `load_skills_from_vfs`、builtin discovery read/list 接收 `AuthIdentity`。
- `derive_runtime_skill_baseline` 将 `input.identity` 传入 builtin loader。
- dynamic provider discovery 与 builtin discovery 使用同一 identity 语义。

## Tests / Verification

- runtime tool composer duplicate name focused test。
- `agentdash-local` workspace root guard tests。
- local relay scheduling tests，覆盖 shell background 与非 shell commands。
- skill loader tests，覆盖 identity 传递到 VFS service read/list。

## Out of Scope

- 不做 typed RuntimeDiscoveryPolicy。
- 不做 VFS per-mount/path authorization。
- 不做 relay prompt typed payload。
