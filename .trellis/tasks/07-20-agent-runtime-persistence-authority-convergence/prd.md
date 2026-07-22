# Agent Runtime 持久化职责与事实边界清理

## Goal

从第一性原理重新定义 Product、Agent Runtime、Complete Agent Host 与 concrete Complete
Agent 的职责和持久化边界，删除 Product、Runtime、Host 对 Agent 执行事实的重复维护，使一次
Agent 操作只存在一条从 Product 输入意图到 Complete Agent 权威执行结果的单向链路。

最终系统应满足：

- Product 只保存产品业务定义和目标关联；输入只作为当前请求交给 Agent，不形成 durable queue；
- Agent Runtime 是纯内存的事务协调、协议适配和实时流转机制；
- Complete Agent Host 是纯内存的 live attachment、selection、routing 与 fencing 机制；
- concrete Complete Agent 保存 source history、context、fork lineage、effect receipt 与
  `read/changes/inspect` 权威事实；
- UI 和 Application 从 Product shell 与实际 Agent 权威读取组合视图，任何可重建 projection
  都不能成为命令、列表或删除操作的正确性前置条件。

本任务不是继续修补 revision、digest、surface 或 projection drift，而是删除制造这些 drift 的
第二、第三事实源。

## Background

### 当前用户可见故障

同一架构链路已经产生一系列表面不同、根因一致的问题：

- optional Complete Agent 无法启动时导致核心应用退出，而不是静默缺席并报告不可用；
- 同一 service instance 重新注册时触发 `different verified facts`；
- Product/Runtime/Host 对 source binding、surface、generation、revision 的副本不一致，连续触发
  `projection drifted`、`binding mismatch`、`expected revision mismatch`；
- AgentRun list、workspace query 因派生 Runtime projection 过期而失败；
- 用户输入已经进入 Dash Agent history，但 UI 没有 live output，重连也读取不到会话内容；
- Dash 把真实执行错误压缩成通用 `execution failure`，无法定位 provider/Core 根因。

### 已确认的代码与数据库事实

- Product 持久化 command claim、launch/recovery/fork saga。
- Managed Runtime 再持久化 operation、idempotency、pending command、binding、
  normalized projection、change 与 outbox。
- Complete Agent Host 再持久化 target、binding、effect、lease、generation、callback route 与
  recovery history。
- Complete Agent/Dash 最终又持久化原生 command/effect/history，并提供 effect inspection。
- `CompleteAgentService::inspect(effect_id)` 已能恢复 Create、Command、Fork 与 SurfaceApply
  的 applied outcome，包括 source/child source coordinate。
- `CompleteAgentStateReconciler::synchronize_source` 在生产代码没有调用者；现有调用均为测试。
- Native Dash production composition 注入 `NoopDashExecutionCallbacks`，live delta 被直接丢弃。
- Runtime Product change worker 仍查询已从 Product binding 删除的 `source_binding`。
- 实际数据库中，失败输入已存在于 Dash history；对应 Runtime source projection 仍为空。
- 一个只有 4 个 operation 的 Runtime thread 已累计 148 条 change 与 148 条 outbox，且没有
  source change，证明 Runtime 主要持久化自身生命周期噪声而非不可重建业务事实。
- Dash 同时保存完整 repository JSONB 与 branch/history/command/effect/change 关系镜像，并在
  每次读取时逐项验证镜像相等。

详细证据记录在
[`research/current-architecture-audit.md`](./research/current-architecture-audit.md)。

### 既有架构决策的冲突

`07-17-agent-runtime-compaction-state-protocol-review` 同时规定：

- concrete Agent 是 native history/context/fork 的权威；
- Complete Agent 必须提供 authoritative `read/changes/inspect`；
- Product 拥有 restart-safe saga；
- Managed Runtime 拥有 durable operation、pending delivery、normalized conversation 与
  durable platform change tail。

这相当于同时选择“Runtime 是 durable workflow engine”和“Product + Agent 是 durable
端点”两种互斥模型。`a535ae016` 将生产组合硬切到这套未完成模型，并删除旧 session/journal/stream
路径，但没有证明 Complete Agent source 到前端的端到端 tracer bullet。

## First-Principles Persistence Rule

一份状态只有同时满足下列条件才允许持久化：

1. 所有相关进程内存丢失后，该事实仍必须成立；
2. 无法从其真实 owner 的 durable intent、`read`、`changes` 或 `inspect` 安全重建；
3. 不保存会造成不可接受的业务输入丢失、重复外部副作用或安全 fencing 失效；
4. 它拥有唯一领域 owner、独立生命周期和明确更新入口；
5. 它不是为了缓存、JOIN、展示、availability、diagnostic 或跨层一致性校验而复制的事实。

满足规则也不意味着需要拆成独立全局表。局部事实优先进入其归属 owner document；必要 scalar
只承担 owner lookup、唯一性或索引，不得用于重建第二份 canonical state。

