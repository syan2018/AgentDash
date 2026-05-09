# ContextFrame 卡片信息架构重构（方案B 双层 shell + WYSIWYG 单列）

## Goal

重构 [`ContextFrameCard.tsx`](../../../frontend/src/features/session/ui/ContextFrameCard.tsx) 与外层 [`AggregatedContextFrameGroupEntry`](../../../frontend/src/features/session/ui/SessionEntry.tsx) 的信息架构。把外层升级为 frame 选择 shell、让每个 frame 以 kind 衍生的 token 清晰标注、内层 body 严格按 Agent 实际收到的 prompt 顺序单列平铺——让用户在 session feed 里**所见即所得地看到 Agent 的上下文真相**，同时折叠层级不超过 2 层。

## Core Design Principle

**所见即所得（WYSIWYG-for-Agent）**：UI 展示顺序 = Agent 实际收到的 prompt 顺序。
- section 严格按 `ContextFrame.sections[]` 数组原顺序渲染，前端**禁止**按"重要性"重排序、跳过或默认隐藏任何 section。
- Agent 原文（`rendered_text`）是 ground truth，sections 是它的解构视图；原文作为单列最后一节出现，不是顶层 tab。
- section 之间视觉分隔"轻"，强化"这是一份连续 prompt 的结构化视图"而非"一组并列导航项"。

## What I already know

### 现状问题

- 折叠层级最多 4 层（外层聚合 → 单帧卡 → NoticeSection → 内部 item）
- 外层与内层 header 撞脸（都是 CTX + "Agent 上下文…更新"），无层级辨识
- 展开后第一行 6 个调试 chip 抢占视觉焦点，98% 用户不关心
- 每个 section body 顶部重复 `{title}：{summary}`，外层按钮已展示过，纯冗余
- "Agent 实际收到的文本"埋在第 3 层，是核心卖点却最难触达
- Chip 滥用、灰色胶囊墙、无语义区分
- `capability_delta` 等距平铺 12 类变化，真正的 added/removed 被 effective_capabilities 全量快照淹没
- 长列表 `join("，")` 无法扫读；工具 schema 初始帧可能几十项无任何处理

### 项目硬约束

- **EventCards.tsx:21**：「badge 是唯一染色点，外框和文字保持中性色」
- `▲▼` Unicode 箭头是项目跨卡片统一约定（不在本任务内替换）
- `EventFullCard` 已支持 `debugChips` / `debugLines` / `debugRaw` / `debugBody` 折叠区，本任务复用，不造新原语
- `BADGE` 常量提供 neutral/primary/success/warning/error 五色 token

### 数据形状

12 种 section kind：`bootstrap_context` / `capability_delta` / `tool_schema` / `tool_schema_delta` / `workflow_context` / `hook_injection` / `system_notice` / `workspace_surface` / `skill_surface` / `hook_runtime_surface` / `auto_resume` / `compaction_summary`。顶层含 `kind / source / phase_node / apply_mode / delivery_status / delivery_channel / message_role / rendered_text / sections[]`。

## Decisions（brainstorm 收敛）

| # | 话题 | 决定 |
|---|------|------|
| Q1 | section 视觉语言 | **严格 badge-only**。正文全中性色；diff 用 `+ / − / ↻` 符号（不染色）；section 通过各自 badge token 区分 |
| Q2+Q4 | 布局 & 外层合并 | **双层 shell**。外层 `ContextFrameStream` 承担 frame 选择职能（单帧/多帧统一使用），内层 frame body 单列长页，不引入 tab-in-tab |
| Q3 | Agent 原文默认态 | **默认折叠、一键可开**（明显的 `▸ Agent 实际原文 (N 行·M tokens)` 分节块，状态按 frame 独立记忆） |
| Q5 | 超长 section 处理 | **max-height + 内部滚动**，不引入搜索/过滤/"显示前 N 项"超参 |

## Requirements

### 外层 `ContextFrameStream` shell（合并旧 `AggregatedContextFrameGroupEntry` 与单帧入口）

- [ ] 单帧与多帧共用同一个 shell 组件，保证视觉一致
- [ ] Header：badge `CTX` + 汇总文案（`N 帧 · 最后阶段 {last_phase}` 或单帧时 `1 帧 · 阶段 {phase}`）+ 右侧 `▲▼`
- [ ] 展开后第一行：横向 frame tab 条，按时间顺序排列
  - 每个 tab 的视觉：`[TOKEN] 阶段/关键变化`（例如 `[BDL] apply · +1 −2`、`[BOOT] task_start`、`[CMP] 12 条`、`[RES] stop_continue`）
  - TOKEN 由 `frame.kind` 衍生（见下表）
  - 单帧时 tab 条仍显示（保持一致），此时等效于 "pill label"
  - active tab 视觉用项目现有 `hover:bg-secondary/35` 的反向 `bg-secondary/50`（保持中性色原则）
