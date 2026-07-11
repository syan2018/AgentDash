# Runtime Gateway

Runtime Gateway 是 application-facing typed execution seam。AgentRun 使用具名 `AgentRunRuntime` facade；其他平台 action使用各自具名 gateway。Gateway 不暴露 Driver、Integration factory、placement transport或vendor DTO。

## Agent Runtime Path

```text
Application product command
  -> AgentRunRuntime facade
  -> AgentRuntimeGateway execute/snapshot/events
  -> Managed Runtime
  -> Integration Driver Host
```

- product coordinate只解析为 `AgentRunRuntimeBinding`；不存在字符串 connector/executor分支。
- extension/Canvas/VFS调用从 `run_id + agent_id` 获取canonical binding与Business Surface resource facts。
- command availability、stale guard与typed unsupported在Driver副作用前验证。
- Gateway implementation无持久状态；operation/snapshot/events由Managed Runtime repository持有。
- Remote placement走RuntimeWire，不能经generic Backbone/JSON command transport。

必须测试无binding、stale guard、unsupported、duplicate operation、cross-project authorization与remote Lost。

## Scenario: Agent Runtime Operation Tool Bridge

### 1. Scope / Trigger

AgentFrame surface 被编译为 PR Agent Runtime binding，或 Tool Broker 执行 WorkspaceModule / OperationScript
工具时使用本合同。原因是业务 Operation authority 与 Runtime Thread/Turn/Item recovery 必须保持独立。

### 2. Signatures

```rust
pub struct AgentRunOperationSurfaceTarget {
    pub project_id: Uuid,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub workspace_module_enabled: bool,
}

pub struct PlatformToolBinding {
    pub tool: DynAgentTool,
    pub capability_key: String,
    pub tool_path: String,
}
```

### 3. Contracts

- surface compiler 只在 `workspace_module` capability 有效时追加 platform tools。
- binding registry 保存 runtime name、capability provenance 与 captured AgentRun coordinates。
- MCP/Extension bridge 通过 `AgentRunRuntimeBindingRepository` 把 `(run_id, agent_id)` 解析为
  `RuntimeThreadId`；Operation core 不读取 Runtime 类型。
- `operation_script` 只有顶层 Runtime ToolCall Item；nested call 使用
  `GatewayOperationScriptExecutor` 重新进入 canonical admission，并继承父 tool call trace。
- applied tool-set revision 与 binding generation 仍由 Tool Broker 校验；业务 tool adapter 不复制这套状态机。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| capability 未开放 | 不生成 Workspace tools |
| gateway/engine handle 未装配 | surface/tool execution 返回 unavailable |
| binding 或 tool-set revision 过期 | Tool Broker 返回 stale |
| OperationRef 不在当前 actor surface | invalid arguments |
| nested authority/readiness 变化 | 当前 nested call 重新准入并拒绝 |
| cancel/deadline | 传播到 gateway/engine，记录 terminal error |

### 5. Good / Base / Bad Cases

- Good：binding compile 后 Agent 调用 OperationScript，多个 nested calls 各自产生 Operation audit。
- Base：Agent 调用 `workspace_module_list/describe/invoke/present`。
- Bad：WorkspaceModule 保存 RuntimeThread，或 Runtime aggregate解释 Extension/Interaction dispatch。

### 6. Tests Required

- capability provenance 与 tool path mapping。
- stale binding/generation/tool-set、cancel 与 timeout。
- MCP RuntimeThread resolution、exact OperationRef 与 nested re-admission。
- Interaction presentation attachment 使用 exact run/agent 双坐标。

### 7. Wrong vs Correct

```text
Wrong: WorkspaceModule -> RuntimeSession -> provider dispatch
Correct: Agent Runtime Tool Broker -> API adapter -> canonical Operation Gateway
```
