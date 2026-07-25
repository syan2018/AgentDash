# Relay Runtime Wire production placement activation

## Purpose

本文固定 Remote Complete Agent 从真实 Cloud/Local WebSocket 到 Host verified registration
的 production path。Relay 只承载连接、stream、placement provenance、ack/replay、
backpressure 与 liveness；Agent identity、history、surface、Product policy 和 capability
admission 仍分别归 Complete Agent、Host 与 Managed Runtime。

## Existing executable boundaries

Cloud 入口是 `agentdash-api/src/relay/ws_handler.rs`：

1. backend token 解析可信 `BackendConfig`；
2. 首条 `Register` 校验 payload backend ID 与认证身份一致；
3. `BackendRegistry` 注册连接并发布 runtime health；
4. WebSocket inbound、registry outbound 与 ping 在同一 select loop 运行；
5. 断线清 lease、unregister backend 并发布 offline。

Local 入口是 `agentdash-local/src/ws_client.rs::run_session`：

1. WebSocket connect；
2. 发送 `Register` 并等待 `RegisterAck`；
3. 单 writer task 发送 command result、event 与 capability change；
4. inbound command 进入 `LocalCommandRouter`；
5. 断线后重连。

当前 `LocalCommandRouter` 只拥有 Workspace、Tool、Materialization、MCP、Extension 与
Terminal。它不拥有 Complete Agent lifecycle。当前
`RuntimeWireAgentServiceEndpoint` 可以包装任意真实 `CompleteAgentService`，但只在
Remote integration tests 中使用；`RemoteCompleteAgentService`、
`remote_complete_agent_contribution` 与
`CompleteAgentComposition::register_contribution` 已形成 Cloud 侧完整业务核心。

缺失边界是：

- production WebSocket `RuntimeWirePlacement`；
- Local Complete Agent endpoint catalog；
- dynamic offer lifecycle；
- Host-owned independent remote verification authority；
- Cloud placement registrar 与 disconnect/reconnect lifecycle。

## Authority and trust

`ServiceOfferAdvertisement` 是 Agent/transport claim，不是 `RuntimeOffer`，也不能写入
信任目录自证。

Host verification authority 组合两类可信记录：

- builtin/deployment pinned records；
- 由已认证 backend transport 与版本化 deployment policy 生成的 remote transport
  attestation records。

remote evidence digest 覆盖：

- authenticated backend ID；
- transport incarnation；
- remote service instance 与 binding generation；
- descriptor/profile digest；
- publisher、version、build 与 conformance claim；
- verifier/deployment policy revision。

验证成功后仍统一调用 `CompleteAgentComposition::register_contribution`。Host 原子持久化
verification、offer、placement 与 remote mapping；Relay 不建立 Agent 表。

S5 默认：

- Local endpoint catalog 可以为空；只有真实配置的 Complete Agent 才 advertise；
- 不为了证明 transport 存在而把 Codex/Dash Agent 强制搬到 Local；
- remote trust root 使用服务端预置、版本化 deployment manifest；
- placement vocabulary 保留未来签名 attestation evidence 字段，但 remote claim
  永远不能自证。

## Placement vocabulary

placement DTO 归 `agentdash-agent-runtime-wire`；Relay 顶层 protocol 只包装并调度。

### ServiceOfferAdvertisement

完整 snapshot，至少包含：

```text
advertisement_id
advertisement_revision
transport_incarnation_id
offer_digest
services[]:
  remote_service_instance_id
  remote_binding_generation
  descriptor
  publisher_integration
  service_version
  claimed_build_digest
  claimed_conformance_suite_revision
  health/readiness
```

backend ID 来自 authenticated socket，不接受 payload 自报。同 revision + 同 digest
幂等；同 revision + 异 digest 是 protocol conflict。removal 只改变 availability，不删除
Agent history。

### RuntimeWireOpen / OpenAck

Cloud 分配全局唯一且不复用的 `stream_id`。Open 携带 authenticated backend、transport
ID/incarnation、remote service instance/generation 与 protocol revision。OpenAck 精确
echo provenance 并返回 accepted/rejected；它只证明 endpoint ready，不证明 Host
verification 已成功。

### RuntimeWireFrame / Ack

```text
RuntimeWireFrame {
  stream_id
  placement_provenance
  envelope: RuntimeWireEnvelope
}
```

业务 command/read/change/callback 的 frame ID、critical、ack/replay 与 generation fence
仍由 Runtime Wire owner。placement `Ack` 只优先调度 inner
`RuntimeWireFrame::Ack`，不建立第二套 sequence/ack domain。

### RuntimeWireClosed / PlacementLost

Closed 表达 graceful stream close，只改变 availability。

PlacementLost 携带 stream/provenance、reason code、last received frame、retryable。endpoint
incarnation/generation drift、critical protocol violation、critical queue overflow、resume
window loss 或明确 endpoint removal 才能产生 definitive lost。瞬时 socket disconnect
先表达 transport unavailable，不能制造 Agent failed/completed。

## Backpressure

