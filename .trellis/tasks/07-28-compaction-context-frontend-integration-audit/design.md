# Compaction 状态与上下文权威契约设计

## 设计目标

Compaction 必须成为 Agent owner 内可持久恢复的正式 activity；用户看到的状态、命令可用性和当前模型上下文必须由同一 owner query 派生。

核心 invariant：

1. 任意时刻只有 Agent owner 决定 active activity、active context recipe 和 command availability。
2. `Started`、`Applied`、terminal 的 identity 与 revision 在 Service API、Runtime view、presentation 和 UI 中保持一致。
3. 只有成功应用的 checkpoint 才能改变 active context revision。
4. Native 用户看到的 Exact recipe 与下一 provider round 的输入成员、顺序一致。
5. Codex 无法证明的 provider-private context 必须标为 Observed/Opaque，不能本地补全。
6. Conversation record、ContextFrame 与 structured provider tool 保持各自语义，不复制成第二份内容。

## 目标数据流

```text
Agent durable owner
├─ Activity fold
│  └─ AgentActivitySnapshot
├─ Context materializer
│  └─ AgentContextSnapshot / ModelInputRecipe
├─ Command policy
│  └─ CommandAvailability
└─ Canonical presentation
   └─ timeline/audit items
        │
        ▼
Complete Agent Service API
        │ read / changes：一次 source observation 原子承载 state + presentation
        ▼
AgentRuntimeConnection（可重建连接，不是 durable owner）
        │
        └─ AgentRuntimeView
             ├─ state/control selectors
             │   └─ composer / compact / fork / close / interaction gate
             ├─ canonical presentation
             │   └─ compaction timeline card
             └─ context revision
                 └─ revision-bound AgentContextSnapshot query
                     └─ current context inspector
```

Product Workspace 只消费其中需要展示的 selector，并继续拥有 Product shell、AgentFrame、
resource surface、subject/lineage 与 Workspace Module；它不再保存另一份 Runtime
execution、activity 或 command state。

Canonical presentation 负责审计与展示，不再反向触发 Workspace HTTP refresh以获得控制事实，
也不充当 activity 或 model input authority。

### 三个逻辑平面

- **状态面**：concrete Complete Agent 拥有 lifecycle、activity、interaction、operation、
  context revision/checkpoint 与 canonical records；Runtime connection只是同一事实的可重建
  normalized view。
- **控制面**：命令可用性、stale guard、cancellable 与 command receipt都从同一
  `AgentRuntimeView`读取，最终 admission仍由concrete Complete Agent裁决。
- **展示/查询面**：Conversation Feed渲染canonical records；Context inspector按
  `context_revision`查询完整recipe。两者不能反向推导控制状态。

## 1. Agent activity

建议在 Service API 定义一等 activity：

```text
AgentActivitySnapshot =
  Idle
  | Turn {
      turn_id,
      phase,
      interaction?,
      source_revision
    }
  | ContextCompaction {
      operation_id,
      compaction_id,
      mode,
      phase,
      base_context_revision,
      applied_context_revision?,
      cancellable,
      source_revision,
      started_at,
      terminal?
    }
```

Compaction phase：

```text
queued
running
applied
succeeded
failed
lost
cancelled
```

约束：

- `active_activity` 只包含非 terminal activity。
- `last_compaction_outcome` 可单独保留最近 terminal，供 UI 恢复提示。
- `Applied` 表示 checkpoint 已 durable commit；`Succeeded` 表示 operation 完整结束。
- `Failed/Lost/Cancelled` 不推进 active context revision。
- `cancellable` 由 owner 当前 phase 决定，前端不得猜测。

不把这些状态塞进 Agent lifecycle。Agent 可以是 Active，同时 activity 为 Idle、Turn 或 ContextCompaction。

## 2. Typed compaction terminal

当前无状态的 `ContextCompaction` item 无法表达失败。目标 presentation 必须至少无损投影：

