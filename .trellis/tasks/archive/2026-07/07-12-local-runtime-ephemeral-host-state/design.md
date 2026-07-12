# Local Runtime无数据库Host状态重建设计

## 1. Core Decision

恢复使用同一个canonical `RuntimeThreadId`，只替换Host binding epoch。

```text
AgentRun target
  -> stable RuntimeThread anchor
       -> event/context/operation lineage (unchanged)
       -> current binding coordinates (Managed Runtime projection)
            epoch 1 / old incarnation / Lost
            epoch 2 / new incarnation / Active
```

不采用replacement RuntimeThread。`EventSequence`是per-thread cursor；换thread会迫使产品层发明复合cursor，并破坏旧`BindingLost`、mailbox operation identity、context和guard连续性。同thread rebind只需新增明确journal transition，成本和事实模型都更小。

### Repository evidence

- `crates/agentdash-infrastructure/migrations/0065_agent_runtime_cutover.sql:19-28`把target、thread和Host binding压在单行主键中。
- `crates/agentdash-application-agentrun/src/agent_run/runtime_facade.rs:588`把产品event cursor直接用于当前单一thread subscription。
- `crates/agentdash-agent-runtime/src/model.rs:62-69`已把current binding作为RuntimeThread projection字段；`crates/agentdash-infrastructure/src/persistence/postgres/runtime_repository.rs:1120`也已在projection CAS中更新这些坐标。
- `crates/agentdash-infrastructure/migrations/0064_agent_runtime_driver_host.sql:71-73`与`0061_agent_runtime_managed_state.sql:35-40`的thread unique约束是same-thread多binding epoch需要调整的实际schema blocker。
- `crates/agentdash-agent-runtime-contract/src/driver.rs:43-47`已有typed Driver `Resume` intent，缺口位于产品恢复编排与Managed Runtime rebind transition，而不是Driver协议从零设计。

## 2. Ownership

- Local Host：当前进程incarnation内的definitions、instances、offers、bindings、leases与coordinates。
- Cloud Host：远端proxy instances/offers和每个binding epoch的Host事实。
- Managed Runtime：RuntimeThread journal/projection、当前binding coordinates、operation/outbox、context与late-event quarantine。
- AgentRun composition：稳定target/thread anchor、binding lineage、recovery intent和按需恢复编排。

Local process只从文本profile、credential refs和builtin contributions重建，不保存恢复权威。

## 3. Persistent Model

追加`0068_agent_runtime_binding_recovery.sql`，不修改任何已应用migration。

### 3.1 Product anchor

`agent_run_runtime_thread_anchor`

- `run_id + agent_id`主键。
- `runtime_thread_id`全局唯一且生命周期内不可变。
- `bootstrap_runtime_binding_id`指向epoch 1，供首个`ThreadStart`提交前解析。
- Thread存在后，current binding由`agent_runtime_thread.binding_id`唯一决定。

### 3.2 Binding lineage

`agent_run_runtime_binding_lineage`

- 主键：`run_id + agent_id + binding_epoch`。
- `runtime_binding_id`全局唯一并引用Host binding。
- 保存每个epoch的完整`AgentRunRuntimeBinding`materialization、recovery intent id和创建时间。
- 只append，不更新旧epoch。`list_by_run/list_by_agent`保持“每个target当前binding”语义；新增lineage读取仅供恢复、审计与测试。

当前binding解析：

```text
if agent_runtime_thread exists:
    join thread.binding_id -> binding_lineage
else:
    anchor.bootstrap_runtime_binding_id -> binding_lineage
```

因此不需要第二个current-head CAS，也不会发生Runtime projection已切换、产品head仍指旧binding的双真相。

### 3.3 Recovery intent

`agent_run_runtime_recovery_intent`

- identity覆盖target/thread、expected old binding/generation/revision、next epoch、proposed binding、selected offer与source thread。
- 状态：`prepared -> host_bound -> committed`，任一步可进入`failed`。
- 每个target只允许一个非终态intent；重复请求返回同一intent。
- proposed binding在Host side effect前尚不存在，因此intent中的proposed id不设提前FK；进入`host_bound`后通过lineage/Host FK建立完整约束。

## 4. Host Binding Epochs

Cloud Host允许同一RuntimeThread拥有多个历史binding，但最多一个pending/active/desynchronized binding：

- 移除`agent_runtime_host_binding.thread_id`的全局unique。
- 增加对非终态binding的partial unique index。
- `agent_runtime_source_coordinate`允许同一RuntimeThread在不同binding epoch上保留source coordinate；每个binding仍只能有一个canonical thread/source pair。
- recovery开始前将旧Host binding按expected generation幂等标记Lost。

新binding id由target与`binding_epoch`确定，避免随机重试产生多个identity。Host `bind()`继续以binding id + offer + surface + intent幂等。

`DriverBindIntent::Resume`返回的`source_thread_id`必须等于intent提供的old canonical source id；不同值是Driver contract violation并使新binding Failed。

