# Agent Runtime Wire Relay Transport

## 1. Scope / Trigger

本规范适用于 AgentDash-owned Runtime Wire 经Cloud/Local Relay承载远端Agent service placement的协议、stream状态、断线恢复与本地driver终结点。修改RuntimeWire envelope、Relay message、remote runtime Integration、Local handler、ack/replay/backpressure或Cloud placement resolver时必须复核本规范。

## 2. Signatures

```rust
pub trait RuntimeWirePlacement: Send + Sync {
    async fn send(&self, frame: RuntimeWireEnvelope)
        -> Result<(), RemoteRuntimeTransportError>;
    async fn receive(&self)
        -> Result<RuntimeWirePlacementEvent, RemoteRuntimeTransportError>;
    async fn acknowledge_disconnect(&self);
}

pub trait RuntimeWirePlacementResolver: Send + Sync {
    async fn resolve(
        &self,
        provenance: &RuntimeWireProvenance,
    ) -> Result<Arc<dyn RuntimeWirePlacement>, RemoteRuntimeTransportError>;
}

pub trait RuntimeDriverEndpointResolver: Send + Sync {
    async fn resolve(
        &self,
        provenance: &RuntimeWireProvenance,
    ) -> Result<Arc<RuntimeWireDriverEndpoint>, RuntimeWireHandlerError>;
}
```

`RuntimeWireCommandHandler`维护per-stream negotiation、sequence、ack与outbound pump；`RuntimeWireDriverEndpoint`只终结既有`AgentRuntimeDriver`。Relay message直接承载typed `RuntimeWireEnvelope`。

## 3. Contracts

- Runtime Wire保留真实service definition/instance、binding、driver generation、profile digest与transport provenance。Relay是placement transport，不能生成或覆盖Agent service identity。
- Runtime Wire remote placement provenance包含`HostIncarnationId`。Cloud从当前Backend inventory offer原样投影该identity到proxy instance与open request；Local只接受当前进程incarnation，使旧连接的相同instance/generation无法在Host重启后恢复。
- Open negotiation固定protocol revision、transport profile/digest与max-in-flight。双方不满足revision/profile要求时必须在dispatch前typed reject。
- 每个方向使用严格递增sequence与累计ack。Duplicate frame幂等确认；sequence gap拒绝；超过协商in-flight上限产生backpressure，不丢帧或无限缓冲。
- Transport是持久双向`send/receive` stream，不是有限request/response exchange。Remote driver使用独立receive pump和frame-id correlation；dispatch receipt返回后的异步DriverEvent必须继续经Arc event sink送达。
- 同一Remote dispatch的DriverEvent notification与最终Response进入同一个ordered inbound queue。Response只有在此前events完成canonical sink后才能解除Host dispatch/lease；HostPort callback使用独立可重入correlation路径，不能被该顺序屏障阻塞。
- 同provenance重连按ack cursor清理已确认帧并有序replay未确认帧；provenance任何坐标不同都不能复用stream。一次真实disconnect只产生一次placement loss/binding lost输入。
- Backend断连按`registry.unregister -> placement Disconnected -> remote driver BindingLost -> acknowledge_disconnect -> inventory.withdraw`收敛。`unregister`必须等待disconnect acknowledgment后返回，因为offer撤销会改变driver可达性；ack是BindingLost处理完成的屏障，不是“事件已入队”的确认。
- Remote disconnect先向authoritative sink提交一次BindingLost，再关闭pending response correlation；反向顺序会让pending failure与sink各自产生Lost。
- EOF/transport loss不得伪造Completed；它向Managed Runtime报告Lost输入，由Runtime收敛active operation/Thread状态。旧generation frame在进入Runtime前被fence。
- Local endpoint只把Driver request交给现有Host-owned Driver；Managed Runtime request必须拒绝，避免本机形成第二Runtime。Local handler通过显式resolver注入，无配置时typed error，不fallback到legacy prompt。
- Local每个stream拥有独立锁与outbound pump；全局streams registry锁不能跨driver await。Response、notification与delayed event可以在command返回后持续写入WebSocket。
- Remote Integration由企业/first-party调用者传入真实`AgentServiceDefinition`与factory key；transport crate只提供remote placement factory，不注册`builtin.runtime_wire.remote`伪service。
- Runtime command/receipt/event保持canonical typed形状，包括bind start/resume/fork、typed inspect、surface/hook/tool receipts；禁止RuntimeEnvelope -> `serde_json::Value` -> 再反序列化。

