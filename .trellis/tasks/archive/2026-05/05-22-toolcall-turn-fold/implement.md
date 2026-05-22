# Implement — 前端 ToolCall 轮次聚合与折叠重构

执行顺序自上而下；每步完成后勾选并继续下一步。每个阶段末尾给出验证命令。

## 阶段 A：类型与算法骨架

- [ ] **A1**：在 [types.ts](packages/app-web/src/features/session/model/types.ts) 的 `ToolAggregationType` 联合中新增字面量 `"turn_fold"`，放在联合首位。其他字面量保留。
- [ ] **A2**：导出 `aggregateEntries` 为命名 export（当前是 [useSessionFeed.ts:126](packages/app-web/src/features/session/model/useSessionFeed.ts#L126) 内部 helper），方便单测。文件末尾追加 `export { aggregateEntries }`。
- [ ] **A3**：实现新的 `aggregateEntries`，替换 [useSessionFeed.ts:126-255](packages/app-web/src/features/session/model/useSessionFeed.ts#L126-L255) 整段。
  - 引入辅助 `classifyEntry(entry)` 返回 union type `"turn_boundary" | "message" | "tool_like" | "thinking" | "context_frame" | "non_agg"`
  - 引入辅助 `isEffectivelyEmptyMessage(entry)`：见 design §2.3
  - 删除原 `getToolAggregationType` 中的命令关键字分类（cat/grep/curl/sed 全部删除）；保留 `getToolAggregationType` 函数名但内容简化为：tool-like 命中 → `"turn_fold"`，否则 `null`
  - 删除原 `isFileEditEvent`、`getFilePathFromEvent`、`currentDiffGroup` 全部相关代码
  - 思考组、context_frame 组逻辑保持不变，但注意它们触发时要先 `flushUnit()`
  - 单条解聚仍保留：`turn_fold` 仅含 1 条 entry → 还原为单条 entry
- [ ] **A4**：修正 `aggregateEntries` 的 `flushGroups` → `flushAll`，把 `currentDiffGroup` 移除；添加 `flushUnit`（仅 flush turn_fold）方便分支调用
- [ ] **A5**：`isAggregatedDiffGroup` 类型守卫保留实现不变（语义上仍合法判定，只是新算法不再产出该值）。在函数注释加一行：`// 历史保留；新算法不再产出 file_edit 类型`

**验证 A**：
```bash
cd packages/app-web && pnpm typecheck
```
预期：通过。

---

## 阶段 B：单元测试

- [ ] **B1**：新建 [useSessionFeed.test.ts](packages/app-web/src/features/session/model/useSessionFeed.test.ts)。
- [ ] **B2**：编写 fixture 工厂函数：
  - `mkCmdEntry(id, command, opts?)` → 产出 `item_started` 含 `commandExecution` 的 `AcpDisplayEntry`
  - `mkFileChangeEntry(id, path)` → `fileChange`
  - `mkMcpEntry(id)` → `mcpToolCall`
  - `mkMessageEntry(id, text)` → `agent_message_delta`，`accumulatedText = text`
  - `mkTurnStarted()` / `mkTurnCompleted()`
  - `mkReasoningEntry(id)` → `reasoning_text_delta`
- [ ] **B3**：实现 design §5.1 表格 T1–T10 全部测试用例。每个用例 `expect(aggregateEntries(input))` 数组长度 + 类型 + 关键字段。
- [ ] **B4**：补充一个边界测试 T11：完全空 entries 数组返回 `[]`。
- [ ] **B5**：补充流式测试 T12：消息从空 → 非空的两次快照分别调用 `aggregateEntries`，验证第一帧（空文本）能聚合两侧 tool，第二帧（有文本）切断它们。

**验证 B**：
```bash
cd packages/app-web && pnpm test useSessionFeed
```
预期：所有用例通过。

---

## 阶段 C：UI 渲染

- [ ] **C1**：在 [SessionEntry.tsx](packages/app-web/src/features/session/ui/SessionEntry.tsx) 中删除 `AggregatedDiffGroupEntry` 函数体。
- [ ] **C2**：删除 `SessionEntry` 中 `if (item.aggregationType === "file_edit")` 分支（[SessionEntry.tsx:58-60](packages/app-web/src/features/session/ui/SessionEntry.tsx#L58-L60)），统一走 `AggregatedToolGroupEntry`。
- [ ] **C3**：删除 `getAggregationBadgeConfig` 函数（[SessionEntry.tsx:368-394](packages/app-web/src/features/session/ui/SessionEntry.tsx#L368-L394)），在 `AggregatedToolGroupEntry` 内 inline 固定值 `{ token: "TOOLS", label: "工具调用" }`。
- [ ] **C4**：改造 `AggregatedToolGroupEntry`（[SessionEntry.tsx:234-281](packages/app-web/src/features/session/ui/SessionEntry.tsx#L234-L281)）：
  - 添加 `const hasPendingApproval = group.entries.some(e => e.isPendingApproval)`
  - `useState(hasPendingApproval)` 作为 `expanded` 初值
  - 添加 `useEffect` 监听 `hasPendingApproval` 变 true 时 `setExpanded(true)`
  - **展开内容直接复用 `SingleEntry` 渲染**：把 `AcpToolCallCard ... compact={true}` 替换为 `<SingleEntry entry={entry} sessionId={sessionId} />`
  - compact 模式 prop 本身**不删**（保留 `AcpToolCallCard` 内的 compact 分支以备后续）
- [ ] **C5**：增强 `buildKindSummary`（[SessionEntry.tsx:396-419](packages/app-web/src/features/session/ui/SessionEntry.tsx#L396-L419)）：
  - 文案改为：`运行 N 条命令` / `编辑 N 个文件` / `调用 N 个 MCP 工具` / `调用 N 个工具` / `搜索 N 次` / `其他 N 项`
  - 追加 pending / failed 角标：
    ```
    const pending = entries.filter(e => e.isPendingApproval).length
    const failed = entries.filter(e => /* getThreadItemStatus === "failed" */).length
    if (pending > 0) parts.push(`${pending} 待审批`)
    if (failed > 0) parts.push(`${failed} 失败`)
    ```
- [ ] **C6**：把 `SingleEntry`（[SessionEntry.tsx:79-214](packages/app-web/src/features/session/ui/SessionEntry.tsx#L79-L214)）从文件局部函数改为同文件 named export（`export function SingleEntry(...)` 或 `export const SingleEntry = ...`）。`AggregatedToolGroupEntry` 调用即可。注意 `SingleEntry` 自身仍可继续被 `SessionEntry` 直接使用（不出包）。

**验证 C**：
```bash
cd packages/app-web && pnpm typecheck && pnpm lint
```
预期：通过，无 unused 警告。

---

## 阶段 D：联调验证

- [ ] **D1**：启动 dev server：
  ```bash
  cd packages/app-web && pnpm dev
  ```
- [ ] **D2**：打开浏览器中一个有真实历史 tool call 的 session。
- [ ] **D3**：手动核对 PRD 的 AC1–AC9：
  - AC1：5 条命令折叠为一条，摘要正确
  - AC2/AC3：空消息透明、非空消息切断（找一个对话历史里包含 "嗯"、"好" 这种短消息验证非空切断；空字符串消息可能少见，可在控制台手工调用 stream 注入测试）
  - AC4：单条 tool 不折叠
  - AC5：跨 turn 不合并
  - AC6：混合类型摘要
  - AC7：展开后看到完整卡列表（与未聚合时一致）；每张卡可点 ▼ 看 input/output/diff
  - AC8：pending approval 默认展开
  - AC9：流式 stdout 在展开后的 `CommandExecutionCard` 内实时刷新
- [ ] **D4**：手动核对回归点：
  - 思考组（reasoning）渲染未受影响
  - context_frame 流仍正常
  - 单 turn 内 tool 之间正常间隔（无 visual 重影）

**Review gate**：D 全部通过后再 commit。

---

## 阶段 F：命名重构（Acp* → Session*）

**这一阶段对功能没有影响，纯重命名**。建议**单独一个 commit**，便于 review 与 rollback。

- [ ] **F1**：在 `packages/app-web/src` 范围内按映射表逐个 rename。推荐顺序（先类型，后组件，后引用）：
  1. `model/types.ts`：`AcpDisplayEntry` → `SessionDisplayEntry`、`AcpDisplayItem` → `SessionDisplayItem`、`AcpToolCallState` → `SessionToolCallState`
  2. `ui/SessionToolCallCard.tsx`：`AcpToolCallCard` → `SessionToolCallCard`、`AcpToolCallCardProps` → `SessionToolCallCardProps`
  3. `ui/SessionMessageCard.tsx`、`ui/SessionPlanCard.tsx`、`ui/SessionTaskContextCard.tsx`、`ui/SessionOwnerContextCard.tsx`、`ui/SessionTaskEventCard.tsx`、`ui/SessionSystemEventCard.tsx`、`ui/SessionUsageCard.tsx`、`ui/SessionCompanionRequestCard.tsx`：各自 `AcpXxxCard` / `AcpXxxCardProps` → `SessionXxxCard` / `SessionXxxCardProps`
  4. `ui/SessionCapabilityCard.tsx`：`AcpSessionCapabilityCard` → `SessionCapabilityCard`（注意：去重 Session）
  5. 更新所有引用方（grep 列出的 20 个文件）
- [ ] **F2**：测试文件命名一并更新：`SessionSystemEventCard.test.tsx` 内部引用同步改名
- [ ] **F3**：`features/session/ui/index.ts` re-export 列表同步
- [ ] **F4**：执行方式建议：
  - 优先用 IDE rename symbol（VSCode F2）— 自动同步引用
  - 或 `sed -i` 逐个标识符替换：`grep -rl 'AcpToolCallCard' packages/app-web/src | xargs sed -i 's/AcpToolCallCard/SessionToolCallCard/g'`（每个标识符一次，注意 Props 也要同步）
- [ ] **F5**：验证全无 Acp 残留：
  ```bash
  grep -rE '\bAcp[A-Z]' packages/app-web/src
  ```
  预期：无输出
- [ ] **F6**：完整 check：
  ```bash
  cd packages/app-web && pnpm check
  ```
  预期：typecheck + lint + test 全绿

**Rollback**：单 commit 回退。

---

## 阶段 E：完成

- [ ] **E1**：运行包级别 check：
  ```bash
  cd packages/app-web && pnpm check
  ```
- [ ] **E2**：进入 Trellis Phase 3，依次：
  - 3.1 `trellis-check`
  - 3.3 spec 更新（如有新约定，例如"前端聚合按 turn 段折叠"作为 frontend spec 落到 `.trellis/spec/frontend/`）
  - 3.4 commit

## Rollback Points

- 阶段 A 完成但 B/C 未做：算法已替换，UI 仍调旧字段。**会出现摘要错乱**，回滚到 commit 前。
- 阶段 C 完成但 D 未通过：`git stash` 或 `git restore` 单 commit。
- 阶段 E 完成后发现回归：`git revert` HEAD。

## 已知非目标（不要在本任务里做）

- **完整卡 header 的信息密度增强**（按工具类型在 header 行直接显示 exit code / 耗时 / +N -M / args 摘要）—— 后续单独任务，已在 design §8 备忘可用协议字段
- 虚拟滚动（react-virtuoso）
- Turn 顶层摘要带
- 按 tool name / MCP server 的更细分桶
- 聚合内过滤 / 搜索
- 后端协议改动
- compact 模式本体废弃 / 重构（保留现状）
