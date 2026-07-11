# Frontend / Backend Contracts

## 1. Scope / Trigger

本规范约束浏览器与 API 之间的共享 DTO、AgentRun control plane、Runtime stream、Workspace Module/Canvas presentation，以及跨端资源引用。新增 endpoint、生成类型、命令按钮、事件 reducer 或资源坐标时必须复核。

## 2. Contract Crate Shape

```text
agentdash-contracts
  -> product/resource DTOs
  -> packages/app-web/src/generated/*

agentdash-agent-runtime-contract
  -> Runtime command/snapshot/event/profile DTOs
  -> packages/app-web/src/generated/agent-runtime-contracts.ts

agentdash-agent-runtime-wire
  -> Cloud/Local Driver transport DTOs
  -> packages/app-web/src/generated/agent-runtime-wire.ts
```

- Rust 类型与生成器是 wire shape 的事实源；TypeScript 不复制手写同名 DTO。
- Runtime Contract、RuntimeWire 与 Backbone/product contracts 是三套独立合同，不能因字段相似而互相反序列化中转。
- JSON 使用 `snake_case`；可选字段由 Rust serde/TS 导出共同定义。

## 3. AgentRun Runtime Contract

### Execution Profile discovery

- 执行器选择器读取的是产品级 `ExecutionProfileDto`，其稳定 identity 来自受信 Integration definition；该 DTO 只表达名称、availability 与 unavailable reason，不携带 RuntimeOffer、service instance、generation 或 placement credential。
- Native `PI_AGENT` 与 Codex `CODEX` 是独立 execution profile。definition 已注册但尚未首次 provision RuntimeOffer 是合法状态；discovery 不以当前 offer 数量决定 profile 是否存在。
- Native discovered-options 从 LLM Provider effective catalog 投影 provider/model 与精确不可用原因；Codex profile 不伪造 Native Provider/model 列表。
- ProjectAgent create/update 与 discovery 使用同一 profile-to-definition catalog 校验，避免 UI 可选值与 API 可保存值产生第二套枚举。
- Rust contracts 及生成 TypeScript 是 discovery/options DTO 的事实源，前端 feature model 不复制同名字段结构。

### Signatures

```text
GET  /agents/discovery
GET  /agents/discovered-options/stream?executor={PI_AGENT|CODEX}
POST /projects/{project_id}/agents/{project_agent_id}/agent-runs
POST /agent-runs/{run_id}/agents/{agent_id}/composer-submit
POST /agent-runs/{run_id}/agents/{agent_id}/cancel
GET  /agent-runs/{run_id}/agents/{agent_id}/runtime
GET  /agent-runs/{run_id}/agents/{agent_id}/runtime/context
GET  /agent-runs/{run_id}/agents/{agent_id}/runtime/events/stream/ndjson
POST /agent-runs/{run_id}/agents/{agent_id}/runtime/context/compact
POST /agent-runs/{run_id}/agents/{agent_id}/runtime/tool-approvals/{id}/{approve|reject}
```

```rust
AgentRunAcceptedRefs {
    run_ref,
    agent_ref,
    frame_ref?,
    runtime_thread_id?,
    runtime_operation_id?,
}

AgentRunCommandReceipt {
    client_command_id,
    status,
    duplicate,
    accepted_runtime_operation_id?,
    message?,
}
```

### Contracts

- Project Agent create 先建立 Lifecycle run/agent/frame 产品事实，再通过 `AgentRunProductDelivery` 提交首条 canonical Runtime mailbox command。响应返回产品 refs 与可选 Runtime thread/operation refs。
- ProjectAgent 是启动默认模板；create-run 的 `executor_config` 与 `backend_selection` 是本次 RunLaunchProfile intent。admission 在 provision 前将 effective executor/provider/model 写入 AgentFrame execution profile，并将 backend intent传给 Host offer selection；它们不是无状态 HTTP override，也不改写 ProjectAgent defaults。
- Composer submit 返回 queued mailbox identity 或 canonical `OperationReceipt`；重复 `client_command_id` 返回同一 operation，不创建第二次 Driver side effect。
- UI 命令可用性只读取 Runtime snapshot 的 `command_availability`。Lifecycle status、executor kind、Backbone、transcript 或 HTTP success 不能推导 submit/steer/interrupt/compact/resolve 权限。
- `AgentRunRuntimeBinding` 是 `run_id + agent_id` 到 Runtime thread/Host binding 的唯一产品执行坐标。浏览器不接触 Driver source IDs、Host lease 或 placement credential。
- Runtime feed 由 snapshot transcript 建立 baseline，再按 durable cursor 消费 `RuntimeEventEnvelope`。重连携带最后 cursor；retention gap/Lost 使用 typed Runtime error。
- Runtime context、compaction、interaction 与 tool approval 均通过 facade/canonical operation；不存在独立 session command、protocol turn ID 或 vendor DTO 路径。
- Mailbox 只持久化 queued product intent 与 `accepted_runtime_operation_id`。没有 canonical command 的管理动作不进入 UI，也不保留死 endpoint。

### Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| execution profile definition 未进入最终 Host inventory | discovery 保留 profile 并返回 `available=false + unavailable_reason`；ProjectAgent 写入拒绝未知 profile |
| create-run executor/provider/model override 合法 | 与 ProjectAgent defaults 合并，写入新 AgentFrame revision后再 provision |
| explicit backend 有匹配 activated offer | 只绑定该 backend placement并持久化 binding coordinates |
| explicit backend 无匹配 offer | typed unavailable；不得回退任意 backend或 InProcess instance |
| `PI_AGENT` 没有 executable Provider | profile 可见但 disabled；options 返回 Provider 诊断，不依赖 RuntimeOffer |
| `CODEX` definition 已注册 | profile 可选；options 不伪造 Native Provider/model |
| options executor 未知 | `400 Bad Request`，不探测 Connector 或任意 offer |
| AgentRun target 不存在或跨 Project | not found/authorization error before Runtime side effect |
| client command id 为空 | `400 Bad Request` |
| stale Runtime revision/active turn | typed stale error；前端刷新 snapshot |
| command availability=false | UI 禁用且 API 在副作用前拒绝 |
| command queued | 返回 mailbox message identity；worker 后续写 accepted operation |
| command duplicate | 返回原 operation receipt |
| binding disconnect | snapshot/event 显示 `Lost`，旧 generation 晚到事件不改变 UI |

### Tests Required

- Contract generation/check 覆盖 product refs、Runtime snapshot/event/profile 与 RuntimeWire。
- Production composition test 断言最终 `IntegrationDriverHost` inventory 包含动态装配的 Native definition 和已注册的 Codex definition。
- Discovery/API tests 覆盖 Native/Codex 独立 availability、未知 profile、Provider diagnostic 与 options NDJSON。
- Selector tests 断言不可用 profile/Provider 保持可见、disabled 且展示原因。
- Service tests 覆盖 URL encoding、create/composer/cancel/context/approval endpoints。
- Command-state tests 证明 availability 只取 Runtime snapshot。
- Feed tests 覆盖 snapshot baseline、durable cursor、duplicate event、reconnect 与 typed stream error。
- Project Agent create E2E 覆盖 lifecycle facts -> ProductDelivery -> binding/thread -> operation response。

## 4. Companion and Workflow Product Facts

- Companion/subagent dispatch 以 Lifecycle run/agent/frame、assignment/activity attempt 与 canonical Runtime thread/operation refs表达。
- Workflow、Gate、Task、Story 只保存产品编排与 evidence 坐标；Runtime terminal 通过 canonical Runtime event/snapshot 投影，不保存另一份执行 session 状态。
- 等待与 gate delivery 进入 canonical AgentRun mailbox。恢复依赖 mailbox claim/lease 与 accepted Runtime operation，而不是进程内 callback。
- UI 可以展示 Runtime trace link，但不得把 trace metadata当作 AgentRun command authority。

## 5. Workspace Module, Canvas and VFS

- Workspace Module presentation payload 的 concrete URI 是 tab identity；浏览器不根据 view key 猜测资源 URI。
- Agent-facing operation 只来自 generated operation catalog。panel-only action 不自动成为 Agent tool。
- Canvas runtime snapshot、VFS resource surface 与 Agent tool 使用同一当前 AgentFrame/Business Surface projection；Frame 是产品期望，不是 Runtime lifecycle authority。
- Runtime-bound Canvas/extension invocation 以 `run_id + agent_id` 进入 API，后端通过 canonical `AgentRunRuntimeBinding` 获取 thread/binding coordinate。
- Backend placement 与 VFS mount access 是资源 facts；它们约束 Business Surface/Tool Broker，但不创建 Runtime capability guarantee。
- iframe/webview 只发送声明的 action/channel key 与 input；父页面补齐 AgentRun/Project identity，API 完成 authorization 与 binding resolution。

## 6. MCP and External Resource Contracts

- MCP preset contract 分离 declaration、credential refs、placement requirement 与 probe result。secret 不进入共享 DTO。
- Runtime tool availability 是 Business Surface required contribution 与 bound Runtime profile 的交集；MCP catalog 存在不等于 Driver 能原生或精确消费。
- Remote/local resource references 使用 typed owner/mount/backend coordinate；浏览器不发送本机绝对路径作为业务身份。
- 外部 service/provider 不可用时返回 typed diagnostic；不选择任意在线 backend 或另一 provider fallback。

## 7. Good / Base / Bad Cases

- Good：Draft 创建返回 run/agent/frame 与 Runtime thread/operation；页面随后从 runtime inspect/events 渲染 transcript，并从 snapshot availability启用 interrupt。
- Good：首次运行前 RuntimeOffer 表为空，selector 仍从最终 Host definition inventory 展示 `PI_AGENT`/`CODEX`。
- Base：没有 executable Provider 时 `PI_AGENT` disabled 并展示凭据诊断，`CODEX` availability 独立计算。
- Bad：API 读取 composition 前的临时 definition registry，导致动态装配的 Native definition 在真实启动中消失。
- Base：首条消息排队，响应只有 mailbox identity；worker dispatch 后 workspace refresh 观察 accepted operation 与新 cursor。
- Bad：前端调用已经没有后端实现的 fork/mailbox endpoint，或根据 `execution_status=running` 自行启用 cancel。
- Good：Canvas presentation 用 `canvas://{mount_id}` 打开 tab，并通过当前 AgentFrame surface刷新资源。
- Bad：把 RuntimeWire frame转成 Backbone JSON 再由 UI 推导 Runtime terminal。

## 8. Wrong vs Correct

```ts
// Wrong
const canCancel = lifecycleAgent.status === "running";

// Correct
const canCancel = runtimeInspect.snapshot
  ?.command_availability.interrupt?.available === true;
```

```rust
// Wrong
let thread_id = request.protocol_turn_id;

// Correct
let binding = agent_run_runtime_binding_repo.load(&target).await?;
let receipt = agent_run_runtime.send_message(command).await?;
```

```rust
// Wrong: composition 前 registry 不是生产 Host inventory
let profiles = app_state.runtime_definition_registry.definitions();

// Correct: discovery、ProjectAgent validation 与 Relay trust 共用最终 Host
let profiles = app_state.services.agent_runtime_host.definitions();
```
