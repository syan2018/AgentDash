# ChannelService 完整通信主干执行计划

## Current State

本任务是完整端到端实现任务，当前状态为 `planning`。进入实现前必须完成：

- `prd.md`、`design.md`、`implement.md` 已收敛。
- `implement.jsonl`、`check.jsonl` 已有真实上下文条目。
- `work-items/` 已初始化，每个工作项有独立追踪文件。
- 用户确认进入实现后运行 `python ./.trellis/scripts/task.py start .trellis/tasks/07-07-channel-communication-capability-model`。

## Dispatch Model

主会话是 dispatcher，负责拆分、派发、等待、合并、更新工作项状态和提交规划。实现 worker 和检查 worker 只处理被派发的工作项。

Trellis 标准链路：

```text
Phase 1 Plan
  -> validate task context
  -> start task
Phase 2 Execute
  -> implement worker per work item or dependency batch
  -> targeted checks after each work item
  -> check worker after each batch
  -> integration checks after all batches
Phase 3 Finish
  -> trellis-update-spec
  -> batched commit plan
  -> /trellis:finish-work
```

Dispatcher 每次派发都从当前任务路径开始：

```text
Active task: .trellis/tasks/07-07-channel-communication-capability-model
Work item: .trellis/tasks/07-07-channel-communication-capability-model/work-items/WI-XX-*.md
```

## Work Item Tracking

工作项索引在 `work-items/README.md`。每个 `WI-*.md` 维护自己的状态：

```text
planned -> dispatched -> implementing -> checking -> ready_for_integration -> done
blocked 可从任意状态进入；解除后回到 dispatched 或 checking。
```

Dispatcher 更新规则：

- 派发前：在 `work-items/README.md` 和对应 `WI-*.md` 标记 `dispatched`，写入 worker/channel 名。
- worker 完成后：记录摘要、修改文件、已跑命令、遗留风险。
- targeted check 通过后：标记 `ready_for_integration`。
- 全量集成检查通过后：标记 `done`。
- 如果实现发现设计缺口：回到 planning，更新 `prd.md` / `design.md` / 当前 `WI-*.md`，再继续。

## Work Items

| ID | 工作项 | 依赖 | 可并行 |
| --- | --- | --- | --- |
| WI-01 | Domain Channel Document Model | 无 | WI-02 调研、WI-09 合同草拟 |
| WI-02 | Owner Document Mutation Contract | WI-01 mutation 类型草案 | WI-04、WI-09 |
| WI-03 | LifecycleRun Registry Persistence | WI-01、WI-02 | WI-06 |
| WI-04 | ChannelOwnerStore And BindingResolver | WI-01 | WI-02、WI-09 |
| WI-05 | ChannelService Core | WI-01、WI-04 | WI-06 前半、WI-07 前置 mapper |
| WI-06 | CapabilityState.channel Projection | WI-01、WI-04 | WI-03、WI-05 部分 |
| WI-07 | Mailbox/Gate Materialization | WI-01、WI-05 | WI-06 |
| WI-08 | Companion/SubAgent/Human/Async Wake Convergence | WI-05、WI-07 | 无，集成项 |
| WI-09 | Provider-neutral IM Contract | WI-01、WI-04 | WI-02、WI-06 |
| WI-10 | Integration, Static Checks, Cleanup | WI-01 至 WI-09 | 无，收束项 |

## Parallel Dispatch Plan

### Batch A: Foundation

1. 派发 WI-01。
2. WI-01 完成后运行 domain targeted checks。
3. Check worker 审 WI-01 的 domain API、serde default、validation 和 prune 规则。

### Batch B: Parallel Contracts

WI-01 的公共类型稳定后并行：

- WI-02: owner document mutation helper / repository contract。
- WI-04: `ChannelOwnerStore` / `ChannelBindingResolver` ports。
- WI-09: provider-neutral IM envelope / unresolved binding 合同。
- WI-06 前半: `CapabilityState.channel` 类型与 default 空态。

Batch B 结束后跑一次 compile gate，确认 shared API 没分叉。

### Batch C: Persistence And Service

- WI-03 依赖 WI-02，落 `LifecycleRun.channel_registry` migration、mapping、mutation tests。
- WI-05 依赖 WI-04，落 `ChannelService` core 和 delivery planning。
- WI-06 后半接 projection / Accumulate replay。

Batch C 后派 check worker 做 cross-layer review：domain -> repository -> application -> capability。

### Batch D: Materialization And Old Path Convergence

- WI-07 接 mailbox/gate materializer 和 `ChannelAddress` mapper。
- WI-08 清 Companion/SubAgent/human response/terminal wake 旧直接投递路径。

Batch D 后跑 integration tests 和静态扫描，确认旧路径只剩 materializer/resolver 边界。