## 5. Managed Runtime Rebind Contract

新增明确的`RuntimeCommand::ThreadRebind`，而不是扩展当前同binding的`ThreadResume`：

- 输入包含thread、recovery intent、expected old binding/generation、新binding/generation/source、profile digest与new effective profile。
- admission要求当前status为Lost、无active turn/pending interaction、old coordinates完全匹配、new profile保证`ThreadResume`与既有surface/hook边界。
- Runtime repository通过现有projection revision CAS和Host binding/source复合FK，在一个`RuntimeCommit`中：
  - append operation acceptance；
  - append`BindingReestablished { old, new, epoch }`；
  - 更新projection的binding/generation/source/profile/status；
  - 保留context、transcript、event/operation sequence、settings/tool/hook revisions。

现有`ThreadResume`只处理同binding suspended thread；不得用于Lost后的换binding恢复。

Runtime rebind提交后再把intent标记committed。若进程在两者之间退出，reconciler看到thread current binding等于intent new binding即可幂等补写committed。

## 6. Recovery Orchestration

恢复由下一条`send_message`或mailbox drain触发：

1. load stable anchor/current binding和Runtime snapshot。
2. Active直接投递；Lost进入recovery coordinator；Closed不可恢复。
3. 读取旧Host binding/offer/instance，固定原service definition与placement owner。
4. 选择新available offer：不同旧binding epoch，匹配原owner，保证Resume并满足旧materialized surface/hook。
5. 创建或加载durable recovery intent。
6. 标记旧Host binding Lost，复制旧surface到new binding identity。
7. Host以新offer和`Resume(old_source_thread_id)`创建新binding。
8. append binding lineage epoch。
9. 执行Managed Runtime `ThreadRebind`。
10. intent收敛committed，然后同一用户请求执行新的`TurnStart`。

inventory尚未就绪时返回retryable unavailable；mailbox保持queued并按现有drain机制重试。Backend注册/offer sync不主动恢复所有历史AgentRun。

## 7. Relay Lifecycle

- `HostIncarnationId`属于offer provenance、placement request与stream identity。
- Relay一旦调用disconnect并发出exactly-once `BindingLost`，旧placement route终态；Backend重新注册不reopen旧route。
- 新Host binding通过新offer解析新placement。相同Local process的网络重连也使用新Cloud binding generation；Local process重启则同时使用新Host incarnation。
- 迟到旧frame在Local incarnation admission或Managed Runtime old binding/generation admission处拒绝。

## 8. Outbox, Mailbox And Guards

- Runtime outbox entry继续携带generation；worker同时检查current thread generation与operation terminal。旧generation且operation已Lost的work直接ack，不能在rebind后的Active thread上循环release。
- accepted mailbox message保持原operation id和Lost terminal；同client id replay返回原receipt。queued message尚无operation，可在recovery后创建新Turn operation。
- command guard继续使用thread id + Runtime revision + active turn。Rebind推进revision，所以恢复前guard自然stale；无需binding epoch进入前端协议。
- event subscription仍是单thread `EventSequence`，无需复合cursor或跨thread merge。

## 9. Ephemeral Local Bootstrap

```text
Local process start
  -> load machine/profile/workspace/MCP text facts
  -> collect trusted Integration definitions
  -> create HostIncarnationId
  -> rebuild instances/offers in EphemeralAgentRuntimeHostRepository
  -> connect relay and advertise current-incarnation offers
```

`agentdash-local`不构造PostgreSQL pool、不执行Dashboard migrations、不读取旧Local DB目录。

## 10. Diagnostics

- Local：backend id、host incarnation、service instance、offer generation、advertised result。
- Cloud inventory：source incarnation/generation、proxy instance、activation/withdraw result。
- Recovery：recovery id、target、epoch、old/new binding/generation、stage、result/reason。
- AgentRun inspect：current epoch与`active | lost | recovering | recovery_failed`摘要。

所有日志排除credential、business input与secret-bearing config。

## 11. Failure Matrix

| Failure point | Canonical retry behavior |
| --- | --- |
| intent prepared前 | 没有side effect，下次重新选择 |
| prepared后、Host bind前 | 复用同intent/new binding id |
| Driver bind成功、host_bound写入前 | Host bind按identity幂等返回并补写host_bound |
| host_bound后、lineage前 | 从Host binding补写lineage |
| lineage后、Runtime rebind前 | 重放同一ThreadRebind operation |
| Runtime rebind后、intent committed前 | 根据thread current binding补写committed |
| 新offer消失或Resume拒绝 | intent Failed，thread保持Lost，不fallback |
| 并发恢复 | partial unique + expected old coordinates只允许一个winner |

## 12. Rejected Alternative

不使用replacement RuntimeThread + binding head + composite cursor。该方案要求重写AgentRun event API、mailbox lineage、context读取和所有guarded command，却没有提供额外业务价值；同thread rebind已经满足旧事实不可变、current coordinates可演进和cursor连续性。