## Requirements

### R1. 固定四层职责

- Product 拥有 AgentRun/LifecycleAgent、AgentFrame、workflow、Product lineage、执行配置意图，
  以及 AgentRun 到实际 Complete Agent source 的稳定关联。
- Agent Runtime 不拥有任何跨重启业务事实，只负责进程内协调、超时、取消、协议映射、实时
  normalize 与 broadcast。
- Complete Agent Host 不拥有跨重启 current inventory，只负责本次进程的 attach、describe、
  verify、selection、binding route、callback route 与 live fencing。
- concrete Complete Agent 独占 native source history/context/fork、command/effect receipt、
  applied surface evidence、snapshot/change 与 effect inspection。
- 物理共用 PostgreSQL 不改变领域 owner；Dash/Codex 原生事实不得进入 Product owner document。

### R2. Product 使用同步输入交接合同

- `target + client_command_id` 必须能确定性产生稳定 command/effect identity。
- Product 不承诺 Agent 离线时接受输入，也不提供后台补投递。
- 在 Complete Agent 返回 `Accepted`、`AlreadyApplied`、`Applied` 或 typed rejection 之前，
  输入只存在于当前 API handoff；只有 Agent 明确接收后 API 才能报告成功。
- Complete Agent 接收后，Product 不再维护对应 Runtime operation、Turn、Item、terminal 或
  availability 状态。
- Agent 不可用时返回 typed unavailable；调用者使用相同 `client_command_id` 重试，Agent 通过
  stable effect identity 保证幂等。
- Product 不保存 command claim、pending input、mailbox delivery、background retry 或交接
  receipt ledger。

### R3. Agent Runtime 回归纯内存

- 删除 `ManagedRuntimeStateRepository` 作为 durable authority 的生产职责。
- 删除 Runtime operation、idempotency、pending command、binding、source identity map、
  normalized source projection、change、outbox 与 surface snapshot 的持久化模型。
- Runtime command admission 直接基于 Product intent、当前 Complete Agent descriptor/snapshot
  与 command-specific coordinate，不基于 persisted aggregate revision。
- source/native identity 优先沿用 Agent identity；确需平台 identity 时使用确定性派生，或由
  concrete Agent adapter/source document 维护，不在 Runtime 建全局映射表。
- normalized snapshot 是按需 read model；live delta 是进程内 presentation。两者均可丢弃、
  重建，且不得参与业务写入 gate。

### R4. Complete Agent Host 回归纯内存

- 删除 Host revision graph 对 runtime target、binding、source、effect、lease、generation、
  provisioning/recovery history 的 durable ownership。
- stable Product intent 与 Complete Agent `inspect` 共同处理 post-dispatch unknown；Host 不保存
  第二份 effect ledger。
- service definition/build/profile 的稳定配置属于 Integration/configuration；本次连接的
  attachment、placement、incarnation、offer、verification result 与 availability 属于 live
  catalog。
- Host 重启后重新 attach/describe/verify/apply surface；旧 route/incarnation 默认未知并拒绝，
  不依赖 durable tombstone。
- optional Agent materialization 失败只使对应 Agent selection 不可用并保留诊断，不终止核心应用。

### R5. Reverse callback 由实际 effect owner 保证幂等

- callback route、deadline 与 generation fence 属于 Host memory。
- Tool/Hook 的真实副作用及可重放结果由 Tool Broker、Hook handler 或对应 Product effect owner
  保存；Host callback repository 不再作为通用结果账本。
- duplicate callback 使用 invocation 自带 idempotency key 向真实 handler 重放相同 receipt。
- Host 重启后旧 callback route 默认拒绝；Complete Agent 通过重新 apply surface 获得新 route。
- 如果某 handler 不能按 identity inspect/replay，其持久化缺口必须在 handler owner 修复，不能
  转嫁给通用 Runtime/Host ledger。

### R6. Product 删除 Agent 执行投影与跨层 gate

- Product owner document 只保存 Product identity/configuration/association，不复制 Runtime
  source/applied/activation、Host binding/generation 或 Agent history/effect。
- AgentRun list、workspace、delete 与普通 Product query 在 Agent 不可用时仍返回 Product shell；
  Agent presentation 作为 optional enrichment。
- Product command 不以 Runtime projection stale、availability revision、surface digest 对比作为
 通用 admission gate。
- AgentFrame history 与 Product binding 按 LifecycleAgent/AgentRun 归属收口为 owner-local
  JSONB；不得把 concrete Agent source document 合并进同一 Product 文档。
- 删除 Product command claim、mailbox/background delivery、交接 receipt ledger，以及只为恢复
  同一 Agent command 建立的 Product saga/global delivery 表。

### R7. UI 直接消费 Agent 权威读取