### Batch E: Final Integration

WI-10 做全量检查、死代码清理、文档同步和提交准备。

## Dispatch Commands

创建一个任务级 channel：

```powershell
$TASK=".trellis/tasks/07-07-channel-communication-capability-model"
trellis channel create channel-service-dispatch --task "$TASK" --by dispatcher --cwd "$PWD"
```

每次发送给 worker 的 brief 必须包含 active task、work item 和角色自豁免：

```powershell
$WI="$TASK/work-items/WI-01-domain-document-model.md"
$brief = @"
Active task: $TASK
Work item: $WI

You are already the trellis-implement worker for this work item.
Implement this work item directly. Do not spawn trellis-implement or trellis-check.
Read the injected prd/design/implement files, then read the work item file.
Before finishing, report changed files, commands run, remaining risks, and the next status for the work item tracker.
"@
```

派发 implement worker：

```powershell
trellis channel spawn channel-service-dispatch `
  --agent implement `
  --provider codex `
  --as wi01-impl `
  --file "$TASK/prd.md" `
  --file "$TASK/design.md" `
  --file "$TASK/implement.md" `
  --file "$TASK/work-items/WI-01-domain-document-model.md" `
  --jsonl "$TASK/implement.jsonl" `
  --cwd "$PWD" `
  --timeout 90m

$brief | trellis channel send channel-service-dispatch `
  --as dispatcher `
  --to wi01-impl `
  --delivery-mode requireRunningWorker `
  --stdin

trellis channel wait channel-service-dispatch `
  --as dispatcher `
  --from wi01-impl `
  --kind done,error `
  --timeout 90m
```

派发 check worker：

```powershell
trellis channel spawn channel-service-dispatch `
  --agent check `
  --provider codex `
  --as wi01-check `
  --file "$TASK/prd.md" `
  --file "$TASK/design.md" `
  --file "$TASK/implement.md" `
  --file "$TASK/work-items/WI-01-domain-document-model.md" `
  --jsonl "$TASK/check.jsonl" `
  --cwd "$PWD" `
  --timeout 45m

$checkBrief = @"
Active task: $TASK
Work item: $WI

You are already the trellis-check worker for this work item.
Review and fix this work item directly. Do not spawn trellis-implement or trellis-check.
Check against prd/design/implement, the work item file, and check.jsonl context.
Before finishing, report findings, fixes, commands run, residual risk, and whether the work item can move to ready_for_integration.
"@

$checkBrief | trellis channel send channel-service-dispatch `
  --as dispatcher `
  --to wi01-check `
  --delivery-mode requireRunningWorker `
  --stdin

trellis channel wait channel-service-dispatch `
  --as dispatcher `
  --from wi01-check `
  --kind done,error `
  --timeout 45m
```

并行等待多个 worker 时使用 `--all`：

```powershell
trellis channel wait channel-service-dispatch `
  --as dispatcher `
  --from wi02-impl,wi04-impl,wi09-impl `
  --kind done,error `
  --all `
  --timeout 90m
```

## Interleaved Checks

每个工作项有 targeted checks；每个 batch 有 integration gate；最后 WI-10 做 full-scope check。

| 时机 | 检查 |
| --- | --- |
| WI-01 | `cargo test -p agentdash-domain channel`；`cargo check -p agentdash-domain` |
| WI-02 | owner document helper unit tests；`cargo check -p agentdash-infrastructure` |
| WI-03 | LifecycleRun repository roundtrip / mutation tests；`pnpm run migration:guard` |
| WI-04 | application port/service tests for owner store and unresolved binding |
| WI-05 | ChannelService unit tests；static scan for owner global scan |
| WI-06 | capability default / Accumulate replay tests；`cargo check -p agentdash-spi -p agentdash-application-agentrun` |
| WI-07 | mailbox/gate materializer tests；address-to-mailbox mapper tests |
| WI-08 | companion/subagent/human/terminal integration tests；old direct delivery static scan |
| WI-09 | provider-neutral envelope tests；binding unresolved/unsupported tests |
| WI-10 | full affected-package `cargo test` / `cargo check`；static checks below |

Static checks:

```powershell
rg -n "CREATE TABLE .*channel|channel_participants|channel_bindings" crates/agentdash-infrastructure/migrations
rg -n "LifecycleChannel" crates
rg -n "list_all\(|list_by_project\(|scan.*LifecycleRun" crates
rg -n "accept_intake_message|LifecycleGateResolver|GateDeliveryIntent" crates/agentdash-application*
```

Expected result:

- channel table scan has no migration-added `channels` / `channel_participants` / `channel_bindings` table.
- `LifecycleChannel` has no new first-class model.
- ChannelService code has no startup/global owner scan.
- Companion direct delivery calls remain only inside Channel materializer or resolver boundary.

## Commit Plan

Commits happen in Phase 3.4 after WI-10 full check. Dispatcher drafts the exact plan from `git status --porcelain` and only includes files edited by this task.

Expected commit batches:

0. `docs(channel): 明确 ChannelService 派发与工作项追踪`
   - planning artifacts only, if committing this dispatch plan before implementation
1. `feat(channel): 建立 Channel 领域文档模型`
   - WI-01
2. `feat(database): 增加 owner document mutation 与 LifecycleRun registry`
   - WI-02, WI-03
3. `feat(channel): 接入 ChannelOwnerStore 与 ChannelService`
   - WI-04, WI-05, WI-09
4. `feat(capability): 增加 channel capability 投影`
   - WI-06
5. `feat(channel): 打通 Mailbox/Gate materialization`
   - WI-07
6. `feat(companion): 收束 runtime wake 到 ChannelService`
   - WI-08
7. `test(channel): 补齐 Channel 全链路验证`
   - WI-10 and cross-cutting tests

Commit message body uses project style:

```text
type(scope): 可保留英文专业用词的中文提交信息

