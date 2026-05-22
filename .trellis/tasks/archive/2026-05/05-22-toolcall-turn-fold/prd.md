# 前端ToolCall轮次聚合与折叠重构

## 背景

当前 `useSessionFeed.aggregateEntries()` 采用「按 tool 类别 + 相邻聚合」策略，存在三类问题：

1. 聚合范围有限：未识别命令 / 未列入分类的工具不会聚合，散落成多张卡片
2. 折叠后展开体验差：聚合卡展开后是 `compact` 卡，需要再点一次才能看到详情
3. 缺少「全量收敛」视图：没有一行摘要标题（如 "已运行 N 条命令 · 已编辑 M 个文件"）

数据层（`rawEntries` 与 `AggregatedEntryGroup.entries`）保留完整，本任务只重构聚合策略与渲染层。

## Goal

把「按类别相邻聚合」替换为「按 turn 内连续 tool call 段聚合」：用户的非空 AgentMessage 是聚合段的天然分隔；空消息透明跳过；段内不再做类别二次分桶，按时间顺序排列；段长度 ≥2 才折叠，单条直接暴露；折叠态显示一行摘要。

## Requirements

### R1 聚合单元定义（fold unit）

一个 fold unit 是 turn 内一段「可被一起折叠」的连续 thread item。边界规则：

- **turn 边界**：跨 turn 的 tool call 必须分到不同 unit（`turn_started` / `turn_completed` 强制 flush）
- **非空 AgentMessage 边界**：非空 `agent_message` / `agent_message_delta` 累积出的可见文本会切断 unit
- **空 AgentMessage 透明**：累积文本为空（trim 后长度为 0）的 message 不切断 unit，等同不存在
- **非聚合事件透明 vs 切断**：现有 `isNonAggregatableEvent()` 列表（platform/token_usage/thread_status/error/approval_request 等）保持现有行为——它们 flush 当前 unit；本任务不改这部分

### R2 fold unit 渲染规则

- **unit 内 entry 数 = 1**：直接渲染单条 `AcpToolCallCard`（与今天单条形态一致），不进入折叠组
- **unit 内 entry 数 ≥ 2**：渲染为可折叠组，默认折叠态
- **unit 内顺序**：严格按事件到达顺序展示，不再按 tool 类别二次分组（移除现有"file_edit"、"info_gather" 等子桶逻辑作为聚合维度，但分类信息可用于摘要计数）

### R3 折叠态摘要

折叠态标题为单行摘要，规则：

- 按 thread item 类型 / tool 类别统计计数，组合为一行：`已运行 3 条命令 · 已编辑 2 个文件 · 调用 1 个工具`
- 计数维度（MVP）：
  - `commandExecution` → "运行 N 条命令"
  - `fileChange` → "编辑 N 个文件"
  - `mcpToolCall` / `dynamicToolCall` → "调用 N 个工具"
  - `webSearch` → "搜索 N 次"
  - 其他 → "其他 N 项"
- 文案中不出现的桶（计数为 0）不展示；用 `·` 分隔
- 摘要后附"展开 / 收起"按钮（沿用现有 UI 风格）

### R4 展开态

- 展开后**直接放未聚合时的完整卡片**进容器：与 `SingleEntry` 渲染分支一致——
  - `commandExecution` → `CommandExecutionCard`
  - 其他 ThreadItem → `AcpToolCallCard`（非 compact）
- 每张完整卡 header 同时可见，**detail 默认 collapsed**：用户挑感兴趣的那张点 ▼ 看 input/output/diff
- 取消传递 `compact` prop（compact 模式本身保留以备后续，但本任务无消费方）
- 大单元（entry 数 > 上限阈值）的虚拟滚动**不在本任务范围**

### R6 命名重构（去 Acp 前缀）

项目早已脱离 ACP 协议，但 `packages/app-web/src/features/session/` 下仍残留 `Acp*` 前缀的符号。本任务一并改名，文件名已是 `Session*` 前缀，符号补齐对齐。

**重命名映射**（13 个符号 + 各自 Props）：

| 旧名 | 新名 |
|---|---|
| `AcpToolCallCard` | `SessionToolCallCard` |
| `AcpMessageCard` | `SessionMessageCard` |
| `AcpPlanCard` | `SessionPlanCard` |
| `AcpTaskContextCard` | `SessionTaskContextCard` |
| `AcpOwnerContextCard` | `SessionOwnerContextCard` |
| `AcpSessionCapabilityCard` | `SessionCapabilityCard` |
| `AcpTaskEventCard` | `SessionTaskEventCard` |
| `AcpSystemEventCard` | `SessionSystemEventCard` |
| `AcpUsageCard` | `SessionUsageCard` |
| `AcpCompanionRequestCard` | `SessionCompanionRequestCard` |
| `AcpDisplayEntry` | `SessionDisplayEntry` |
| `AcpDisplayItem` | `SessionDisplayItem` |
| `AcpToolCallState` | `SessionToolCallState` |

