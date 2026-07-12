# Local Runtime无数据库Host状态重建

## Goal

让正式Tauri Desktop Local Runtime与Standalone Runner成为无数据库的本机执行包，并在Local Host重启或Relay已宣告断连后，由云端canonical Runtime以同一RuntimeThread、全新Host binding epoch和Driver `Resume`恢复后续执行；产品事件游标、上下文、邮箱幂等与审计链不能因恢复而换线或重置。

## Background

- 当前分支已完成`HostIncarnationId`传递、旧incarnation admission拒绝、正式ephemeral Host repository、Local PostgreSQL启动链移除，以及inventory identity/诊断补强。
- 独立check确认旧命令隔离成立，但现有Backend重注册仍尝试reopen携带旧incarnation的placement；新Local Host会正确拒绝，因此AgentRun只会保持`Lost`，没有canonical rebind/resume闭环。
- `agent_run_runtime_binding`当前把产品target、RuntimeThread和Host binding压成单行immutable anchor；直接替换RuntimeThread会破坏per-thread `EventSequence`、旧`BindingLost`审计、mailbox operation replay和guarded command语义。
- Managed Runtime的事件序列、context、operation与transcript都以RuntimeThread为lineage。RuntimeThread本身可以通过新journal event更新当前Host binding坐标，无需创建replacement thread。
- Local Host持久状态不是业务事实：definitions、profile、credential refs与云端source coordinate足以让新incarnation重建offer并执行Driver `Resume`。

## Requirements

### R1. Local Host状态归属

- Local Host的service instance、offer、binding、lease与coordinate均为单个Host incarnation内的执行状态。
- Project、AgentRun、RuntimeThread、binding recovery intent、source thread与恢复裁决由云端Managed Runtime和AgentRun facade拥有。
- 本机workspace、MCP、machine identity、profile与credential reference继续读取现有文本或系统事实源，不新增第二持久化格式。

### R2. Ephemeral Host Repository

- 提供正式production-grade ephemeral `AgentRuntimeHostRepository`，复用完整repository invariants，不以测试`Fixture`命名或依赖测试支持层。
- `agentdash-local`启动时从Integration definitions/profile重建service instances与offers；进程退出后pending/active binding、lease和coordinate自然失效。
- Tauri embedded runner、Web dev local runtime与Standalone Runner共用同一无数据库bootstrap。

### R3. Host Incarnation与旧命令隔离

- 每次Local Host进程启动建立新的不可复用`HostIncarnationId`。
- offer advertisement、Runtime Wire provenance/stream identity、binding admission与Driver endpoint resolution携带并校验incarnation。
- 旧connection、incarnation、generation、binding command、lease和迟到event必须在Driver side effect或Runtime projection推进前被拒绝或quarantine。
- Relay一旦把断连宣告为placement loss，旧route即终态，不在Backend注册时reopen；新执行只能由新binding创建新placement。

### R4. Canonical Same-Thread Rebind

- `BindingLost`继续把旧Host binding、active Turn和active Operation收敛为Lost；旧事实不可删除、复活或改写。
- AgentRun的`run_id + agent_id -> RuntimeThreadId`保持稳定。恢复不得创建replacement RuntimeThread，不改变AgentRun事件cursor域，不丢失旧`BindingLost`或transcript/context历史。
- 为同一RuntimeThread创建单调`binding_epoch`的新Host binding，使用旧canonical `source_thread_id`与`DriverBindIntent::Resume`；Driver返回的source thread必须与resume intent完全一致。
- Managed Runtime通过显式`ThreadRebind` command/event，以CAS校验旧binding/generation/revision后原子更新当前binding/generation/source/profile并回到Active。
- 只有新offer保证`ThreadResume`、满足旧materialized surface/hook要求且属于原service definition与placement owner时允许恢复；不切换Backend、不降级能力、不fallback为`Start`。
- 恢复在下一条产品命令或mailbox drain时按需触发。Backend上线只同步offers，不批量唤醒历史AgentRun；无可用新offer时保持可诊断、可重试的Lost。

### R5. Binding Lineage与Recovery Intent

