# Design — 前端 ToolCall 轮次聚合与折叠重构

## 范围

只改前端：`packages/app-web/src/features/session/`
- `model/useSessionFeed.ts` — 重写聚合算法
- `model/types.ts` — 调整聚合组类型
- `ui/SessionEntry.tsx` — 折叠组渲染分支
- 新增 `model/useSessionFeed.test.ts` — 单元测试

后端事件协议、`useSessionStream.ts` 累积逻辑、`SessionToolCallCard` / `CommandExecutionCard` 内部实现**不动**。

## 核心改动一句话

把 `aggregateEntries()` 从「按 tool 类别相邻聚合」改成「按 turn 内连续 tool-call 段聚合」，段边界由 turn 切换 + 非空 AgentMessage 决定；段内不二次分桶；摘要按 ThreadItem 类型计数。

## 1. 数据模型

### 1.1 简化聚合组类型

`ToolAggregationType` 联合**清理为单值**：

```ts
// types.ts
export type ToolAggregationType = "turn_fold";
```

`AggregatedEntryGroup.filePath` 字段**移除**（仅在原 file_edit 分组使用，新算法不再产出）。

### 1.2 类型守卫

- `isAggregatedDiffGroup` 函数**移除**（无消费方）
- `isAggregatedGroup` 保留不变

## 2. 聚合算法（`aggregateEntries` 重写）

### 2.1 输入分类

对每条 `AcpDisplayEntry`，根据其 `event` 分到以下五类之一：

| 类别 | 判定 | 行为 |
|---|---|---|
| **TURN_BOUNDARY** | `event.type === "turn_started" \|\| event.type === "turn_completed"` | flush 当前 fold-unit，自身作为单独 entry 推入结果 |
| **MESSAGE** | `event.type === "agent_message_delta"` | 看 `accumulatedText`：trim 后非空 → flush + 自身 push；空 → **完全丢弃**（不 flush 也不 push）* |
| **TOOL_LIKE** | `extractThreadItem(event)` 命中 `commandExecution / fileChange / mcpToolCall / dynamicToolCall / webSearch` | 加入当前 fold-unit；若不存在则开启新的 |
| **THINKING** | 现有 `isThinkingEvent(event)` | 走现有 `aggregated_thinking` 分支（不变）—— 但思考事件会 flush 当前 tool fold-unit |
| **CONTEXT_FRAME** | 现有 `isContextFrameEvent(event)` | 走现有 `aggregated_context_frames` 分支（不变）—— 同样 flush tool fold-unit |
| **OTHER_NON_AGG** | `isNonAggregatableEvent` 命中且不属于以上 | flush + 自身 push（与现状一致） |
| **OTHER_PASSTHROUGH** | 其余 | flush + 自身 push |

\* 空消息透明的精确处理：见 §2.3。

### 2.2 流程（伪代码）

```
let unit: AggregatedEntryGroup | null = null
let thinking: AggregatedThinkingGroup | null = null
let ctxFrame: AggregatedContextFrameGroup | null = null
const out: AcpDisplayItem[] = []

function flushUnit() {
  if (unit) { out.push(unit); unit = null }
}
function flushThinking() { ... }
function flushCtxFrame() { ... }
function flushAll() { flushUnit(); flushThinking(); flushCtxFrame() }

for (const entry of entries) {
  const c = classify(entry)

  switch (c) {
    case TURN_BOUNDARY:
    case OTHER_NON_AGG:
    case OTHER_PASSTHROUGH:
      flushAll(); out.push(entry); break

    case MESSAGE:
      if (isEffectivelyEmptyMessage(entry)) {
        // 完全丢弃：不 push、不 flush
        continue
      }
      flushAll(); out.push(entry)
      break

    case TOOL_LIKE:
      flushThinking(); flushCtxFrame()
      if (!unit) {
        unit = { type: "aggregated_group", aggregationType: "turn_fold",
                 entries: [entry], id: entry.id, groupKey: `fold-${entry.id}` }
      } else {
        unit.entries.push(entry)
      }
      break

    case THINKING:
      flushUnit(); flushCtxFrame()
      // 现有 thinking 分支
      ...

    case CONTEXT_FRAME:
      flushUnit(); flushThinking()
      // 现有 ctxFrame 分支
      ...
  }
}
flushAll()

// 单条解聚：unit.entries.length === 1 → 还原为单条 entry
return out.map(item =>
  isAggregatedGroup(item) && item.aggregationType === "turn_fold"
                          && item.entries.length === 1
    ? item.entries[0]!
    : item
)
```

