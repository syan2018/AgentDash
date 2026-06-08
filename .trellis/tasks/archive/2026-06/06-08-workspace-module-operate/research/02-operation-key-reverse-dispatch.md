# Research: operation_key 反解析 → extension action / channel method 调用

- **Query**: Child 1 把 runtime_actions 与 protocol_channels.methods 都投影成 operation；invoke 时如何反解回真正调用；现有标识是 action_key 还是 channel.method
- **Scope**: internal
- **Date**: 2026-06-08

## Findings

### Child 1 投影出的 operation_key 形状（对照表）

来自 `crates/agentdash-application/src/workspace_module/mod.rs`：

| origin | operation_key 构造 | file:line | 反解析目标 |
|---|---|---|---|
| `runtime_action` | `action.action_key` 原样 | mod.rs L58 | extension runtime action |
| `protocol_channel` | `format!("{}.{}", channel.channel_key, method.name)` | mod.rs L75 | channel method 调用 |
| `canvas` | `format!("binding.{}", binding.alias)` | mod.rs L149-151 | canvas data binding（**非可执行 operation**，schema 为 None） |
| `builtin` | 本轮未投影（`build_workspace_modules` 不产 builtin） | mod.rs L36-44 | — |

`WorkspaceModuleOperation`（contracts L89-100）带 `origin: String`——**invoke 分支派发的权威依据就是 describe 出来的 `origin`**，而不是靠 operation_key 字符串形态去猜。Child 2 在 invoke 时应：先 `describe` 出 module，找到 `operation_key == 入参` 的 operation，读它的 `origin` 决定分支。

### runtime_action 调用：action_key 直用

`ExtensionRuntimeActionProvider::invoke`（runtime_gateway/extension_actions.rs L122-227）：

```rust
let action_key = request.action_key.as_str();          // L139
let (installation, action) = installations.iter().find_map(|installation| {
    installation.manifest.runtime_actions.iter()
        .find(|action| action.action_key == action_key)  // L143-148
        .map(|action| (installation, action))
}).ok_or_else(|| capability_denied("extension runtime action 未启用或不可见: {action_key}"))?;  // L150-155
```

所以 runtime_action 的 **operation_key == extension `action_key` == RuntimeInvocationRequest.action_key**，一一对应。invoke 分支：直接 `RuntimeActionKey::parse(operation_key)` → 构造 `Session` request + `Backend` target → `gateway.invoke`（dynamic provider 命中，见 01 文档）。

调用后还会校验：`action.kind == SessionRuntime`（L157-162，否则 denied）、`installation.package_artifact` 必须存在（L163-171，否则 `Conflict`）、`validate_action_permissions`（L172，见 06 文档）。

### channel method 调用：不走 action_key，走专用 `ExtensionRuntimeChannelInvoker`

channel 调用**完全绕开 RuntimeGateway / action_key**，是另一条独立路径（extension_actions.rs L354-456）：

```rust
pub struct ExtensionRuntimeChannelInvoker { installations, transport }
pub async fn invoke(&self, request: ExtensionRuntimeChannelInvokeRequest)
    -> Result<ExtensionRuntimeChannelInvokeResult, RuntimeInvocationError>
```

`ExtensionRuntimeChannelInvokeRequest`（L325-337）字段：
```rust
{ project_id: Uuid, session_id: String, backend_id: String,
  workspace: Option<ExtensionInvocationWorkspaceContext>,
  consumer: ExtensionRuntimeChannelConsumer,   // ExtensionPanel{key} | UserCanvas{id} | SessionUser
  channel_key: String,           // 如 "demo.api"（可短名，需 consumer scope）
  dependency_alias: Option<String>,
  method: String,                // 如 "readProfile"
  input: Value, trace: RuntimeTrace }
```

`resolve_channel_invocation`（L466-534）：按 `channel_key` 在所有 enabled installation 的 `manifest.protocol_channels` 里找 provider+channel（L490-505），再在 `channel.methods` 里按 `method.name == request.method` 找 method（L506-518）。

> **operation_key → channel 调用的反解析**：Child 1 的 operation_key 是 `{channel_key}.{method.name}`。Child 2 需要在 invoke 时**把它拆回 `channel_key` 与 `method`**。注意 `channel_key` 本身含点（如 `demo.api`），method 是最后一段（`readProfile`）→ 拆分应 `rsplitn(2, '.')`：右边是 method，左边是 channel_key。比靠字符串拆更稳的做法：在 describe 出的 operation 上**额外保留 channel_key/method 的结构化引用**，或重新查 installation manifest 用 method.name 匹配。当前 operation DTO 只有扁平 `operation_key`，没保留 channel_key/method 分量——Child 2 需要据 origin=protocol_channel 自行拆或重查。

agent 工具调 channel 的 consumer 应是 `ExtensionRuntimeChannelConsumer::SessionUser`（参照 extension_runtime 路由 L219 在无 consumer_extension_key 时的默认值）。

### canvas / builtin origin

- `origin == "canvas"`：operation_key `binding.{alias}`，`input_schema`/`output_schema` 都是 None（mod.rs L156-157）。**这不是可 invoke 的 RPC**，是声明式数据绑定。Child 2 的 canvas invoke 分支（见 03 文档）实际复用的是 RuntimeGateway 上的 canvas runtime action（与 binding operation 不是同一回事）——design 需要澄清 canvas module 的「可 invoke operation」到底投影自哪里（当前 Child 1 只投影了 binding，没投影 canvas runtime bridge 上的 action）。这是一个 **design 需要决断的缺口**。
- `origin == "builtin"`：本轮 `build_workspace_modules` 不产出，invoke 分支可先返回未知 operation / 未实现。

## Caveats / Not Found

- operation DTO（`WorkspaceModuleOperation`）只携带扁平 `operation_key + origin`，**不保留 channel_key/method 或 action 的结构化分量**。Child 2 反解析 channel operation 需要靠 `rsplitn` 拆 key 或重查 manifest，design 应明确选哪种（建议重查 manifest 以避免 channel_key 含点的歧义）。
- canvas module 的 operations 当前只有 binding（不可执行）。"canvas invoke 分支"复用的现有 service 见 03 文档，但**它接受的是 RuntimeActionKey 形态的 runtime action，不是 binding alias**——Child 1 投影与 Child 2 invoke 之间在 canvas 这一侧存在语义错位，需 design 收口。