- 产品持久化拆为稳定RuntimeThread anchor、append-only binding lineage和durable recovery intent；当前Host binding以Managed Runtime projection为canonical head，不维护第二个可漂移current指针。
- 初始binding为epoch 1；每次恢复分配`epoch + 1`和新的binding identity。旧Host binding必须先标记Lost，同一thread最多存在一个pending/active binding。
- Recovery intent在Driver side effect前持久化，至少记录target/thread、旧binding/generation/revision、新epoch/binding/offer、source thread和状态`prepared | host_bound | committed | failed`。
- prepared、Host bind、lineage insert、Runtime rebind之间允许进程崩溃；重试必须从intent和Runtime projection幂等收敛，不能产生双binding或重复Driver bind。
- 初始ThreadStart前由anchor的bootstrap binding解析当前binding；Thread建立后，当前binding必须由`agent_runtime_thread.binding_id`关联lineage得到。

### R6. Outbox、Mailbox与Guard连续性

- AgentRun event API继续使用同一RuntimeThread的`EventSequence`，恢复前后的事件严格连续；不引入复合cursor。
- 已accepted且因断连Lost的operation保持Lost；同一`client_command_id`重放不得变成新operation。尚未accepted的mailbox message可在恢复成功后正常dispatch。
- 旧generation outbox在rebind后按operation terminal/generation事实完成ack或隔离，不能在Active新binding上无限重试。
- guarded command继续校验稳定thread与Runtime revision；`ThreadRebind`推进revision，因此恢复前取得的guard必然stale。

### R7. 移除Local PostgreSQL启动链

- `agentdash-local`不再启动embedded PostgreSQL或执行Dashboard migrations，也不持有PostgreSQL runtime handle/pool。
- Local Runtime启动、credential claim、relay registration与Driver Host availability不依赖Dashboard schema。
- 既有Local DB目录不读取、不迁移、不自动删除；仅云端Dashboard/Managed Runtime继续使用PostgreSQL。

### R8. 可诊断恢复

- Local bootstrap、offer advertisement、cloud inventory、recovery intent、Host bind和Runtime rebind记录结构化diagnostics：incarnation、offer generation、binding epoch、old/new binding、result/reason。
- AgentRun inspect暴露`active | lost | recovering | recovery_failed`及当前binding epoch/恢复诊断；不得记录credential、业务输入或secret-bearing config。

## Acceptance Criteria

- [ ] Tauri Desktop Local Runtime与Standalone Runner启动时不创建PostgreSQL进程、数据目录或`_sqlx_migrations`，旧Local DB目录存在或缺失均不影响启动。
- [ ] Local definitions/instances/offers可从空内存状态重建，Backend online与首次AgentRun成功。
- [ ] Local Host重启产生新incarnation；旧command/lease/event在Driver side effect或Runtime projection前被拒绝。
- [ ] 断连只产生一次`BindingLost`；旧Host binding/operation/turn保持Lost，新binding使用更高epoch和`DriverBindIntent::Resume`。
- [ ] 恢复前后使用同一RuntimeThread；从恢复前cursor读取可依次看到`BindingLost`、`BindingReestablished`与新Turn事件，无gap、重置或旧审计丢失。
- [ ] recovery intent在prepared、host_bound、lineage written、Runtime committed四个崩溃点均可幂等收敛，且并发恢复只产生一个新binding。
- [ ] 新offer缺少Resume能力、surface/hook不匹配、placement owner变化或inventory未就绪时保持typed Lost/retryable，不执行`Start`或跨Backend fallback。
- [ ] 旧generation outbox不会在新binding上重复dispatch；accepted Lost mailbox operation不漂移，queued message可在恢复后dispatch。
- [ ] 定向测试覆盖Host、Managed Runtime、AgentRun facade/repository、Runtime Wire与relay；真实`pnpm dev`、`pnpm dev:desktop`和Standalone Runner前台/service路径验证无数据库启动、重启恢复和后续Turn成功。

## Out of Scope

- 云端Dashboard PostgreSQL与Managed Runtime durable store不移除。
- 不恢复断连时尚未由Driver确认的active Turn；该Turn保持Lost，恢复只允许后续新Turn。
- 不做跨Backend failover、Driver不支持Resume时的Start降级、旧Local PostgreSQL兼容读取/数据迁移或双写。
- Runner自动更新与Desktop API sidecar migration继续由各自发布任务处理。