### 2.3 「空消息丢弃」的精确语义

判定函数：

```ts
function isEffectivelyEmptyMessage(entry: AcpDisplayEntry): boolean {
  if (entry.event.type !== "agent_message_delta") return false
  const text = entry.accumulatedText ?? entry.event.payload.delta ?? ""
  return text.trim().length === 0
}
```

**关键点：**
- 空消息 entry **不推入 `out`**：原本它在现状下也不产生可见输出（`AcpMessageCard` 收到空字符串 → 空气泡），丢弃后视觉一致
- **不 flush** 当前 `unit`：前后两侧 tool call 自然合并到同一 unit
- streaming 抖动：第一帧空文本时丢弃，后续帧若变非空再切断 unit。useMemo 重算结果会让用户只看到最终稳定态，无视觉跳动

### 2.4 turn_id 利用与否

`AcpDisplayEntry.turnId` 字段存在。当前算法**不依赖** `turnId` 边界，仅依赖 `turn_started`/`turn_completed` 事件 flush。这是有意为之：
- 流式过程中，`turn_started` 事件先到，后续 tool call 才到，自然形成新 unit
- 不引入 `turnId` 跨条目比较，避免事件乱序导致的边界判定问题
- 若 `turn_started` / `turn_completed` 缺失（异常路径），fold-unit 会跨"语义 turn"——这是退化但安全的行为

## 3. UI 改动（`SessionEntry.tsx`）

### 3.1 删除 `AggregatedDiffGroupEntry` 分支

`isAggregatedGroup(item) && item.aggregationType === "file_edit"` 现实中不再产出，删除该分支以简化代码。`AggregatedDiffGroupEntry` 函数本身可删除或保留为 dead code（**选择删除**，避免 lint 警告未使用）。

### 3.2 改造 `AggregatedToolGroupEntry`

- **不再依赖 `aggregationType`** 决定 badge：`turn_fold` 类型统一显示为"工具调用"框（badge token = `TOOLS`，label = `工具调用`）
- **摘要计数**：复用 `buildKindSummary`，文案微调：
  - `commandExecution` → `运行 N 条命令`
  - `fileChange` → `编辑 N 个文件`
  - `mcpToolCall` → `调用 N 个 MCP 工具`
  - `dynamicToolCall` → `调用 N 个工具`
  - `webSearch` → `搜索 N 次`
  - 其他 → `其他 N 项`
- **状态角标**：扫描 entries：
  - `pendingApproval` 计数 > 0 → 摘要末尾追加 `· N 待审批`，且 `expanded` 默认为 `true`
  - 任意 entry 的 `getThreadItemStatus(item) === "failed"` → 摘要末尾追加 `· N 失败`
- **展开内容直接复用 `SingleEntry` 渲染**：与未聚合时的卡片完全一致——
  - `commandExecution` → `CommandExecutionCard`
  - 其他 ThreadItem → `AcpToolCallCard`（**不传** `compact` prop）
  - 每张卡 header 默认可见，自带 ▼ 按钮支持二次展开看 input/output/diff/stdout
- **不再使用 compact 模式**：取消 `compact={true}` 的传递；compact 分支本身不删除（保留以备后续）

### 3.3 删除 `getAggregationBadgeConfig`

不再需要（永远是 `turn_fold` → `TOOLS / 工具调用`）。可保留为 fallback 函数返回固定值，或直接移除并 inline。**选择移除**。

### 3.4 默认展开规则

`AggregatedToolGroupEntry` 中 `useState(false)` 改为：

```ts
const hasPendingApproval = group.entries.some(e => e.isPendingApproval)
const [expanded, setExpanded] = useState(hasPendingApproval)
```

并 `useEffect` 在 `hasPendingApproval` 变 true 时强制展开（参考 `SessionToolCallCard.tsx:72-76` 现有模式）。

## 4. 边界与风险

### 4.1 React key 稳定性

`groupKey = "fold-${entries[0].id}"`。当首条 entry 切换（例如 unit 头部新增）时 key 变化 → 整组重挂载。这种情况只在算法首次将一个 unit "挂"到不同首条 entry 时发生。当前实现里 unit 的首条 entry 一旦确定就不会变（追加只会 push 到尾部），所以稳定。

### 4.2 现有 `isAggregatedGroupEqual` 比对

