# Research: 权限裁决（extension action 调用现有检查点 + invoke 服务端校验落点）

- **Query**: extension action 调用现有权限检查点（permissions/capability decision）；invoke 服务端校验 operation 归属 module + schema 的合适落点
- **Scope**: internal
- **Date**: 2026-06-08

## Findings

### 现有 extension action 权限检查点（在 provider.invoke 内）

`ExtensionRuntimeActionProvider::invoke`（runtime_gateway/extension_actions.rs L172）调 `validate_action_permissions`（L264-283）：

```rust
fn validate_action_permissions(installation, action, request)
    -> Result<Vec<ExtensionPermissionDecision>, RuntimeInvocationError> {
    for permission in &action.permissions {
        let decision = installation.manifest.evaluate_action_permission(&action.action_key, permission);
        if !decision.allowed {
            return Err(RuntimeInvocationError::capability_denied(decision.denial_message(), ..));
        }
        decisions.push(decision);
    }
    Ok(decisions)
}
```

`evaluate_action_permission`（`crates/agentdash-domain/src/shared_library/value_objects.rs` L1164-1198）→ `ExtensionPermissionDecision`（L1203-1209）：
```rust
pub struct ExtensionPermissionDecision {
    pub requested_permission: String, pub action_key: String,
    pub capability_family: String, pub allowed: bool,
    pub reason: ExtensionPermissionDecisionReason,   // Allowed / MissingRuntimeAction / ...
}
```
`denial_message()`（L1212-）给人读的拒绝原因。

裁决结果会被塞进 `RuntimeInvocationOutput.metadata["permission_decisions"]`（extension_actions.rs L211-221）——**这是权限审计 trace 的现有落点**。

> 含义：runtime_action 分支的权限裁决**已经在 provider 内完成**，Child 2 的 invoke 元工具走 runtime_action 时**不需要重复权限检查**（gateway → provider 已裁）。元工具只需在 provider 报 `CapabilityDenied` 时正确映射成 agent tool error（参照 tool_adapter.rs L145-149：`CapabilityDenied → ExecutionFailed`）。

### channel 调用的权限/依赖检查点

`ExtensionRuntimeChannelInvoker::invoke` → `resolve_channel_invocation`（L466-534）做：
- consumer 安装存在性检查（L470-486）。
- `ensure_consumer_dependency`（L613-）：consumer 必须在 manifest `extension_dependencies` 声明了该 provider 的 channel，否则 `capability_denied`（跨 extension 调用的依赖闸门）。
- channel/method 存在性（L490-518）。

agent 工具以 `SessionUser` consumer 调 channel 时，`consumer_installation = None`（L474-475），跳过 dependency 检查（`ensure_consumer_dependency` L620-622 直接 Ok）。即 SessionUser 不受 extension-to-extension 依赖闸门约束，但仍受 channel/method 存在性约束。channel 方法**没有**像 runtime_action 那样逐 permission 调 `evaluate_action_permission`（channel method 的 permission 在投影里有 `permissions`，但 invoker 不裁——这是现状）。

### invoke 服务端「operation 归属 module + schema」校验的合适落点（R2，Child 2 新增）

现有链路里**没有** operation→module 归属校验，也没有 input schema 校验——这两项是 Child 2 要新增的服务端裁决。建议落点（自外向内）：

1. **operation 归属 + schema 校验：在 `workspace_module_invoke` 元工具的 `execute()` 内、调 gateway 之前**。理由：
   - 元工具已能 `resolve_visible_modules`（tools.rs L26-48，复用 Child 1 的 `build_workspace_modules + capability 过滤`）。先按 `module_id` 找 module（不可见 → 拒绝，复用 describe 的 `module_not_found` 模式 tools.rs L214-228），再按 `operation_key` 找 operation（未知 operation → 拒绝，acceptance 要求）。这一步同时拿到 operation 的 `origin`（决定分支）与 `input_schema`（用于校验）。
   - input schema 校验：operation 的 `input_schema: Option<Value>` 是 JSON Schema。用现有 schema 校验工具（项目内 `jsonschema` 或 `schemars` 校验器；需确认依赖）对 `input` 校验，不匹配 → `InvalidArguments`。**describe 的 schema 与服务端校验必须成对**（PRD Notes 风险点）。
2. **可见性/capability 裁切**：已由 `WorkspaceModuleDimension::allows(module_id)` 在 `resolve_visible_modules` 内完成（tools.rs L45，`flow.workspace_module`），与 list/describe 同源（D4）。invoke 元工具复用同一 visibility 即天然内聚——agent 不可见的 module 直接 module_not_found。
3. **extension permission 裁决**：交给 provider（已有，见上），不在元工具重复。
4. **backend 归属**：HTTP 侧有 `ensure_project_backend_access`（extension_runtime 路由 L139），agent 侧 backend 自推自 context（见 04 文档），归属隐含正确（同一 session 的 vfs/backend_execution），design 可决定是否仍校验。

### capability 总闸：`is_capability_tool_enabled`

工具能否被装配本身受 `flow.is_capability_tool_enabled(CAP_WORKSPACE_MODULE, "workspace_module_invoke", Some(ToolCluster::WorkspaceModule))` 控制（provider.rs L336-340 模式）。Child 2 需为 invoke/present 各加一个 capability tool gate（与 list/describe 同 cluster `WorkspaceModule`，cap `CAP_WORKSPACE_MODULE`，见 `agentdash_spi::platform::tool_capability`）。

## Caveats / Not Found

- channel method 的 per-permission 裁决现状缺失（invoker 不调 evaluate）；若 design 要求 channel 也逐 permission 裁，需在 `ExtensionRuntimeChannelInvoker` 或元工具补，属新增面。
- 项目内 JSON Schema 运行时校验器的具体依赖未在本轮确认（需 grep `jsonschema`/`schemars validate`）。describe 出的 input_schema 是 `serde_json::Value`，校验器待 design 选型。
- operation→module 归属与 input schema 校验是 Child 2 全新逻辑，无现成实现可抄；可见性裁切与 extension permission 裁决可复用现成（分别在 tools.rs visibility 过滤 与 provider 内）。
