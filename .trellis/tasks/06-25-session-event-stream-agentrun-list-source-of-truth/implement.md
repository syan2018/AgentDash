# 收束 Session 前端事件流与 AgentRun 列表事实源 Implement

## Phase 0: Guard Existing Work

- [ ] 确认工作区未提交变更中是否有他人改动；只触碰本任务相关文件。
- [ ] 不回退 `.trellis/tasks/06-25-crate-split-boundary-cleanup/` 等已存在未提交目录。

## Parallel Execution Strategy

本任务适合拆成 4 条并行工作线，但需要一个主协调者负责 contract 命名与最终集成，避免各线各自定义事实源。

### Recommended Workstreams

| 工作线 | 可并行性 | 主要文件 | 交付物 | 阻塞关系 |
| --- | --- | --- | --- | --- |
| A. Session reducer / ordering | 高 | `sessionStreamReducer.ts`、`types.ts`、`sessionStreamReducer.test.ts` | durable / ephemeral 拆轴、同 item freshness guard、混合批次测试 | 需要先确定 `SessionDisplayEntry` ordering 字段命名 |
| B. Session feed / tool burst | 高 | `useSessionFeed.ts`、`useSessionFeed.test.ts`、必要的 entry/group UI | in-progress tool 默认进 tool burst、bounded output 仍单卡、thinking merge 不共轴排序 | 依赖 A 暴露的 ordering 字段，但可先改分类与测试 |
| C. AgentRun list backend projection | 高 | `lifecycle_agents.rs`、`workspace/query.rs`、contract/test | `shell.last_activity_at` 与后端 keyset 排序同源 | 可独立于 A/B；若改 contract 需协调生成 |
| D. AgentRun list frontend cache/invalidation | 中 | `AgentRunShortcutList.tsx`、`active-agent-run-list.tsx`、`AgentRunWorkspacePage.tsx`、可选 store | 侧栏/完整列表共用 Project-scoped list projection，事件触发刷新并移除固定轮询 | 依赖 C 的 timestamp 语义，但可先搭 store/invalidation |

### Critical Serial Decisions

并行前必须由主协调者先定 3 个小决策：

- `SessionDisplayEntry` 的 UI ordering 字段名与最小结构。推荐：`timelineOrder` + `progressSeq`。
- AgentRun root list entry 的 `shell.last_activity_at` 语义。推荐：root 使用 `LifecycleRun.last_activity_at`，child 可以继续展示 agent-level activity。
- 前端 AgentRun list projection 的缓存边界。推荐：新增 Project-scoped store，`AgentRunShortcutList` 与 `ActiveAgentRunList` 共用。
- AgentRun list projection 的刷新机制。推荐：统一事件驱动失效/刷新，移除固定周期轮询兜底；缺事件时补事件或在 command success 分支显式 refresh。

这些决策不需要更多代码研究，已经由 PRD/Design 的唯一事实源原则决定。定完后四条线可以同时开工。

### Suggested Parallel Schedule

1. 主协调者先提交/广播 shared decisions，不改业务逻辑。
2. A 与 B 同时写前端 session 测试；A 负责 reducer 失败测试，B 负责 feed/tool burst 失败测试。
3. C 同时写后端 projection 测试并修 `shell.last_activity_at` 来源。
4. D 同时搭 list projection store 和 refresh API，但先不依赖 C 的具体实现细节。
5. A 完成后，B 合并 `timelineOrder` 适配，移除 mixed `eventSeq` 排序。
6. C 完成后，D 移除前端二次排序或改为只信任后端 list order。
7. 主协调者做最终集成验证：session tests、AgentRun list tests、contract check、frontend typecheck。

### Sub-agent Allocation

- Agent 1：Session reducer specialist。只碰 reducer/type/test，目标是消除 durable / ephemeral 共轴与同 item 回写。
- Agent 2：Session feed/tool burst specialist。只碰 feed aggregation/UI tests，目标是 in-progress tool 进入 burst 且保持 bounded 单卡。
- Agent 3：Backend projection specialist。只碰 AgentRun list API/query/contract tests，目标是排序字段同源。
- Agent 4：Frontend list projection specialist。只碰 AgentRun list store/sidebar/full list/workspace invalidation，目标是详情页与列表刷新一致。
- Main coordinator：审查 shared type/contract 命名、解决冲突、跑最终验证、更新 spec。

### Merge Order

推荐合并顺序：

1. C backend projection：它改动相对独立，能先固定 AgentRun list contract。
2. A reducer ordering：它定义 session display ordering 基础。
3. B feed/tool burst：在 A 后接入最终 ordering 字段，减少返工。
4. D frontend list store/invalidation：在 C 后移除二次排序并接入最终 timestamp 语义。

