# 收束 Session 前端事件流与 AgentRun 列表事实源 Design

## Architecture Intent

本任务的设计原则是每条 UI 链路只有一个事实源：

- Session trace 的 durable 历史事实来自后端 session event log，坐标是 durable `event_seq`。
- Session trace 的 in-flight 进度事实来自后端 ephemeral event buffer，坐标是 `ephemeral_seq`，只服务同一运行中的 UI patch / preview，不参与 durable timeline 排序。
- Tool UI identity 来自 `AgentDashThreadItem.id` / item id，而不是事件到达顺序。
- AgentRun 列表排序事实来自后端 list projection 暴露的 activity timestamp，前端不再用另一个 timestamp 重排。

## Session Event Flow

当前问题来自把 durable 与 ephemeral 两套坐标塞进同一个 `SessionDisplayEntry.eventSeq`：

```text
durable backlog/event:
  event_seq = session event log sequence

ephemeral event:
  event_seq = per-session ephemeral_seq
```

前端应拆分这两个概念：

```ts
interface SessionDisplayEntry {
  eventSeq: number;              // durable event seq when available; diagnostic only for ephemeral
  timelineOrder: TimelineOrder;  // UI ordering source
  progressSeq?: number;          // ephemeral seq for progress dedup
}
```

推荐的 `timelineOrder` 形态：

```ts
type TimelineOrder =
  | { kind: "durable"; seq: number }
  | { kind: "anchored_progress"; anchorId: string; progressSeq: number }
  | { kind: "local_progress"; receivedOrdinal: number; progressSeq: number };
```

其中：

- durable event 按 durable `event_seq` 排列。
- `item_updated`、tool progress delta、text delta 等 progress event 优先锚定已有 `item:{item_id}` 或 `delta:{item_id}` entry。
- 无可锚定对象的 progress event 使用本地 received ordinal 暂挂到当前 turn 尾部；后续 durable anchor 到达后转为 anchored progress。
- progress event 的 `progressSeq` 只用于同一 progress lane 去重，不能和 durable seq 比较。

## Reducer Contract

`reduceStreamState` 不应按 event.ephemeral 把一个 incoming batch 拆成两段执行。它应按 transport 接收顺序或显式 merged order 应用，并在同一 item 上执行 freshness guard：

- durable `item_started` 可以创建 entry。
- ephemeral `item_updated` 可以更新 entry，并记录该 item 的 latest progress freshness。
- 后到的 durable `item_started` 如果比已有 item 状态旧，不覆盖 event payload 中的状态字段；它只能补齐 durable anchor / timestamp。
- durable `item_completed` 是终态权威，可以覆盖 progress state，并清理 `isStreaming` / pending approval。

建议在 reducer 内维护局部 freshness map 或在 entry 上记录：

```ts
progressSeq?: number;
lastEventKind?: "durable_started" | "progress_updated" | "durable_completed";
```

判断原则：

- completed > progress updated > started。
- 对同一 item，低优先级事实不回写高优先级 UI 状态。
- terminal assistant / reasoning 的 finalize 规则保留。

## Tool Burst Contract

用户确认 tool burst 没问题，并要求新 tool 默认直接进入 tool burst。

现状 `classifyEntry` 把 in-progress tool 分类为 `active_tool`，会 flush 已完成 tool group 并单卡展示。修复后：

- `tool_like` 覆盖 terminal 与 in-progress tool。
- Tool burst 可以包含 in-progress item。
- 单个 tool 是否独立展示由内容重要性决定，而不是状态是否 in progress。
- bounded / truncation marker 的 tool 仍可保持单卡可见，因为用户需要直接看到裁切状态。

推荐分类：

```text
tool_like:
  tool item，且没有 bounded/truncation hard visibility marker

tool_single:
  bounded output / truncation / 需要独立审批面板的特殊 item

hard_boundary:
  user input, agent message, visible error, approval, context_frame
```

