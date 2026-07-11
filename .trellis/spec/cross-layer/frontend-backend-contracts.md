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

### Signatures

```text
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
- Composer submit 返回 queued mailbox identity 或 canonical `OperationReceipt`；重复 `client_command_id` 返回同一 operation，不创建第二次 Driver side effect。
- UI 命令可用性只读取 Runtime snapshot 的 `command_availability`。Lifecycle status、executor kind、Backbone、transcript 或 HTTP success 不能推导 submit/steer/interrupt/compact/resolve 权限。
- `AgentRunRuntimeBinding` 是 `run_id + agent_id` 到 Runtime thread/Host binding 的唯一产品执行坐标。浏览器不接触 Driver source IDs、Host lease 或 placement credential。
- Runtime feed 由 snapshot transcript 建立 baseline，再按 durable cursor 消费 `RuntimeEventEnvelope`。重连携带最后 cursor；retention gap/Lost 使用 typed Runtime error。
- Runtime context、compaction、interaction 与 tool approval 均通过 facade/canonical operation；不存在独立 session command、protocol turn ID 或 vendor DTO 路径。
- Mailbox 只持久化 queued product intent 与 `accepted_runtime_operation_id`。没有 canonical command 的管理动作不进入 UI，也不保留死 endpoint。

### Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| AgentRun target 不存在或跨 Project | not found/authorization error before Runtime side effect |
| client command id 为空 | `400 Bad Request` |
| stale Runtime revision/active turn | typed stale error；前端刷新 snapshot |
| command availability=false | UI 禁用且 API 在副作用前拒绝 |
| command queued | 返回 mailbox message identity；worker 后续写 accepted operation |
| command duplicate | 返回原 operation receipt |
| binding disconnect | snapshot/event 显示 `Lost`，旧 generation 晚到事件不改变 UI |

### Tests Required

- Contract generation/check 覆盖 product refs、Runtime snapshot/event/profile 与 RuntimeWire。
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
