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

ThreadNameUpdatedNotification {
  threadId: string,
  threadName?: string | null,
}
```

RuntimeWire 通过 `RuntimeWireEnvelope` 透明承载 Driver command/event/response 与 reverse HostPort frame，不转换为 `BackboneEnvelope` 或 `serde_json::Value` 中转。

## 3. Contracts

- Managed Runtime journal 是 Runtime lifecycle 的唯一 durable authority。Backbone 事件不能创建、完成或恢复 Runtime operation/turn/item/interaction。
- UI 的 submit/steer/interrupt/compact/resolve 可用性只读取 canonical Runtime snapshot 的 `command_availability`，不从 Backbone、Lifecycle status、executor kind 或 transcript 推导。
- PTY terminal、workspace module、AgentFrame/product notification 是独立资源事实；它们可以引用 Runtime coordinate 用于展示和关联，但不能改变 Runtime state。
- Relay 对 RuntimeWire 保持 typed envelope、stream sequence、ack/replay 与 generation fencing；产品 Backbone relay 与 RuntimeWire 是两条不同协议 lane。
- Vendor adapter 把 Codex/Native/enterprise 事件映射为 AgentDash-owned Runtime event。Vendor DTO 与 Backbone 都不能泄漏进 canonical Runtime contract。
- Durable cursor 只属于 canonical Runtime journal；Backbone product stream 使用自己的产品事件顺序，不复用 Runtime cursor。
- Runtime journal 内部使用 owned `PlatformEvent::ContextFrameChanged` 保存完整 ContextFrame。Session journal contract 在单一 protocol normalization boundary 将它投影为 `SessionMetaUpdate { key: "context_frame" }`，原因是现有 session reducer、feed grouping 与 UI 以该 wrapper 表达稳定展示协议；映射只替换 wrapper，frame payload、空值、section 顺序与相邻 frame 边界保持不变。
- Session carrier/wrapper可以承载Runtime routing与cursor元数据；wrapper内owned `BackboneEvent`的discriminant、payload、顺序、optional/null语义与pinned Codex App Server协议及AgentDash扩展保持完全一致。frontend reducer只消费protected body，不感知Runtime internal fact。
- Session durable `event_seq`沿用raw Runtime EventSequence（加fork继承prefix），允许internal-only facts形成空洞；GET、NDJSON live、断线replay与fork point共享该坐标，使cursor之后的第一条可见tool terminal/ContextFrame不会被跳过。
- `thread_name_updated` 使用 pinned Codex-shaped owned notification 作为标准 session
  presentation variant，set/replace/clear 都进入同一 generated union。Native 与 Codex
  producer 只负责生成完整标准 payload，Runtime admission 负责 durable/source/name 约束，
  因而前端与 AgentRun 不需要 executor-specific 标题事件。

## 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Backbone 收到 Runtime lifecycle payload | 拒绝 contract drift；由 adapter/Managed Runtime 写 canonical journal |
| UI 只有 Backbone terminal notification | 保持命令不可用性由 Runtime snapshot 决定 |
| RuntimeWire sequence gap | typed protocol violation；stream 进入 Lost/reopen 流程 |
| RuntimeWire duplicate frame | 按 stream sequence 幂等 ack，不重复 Driver/HostPort 副作用 |
| PTY terminal exits | 更新 terminal resource projection，不 terminalize Agent Runtime |
| stale Driver generation emits event | fence/quarantine；Backbone 不可用于复活状态 |
| carrier metadata变化但protected body相同 | reducer输出与main一致；wrapper差异不进入item/card identity |
| visible event前存在internal cursor gap | `after`从raw cursor继续并返回下一条可见event，不按过滤后序号跳过 |
| `thread_name_updated`为string或显式null | generated Rust/TS union保留同一标准variant与optional/null语义 |
| Native或Codex试图发送executor-specific标题事件 | contract/source gate失败；producer必须映射为标准variant |

## 5. Good / Base / Bad Cases

- Good：Driver event 经 adapter 进入 Managed Runtime journal；UI Runtime feed 读取 canonical cursor，同时 Workspace Module presentation 作为独立 Backbone 产品通知打开面板。
- Good：`WorkspaceModulePresentationRequested` 使用独立 typed discriminant。journal replay恢复
  审计事实，live observer消费命令式展示意图；`ControlPlaneProjectionChanged`只触发投影刷新。
- Good：Native与Codex都提交`ThreadNameUpdatedNotification`，session、Runtime projection与产品
  invalidation共享同一个event discriminant。
- Base：本机 PTY 退出只更新终端卡片，Agent Runtime turn 继续由 Runtime snapshot/events 表达。
- Bad：把 Codex `turn/completed` 转成 Backbone terminal，再由 Application 反推 AgentRun 已完成；这会制造第二事实源。
- Bad：为Native新增`source_session_title_updated`并在AgentRun外层翻译；这会让executor决定消费契约。

## 6. Tests Required

- Contract/schema generation 覆盖 Backbone、Runtime Contract 与 RuntimeWire 三套互不冒充的类型。
- Runtime facade tests 证明 Backbone 不参与 canonical operation/snapshot mutation。
- Frontend tests 证明 command availability 只取 Runtime snapshot。
- Relay tests 覆盖 RuntimeWire typed envelope、sequence gap、duplicate、ack/replay 与 reconnect fencing。
- Resource tests 证明 PTY terminal 与 Runtime terminal 使用不同 discriminant 和 reducer。
- main parity账本逐文件固定现有Session UI，并把protected body送入现有reducer，断言user、多工具单卡、业务错误后continuation与final assistant顺序。
- Journal tests覆盖raw cursor gap、live→replay与fork cutoff，禁止对filtered presentation重新enumerate。
- protocol generation与source gate断言标准union包含`thread_name_updated`，且仓库不存在第二个
  source-title事件、payload或adapter名称。

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

```rust
// Wrong: 每个executor发自己的业务标题事件。
BackboneEvent::NativeConversationTitleUpdated { title }

// Correct: 所有producer使用pinned Codex-shaped通用事件。
BackboneEvent::ThreadNameUpdated(ThreadNameUpdatedNotification {
    thread_id: source_thread_id,
    thread_name: Some(title),
})
```