Cloud 与 Local 当前 unbounded sender 不能作为最终 placement：

- 每连接有界 control queue；
- 每 stream 有界 critical data queue；
- open/ack/close/lost 保留容量或独立高优先级 lane；
- critical frame 不丢弃；容量耗尽进入 typed PlacementLost，使 pending request
  unknown/reconcile；
- heartbeat/telemetry 等 noncritical observation 可合并；
- unacked critical frames 只在相同 transport incarnation 与 endpoint generation 下
  有界重放。

## Production modules

### Runtime Wire

- `agentdash-agent-runtime-wire/src/placement.rs`
- placement schema/codegen/freshness tests

### Relay

- `agentdash-relay/src/protocol/runtime_wire.rs`
- Relay message variants：Open/OpenAck/Frame/Ack/Closed/Lost/Advertisement
- 非 Agent MCP、Terminal、Extension、Workspace、VFS lanes 保持独立

### Cloud/API

- `agentdash-api/src/relay/runtime_wire_placement.rs`
  - stream registry；
  - bounded queues；
  - provenance/generation fence；
  - offer registrar；
  - concrete `RuntimeWirePlacement`。
- `relay/ws_handler.rs`
  - RegisterAck 后才接受 advertisement；
  - Runtime Wire 分派；
  - disconnect 通知 placement registry。
- `bootstrap/relay.rs` / `app_state.rs`
  - verification authority；
  - remote offer registrar；
  - `CompleteAgentComposition` dynamic register/detach。

### Infrastructure/Host

- `complete_agent_composition.rs`
  - pinned + trusted remote verification authority；
  - advertisement claim 与 independent evidence 的交叉验证；
  - 统一 `register_contribution`。
- Host live registry 可 detach/replace handle；durable verification/offer/placement history
  仍保持 Host authority。

### Local

- `agentdash-local/src/runtime_wire_placement.rs`
  - endpoint catalog；
  - offer revision/digest；
  - stream router 与 endpoint pump；
  - bounded queues。
- `ws_client.rs`
  - RegisterAck 后 advertise；
  - Runtime Wire 不进入 `LocalCommandRouter`；
  - reconnect 保留同 transport incarnation，或显式开启新 incarnation。

## Activation order

1. 冻结 placement DTO/schema，不改变 production route。
2. 建立 Local endpoint catalog 与 Cloud placement registry。
3. 建立 Host-owned remote verification authority。
4. RegisterAck 后启用完整 offer snapshot。
5. trusted offer 执行 Open/OpenAck，构造 Cloud placement 与 remote contribution。
6. `register_contribution().materialize().describe()` 真实跨 WebSocket，Host 提交 verified
   facts 并 attach Remote proxy。
7. Runtime command/read/change/inspect 与 reverse Tool/Hook callback 走唯一 Runtime Wire。
8. tracer bullets 通过后删除旧 Prompt/SessionEvent Agent lane。

## Required tracer bullets

- backend token / Register identity mismatch 不能进入 registrar；
- advertisement exact replay、digest conflict 与 untrusted claim rejection；
- Open provenance/generation drift 在 endpoint side effect 前拒绝；
- `register_contribution -> describe` 真实跨 API/Local WebSocket，并由 PostgreSQL Host
  repository记录 verification/offer/placement/mapping；
- execute/read/changes/inspect 与 reverse Tool/Hook callback 双向往返；
- duplicate frame/ack、frame gap、stale generation 与 wrong stream provenance；
- side effect applied 后 response loss，通过同 effect inspect 收敛且不重复执行；
- reconnect 同 incarnation 可 reattach；新 endpoint generation 拒绝旧 stream；
- critical queue overflow -> PlacementLost；graceful Closed 不产生 Agent terminal；
- API restart 后从 Host PG facts恢复，Local re-advertise/re-open；
- `u64::MAX` revision/frame roundtrip 与 exhaustion typed error；
- Workspace/Terminal/MCP/Extension/VFS lanes regression。

测试必须使用真实 `ws_handler`、`ws_client`/placement router、
`RemoteCompleteAgentService`、`CompleteAgentComposition`、PostgreSQL Host repository 和
一个 concrete deterministic Complete Agent test service。该 service 位于真实 endpoint
后；禁止用拦截 `placement.send()` 后即时伪造 response 的 loopback facade替代
production composition。

## Legacy deletion gate

只有以下条件全部成立才删除 Relay 旧 Agent lane：

- production Agent command 只经 Runtime → Host → Complete Agent → Runtime Wire；
- Local endpoint route 已存在，且空 catalog 被诚实表达为无 Remote Agent；
- Prompt/Cancel/Steer response 与 SessionEvent normal consumers 为零；
- conversation 只来自 Runtime canonical snapshot/change；
- Workspace/Terminal/MCP/Extension 仍走各自 lanes；
- reconnect、gap、interaction、compaction、callback tracers 通过；
- `cargo metadata`、`rg` 与 generator freshness 证明旧 protocol 零消费者；
- 不保留 fallback、dual registration 或 compatibility facade。

