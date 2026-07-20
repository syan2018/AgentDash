# Complete Agent 持久化边界收敛设计

## 1. Design Goal

把 Complete Agent Host 分成两个生命周期不同的深模块：

1. **Live Complete Agent Catalog**：管理当前进程或连接中可调用的 adapter；完全 process-local。
2. **Durable Runtime Host**：管理 binding、generation、effect、lease 与恢复证据；使用 PostgreSQL。

外部调用者只需要“解析一个当前可用 target”和“以该 target provision/recover/dispatch”，不再了解
descriptor、verification、offer、placement 各自如何存取或重建。

## 2. Identity Model

### 2.1 Stable Product Identity

`ProductExecutionProfileRef` 继续持有产品意图：

- profile key；
- immutable configuration；
- profile digest；
- credential scope/reference。

它不是 Complete Agent service registration，也不证明当前 adapter 可用。

### 2.2 Logical Service Key

`AgentServiceInstanceId` 降级为逻辑 instance key：

- Codex 固定配置：`builtin.codex-app-server.default`；
- Native：由 Product profile digest 与 credential scope 派生；
- Remote：由受信 advertisement 的逻辑 service identity 派生。

逻辑 key 可用于同一 incarnation 内缓存和恢复兼容性匹配，但不能直接作为 dispatch endpoint。

### 2.3 Live Attachment Identity

新增 opaque `CompleteAgentLiveAttachmentId`，覆盖：

- logical service key；
- Host identity/incarnation；
- placement kind；
- transport/connection epoch（适用时）；
- remote identity/generation mapping（适用时）。

同一个逻辑 service 在不同 Host incarnation 中必须得到不同 attachment ID。process-local registry
以 attachment ID 为 key，禁止按逻辑 instance key 为旧 binding 兜底解析。

### 2.4 Durable Target Snapshot

新增可序列化 `CompleteAgentBindingTarget`（最终名称可按现有 contract 统一），至少包含：

```rust
pub struct CompleteAgentBindingTarget {
    pub logical_instance_id: AgentServiceInstanceId,
    pub live_attachment_id: CompleteAgentLiveAttachmentId,
    pub definition_id: AgentServiceDefinitionId,
    pub verified_build_digest: AgentPayloadDigest,
    pub verified_profile_digest: AgentProfileDigest,
    pub offer_profile_digest: AgentProfileDigest,
    pub placement: CompleteAgentPlacement,
    pub remote_binding: Option<CompleteAgentRemoteBindingFact>,
}
```

`CompleteAgentBinding`、`CompleteAgentRuntimeTarget`、provision/recovery/effect 均引用或包含 exact target。
binding snapshot 是历史事实；live catalog entry 是当前可用性。

## 3. Live Catalog Interface

Live catalog 应隐藏 materialization、Host verification、offer derivation 和进程 registry：

```rust
#[async_trait]
pub trait CompleteAgentLiveCatalog: Send + Sync {
    async fn attach(
        &self,
        contribution: CompleteAgentRegistrationContribution,
    ) -> Result<CompleteAgentLiveSelection, CompleteAgentLiveCatalogError>;

    async fn resolve(
        &self,
        attachment_id: &CompleteAgentLiveAttachmentId,
    ) -> Option<CompleteAgentLiveSelection>;

    async fn availability(
        &self,
        logical_instance_id: &AgentServiceInstanceId,
    ) -> CompleteAgentAvailability;
}
```

`CompleteAgentLiveSelection` 同时携带 exact durable target snapshot 与 live service handle；调用者不分别
查询 descriptor、offer、placement 或 verification。

Catalog 内部规则：

- 相同 incarnation、相同 attachment identity、相同 verified facts：返回同一 entry；
- 相同 attachment identity、不同 facts：typed conflict；
- 不同 incarnation：自然产生不同 attachment；
- attachment retired 后不再 resolve；
- optional materialization/describe failure 形成 availability diagnostic，不写 durable Host facts。

## 4. Selection and Provisioning Flow

### 4.1 Codex

启动时 materialize Codex contribution 并 attach 到 live catalog。成功后 catalog 有当前 Codex
attachment；失败后保存 process-local diagnostic，应用继续启动。

### 4.2 Native

`ProductionCompleteAgentServiceSelector::select_dash`：

1. 校验 Product profile；
2. 解析 Provider、Model 与 credential scope；
3. 构建 Native contribution；
4. 通过 live catalog attach/ensure；
5. 返回 exact live selection。

同一 Product profile 在同一 Host incarnation 中由 attachment key 幂等复用；跨重启重新
materialize。

### 4.3 Remote

Runtime Wire advertisement 通过 trust 验证后 attach。attachment ID 覆盖 transport connection
epoch。断连将 entry retire；重新连接必须产生新 attachment，不能复用旧 transport epoch。

### 4.4 Provision

Product provisioner 把 `CompleteAgentLiveSelection.target` 交给 Durable Runtime Host。Host：

1. 以 selection 的 effective offer 编译 surface；
2. 创建 generation；
3. 持久化 exact target、binding、callback/source/effect intent；
4. 通过 exact attachment ID 调用 live service；
5. settlement 继续遵守已有 effect inspect/reconcile 规则。