## 4. Validation & Error Matrix

| 场景 | 必须得到的结果 |
| --- | --- |
| protocol revision/profile不兼容 | Open阶段typed reject |
| frame sequence重复 | 不重复dispatch，返回当前ack |
| frame sequence跳号 | gap error，不推进cursor |
| unacked达到max-in-flight | backpressure，不覆盖旧frame |
| 同provenance重连 | prune acked并顺序replay unacked |
| provenance/binding/generation变化后resume | reject，建立新stream |
| receipt返回后driver发送terminal/event | receive pump继续转发，不丢失 |
| dispatch response越过前序DriverEvent | response保持pending，直到前序event sink完成 |
| EOF/断线 | exactly-once BindingLost/Lost输入，不报Completed |
| Disconnected已入队但remote driver尚未处理 | Relay等待`acknowledge_disconnect`，不得先撤销offer |
| Local resolver找不到Host driver/generation | typed error，无legacy fallback |
| Local endpoint收到Managed Runtime request | typed unsupported，禁止第二Runtime |
| stale generation event/frame | fence，不推进canonical projection |

## 5. Good / Base / Bad Cases

**Good case:** Remote enterprise service以真实service provenance打开stream，协商revision/profile/in-flight，dispatch response通过frame correlation返回，后续异步Item/terminal继续由receive pump发送；断线重连按ack replay未确认帧。

**Base case:** 对端重复发送最后一帧，transport不重复调用Driver，只回当前累计ack。

**Bad case:** API把每个RuntimeWireFrame塞进“一个request只等一个response”的pending map，或Local endpoint创建自己的Managed Runtime。这会丢异步事件或形成双事实源，必须由持久stream和Host resolver边界替代。

## 6. Tests Required

- Relay state测试覆盖negotiation、ordered sequence、duplicate、gap、max-in-flight、ack prune、unacked replay、同/异provenance reconnect与disconnect once。
- Cloud placement测试必须并发执行`unregister`与`receive`，断言收到Disconnected后只有调用`acknowledge_disconnect`才完成unregister；WebSocket断连集成测试断言offer withdraw发生在该屏障之后。
- Remote driver测试覆盖response correlation、多个in-flight request、receipt后delayed event、EOF Lost、generation fence及全部receipt字段。
- Ordered inbound测试阻塞event sink并断言dispatch response仍pending，释放event后response完成；HostPort callback roundtrip必须同时证明可重入无死锁。
- Local handler loopback覆盖open -> describe/dispatch -> response -> delayed event -> ack，duplicate ack-only与invalid generation。
- 验证endpoint拒绝Managed Runtime request，handler无resolver时无fallback。
- Contract/Wire generation与round-trip测试证明typed envelope没有Value中转。
- WP08 production test必须覆盖Cloud多帧placement resolver、真实Host resolver注入、WebSocket event channel、disconnect/reconnect与legacy RelayPrompt删除。
- Relay/Remote/Local/API scoped tests、strict clippy、contracts、fmt与diff check必须通过。

## 7. Wrong vs Correct

```rust
// Wrong: 一次exchange只能返回dispatch期间已经收集到的事件。
let frames = placement.exchange(request).await?;

// Correct: request/response和异步events共享持久双向stream。
placement.send(request).await?;
while let Ok(frame) = placement.receive().await {
    route_correlated_response_or_event(frame).await?;
}
```

```rust
// Wrong: Disconnected只入队便撤销offer，driver可能在处理BindingLost前失去执行上下文。
route.disconnect().await;
inventory.withdraw(backend_id).await?;

// Correct: disconnect等待remote driver确认BindingLost已经处理。
registry.unregister(backend_id).await;
inventory.withdraw(backend_id).await?;
```

```rust
// Wrong: Relay以transport名称注册成Agent service。
remote_runtime_contribution(builtin_runtime_wire_definition());

// Correct: transport保留调用者提供的真实service definition。
remote_runtime_contribution(enterprise_definition, placement_factory);
```