如果 approval 需要和 tool 同处一个 burst，应在 UI 内把 approval state 渲染到对应 tool entry；如果 approval 仍是一条独立 event，则它作为 hard boundary 保持现状。默认先保持 approval hard boundary，避免改变审批交互语义。

## AgentRun List Source Of Truth

后端 `/projects/{project_id}/agent-runs` 当前排序使用 `LifecycleRun.last_activity_at`，但 shell 暴露 `LifecycleAgent.updated_at`。修复应收束为一个语义：

```text
AgentRunWorkspaceShell.last_activity_at = list projection activity timestamp
```

推荐使用 run-level `LifecycleRun.last_activity_at` 作为 root list entry 的排序与展示事实源，原因：

- API 已按 run-level keyset 分页。
- 一个 run 下可能有多个 agent / companion，列表分页边界应稳定在 run 级。
- shell 展示与 cursor 使用同一 timestamp 后，前端无需再次解释排序。

对 child / companion row：

- child shell 可以保留 agent-level activity，用于 child row 相对时间。
- root entry 的分页与 shortcut 排序必须使用 root entry shell 的同一 timestamp。
- 如果需要 child activity 抬升 parent run，应在后端更新 `LifecycleRun.last_activity_at`，而不是前端扫描 children 推导。

## Frontend List Refresh

当前详情页是热 projection，侧栏/列表是冷 HTTP state。推荐新增 Project-scoped AgentRun list projection store：

```text
useAgentRunListStore(projectId)
  - fetchFirstPage(projectId)
  - refreshProject(projectId)
  - invalidateProject(projectId, reason)
  - entriesByProjectId
```

`AgentRunShortcutList` 与 `ActiveAgentRunList` 共用此 store。详情页和 Project 事件流在以下事件触发 invalidate/refresh：

- draft started
- command submitted
- turn end
- session_meta_updated
- mailbox_state_changed
- cancel / resume / promote / delete mailbox

列表正确性不保留固定周期轮询兜底。刷新路径应统一来自事件驱动失效与写命令成功后的显式 refresh，原因是轮询兜底会掩盖缺失的事件 contract，让侧栏/列表和详情页的刷新路径长期分叉。

如果某个后端状态变化目前没有 Project / workspace 事件，修复方向是补齐事件或在相关 command success 分支显式触发 store refresh，而不是保留 30 秒 poller。Store 可以保留用户手动刷新入口和页面首次加载 fetch，但不能把固定间隔轮询作为一致性机制。

## Contracts And Migration

如果只改变 `shell.last_activity_at` 的赋值来源，不需要新增字段或数据库 migration。

如果新增 `activity_at` / `sort_activity_at` 字段，应同步：

- Rust contract type
- generated TypeScript
- 前端 types consumption
- `pnpm run contracts:check`

预研阶段推荐直接修正 `shell.last_activity_at` 语义，不新增兼容字段。

## Risk Areas

- Reducer 改顺序会影响 streaming assistant/reasoning finalize，需要保留终态权威覆盖。
- Tool burst 纳入 in-progress 后，`SessionEntry` / `ToolCallCardShell` 必须能在 aggregated group 内展示运行中状态。
- AgentRun list store 如果引入全局缓存，需要避免和已有 `lifecycleStore` 重叠保存同一业务事实；它应只缓存 list projection，不保存 command authority。

## Validation Strategy

- 前端 reducer 单测覆盖 mixed durable/ephemeral。
- feed 单测覆盖 in-progress tool 默认进入 burst。
- list projection 后端单测覆盖 shell timestamp 与 cursor timestamp 一致。
- 前端列表单测覆盖 shortcut 不再二次使用不同 timestamp 重排。
- 前端 store / 页面测试覆盖 command submit、turn end、mailbox、session meta 事件触发 list store invalidation。
- 测试覆盖 shortcut/list 不再注册固定周期 poller。