- conversation snapshot 由 Complete Agent `read` 经内存 adapter normalize 后返回。
- ordered durable Agent change tail 可直接映射；live/snapshot-only Agent 在 gap 或重连时重新
 读取 authoritative snapshot。
- provider/Core live delta 经真实 callbacks 进入 Runtime memory broadcast；不得注入 Noop sink。
- live partial delta 不冒充 durable history；终态/重连内容以 Complete Agent history 为准。
- projection 不可用或 Agent 离线时返回 typed availability，不返回 projection drift/conflict。

### R8. concrete Agent 存储按 owner document 收敛

- Dash source history、context、branch、command/effect state 继续属于 Dash/Complete Agent。
- 选择 `DashAgentRepositoryState` JSONB 为 source canonical document 时，删除从中机械拆出的
  branch/history/command/effect/change 镜像及逐次 drift verification。
- Create 前没有 source coordinate 的 effect receipt 可保留独立、Agent-owned、按 effect id
  查询的 ledger；它不能进入 Product、Runtime 或 Host。
- Dash 必须保留真实 `DashCoreError` 的可诊断语义，不能统一替换为 `execution failure`。
- external Agent（例如 Codex）继续以其原生 store 为 authority；平台不复制其 history。

### R9. Revision 只证明真实并发命题

- owner document revision 仅用于该 owner 内部 CAS。
- Agent native snapshot/change revision 仅用于 Agent 读取和 cursor。
- active turn、interaction、fork cutoff、surface apply 等并发要求使用对应 typed coordinate。
- 删除 Product expected revision 对 Runtime projection、Runtime revision 对 Host evidence、
  List/query 对 derived projection currentness 的跨层比较。
- 无关 availability、diagnostic 或 presentation 更新不能使已合法输入失效。

### R10. Schema 与 migration hard cut

- 使用实施时下一个可用 migration 序号，forward 删除已经应用到开发数据库的错误 schema；
  不修改既有 migration 历史。
- 最终 schema 不包含 Runtime、Host、Callback 的 durable owner document 或 normalized 镜像表。
- 删除 Runtime→Product change delivery worker、cursor/claim state 与相关 readiness。
- Product-local binding、Frame 按 owner document 收口；删除 command claim、pending input、
  mailbox delivery 与无独立生命周期的全局局域事实表。
- Dash 保留 Agent-owned canonical source/effect store，删除 canonical JSONB 的关系镜像。
- 项目未上线，不提供兼容 reader、fallback、dual write 或旧 schema 数据保留；migration 明确
  清理无法证明的新旧中间状态。

### R11. 用端到端纵切替代孤立模块证明

- 真实 production composition 必须覆盖：
  `Product input → stable effect → Complete Agent accept/execute → live delta → Agent history
  → reconnect read → UI conversation`。
- crash 回归必须覆盖 dispatch 前、Agent accepted 后、Create/Fork applied 后、callback applied
  后与流式连接中断窗口。
- 漏装 callback、read mapper、stream broadcaster 或 inspect recovery 时，composition test 必须
  失败。
- 单包 coordinator/repository 单测不能替代跨 Product/Runtime/Host/Agent/UI 的组合验收。

### R12. 规范只记录最终理由

- 更新 07-17 后续形成的 Runtime kernel、persistence、Host、AgentRun facade、Native adapter 与
  cross-layer stream specs，使其使用最终边界。
- 文档解释为什么 Product 输入采用同步 handoff、Runtime/Host 可重建、Agent 是执行 authority。
- 不记录旧表、旧补丁、fallback 或仅对本任务有意义的迁移流水。

### R13. Accepted Surface 保存可逆的语义证据

- `AgentFrame::CapabilityState`是平台能力篮子的完整事实；Product surface必须从同一事实同时生成
  model可见提示与机器可读manifest，使Agent输入和平台展示不会形成两套能力定义。
- Dash只接纳执行所需的instruction/tool surface；Native Adapter根据accepted structured evidence
  生成ContextFrame，避免Dash依赖平台展示概念，也避免Adapter解析自然语言提示词。
- 工具定义owner声明其protocol projector；该projector随accepted ToolCall进入native history，使
  历史消息的协议类型不受当前surface生命周期影响。
- accepted `DashSurface.tools`同时派生provider `tools[]`机器契约与Dash system prompt中的可读参数
  摘要；两条投影共享名称、description、schema和必填语义，不保存第二份工具说明事实。
- callback工具结果以typed content parts与structured details进入Dash native history；provider
  transcript和AgentDash专属Card都从该结果派生，使实时、重载与不同展示协议保持同一正文。
- protocol projector只决定AgentDash ThreadItem展示family。参数展示字段不完整时仍由工具owner执行并
  返回typed validation result，展示投影不承担工具准入职责。