[useSessionFeed.ts:266-317](packages/app-web/src/features/session/model/useSessionFeed.ts#L266-L317) 用于 `displayItems` 引用稳定。`turn_fold` 仍是 `aggregated_group` 类型，`groupKey` + `entries` 长度 + `entryShallowEqual` 三段比对足够，无需改动。

### 4.3 与思考组 / context frame 组的交互

思考组（`reasoning_*`）和 context frame 组保持现有逻辑不变。在新算法里，遇到 thinking 或 context_frame 事件时**强制 flush 当前 tool unit**——这是对的：思考与 context 显示是另一种语义内容，不该塞进工具折叠组。

### 4.4 Pending approval 阻断流的疑虑

`approval_request` 走 `isNonAggregatableEvent` 分支，会 flush。**但 tool call 自身的 `isPendingApproval` 标记不来自 `approval_request` 事件**——它是 `AcpDisplayEntry` 上的派生字段（见 `useSessionStream`）。所以一个 `item_started` 的 tool 在 unit 里被聚合后，等审批的状态由 `entry.isPendingApproval` 标记，UI 层处理（见 §3.4）。

### 4.5 streaming command 的实时输出

聚合卡内的 compact 卡不显示 stdout（现状如此）。流式中的命令在摘要计数里实时刷新即可。完整 stdout 可见性问题随"compact 二次展开"后续任务一并解决。

### 4.6 性能

- 聚合算法仍是 O(n) 单趟扫描
- `useMemo` 依赖 `entries` 引用变化，已有
- 大 unit 展开后多卡片渲染：本任务**不引入虚拟化**（PRD R4 已声明 P1 后置）

## 5. 测试策略

### 5.1 单元测试 `useSessionFeed.test.ts`（vitest）

提取 `aggregateEntries` 为独立 export（或 `__test__` 命名空间），针对 fold 算法编写纯函数测试。构造 `AcpDisplayEntry[]` fixtures 覆盖：

| 用例 | 输入序列 | 期望输出 |
|---|---|---|
| T1 单条 tool 不折叠 | `[cmdExec1]` | `[cmdExec1]` |
| T2 双条 tool 折叠 | `[cmdExec1, cmdExec2]` | `[turn_fold(2)]` |
| T3 空消息丢弃 | `[cmd1, msg(""), cmd2]` | `[turn_fold([cmd1, cmd2])]`（空消息不出现） |
| T4 非空消息切断 | `[cmd1, cmd2, msg("ok"), cmd3]` | `[turn_fold([cmd1, cmd2]), msg, cmd3]` |
| T5 turn 边界切断 | `[cmd1, turn_completed, turn_started, cmd2]` | `[cmd1, turn_completed, turn_started, cmd2]` |
| T6 混合类型聚合 | `[cmd1, fileChange1, mcpCall1]` | `[turn_fold(3)]` |
| T7 thinking 切断 | `[cmd1, reasoning_delta, cmd2]` | `[cmd1, aggregated_thinking, cmd2]` |
| T8 context_frame 切断 | `[cmd1, contextFrame, cmd2]` | `[cmd1, aggregated_context_frames, cmd2]` |
| T9 跨 turn 不合并 | `[cmd1, turn_completed, turn_started, cmd2]` | 与 T5 一致 |
| T10 pending approval 保留标记 | `[cmd1{pending}, cmd2]` | `turn_fold` 中 entries[0].isPendingApproval === true |

### 5.2 空消息丢弃注

按 §2.3 决策，空消息**直接丢弃**（不 push、不 flush）。视觉上和现状一致——现状下空 `accumulatedText` 进 `AcpMessageCard` 也是空气泡。丢弃后 React tree 更干净，且测试断言更简单。

### 5.3 手动验证

启动 `pnpm dev`（app-web），打开真实 session：
- 触发一段 5+ tool call 的工作 → 折叠为一条
- 在中途让 agent 输出文本（"我先这样做"）→ 切断成两段
- 包含 pending approval 的会话 → 默认展开，可批准
- 长命令流式输出 → 展开实时滚动

## 6. Rollback 策略

改动集中于一个文件 + 类型微调 + UI 一个组件。回滚 = `git revert` 单 commit。无数据迁移、无后端改动。

## 7. 命名重构（R6）

文件名已是 `Session*` 前缀，符号侧仍残留 `Acp*`。一律去前缀对齐。

### 7.1 改名清单

13 个符号 + 各自 Props（共 ~26 个标识符）：

| 文件 | 旧符号 | 新符号 |
|---|---|---|
| `ui/SessionToolCallCard.tsx` | `AcpToolCallCard` / `AcpToolCallCardProps` | `SessionToolCallCard` / `SessionToolCallCardProps` |
| `ui/SessionMessageCard.tsx` | `AcpMessageCard` / `AcpMessageCardProps` | `SessionMessageCard` / `SessionMessageCardProps` |
| `ui/SessionPlanCard.tsx` | `AcpPlanCard` / `AcpPlanCardProps` | `SessionPlanCard` / `SessionPlanCardProps` |
| `ui/SessionTaskContextCard.tsx` | `AcpTaskContextCard` | `SessionTaskContextCard` |
| `ui/SessionOwnerContextCard.tsx` | `AcpOwnerContextCard` | `SessionOwnerContextCard` |
| `ui/SessionCapabilityCard.tsx` | `AcpSessionCapabilityCard` | `SessionCapabilityCard` |
| `ui/SessionTaskEventCard.tsx` | `AcpTaskEventCard` | `SessionTaskEventCard` |
| `ui/SessionSystemEventCard.tsx` | `AcpSystemEventCard` | `SessionSystemEventCard` |
| `ui/SessionUsageCard.tsx` | `AcpUsageCard` | `SessionUsageCard` |
| `ui/SessionCompanionRequestCard.tsx` | `AcpCompanionRequestCard` | `SessionCompanionRequestCard` |
| `model/types.ts` | `AcpDisplayEntry` | `SessionDisplayEntry` |
| `model/types.ts` | `AcpDisplayItem` | `SessionDisplayItem` |
| `model/types.ts` | `AcpToolCallState` | `SessionToolCallState` |

### 7.2 影响面

引用方涵盖以下 20 个文件（grep 结果）：
- `features/task/task-agent-session-panel.tsx`
- `features/session/ui/index.ts`
- `features/session/ui/Session*.tsx`（10 个）
- `features/session/ui/CommandExecutionCard.tsx`
- `features/session/ui/SessionList.tsx`
- `features/session/ui/SessionEntry.tsx`
- `features/session/ui/SessionChatView.tsx`
- `features/session/model/useSessionStream.ts`
- `features/session/model/useSessionFeed.ts`
- `features/session/model/types.ts`

执行方式：每个标识符在 IDE 里 rename symbol（或 sed -i）— 见 implement 阶段 F。

### 7.3 命名冲突检查

- `SessionMessageCard` / `SessionToolCallCard` / `SessionPlanCard` 等：文件名已是这个名字，但导出的 `default` 是同名再导出 `AcpXxx`。改名后文件 default export 与命名 export 一致即可，不冲突。
- `SessionEntry` 已存在，不影响。

## 8. 协议字段调研（备忘，非本任务实现）

`backbone-protocol.ts:269+` `ThreadItem` 联合上的现成字段：

| ThreadItem 类型 | 可直接读取的关键字段 |
|---|---|
| `commandExecution` | `exitCode: number \| null`、`durationMs: number \| null`、`aggregatedOutput: string \| null`、`commandActions: CommandAction[]` |
| `mcpToolCall` | `durationMs: number \| null`、`arguments: JsonValue`、`error: McpToolCallError \| null` |
| `dynamicToolCall` | `durationMs: number \| null`、`arguments: JsonValue`、`success: boolean \| null` |
| `fileChange` | `changes[].diff: string`（unified diff，需解析才能算 +N -M） |
| `webSearch` | `query: string`、`action: WebSearchAction \| null` |

**给后续 header 信息密度增强任务的输入**：
- 命令卡 header 可显示 `exit 0 · 1.2s`
- MCP/动态工具 header 可显示 `1.2s` 与参数 1 行预览
- 文件变更 +N -M 需 diff 解析（或后端补充字段）

本任务**不消费上述字段**，仅在 design 备忘记录。

## 9. 关键代码引用

- 现有算法：[useSessionFeed.ts:126-255](packages/app-web/src/features/session/model/useSessionFeed.ts#L126-L255)
- UI 折叠组件：[SessionEntry.tsx:234-281](packages/app-web/src/features/session/ui/SessionEntry.tsx#L234-L281)
- 单卡（compact 分支）：[SessionToolCallCard.tsx:108-122](packages/app-web/src/features/session/ui/SessionToolCallCard.tsx#L108-L122)
- 类型定义：[types.ts:236-345](packages/app-web/src/features/session/model/types.ts#L236-L345)