```text
ContextCompactionStarted {
  operation_id,
  compaction_id,
  mode,
  base_context_revision
}

ContextCompactionApplied {
  operation_id,
  checkpoint_id,
  context_revision,
  summary_frame_id,
  recipe_digest
}

ContextCompactionSucceeded { operation_id, context_revision }
ContextCompactionFailed { operation_id, code, message, retryable }
ContextCompactionLost { operation_id, reason }
ContextCompactionCancelled { operation_id, phase }
```

可映射到 canonical item lifecycle，但必须满足：

- UI 能区分 in-progress/succeeded/failed/lost/cancelled；
- Product projector 能只接受 succeeded checkpoint；
- reload/reconnect 不依赖 transient error toast；
- 不再把 Failed/Lost 映射为普通 `ItemCompleted`。

## 3. 单一 command matrix

Activity 与 command availability 必须由 Agent owner 计算或由 Runtime 对 owner activity 做无损、唯一的 policy projection。Product 不再建立第二套相反规则。

如果沿用既有规范的 deferred 语义：

| Activity | Submit | Steer | Interrupt/Cancel | Compact | Fork | Close |
| --- | --- | --- | --- | --- | --- | --- |
| Idle | Start Turn | unavailable | unavailable | Start compaction | available | available |
| Turn | durable Steer | durable Steer | available | durable queue | unavailable | policy-defined |
| Compaction queued | durable deferred input | unavailable | cancel queue | duplicate unavailable | unavailable | policy-defined |
| Compaction running（side effect 前） | durable deferred input | unavailable | cancel compaction | unavailable | unavailable | unavailable |
| Compaction applied | durable deferred input | unavailable | unavailable | unavailable | unavailable | unavailable |
| Lost | unavailable | unavailable | recovery only | unavailable | unavailable | recovery only |

若产品选择“压缩期间拒绝新输入”，则把 Submit 明确投影为 unavailable。两种选择都比“UI 显示 enabled、owner 返回 conflict”正确。本设计建议保持 `07-17` 已定义的 durable deferred 语义。

草稿编辑、附件准备、只读 context refresh、timeline 展开、Workspace/VFS 浏览不属于 context head mutation，可以继续使用。真正提交、重复压缩、Fork、Close 和可能改变 interaction state 的响应必须由 command matrix 控制。

## 4. Exact CompactionCheckpoint

Dash owner 应形成可查询的 typed checkpoint：

```text
CompactionCheckpoint {
  checkpoint_id
  operation_id
  compaction_id
  mode
  terminal

  source_revision
  source_head
  source_digest
  base_context_revision
  applied_context_revision

  summary_frame
  compacted_range
  retained_range
  retained_record_ids
  included_tool_pair_ids
  structured_outcome_ids

  usage_before
  usage_after_estimate
  usage_confirmation_status
  created_at
}
```

`retained_record_ids` 可由稳定 range/revision 确定时不必重复持久化全部内容，但 query 结果必须能明确给出 membership，不能仅依赖 compaction event 的位置。

### Compactor 与 restore 共用 recipe

先构建 provider-neutral recipe，再由不同 consumer 使用：

```text
active stable ContextFrames
+ previous successful CompactionSummary frame
+ compacted prefix conversation
+ complete ToolCall/ToolResult pairs
+ structured interaction outcomes
+ retained suffix
+ structured provider tools
```

同一 recipe 驱动：

- compactor provider request；
- checkpoint digest；
- post-compaction provider request；
- Agent context snapshot；
- Product current-context view；
- provider capture test。

每项被 compact 的事实必须满足二选一：

- 被 summary input 完整覆盖；
- 明确属于 retained suffix。

不存在第三类隐藏丢失。

## 5. CompactionSummary ContextFrame

生产 producer 直接使用协议已有的 `ContextFrameSection::CompactionSummary`，不再包成 `SystemNotice`。

最低字段：

- `compaction_id` / `checkpoint_id`；
- `projection_version` / `context_revision`；
- strategy、trigger、mode、phase；
- source revision/digest/range；
- first kept record/event coordinate；
- compacted-until reference；
- messages/tools compacted；
- tokens before / after estimate；
- summary `rendered_text`；
- usage confirmation state。