- 分点描述具体更新内容
- 分点描述验证结果
```

If unrelated dirty files exist, list them as unrecognized and exclude them from the commit plan unless the user explicitly confirms inclusion.

## Context Recovery After Compaction

恢复主持任务派发上下文时按这个顺序读取：

```powershell
python ./.trellis/scripts/task.py current --source
Get-Content -Raw .trellis/tasks/07-07-channel-communication-capability-model/task.json
Get-Content -Raw .trellis/tasks/07-07-channel-communication-capability-model/prd.md
Get-Content -Raw .trellis/tasks/07-07-channel-communication-capability-model/design.md
Get-Content -Raw .trellis/tasks/07-07-channel-communication-capability-model/implement.md
Get-Content -Raw .trellis/tasks/07-07-channel-communication-capability-model/work-items/README.md
Get-Content -Raw .trellis/tasks/07-07-channel-communication-capability-model/work-items/WI-*.md
Get-Content -Raw .trellis/tasks/07-07-channel-communication-capability-model/implement.jsonl
Get-Content -Raw .trellis/tasks/07-07-channel-communication-capability-model/check.jsonl
git status --short
trellis channel list --all
```

如果 `task.py current --source` 没有 active task，用本文件所在路径作为主持任务路径。若有 worker 正在跑，从 `work-items/README.md` 找 channel / worker handle，再用：

```powershell
trellis channel messages channel-service-dispatch --raw --no-progress
trellis channel wait channel-service-dispatch --as dispatcher --from <worker> --kind done,error --timeout 1s
```

恢复后先更新对应 `WI-*.md` 的 `Progress Log`，再继续派发。

## Files To Expect

- `crates/agentdash-domain/src/channel/`
- `crates/agentdash-domain/src/workflow/entity.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs`
- `crates/agentdash-infrastructure/migrations/`
- `crates/agentdash-spi/src/connector/mod.rs`
- `crates/agentdash-spi/src/session_persistence.rs`
- `crates/agentdash-application-agentrun/src/agent_run/runtime_capability.rs`
- `crates/agentdash-application*/` 中适合承载 `ChannelService` 与 materializer 的 application module

具体路径以代码搜索结果为准，优先遵守既有 crate 边界。

## Decisions To Preserve

- Channel 是一等领域与 `ChannelService` 主干；它不等价于一等关系表。
- LifecycleRun owner document 是 runtime Channel registry 的持久化边界。
- Owner document 通过原子 mutation port 写入；broad aggregate update 保留独立 document column。
- 新增 Channel owner document column 使用 `jsonb`，列名使用业务语义名，Rust 侧映射为 typed `ChannelRegistryDocument`。
- Project owner store 的具体物理承载不在本任务决定；后续由 Assets 系统收束。
- ChannelService 只按 owner ref lazy load registry。
- Channel participants、binding、broadcast policy、message/delivery planning 是 Channel registry 事实。
- `CapabilityState.channel` 是 AgentFrame 可见操作投影，不是 membership 或 policy 事实源。
- `ChannelAddress` 只负责 source/delivery attribution。
- Mailbox 负责 AgentRun durable consumption；LifecycleGate 负责 wait/result authority。

## Follow-up Tasks

- 具体 IM provider adapter。
- Project Channel Asset 物理承载。
- 完整 Channel event log / audit outbox，如企业审计要求需要。
- Extension Protocol Channel 命名或统一 Channel 体系收束。
- 既有 `LifecycleGate`、`agent_run_mailbox_messages`、`agent_run_lineages` 是否应向 owner document 或更窄事实表收敛的独立数据库设计审计。