- [ ] 选中 tab 后在 tab 条下方渲染对应 frame body，切换不卸载已展开的折叠状态

### Frame kind → token 对照表

| `frame.kind` | Token | Badge variant |
|--------------|-------|---------------|
| `runtime_context_update`（含 capability/tool schema delta 的混合 bundle） | `BDL` | `neutral` |
| `bootstrap_context` | `BOOT` | `primary` |
| `workspace_surface` | `WS` | `neutral` |
| `skill_surface` | `SKL` | `neutral` |
| `hook_runtime_surface` | `HOOK` | `neutral` |
| `auto_resume` | `RES` | `warning` |
| `compaction_summary` | `CMP` | `warning` |
| 其他未知 kind | 取前 4 字母大写 | `neutral` |

### 内层 Frame body（单列长页）

- [ ] 严格按 `frame.sections[]` 原顺序渲染，不排序、不跳过、不合并
- [ ] 每个 section 渲染为一个独立块：
  - 顶部一行：section token badge（见下表）+ 一句 `title — 一行 summary/计数`
  - 直接展开 body，**不再独立折叠**（层级 2 上限）
  - 每个 section body 顶部**不再重复** `{title}：{summary}`（已合并进 header 行）
- [ ] Body 末尾依次：
  - `▸ Agent 实际原文 (N 行)` —— 默认折叠，点击展开为 `pre` 块
  - `▸ 调试信息` —— 默认折叠，内含 kind/source/channel/role/delivery/sections 等 chip + 完整 JSON
- [ ] 超长子内容（tool_schema 工具列表、fragment.content、skill 列表等）用 `max-h-96 overflow-auto` 限高内部滚动，内容不过滤

### Section kind → token 对照表

| `section.kind` | Token | Badge variant |
|----------------|-------|---------------|
| `bootstrap_context` | `BOOT` | `primary` |
| `capability_delta` | `CAP` | `neutral` |
| `tool_schema` | `TOOL` | `neutral` |
| `tool_schema_delta` | `TOOL` | `neutral` |
| `workflow_context` | `WF` | `neutral` |
| `hook_injection` | `HOOK` | `neutral` |
| `system_notice` | `SYS` | `neutral` |
| `workspace_surface` | `WS` | `neutral` |
| `skill_surface` | `SKL` | `neutral` |
| `hook_runtime_surface` | `HOOK` | `neutral` |
| `auto_resume` | `RES` | `warning` |
| `compaction_summary` | `CMP` | `warning` |

### 各 section body 的渲染规则

- **capability_delta**：diff 列表，每项独立一行，前缀 `+ ` / `− ` / `↻ `（纯符号，不染色）；顺序：增 → 减 → 变更；`effective_capabilities` 作为一个独立的小节 `当前生效能力 (N 项)` 折叠块放在最末，不与增减混排
- **tool_schema / tool_schema_delta**：工具项一行一个，展示 name + description（截断）+ capability_key/source chip；点击工具项可展开 parameters_schema JSON（单项级别的折叠，是唯一允许的第 3 层折叠）；外层用 `max-h-96` 限高
- **bootstrap_context / workflow_context / hook_injection**：fragment 按序铺开，每项头部 slot/source chip + `pre` 正文；正文限高 `max-h-48`
- **system_notice**：title 已在 header，body 直接 `pre` 展示
- **workspace_surface**：`cwd` / `default_mount` chip + mount 列表
- **skill_surface**：`read_tool` chip + skill 列表
- **hook_runtime_surface**：pending_action_count 单 chip
- **auto_resume**：`reason` chip + `prompt` 的 `pre` 展示
- **compaction_summary**：messages/tokens/timestamp chip + `compacted_until_ref` JSON 折叠

## Acceptance Criteria

- [ ] Shell 折叠态：1 行汇总"N 帧 · 最后阶段 X"，不依赖 `md:block`
- [ ] 任意 frame 的 Agent 原文 ≤ 2 次点击可达（展开 shell → 点原文分节）
- [ ] 折叠层级最多 2 层（section 不再独立折叠），唯一例外：单个 tool schema 项的 parameters JSON
- [ ] 严格按 `frame.sections[]` 顺序渲染，移除所有"按重要性排序"或"隐藏全量快照"的前端逻辑
- [ ] `ContextFrameCard.test.tsx` / `SessionEntry.context-frame.test.tsx` 全部通过（文案断言可更新，数据契约解析断言不变）
- [ ] 不引入新 npm 依赖
- [ ] `pnpm lint` / `pnpm typecheck` 通过
- [ ] 视觉走查 8 种核心 frame（bootstrap / runtime_context_update / workspace_surface / skill_surface / hook_runtime_surface / auto_resume / compaction_summary / tool_schema 初始帧）截图对比 before/after

## Definition of Done

- 所有现有 + 新增测试通过
- `pnpm lint && pnpm typecheck` 全绿
- 8 种核心 frame 截图 before/after 对比
- 不引入新依赖
- 不改动后端 ContextFrame 数据契约

