# Design

## Problem

Extension Host 的 workspace/process API 当前从 `LocalRuntimeProfile.workspace_roots` 取默认 root。这个值属于 local backend 进程级配置，表达已登记或可诊断的本机根集合；它不等价于当前 session 的 workspace cwd。

协议调用链里已经有 Project、session 与 backend placement，缺失的是把 session VFS default mount 的 `root_ref` 作为 invocation fact 下发到 local runtime。

## Source Of Truth

当前 session workspace root 使用 session context plan 的最终 `surface.vfs`：

- 优先选择 `vfs.default_mount_id` 对应 mount。
- mount 必须属于当前 invocation 的 `backend_id`。
- mount 的 `root_ref` trim 后不能为空。
- 若 default mount 不属于当前 backend，则选择同 backend 且 root_ref 非空的第一个 mount；原因是 extension action 的执行 placement 已经由 backend target 决定。
- 若没有匹配 mount，则 invocation 仍可发起，但 workspace/process host API 在真正使用默认 root 时返回未绑定 session workspace root。

## Wire Shape

在 relay protocol 增加 optional workspace invocation context：

```rust
pub struct ExtensionInvocationWorkspaceRelay {
    pub mount_id: String,
    pub root_ref: String,
}
```

`CommandExtensionActionInvokePayload` 与 `CommandExtensionChannelInvokePayload` 都携带：

```rust
pub workspace: Option<ExtensionInvocationWorkspaceRelay>
```

字段 optional 的原因是不是所有 extension action/channel 都需要 workspace/process API；没有 workspace 的 session 仍可以执行纯 profile/env/http 类能力。

## Gateway

`RuntimeInvocationRequest.metadata` 承载 route 已解析的 workspace invocation context，避免把 extension-only 字段扩散到通用 RuntimeContext。

Application 层提供小型 helper 从 metadata 读取并转换为 relay payload；`ExtensionRuntimeChannelInvokeRequest` 则直接持有同一 application 类型字段。Action 与 channel 都在发送 relay payload 时写入 workspace context。

## API Route

`invoke_project_extension_runtime_action` 与 `invoke_project_extension_runtime_channel`：

1. 保留现有 Project/session/backend 权限校验。
2. 复用 `build_session_context_plan()` 获取最终 session surface。
3. 从 VFS 中解析当前 backend 的 workspace root。
4. 对 action 写入 `RuntimeInvocationRequest.metadata`。
5. 对 channel 写入 `ExtensionRuntimeChannelInvokeRequest.workspace`。

## Local Runtime

`LocalExtensionHostActivation` 增加：

```rust
pub default_workspace_root: Option<PathBuf>
```

`ActiveExtension` 保存该字段。Host API root 解析顺序：

1. 调用参数或 options 中的 `workspace_root`。
2. activation 中的 `default_workspace_root`。
3. 返回 `extension host 未绑定 session workspace root`。

`workspace_roots` 保留给 profile summary 与 ToolExecutor 的已登记根集合，但不参与默认 root 兜底。显式 workspace root 继续由 ToolExecutor 校验，session default root 会作为当前 invocation 的有效 root 加入 executor 的 allowed roots，确保 `workspace_roots` 为空时也能执行当前 session cwd 内的文件与 shell 操作。

## Demo

恢复 protocol-demo：

- runtime actions: profile、echo、workspace_demo、shell_demo。
- protocol channel methods: echo、getProfile、readEnv、inspectWorkspace、runShell。
- panel 展示 workspace 与 shell 结果。
- tests 覆盖 workspace/process action 与 channel 方法。