## 5. Dispatch Fencing

dispatch 的 admission 坐标为：

```text
runtime thread
+ binding id
+ binding generation
+ live attachment id
+ host incarnation
+ source coordinate
+ lease owner/token/epoch/expiry
```

任何坐标不匹配都 typed reject。Registry 解析失败表示 attachment 不在当前进程，不能回退到相同
logical instance key。

Driver callback/event 同样携带 attachment/incarnation fence；binding generation 不能单独替代
placement epoch。

## 6. Restart and Recovery

重启后：

1. Durable Runtime Host 加载 binding/effect/lease 等事实；
2. 新进程创建新 Host incarnation 与空 live catalog；
3. static/remote/dynamic adapter 按当前条件重新 attach；
4. 旧 binding 的 attachment 不在 catalog，因此不可 dispatch；
5. recovery planner 根据 Product profile 解析兼容的新 live selection；
6. 新 target 使用 `previous_generation + 1`；
7. 旧 callback route、lease 与 attachment 永久 fenced；
8. 未决 effect 使用原 identity 和 source evidence inspect/reconcile。

兼容性至少比较 definition、verified build/profile 与 required surface。没有兼容 attachment 时保持
Lost/Unavailable/InspectionRequired，不切换到其它 executor。

## 7. Persistence Model

### 7.1 Remove Global Live Inventory

新 migration 删除：

- `agent_service_instance`
- `agent_service_verification`
- `agent_runtime_offer`
- `agent_runtime_placement`
- `agent_runtime_remote_binding`

`CompleteAgentHostFacts` 同步移除：

- `service_instances`
- `service_verifications`
- `offers`
- `placements`
- `remote_bindings`

### 7.2 Retain Durable Execution Facts

保留并调整：

- runtime lifecycle target；
- lifecycle effect；
- binding；
- source coordinate；
- callback/revocation；
- dispatch effect/attempt；
- binding lease/epoch；
- target provisioning/recovery。

Runtime target 与 binding 保存 exact target snapshot。effect 以 binding/generation 为 owner，避免
重复保存“当前 service instance”第二事实。

### 7.3 Migration Strategy

项目未上线，新 migration 对错误模型做 hard cut：

1. 审计并删除 Product/Runtime 对旧 Host binding 的依赖行；
2. 按依赖顺序删除 Host-owned target/effect/binding/inventory 表；
3. 以新 target snapshot schema 重建 Host-owned durable 表；
4. 重置 Host repository revision/facts 到只包含 durable maps 的空图；
5. 保留 canonical Managed Runtime journal/projection 和 Product profile/provider 配置；
6. 验证现有数据库顺序升级与空库 migration 最终一致。

不修改 `0084`，不双写，不转换错误模型下的开发态 binding/effect。

## 8. Discovery

Execution profile discovery 不再读取 Durable Runtime Host repository：

- `PI_AGENT.known = true`
- `PI_AGENT.available = provider_catalog.any(executable)`
- `CODEX.known = true`
- `CODEX.available = live_catalog.current_availability(codex_instance)`
- unknown profile = typed reject

Project Agent 可以保存已知但当前 unavailable 的 profile；真正执行时由 selection 返回 typed
unavailable，避免把瞬时在线性变成持久配置校验。

## 9. Failure Policy

- Provider credential/health、Codex executable、remote connection 不可用：adapter unavailable，
  核心应用继续。
- 当前 attachment descriptor mismatch、verification claim drift、同 attachment identity 事实冲突：
  typed integrity error 并隔离该 attachment；不得写 durable binding。
- durable binding/effect graph 损坏：Host invariant，继续 fail-fast，因为执行正确性无法保证。
- live catalog 中没有旧 attachment：正常重启或断连状态，不是数据库 corruption。

## 10. Rejected Designs

- **允许覆盖 placement**：旧 generation 会命中新进程 handle。
- **持久化并复用 Host incarnation**：失去进程重启 fencing。
- **把 incarnation 拼进逻辑 instance ID**：仍缺少稳定恢复 key，并把 attachment 生命周期泄漏给
  Product profile。
- **启动清空全部 Runtime 数据**：破坏 effect/operation durability。
- **吞掉 duplicate invariant**：掩盖 stale binding 与 identity 冲突。
- **保留 DB inventory 只加 active 标志**：仍把可重建 live state 变成 durable authority，并增加
  心跳、过期和接管复杂度而没有业务价值。

## 11. Test Seams

测试通过深模块 interface 验证，不直接修改内部 map：

- Live catalog：同 incarnation 幂等、跨 incarnation 新 attachment、retire 后不可 resolve；
- Durable Host：binding exact target、dispatch full fence、旧 attachment 不可 fallback；
- Recovery：新 generation、新 attachment、旧 callback/event/lease fenced；
- Effect：restart inspect/reconcile、零重复 dispatch；
- PostgreSQL：hard cut migration、最终 schema、真实 repository graph；
- API composition：optional adapter unavailable 不终止启动，discovery 使用 live availability；
- 进程验收：同一数据根连续两次 `pnpm run dev:server`。