如果需要最快看到用户可感知改善，可以先合并 B 的 “in-progress tool 进入 burst” 小改动和测试；但 A 未完成前，tool 中途回写/错位仍可能存在。

## Phase 1: Add Failing Tests First

- [ ] 在 `sessionStreamReducer.test.ts` 增加 durable + ephemeral 混合批次测试：
  - durable `item_started(cmd-1, inProgress old)`
  - ephemeral `item_updated(cmd-1, inProgress newer)`
  - durable `item_started(cmd-1, old)` 后到时不能覆盖 newer state
- [ ] 增加 ephemeral progress 不按 `ephemeral_seq` 插入 durable timeline 错位的测试。
- [ ] 在 `useSessionFeed.test.ts` 增加 in-progress tool 默认进入 tool burst 的测试。
- [ ] 增加 bounded output / truncation tool 仍保持单卡可见的回归测试。
- [ ] 为 AgentRun list projection 增加后端测试：
  - API 排序 key 与 entry `shell.last_activity_at` 使用同一 timestamp。
  - root entry 与 child row 的 activity 语义清晰。

## Phase 2: Fix Session Reducer Ordering

- [ ] 为 `SessionDisplayEntry` 增加 UI ordering / progress freshness 字段，避免 durable event seq 与 ephemeral seq 共轴。
- [ ] 调整 `reduceStreamState`，避免把 incoming batch 先分 ephemeral 后 durable 导致同 item 回写。
- [ ] 对同一 item 增加 freshness guard：completed > item_updated/progress > item_started。
- [ ] 保留 assistant / reasoning finalization 逻辑，确保 terminal text 仍是权威文本。
- [ ] 确保 rawEvents 仍只记录 durable events，ephemeral 不污染 rawEvents。

## Phase 3: Fix Feed Aggregation

- [ ] 修改 `classifyEntry`，让 in-progress tool 默认走 tool burst lane。
- [ ] 如现有 group UI 无法表达 in-progress status，补齐 `AggregatedEntryGroup` 渲染中的状态展示。
- [ ] 保持 hard boundary 规则：用户输入、agent message、可见错误、approval、context_frame 截断 tool burst。
- [ ] 保持 bounded/truncation tool 单卡可见。
- [ ] 修改 `mergeThinkingIntoDisplayItems`，不要用 mixed `eventSeq` 对 durable / ephemeral display item 统一排序。

## Phase 4: Fix AgentRun List Source Of Truth

- [ ] 后端 `AgentRunWorkspaceShell.last_activity_at` 收束到 list projection activity timestamp。
- [ ] 优先让 root list entry 使用 `LifecycleRun.last_activity_at`，与 keyset cursor 一致。
- [ ] 确认 child / companion row 是否继续使用 agent-level activity；如果使用，命名和 contract 注释需表达清楚。
- [ ] 移除或调整前端 `AgentRunShortcutList` 中与后端分页不一致的二次排序。
- [ ] 确认 `ActiveAgentRunList` 不在已加载窗口内用另一个 timestamp 改变后端顺序。

## Phase 5: Add List Projection Invalidation

- [ ] 新增或复用一个 Project-scoped AgentRun list projection store。
- [ ] `AgentRunShortcutList` 和 `ActiveAgentRunList` 消费同一 store。
- [ ] AgentRun workspace 页面在 draft started、command submitted、turn end、session meta、mailbox 更新时通过事件触发 list projection refresh/invalidate。
- [ ] 移除 `AgentRunShortcutList` / `ActiveAgentRunList` 中用于正确性兜底的固定周期轮询。
- [ ] 对当前没有事件覆盖的状态变化补齐 Project/workspace 事件，或在对应 command success 分支显式调用 list store refresh。
- [ ] 避免把 command authority、conversation snapshot 或业务执行状态复制进 list store；这些仍由 AgentRun workspace projection / backend snapshot 提供。

## Phase 6: Validation

- [ ] `pnpm --filter app-web test -- --run src/features/session/model/sessionStreamReducer.test.ts src/features/session/model/useSessionFeed.test.ts`
- [ ] 运行新增的 AgentRun list projection 后端测试。
- [ ] 如改 contract：`pnpm run contracts:check`
- [ ] 如改前端类型：`pnpm run frontend:check`
- [ ] 视改动范围运行相关 `cargo test -p ...`。

## Notes For Implementation Agent

- 用户已确认 tool burst 本身不需要移除。
- 用户要求新 tool 默认直接进入 tool burst，不再等待 completed。
- 用户要求 AgentRun list projection 统一走事件刷新，不保留轮询兜底。
- 修复讨论必须从唯一事实源出发，不做前端猜测式补丁。
- 项目预研阶段无需兼容旧字段；如果字段语义错了，直接改成正确语义并处理生成/migration。
- 文档或 spec 更新只记录为什么这样做，不记录旧实现流水账。
