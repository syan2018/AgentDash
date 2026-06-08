# Implement · Workspace Module 操作面 (invoke + present)

> 依据 `design.md`。有序执行，每步验证。

## 步骤

1. **operation DTO 补派发分量**（`crates/agentdash-contracts/src/workspace_module.rs`）
   - 新增 `WorkspaceModuleOperationDispatch` 枚举（runtime_action/protocol_channel/canvas/builtin），加到 `WorkspaceModuleOperation`。serde + TS。
   - `generate_ts.rs` 加 export。
   - 验证：`cargo build -p agentdash-contracts && pnpm contracts:check`

2. **聚合层填 dispatch**（`crates/agentdash-application/src/workspace_module/mod.rs`）
   - runtime_action → `{action_key}`；channel method → `{channel_key, method_name}`；canvas → `{canvas_action}`（§4 对齐，无可执行则空 operations）。
   - 更新/新增单测断言 dispatch 正确。
   - 验证：`cargo test -p agentdash-application workspace_module`

3. **backend 解析 helper + provider 注入**（`vfs/tools/provider.rs` 及其构造处）
   - 注入 `Arc<RuntimeGateway>`（+ channel invoker）。
   - backend 解析 helper（session.backend_execution → vfs.default_mount → err），参考 `select_extension_invocation_workspace`。
   - 验证：`cargo build -p agentdash-application`

4. **invoke 工具**（`crates/agentdash-application/src/workspace_module/tools.rs`）
   - `WorkspaceModuleInvokeTool` → `workspace_module_invoke(module_id, operation_key, input)`。
   - execute：取 project → build modules → 定位 module/op → 可见性 → input schema 校验 → 按 dispatch 派发（gateway action / channel invoker / canvas gateway）→ 结构化错误分支齐全。
   - 注册进 WorkspaceModule 工具簇（schema delta 现在含 list/describe/invoke/present）。
   - 单测：runtime_action 派发（mock gateway）、unknown op、schema 不匹配、缺 backend。
   - 验证：`cargo test -p agentdash-application workspace_module`

5. **present 工具**（同 tools.rs）
   - `WorkspaceModulePresentTool` → `workspace_module_present(module_id, view_key, payload?)`。
   - 校验 module 可见 + ui_entry 有 view_key；发 `SessionMetaUpdate{key:"workspace_module_presented"}` + inject_notification（模板 PresentCanvasTool）；无目标诊断。
   - 注册进工具簇。
   - 验证：`cargo test -p agentdash-application workspace_module`

6. **前端接收 present**（`packages/app-web/src/features/workspace-panel` / extension-runtime / canvas-panel）
   - 监听 `workspace_module_presented` meta → `useWorkspaceTabStore.openOrActivate`（extension→workspace tab；canvas→canvas tab）。
   - 验证：`pnpm --filter app-web typecheck`

7. **全量验证**
   ```powershell
   cargo build --workspace
   cargo test -p agentdash-application workspace_module
   cargo test -p agentdash-application runtime_gateway
   cargo test -p agentdash-application capability
   pnpm contracts:check
   pnpm --filter app-web typecheck
   ```

## 回滚点

- 步 1 contract 变更影响 Child 3，先确认 dispatch shape 再继续。
- canvas 分支若现有 gateway 路径不通，退化为"canvas 仅 present，invoke 返回未支持"，并在 design §4 记录。

## Review gate

- 步 1/2 完成（dispatch DTO 定稿）后确认再继续。
