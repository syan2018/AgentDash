# Backbone Product and Resource Event Protocol

## 1. Scope / Trigger

Backbone 只承载 AgentDash 产品通知与资源状态投影，例如 AgentFrame surface 变化、Workspace Module presentation、PTY terminal 与平台诊断。新增跨端产品事件或资源事件时复核本规范。Agent Runtime Thread/Turn/Item/Interaction、operation terminal、context head 与 command availability 不进入 Backbone。

## 2. Signatures

```rust
pub struct BackboneEnvelope {
    pub event_id: String,
    pub timestamp: DateTime<Utc>,
    pub event: BackboneEvent,
    pub metadata: EventMetadata,
}
```

Canonical Agent Runtime stream 使用独立 owned contract：

```text
GET /agent-runs/{run_id}/agents/{agent_id}/runtime/events/stream/ndjson
RuntimeEventEnvelope { cursor, operation_id, event }
```

RuntimeWire 通过 `RuntimeWireEnvelope` 透明承载 Driver command/event/response 与 reverse HostPort frame，不转换为 `BackboneEnvelope` 或 `serde_json::Value` 中转。

## 3. Contracts

- Managed Runtime journal 是 Runtime lifecycle 的唯一 durable authority。Backbone 事件不能创建、完成或恢复 Runtime operation/turn/item/interaction。
- UI 的 submit/steer/interrupt/compact/resolve 可用性只读取 canonical Runtime snapshot 的 `command_availability`，不从 Backbone、Lifecycle status、executor kind 或 transcript 推导。
- PTY terminal、workspace module、AgentFrame/product notification 是独立资源事实；它们可以引用 Runtime coordinate 用于展示和关联，但不能改变 Runtime state。
- Relay 对 RuntimeWire 保持 typed envelope、stream sequence、ack/replay 与 generation fencing；产品 Backbone relay 与 RuntimeWire 是两条不同协议 lane。
- Vendor adapter 把 Codex/Native/enterprise 事件映射为 AgentDash-owned Runtime event。Vendor DTO 与 Backbone 都不能泄漏进 canonical Runtime contract。
- Durable cursor 只属于 canonical Runtime journal；Backbone product stream 使用自己的产品事件顺序，不复用 Runtime cursor。

## 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Backbone 收到 Runtime lifecycle payload | 拒绝 contract drift；由 adapter/Managed Runtime 写 canonical journal |
| UI 只有 Backbone terminal notification | 保持命令不可用性由 Runtime snapshot 决定 |
| RuntimeWire sequence gap | typed protocol violation；stream 进入 Lost/reopen 流程 |
| RuntimeWire duplicate frame | 按 stream sequence 幂等 ack，不重复 Driver/HostPort 副作用 |
| PTY terminal exits | 更新 terminal resource projection，不 terminalize Agent Runtime |
| stale Driver generation emits event | fence/quarantine；Backbone 不可用于复活状态 |

## 5. Good / Base / Bad Cases

- Good：Driver event 经 adapter 进入 Managed Runtime journal；UI Runtime feed 读取 canonical cursor，同时 Workspace Module presentation 作为独立 Backbone 产品通知打开面板。
- Base：本机 PTY 退出只更新终端卡片，Agent Runtime turn 继续由 Runtime snapshot/events 表达。
- Bad：把 Codex `turn/completed` 转成 Backbone terminal，再由 Application 反推 AgentRun 已完成；这会制造第二事实源。

## 6. Tests Required

- Contract/schema generation 覆盖 Backbone、Runtime Contract 与 RuntimeWire 三套互不冒充的类型。
- Runtime facade tests 证明 Backbone 不参与 canonical operation/snapshot mutation。
- Frontend tests 证明 command availability 只取 Runtime snapshot。
- Relay tests 覆盖 RuntimeWire typed envelope、sequence gap、duplicate、ack/replay 与 reconnect fencing。
- Resource tests 证明 PTY terminal 与 Runtime terminal 使用不同 discriminant 和 reducer。

## 7. Wrong vs Correct

```rust
// Wrong: presentation event becomes Runtime authority.
runtime.complete_turn(backbone_event.turn_id).await?;

// Correct: only a validated Driver event enters the Managed Runtime journal.
runtime.accept_driver_event(binding, generation, driver_event).await?;
```

```ts
// Wrong
const canInterrupt = lastBackboneEvent?.type === "turn_started";

// Correct
const canInterrupt = runtimeSnapshot.command_availability.interrupt?.available === true;
```