ContextFrame 的 `rendered_text` 必须与 provider 接收的 summary frame文本完全相同。sections 用于解释和结构化展示，不能重写成另一份语义。

Conversation user/assistant/tool records不转成 ContextFrame；完整上下文查看器通过 recipe 同时展示 frame contributions 与 retained conversation contributions。

## 6. AgentContextSnapshot / ModelInputRecipe

建议在 Complete Agent seam 增加查询值：

```text
AgentContextSnapshot {
  source
  source_revision?
  context_revision?
  captured_at_ms
  authority
  fidelity
  recipe_digest?

  active_activity?
  last_compaction_outcome?
  checkpoint?

  contributions: [
    ContextFrameContribution {
      order
      frame
      delivery
    }
    ConversationRangeContribution {
      order
      records
      start_coordinate
      end_coordinate
      retained_by_checkpoint_id?
    }
    ProviderToolsContribution {
      order
      definitions | digest
    }
    OpaqueProviderContextContribution {
      order
      evidence
      reason
    }
  ]

  usage {
    measured_tokens?
    estimated_tokens?
    measured_at_context_revision?
    status: confirmed | estimated | stale | unavailable
  }
}
```

### Authority/fidelity

- Native Dash：`AgentOwned + Exact`。
- Codex：`AgentObserved + Observed`，provider-private checkpoint 以 opaque contribution 表达。
- 不为 Codex 合成一个看似 Exact 的 retained boundary。
- Product 和前端必须保留这些字段，不能降级为普通 preview。

### Revision

- source mutation 使用单调 `source_revision`。
- 只有成功 Applied checkpoint 生成新 `context_revision`。
- recipe 的成员或顺序改变时生成新 `recipe_digest`。
- provider usage 必须标注对应的 context revision；压缩后尚未有新 provider measurement 时显示 `stale/estimated`。

## 7. Runtime view 与 Product 边界

### AgentRuntimeConnection / AgentRuntimeView

沿用 `07-28-runtime-session-state-chain` 的统一链路。当前名为
`ManagedRuntimeSnapshot` 的DTO只是从Complete Agent authority即时normalize的read model，
不应被描述成Runtime-owned aggregate。最终公开命名由相邻任务收敛，本任务要求其语义至少原样投影：

- `active_activity`；
- `last_compaction_outcome`；
- command availability；
- current context revision/digest；
- context snapshot invalidation/changed evidence；
- authority/fidelity。

同一 AgentRun target只建立一个connection，负责：

- authoritative baseline；
- Agent source change tail；
- gap/reconnect reload；
- process-local presentation overlay；
- target切换隔离；
- 同一 `SourceObservation` 中state与presentation的原子投影。

Compaction不能新建独立连接、游标或state store。

终态收敛也不再按presentation类型枚举。当前connection只识别
`turn_completed`作为reload boundary；目标应按source state revision/change协议收敛，
避免为Turn、Compaction和未来activity分别维护event特判。

### Product Workspace

Product Workspace 不再：

- 根据 canonical item position 猜 active compaction；
- 单独允许 active turn 时 compact；
- 用 `active_compaction_id` 表示最后成功边界。
- 从Runtime snapshot还原一个更弱的`AgentObservation`，再派生第二份execution/commands；
- 由Conversation Feed presentation event驱动Workspace refresh来获得控制状态。

Agent状态UI若需要以下字段，应直接消费统一Runtime view selector：

- `active_activity`：真正进行中的 operation；
- `active_checkpoint_id`：当前 context recipe 使用的最后成功 checkpoint；
- `last_compaction_outcome`：最近 terminal；
- `context_revision`：当前 recipe revision。

Product Workspace继续拥有Product/Resource投影；其loading、refresh或failure不得改变Composer的
execution、cancel或compaction状态。

### Context query

完整context payload可能较大，不需要塞进每次Runtime baseline。Runtime view只发布
`context_revision/recipe_digest/fidelity`；Context inspector用这些坐标查询
`AgentContextSnapshot`：