各自的 `*Props` interface 同步重命名。

### R5 行为兼容

- pending approval / error 状态的 tool call 在折叠组中**不丢失可见性**：摘要中追加 `· N 待审批` / `· N 错误` 角标；折叠态默认展开包含 pending approval 的 unit（参考现有 `SessionToolCallCard` 默认展开 pending 的策略）
- 流式中的 tool call（still streaming）保持现状：可以聚合进 unit，但摘要计数实时更新
- `rawEntries` / `rawEvents` 接口保持不变，下游消费者不受影响

## Acceptance Criteria

- [ ] AC1：单 turn 内连续 5 条 `commandExecution` 折叠为一个 unit，摘要 "已运行 5 条命令"，展开后按顺序显示 5 张完整卡片
- [ ] AC2：连续 tool call 中间夹一个**空** AgentMessage，仍然合并为同一 unit
- [ ] AC3：连续 tool call 中间夹一个**非空** AgentMessage，切成两个 unit；中间消息正常显示在两个 unit 之间
- [ ] AC4：单 turn 内仅有 1 条 tool call 时，不出现折叠卡，直接显示原单卡
- [ ] AC5：跨 turn 的 tool call（turn_started 之后开始的新 call）不会与上一 turn 的 unit 合并
- [ ] AC6：unit 内同时含有命令与文件编辑，摘要正确组合：`已运行 X 条命令 · 已编辑 Y 个文件`
- [ ] AC7：折叠 unit 展开后，每条 entry 渲染为完整卡（与未聚合时一致：commandExecution → `CommandExecutionCard`，其他 → `AcpToolCallCard`）；每张卡 detail 默认 collapsed，可点 ▼ 二次展开
- [ ] AC8：unit 中含 pending approval 的 entry 时，摘要带角标，并默认展开
- [ ] AC9：流式输出过程中，正在 streaming 的命令计入摘要计数，且展开后能看到实时 stdout
- [ ] AC10：`rawEntries` / `rawEvents` 公开字段语义未变，依赖它们的代码无需修改
- [ ] AC11：单元测试覆盖 `aggregateEntries` 新逻辑：空消息丢弃、非空消息切断、turn 边界、单条不折叠、混合类型摘要
- [ ] AC12：手动验证（dev server 启动后真实会话）所有 AC1–AC9 行为符合预期
- [ ] AC13：完成 R6 命名重构后，`grep -E '\bAcp[A-Z]' packages/app-web/src` 无任何匹配；`pnpm typecheck && pnpm lint && pnpm test` 全绿

## Out of Scope

- **完整卡 header 的信息密度增强**：按工具类型在 header 行直接暴露关键信息（命令的 exit code / 文件的 +N -M / MCP 的参数摘要），让用户不点 ▼ 也能看到关键 metric —— 后续单独任务
- 跨 turn 的全局摘要 / Turn 顶部摘要带（"Turn #3：已运行 5 条 · 已编辑 2 个" 这种二级聚合）
- 虚拟滚动（react-virtuoso 等）+ "展开前 N 条"软限制
- 按 tool name / MCP server 的更细桶分类
- 折叠态的过滤、搜索、按类筛选交互
- 后端事件流 schema 调整
- compact 模式本体的废弃 / 重构（保留现状，本任务不删除该 prop 分支）

## Notes

- 用户澄清要点：「AgentMessage 打断是预期，除非 message 为空就自动聚合」「一轮所有的 call 都可以自动折叠，单条暴露在外」「按顺序就好不用过度聚合，但可以给摘要」
- 不在数据层动手脚：`rawEntries` 完整保留，重构集中在 `aggregateEntries()` 与 `SessionEntry`/`AggregatedToolGroupEntry` 渲染分支
- 关键文件：[useSessionFeed.ts](packages/app-web/src/features/session/model/useSessionFeed.ts)、[SessionEntry.tsx](packages/app-web/src/features/session/ui/SessionEntry.tsx)、[SessionToolCallCard.tsx](packages/app-web/src/features/session/ui/SessionToolCallCard.tsx)、[types.ts](packages/app-web/src/features/session/model/types.ts)