- VFS skill发现只有在扫描完成时才可提交空结果；provider不可用属于诊断失败，不能被物化为
  “没有项目skill”的能力事实。

## Acceptance Criteria

### Ownership and schema

- [x] `agent_runtime_state_revision`、`agent_runtime_host_revision`、
  `agent_runtime_callback_revision` 及其生产 repository 不再存在。
- [x] Runtime operation/pending/projection/change/outbox、Host binding/effect/lease/callback 的
  normalized 表和 JSONB authority 均从最终 schema 删除。
- [x] Product schema 不保存 Runtime source/applied/activation、Host generation、Agent history、
  Agent effect 或 derived availability。
- [x] Product schema 不包含 command claim、pending input/outbox、mailbox delivery 或后台重试状态。
- [x] Product binding 与 AgentFrame 是严格 Product owner-local document；Dash/Codex source
  history 仍属于 concrete Agent。
- [x] Dash 不再同时维护完整 repository JSONB 与 branch/history/command/effect/change 镜像。

### Command and recovery

- [x] 相同 `target + client_command_id` 在进程重启后产生相同 effect identity。
- [x] dispatch 前崩溃不会伪造 Agent receipt。
- [x] Agent accepted/applied 后回包丢失时，通过 `inspect` 收敛且不重复副作用。
- [x] Create/Fork 回包丢失后，可从 applied inspection 恢复 source/child source 并完成 Product
  association。
- [x] `Unknown` inspection 保持 typed pending/unavailable，不自动重派或冒充失败。
- [x] Product command 不因无关 projection revision、availability revision 或 derived surface
  digest 改变而拒绝。

### Read and stream

- [x] AgentRun list/workspace/delete 在 Agent 离线、source read 失败或 presentation 缺失时仍可用。
- [x] 用户输入后前端在同一会话获得真实 live delta；生产 composition 不包含 Noop execution
  callback。
- [x] live stream 断开后重连从 Complete Agent authoritative read 恢复同一 conversation。
- [x] snapshot-only Agent 不依赖平台 durable change tail 也能完成重连。
- [x] Agent/Core/provider 的真实失败原因进入诊断和 Agent terminal history，不再统一为
  `execution failure`。

### Host and callback

- [x] 连续启动不会因同一 logical service 的新 attachment/incarnation 触发 durable invariant。
- [x] optional Agent program/credential/materialization 不可用只产生 typed unavailable diagnostic，
  核心应用继续启动。
- [x] Host 重启后旧 route/generation/callback 默认拒绝，新 surface apply 建立新 live route。
- [x] duplicate Tool/Hook callback 从真实 handler owner 按 idempotency key 重放同一结果。

### Migration and verification

- [x] 当前已执行 0094 的开发数据库可通过 forward migration 启动到最终 schema。
- [x] 空库完整 migration 与既有开发库顺序升级得到相同最终 schema/readiness。
- [x] migration 不引入兼容 reader、dual write 或 fallback。
- [x] 负向搜索证明删除的表名、repository、worker、drift/stale gate 与 Noop stream sink 不再进入
  production composition。
- [x] 完整 production tracer bullet、crash matrix、定向 Rust tests、前后端契约生成/typecheck、
  migration guard 和 `git diff --check` 通过。

### Capability presentation

- [x] accepted capability manifest完整表达tool path、MCP、VFS mount、skill、memory、companion、
  channel与workspace module，并从同一native history投影为ContextFrame。
- [x] live/durable工具消息使用工具owner声明的projector，surface撤销后历史专属Card仍可恢复。
- [x] accepted工具定义的完整description/schema进入provider机器契约，并从同一列表生成模型可读参数
  摘要；ContextFrame只承担平台展示投影。
- [x] Core、Dash history与Native Adapter保存typed tool content/details；`fs_read`实时与重载历史均恢复
  AgentDash Read Card的路径、行号和正文，不展示callback envelope JSON。
- [x] 同一turn的surface callback替换只影响后续调用，不改变已经admit的调用route。
- [x] skill扫描失败不会提交权威空inventory。
- [x] 当前开发数据库迁移与真实Draft Canvas/skill/capability/Card tracer通过。

## Out of Scope

- 不把 Dash/Codex history、effect 或 Complete Agent metadata 搬进 LifecycleAgent/AgentRun JSONB。
- 不把 PostgreSQL 替换为其他数据库；本任务只清理事实 owner 与文档边界。
- 不为旧 API/schema 提供兼容层或回退路径。
- 不因为“表少”删除具有独立 Product 生命周期的 workflow、terminal、VFS 或权限事实；它们按同一
  Persistence Rule 单独评估。
- 不要求所有 Complete Agent 提供 durable ordered change tail；authoritative snapshot 是最低
  恢复合同。
- 不改变具体 LLM/provider 能力，除非当前适配器为通过错误 Runtime/Host projection 而伪造能力。
