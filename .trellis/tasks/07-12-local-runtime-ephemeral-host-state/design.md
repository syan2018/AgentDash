# Local Runtime无数据库Host状态重建设计

## 1. Ownership

云端Managed Runtime拥有Thread、Turn、binding intent、outbox与恢复裁决；Local Driver Host只拥有当前进程incarnation内的Integration执行状态。文本profile与builtin contributions足以重建service instances和offers，旧Host进程的binding/lease/coordinate没有跨进程业务权威。

## 2. Ephemeral Repository

在`agentdash-agent-runtime-host`提供正式内存repository。实现与测试fixture共享同一状态机与invariants，但production类型使用明确的ephemeral语义。Local bootstrap直接注入该repository，不再构造PostgreSQL pool。

```text
Local process start
  -> load machine/profile/workspace/MCP text facts
  -> collect Integration definitions + trust manifests
  -> create HostIncarnationId
  -> rebuild service instances and activate offers in memory
  -> connect relay and advertise current-incarnation offers
```

## 3. Incarnation Fencing

新增或明确Host incarnation坐标，使offer、binding与dispatch均绑定当前连接生命周期。incarnation必须由新进程生成且不可复用；不能用从1重新计数的generation单独证明跨重启新旧关系。

云端记录offer的incarnation，Backend断连时将对应Host binding收敛Lost。重连后只接受新incarnation offer，并以Managed Runtime持有的source thread/binding intent执行新bind或resume。旧incarnation command即使迟到，也在进入Driver side effect前拒绝。

## 4. Restart Semantics

- service instance/offer：从definition、profile与credential refs重建。
- pending binding：随旧进程消失；云端outbox在新offer上重新admit。
- active binding：断连后Lost；需要新Driver bind/resume。
- lease/coordinate：只在当前incarnation内有效，重启清空。
- Driver原生source thread：由云端保存的source coordinate或Integration resume contract重新提供，不依赖Local Host DB。

## 5. Dependency Removal

从`agentdash-local::build_ws_config`删除embedded PostgreSQL和全局migration；`ws_client::Config`不再持有session DB runtime。清理`postgresql_embedded`及仅由该路径引入的Postgres repository依赖。已有Local DB目录作为未引用的开发残留，不自动删除。

## 6. Validation

- Repository conformance：ephemeral实现通过现有Host repository行为测试。
- Incarnation：同一binding/generation在旧incarnation可用、新incarnation拒绝。
- Relay：断线令旧binding Lost，新offer重新bind/resume。
- Process：启动Tauri/Runner时无postgres进程和数据目录写入。
- Product：真实Desktop与Standalone Backend online，AgentRun首轮及恢复轮次成功。

## 7. Risk Boundary

最大风险是当前协议只用generation fencing，跨进程从相同generation重新开始可能误接纳旧command。实现必须先完成incarnation contract，再移除持久化generation；不能仅把Postgres repository替换成现有测试fixture后结束。
