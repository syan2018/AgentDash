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
- 原生 VFS/Process/Task 工具只有进入显式 Operation exposure registry 后才获得 `platform:*`
  OperationRef。`PlatformToolOperationProvider` 只依赖窄 access seam；API adapter 重新解析 current
  Product binding/applied surface，并把执行交回 PlatformToolBroker。
- Runtime tool catalog registration 不自动暴露 Operation。控制面、lifecycle、Workspace Module 与
  OperationScript 自身工具不进入原生 Operation exposure，避免递归组合和隐式语义推断。
- Operation descriptor 的 JSON Schema 子集区分执行约束与只读 annotation；Gateway 对
  `anyOf`、`minimum`、`maximum` 执行真实输入校验，`description` 可存在于任意 schema 节点、
  必须是字符串且不参与 admission，原因是原生工具的可见参数合同和执行准入需要保持同源，同时
  参数说明需要无损进入 Agent discovery。
- `operation_script` 只有一个顶层 Runtime ToolCall Item；服务端内部顺序执行 engine preflight/run，
  nested call 使用
  `GatewayOperationScriptExecutor` 重新进入 canonical admission，并继承父 tool call trace。
- Agent-facing program 只提交 source/input/limits/exact requested OperationRefs；descriptor digest、
  effect/replay、principal、scope、authority revision 和 granted capabilities 都由服务端 current
  surface 重建。
- AgentRun 的 authority revision 只由 `ExecutionAuthority::operation_authority_grant()` 生成；
  dynamic Platform/MCP/Extension provider、Workspace Module 与执行 core 使用同一个值，不对同一
  facts 集合另行 hash。
- Gateway 生成 actor surface 时统一应用 actor visibility 与 required capabilities。provider
  discovery 失败可以在宽表面中隔离为 diagnostic；当 exact invoke 或 OperationScript preflight
  请求属于该 provider 的 Operation 时，必须恢复 typed unavailable，而不是误报 OperationRef
  不存在。
- applied tool-set revision 与 binding generation 仍由 Tool Broker 校验；业务 tool adapter 不复制这套状态机。
- Complete Agent callback delivery evidence 只存在于 callback path；server-side nested Operation
  invocation 只携带 RuntimeThread、current applied surface revision、trace/idempotency/deadline，
  Host binding generation 为空。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| capability 未开放 | 不生成 Workspace tools |
| gateway/engine handle 未装配 | surface/tool execution 返回 unavailable |
| binding 或 tool-set revision 过期 | Tool Broker 返回 stale |
| OperationRef 不在当前 actor surface | invalid arguments |
| requested Operation 所属 dynamic provider unavailable | 保留 provider unavailable code |
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
- 原生工具显式 exposure allowlist、`platform:*` exact ref、Broker re-authorization 与 control-tool
  non-exposure。
- Interaction presentation attachment 使用 exact run/agent 双坐标；Agent 不提交 renderer、URI、
  title 或 attachment identity。

### 7. Wrong vs Correct

```text
Wrong: WorkspaceModule -> RuntimeSession -> provider dispatch
Correct: Agent Runtime Tool Broker -> API adapter -> canonical Operation Gateway
```