## Out of Scope

- 后端 ContextFrame schema 改动
- Agent 原文 markdown 渲染（仍用 `pre whitespace-pre-wrap`）
- 跨 frame 对比 / 历史回放 / 搜索过滤
- 用户偏好持久化（哪些 frame 默认选中 / 哪些分节默认展开）
- 替换 `▲▼` 为 icon（项目跨卡片统一约定，不在本任务）
- Section 染色条带 / 左侧彩色 border（违反 badge-only 约束）

## Technical Approach

### 文件切分

- `frontend/src/features/session/ui/ContextFrameStream.tsx` **(新增)**：外层 shell，输入 `AggregatedContextFrameGroup` 或单个 platform event，输出含 frame tab 条的卡片。`SessionEntry.tsx` 原 `AggregatedContextFrameGroupEntry` 和单帧入口统一路由到此组件
- `frontend/src/features/session/ui/ContextFrameBody.tsx` **(新增)**：单帧 body，接收已解析的 `ContextFrame`，输出单列 section 块 + Agent 原文折叠 + 调试折叠
- `frontend/src/features/session/ui/contextFrame/SectionRenderers.tsx` **(新增)**：各 `section.kind` 的 body 渲染函数，按 contract 表一一实现
- `frontend/src/features/session/ui/ContextFrameCard.tsx` **(改造)**：保留为薄包装（`<ContextFrameStream entries={[single]} />`），或直接移除并让 SessionEntry 直接用 Stream
- `frontend/src/features/session/model/contextFrame.ts`：数据层不变，补充 `frameKindToken(kind)` / `sectionKindToken(kind)` 纯函数（可选放这里或放 UI 层）

### 落地节奏（单 PR 内分 commit）

- **commit 1**：新增 `ContextFrameStream` + `ContextFrameBody` 骨架，frame tab 条可用，内层先直出 Agent 原文（不拆 section），覆盖现有入口 + 更新 `SessionEntry.context-frame.test.tsx` 文案
- **commit 2**：实现 12 种 section renderer，按 contract 渲染；更新 `ContextFrameCard.test.tsx` 断言
- **commit 3**：调试折叠块 + Agent 原文折叠块 + max-height 限高；视觉走查截图入 PR

### 风险

- 项目现有测试用 `renderToStaticMarkup` + 文案断言，Tab 切换状态不会在 SSR 下表现 → 需要改用 `@testing-library` fireEvent 或在 test 里直接传 `defaultActiveFrameId` prop
- `AggregatedContextFrameGroupEntry` 调用点在 `SessionEntry.tsx` 的 `aggregated_context_frames` 分支，合并时注意保持 key 稳定（避免滚动列表重渲染）

## Decision (ADR-lite)

**Context**：GPT 最初实现把后端 ContextFrame schema 的 12 种 section 等距平铺 + 4 层折叠，Agent 原文被埋到第 3 层，外层批量容器又与内层卡片视觉撞脸，导致在 session feed 里信息架构不清。

**Decision**：
1. 外层 `AggregatedContextFrameGroupEntry` 升级为真正的 `ContextFrameStream` shell，承担 frame 选择职能；单帧/多帧一视同仁
2. 严格 badge-only 视觉语言（frame 和 section 都按 kind 映射 token + 既有 BADGE variant）
3. 内层单列长页 + 所见即所得（严格按 `sections[]` 顺序），不引入 tab-in-tab
4. Agent 原文和调试信息在单列末尾以折叠块形式出现，不抢视觉焦点但 ≤ 2 次点击可达
5. 超长内容仅靠 max-height 内部滚动，不引入搜索/过滤

**Consequences**：
- 正：信息架构清晰（2 层折叠上限），Agent 原文升级为一等信息，视觉一致性更强，前端不再"代理" section 重要性判断
- 负：重构面积较大（新增 2~3 个组件文件、改造 SessionEntry 入口、更新测试断言）；frame tab 条在超多帧（>10）情况下可能需要横向滚动（本期不处理，标记为后续）

## Technical Notes

### 关键文件

- 主组件：[`frontend/src/features/session/ui/ContextFrameCard.tsx`](../../../frontend/src/features/session/ui/ContextFrameCard.tsx)
- 数据层：[`frontend/src/features/session/model/contextFrame.ts`](../../../frontend/src/features/session/model/contextFrame.ts)
- 外层聚合：[`frontend/src/features/session/ui/SessionEntry.tsx`](../../../frontend/src/features/session/ui/SessionEntry.tsx) `AggregatedContextFrameGroupEntry`
- 卡片原语：[`frontend/src/features/session/ui/EventCards.tsx`](../../../frontend/src/features/session/ui/EventCards.tsx)
- 测试：`ContextFrameCard.test.tsx`、`SessionEntry.context-frame.test.tsx`

### 约束

- badge 是唯一染色点（EventCards.tsx:21）
- 不引入新 npm 依赖
- 不改动后端数据契约