- 返回完整 frame 与 retained records，而非只有 preview；
- 分类/估算是同一 snapshot 上的派生字段；
- 保留 authority/fidelity/revision/digest；
- 不从 timeline 重新推断 membership；
- 提供“当前模型上下文”和“会话审计历史”两个读模型。

## 8. 前端状态与交互

### 统一 view selector与本地 query state

不新增独立的Session compaction store。`activity`、`lastCompactionOutcome` 与
`commandAvailability` 是统一 `AgentRuntimeView` 的selector。

Session只持有：

```text
transportPendingCommand?
contextSnapshot {
  targetKey
  status
  requiredRevision?
  committedRevision?
  value?
  error?
}
```

canonical compaction item负责timeline card；Runtime view activity负责全局gate。两者来自同一次
source observation，并通过operation id对齐。

### UI

- Session header/status 明确显示“正在压缩上下文”及 phase。
- timeline card从 item lifecycle/typed terminal 渲染进行中、成功、失败、lost、cancelled。
- 手动按钮的 local pending 只防止重复发起 HTTP，不代表 operation terminal。
- composer可以继续编辑草稿，但提交由 availability控制。
- duplicate compact、Fork、Close 与 interaction response按 command matrix gate。
- stop/cancel仅在 `cancellable=true` 时提供；否则说明当前 phase不可取消。
- context inspector展示 Exact/Observed、revision、last updated、usage freshness。

### State observation与query reconciliation

收到包含compaction state的source observation时：

1. 同一Runtime connection原子更新activity、commands、context revision与presentation overlay；
2. `context_revision`变化时invalidates current context query；
3. 请求至少达到`requiredRevision`的新context snapshot；
4. authoritative baseline/source revision确认同一observation后移除presentation overlay。

不再：

- 让ContextCompaction presentation event触发Workspace conversation refresh；
- 同时维护Runtime feed与Workspace HTTP commands；
- 依赖页面local promise判断activity terminal。

请求提交必须同时验证：

- `targetKey` 仍匹配；
- response revision 不低于当前 committed revision；
- 若有 `requiredRevision`，不得用更旧结果结束 loading；
- 旧请求可以 abort，晚到结果也必须被 commit fence 拒绝。

## 9. 持久化与恢复

当前 compactor 绑定在 HTTP/service future 上，Started 后进程退出会留下永久 active source。目标必须采用 durable execution：

```text
durable work item
  -> claim/lease
  -> source revision fence
  -> idempotent compactor request/checkpoint apply
  -> terminal commit
```

或在 source inspect/open 时发现 active operation并用同一 identity 幂等恢复。进入 provider side effect 后 outcome 不确定时必须标为 Lost，不得静默恢复 idle。

Native inner effect 与 Complete Agent outer effect必须收敛为一个 owner：

- `inspect(effect_id)` 直接读取 source-owned effect；或
- source mutation 与 outer receipt在同一个 atomic commit 中提交。

项目未上线，不保留双账本兼容层。若 durable schema 需要变化，实施任务必须提供正式 migration。

## 10. 验证原则

测试不能只比较 summary 字符串或人工合成 item 顺序。纵向验证必须对比：

- operation/checkpoint identity；
- source/context revision；
- recipe digest；
- frame与conversation contribution顺序；
- tool call/result membership；
- provider request capture；
- authority/fidelity；
- terminal 后的 command availability；
- reload/reconnect 后的同一状态。

核心场景：

- Native manual success/failure/lost/cancel；
- automatic overflow A/B/C；
- active Turn queue + compaction期间 deferred input；
- Started/Applied/terminal 每个阶段 crash/restart；
- Surface apply/revoke、append frame 与 compaction交错；
- projection 并发请求与 target切换；
- Codex observed/opaque fixture；
- browser E2E 中状态、门禁、context inspector自动收敛。

## 非目标

- 不把所有页面交互在压缩期间整体锁死。
- 不把 conversation records复制成 ContextFrame。
- 不为不可观察的 provider状态制造推测值。
- 不保留旧 Product/Runtime 双命令规则的兼容路径。
