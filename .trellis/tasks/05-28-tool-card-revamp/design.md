# Design — 工具卡片体验整体重做

## 1. 当前架构回顾

```
useSessionStream  →  entries[]  ──┐
                                  ├──→ useSessionFeed.aggregateEntries → displayItems[]
useSessionFeed    →  classify ────┘                                          │
                                                                             ▼
SessionEntry (switch on item kind)
  ├─ AggregatedToolGroupEntry      ← tool burst
  ├─ AggregatedThinkingGroupEntry  ← thinking burst
  ├─ AggregatedContextFrameGroup   ← CTX burst
  └─ SingleEntry → renderToolCallCard → ToolCallCardShell + body
                       │
                       ├─ CommandExecutionCardBody
                       ├─ FileChangeCardBody
                       ├─ McpCardBody
                       ├─ DynamicToolCallCardBody  ─→ GenericJsonBody (兜底)
                       ├─ GenericJsonBody (fsRead/fsGrep/fsGlob)
                       └─ ImageCardBody / WebSearchCardBody / ...
```

## 2. 跨轮合并（R1）的修复方向

### 问题精确定位

`classifyEntry` 三类决策：
- `tool_like` → 工具组累积。
- `visible_boundary` → flush 工具组、推送当前 entry。
- `neutral` → 透明，不影响任何组。

幽灵 boundary 的来源：
- `isContextFrameEvent` 的 entry 当成 `visible_boundary`，进入 side group 收纳。但**进入 side group 之前已经把 toolGroup flushed**（看 `aggregateEntries` 的 visible_boundary 分支）。
- 即使下游 `AggregatedContextFrameGroup` 因为 `parseContextFrame` 返回 null 而渲染 null，flush 行为已经发生。
- `isRenderableSystemEventUpdate` 同样是 visible_boundary，但部分 hook_event 在 SessionSystemEventCard 里也可能渲染极简 / 隐式空。

### 修复策略：分层 boundary

引入 boundary 强弱概念：

| 类别 | 例 | 是否 flush tool group |
|---|---|---|
| **hard boundary** | user_message_chunk, agent_message_delta(非空), reasoning_*, approval_request, error, system_message 等"agent/用户产出的真正可见内容" | 是 |
| **soft boundary** | context_frame, hook_trace（即使可显）、capability_state_changed 等"侧轨信息" | **否**：进 side group，但保持 active toolGroup 不变 |
| **neutral** | 静默事件、empty deltas、turn_started/completed | 透明 |

这样工具组不会被侧轨信息打散；侧轨信息照样能聚合成 CTX side group。**但要解决渲染顺序问题**（侧轨内容应该出现在哪里）。

### 渲染顺序方案

侧轨信息选择两种之一（最终在实现时确认）：

**方案 A · "保留位置"** — 侧轨 entries 不参与 toolGroup，按它们在原 stream 中的位置直接 push 到 result（在 toolGroup 之后或之前各自插入的瞬间）。CTX side group 在多个 CTX 连续到达时合并，否则单独渲染。**toolGroup 跨越 CTX 时不会被打散**——这是关键。

实现：visible_boundary→soft boundary 的处理路径：
```
case "soft_boundary": {
  // 不 flush tool group
  const sideKind = getSideGroupKind(entry);
  if (sideKind && sideGroupMatchesKind(activeSideGroup, sideKind)) {
    activeSideGroup.entries.push(entry);
  } else {
    // 上一组 side group flush（与 result 当前位置无关）
    flushSideGroup();
    activeSideGroup = createSideGroup(sideKind, entry);
  }
}
```

但 result 顺序：当 toolGroup 还活着时，flushSideGroup 会把 CTX 插到 result 哪里？正常流程下，result 里 toolGroup 还没 push（活着的状态）。所以 push 顺序是：
- `[push existing items, ...future]`，CTX 进来时 push CTX → 然后 toolGroup 在最后 flush 时 push。
- 结果序：CTX 出现在 toolGroup **之前**。

这其实没问题——CTX 表示"在这些工具调用前 / 之间发生了一次身份切换"，把它放在 tool burst 之前（作为前导上下文卡）阅读体验是合理的。

**方案 B · "嵌入 toolGroup"** — toolGroup 接受非 tool 类型的辅助 entries（如 CTX），由 AggregatedToolGroupEntry 内部按时间序渲染。复杂度略高，但语义最准确。

**选择**：先做方案 A。如果实测 CTX 出现在 burst 之前阅读不畅，再演进到方案 B。方案 A 的好处是改动局部、单测容易写。

### 渲染层兜底

无论分类怎么改，渲染层增加一个保险：`AggregatedContextFrameGroup` 当 `frames.length === 0` 时返回 null —— 此时上层 SessionEntry 应该能感知"虚渲染"并跳过为它分配空间。当前 React 渲染 null 不占空间，没问题；只是聚合层不该把它视作 boundary。这一层修改保留，作为防御。

## 3. 卡片 shell（R2）抽象

### 新的 props 形状（极简、不过度结构化）

```ts
// ToolCallCardShell.tsx
export interface ToolCallCardShellProps {
  kind: KindMeta;
  header: ToolCardHeaderModel;   // ← 替代当前 title: ReactNode
  status: DisplayStatus;
  isPendingApproval?: boolean;
  sessionId?: string;
  itemId: string;
  durationMs?: number;
  defaultExpanded?: boolean;
  children: ReactNode;
}

// 新增 — 极简两行模型
export interface ToolCardHeaderModel {
  /** 主信息行：各 renderer 自由构造 ReactNode（路径 / 命令 / server/tool / 查询词等）。
   *  注意：不要在这里重复 badge 已经表达的 verb（badge=READ 时不要再写 "Read"）。 */
  primary: ReactNode;
  /** 参数摘要行：通用形态 = 灰色小字。cwd / 行范围 / args 摘要 / target 之类。 */
  secondary?: ReactNode;
}
```

**设计取舍**：放弃 `file` / `range` / `trailing` 结构化字段。各 renderer 根据自己的工具语义自由组装 primary。这样 model 表面积小、心智负担低，未来加新工具不用再改 model schema。`FilePathPill` 仍然抽出来作为**通用 React 组件**给需要的 renderer 用，不做成 model 字段。

### `ToolCardHeader` 渲染（新文件 `ui/ToolCardHeader.tsx`）

通用两行结构：

```
┌────────────────────────────────────────────────────────────────┐
│ [READ]  ld-km://AgentCase/LD-DesignerAssistant-配置迭代.md  ▼  │
│         L1-201                              已完成 · 0.6s     │
└────────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────────┐
│ [RUN]   git status --short                                  ▼  │
│         cwd: d:/ABCTools_Dev/AgentDashboard            执行中   │
└────────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────────┐
│ [TOOL]  filesystem/mounts_list                              ▼  │
│         path: ld-km                                  已完成    │
└────────────────────────────────────────────────────────────────┘
```

- `[BADGE]` = `kind.badge`，沿用现样式。
- 第一行（primary）：直接是 file path / command / `server/tool` / query。**不再写"Read"/"编辑"这种与 badge 重复的 verb**。
- 第二行（secondary）：参数摘要。Read=行范围；commandExecution=cwd；MCP=入参第一个有意义字段；search=`in <target>`；fileChange=`+N 文件 +A -R`。
- 状态/duration 仍然在右侧 status cluster，与现状一致；**不进 secondary**。
- 删除现有 [ToolCallCardShell.tsx:133](packages/app-web/src/features/session/ui/ToolCallCardShell.tsx#L133) 的 `kind.label` 副标题——它和 badge 完全重复。
- **MCP 暂时复用 `TOOL` badge**：`resolveKind` 中 `mcpToolCall` 直接返回 `KIND_REGISTRY.tool`，不再用 `mcp` kind。后续若要专门标识 MCP（独立 icon / 色彩），再单独在 KIND_REGISTRY 改一处即可。`KIND_REGISTRY.mcp` 暂保留不动以免连累 summary 路径，后续清理时再看。

### `FilePathPill`（通用工具组件，不进 model）

需要文件路径时，renderer 自己 `<FilePathPill path={x} range={...} />` 当 primary 用：

```tsx
// 路径过长时基于 basename 完整可见的中段省略
<FilePathPill path="d:/.../session/model/useSessionFeed.ts" range={{ from: 12, to: 241 }} />
```

实现要点（不变）：
- basename 永远完整显示。
- 父目录用 `dir="rtl"` + `text-overflow: ellipsis` 让省略发生在前缀。
- range 作为可选小字尾随。

### renderToolCallCard 改造

各 case header 各自构造，原则：**primary = 该工具最有信息量的"那一句"**，secondary = 参数摘要。

```ts
case "fsRead":
  return {
    kind,
    header: {
      primary: <FilePathPill path={item.path} />,
      secondary: rangeText(item.offset, item.limit),  // "L12-241" or undefined
    },
    body: <ReadCardBody item={item} />,
    status,
  };

case "commandExecution":
  return {
    kind,
    header: {
      primary: <code className="font-mono text-sm">{item.command}</code>,
      secondary: item.cwd ? `cwd: ${item.cwd}` : undefined,
    },
    body: <CommandExecutionCardBody ... />,
  };

case "mcpToolCall":
  return {
    kind,
    header: {
      primary: `${item.server}/${item.tool}`,
      secondary: summarizeArgs(item.arguments),  // "path: ld-km" 之类
    },
  };

case "fileChange": {
  const n = item.changes.length;
  const stats = sumDiffStats(item.changes);
  return {
    kind,
    header: {
      primary: <FilePathPill path={item.changes[0]!.path} />,
      secondary: n > 1
        ? `+${n - 1} 文件 · +${stats.added} -${stats.removed}`
        : `+${stats.added} -${stats.removed}`,
    },
  };
}

case "dynamicToolCall":
  return {
    kind,
    header: getDynamicToolHeader(item),
    body: dispatchDynamicBody(item),
    status,
  };
```

`getDynamicToolHeader` 按 tool 名分支：
- `read` → `{ primary: <FilePathPill path={args.path} />, secondary: rangeText(args.offset, args.limit) }`
- `write` / `edit` / `str_replace_editor` / `applypatch` → `{ primary: <FilePathPill path={args.file_path ?? args.path} /> }`
- `grep` → `{ primary: <code>"{args.pattern}"</code>, secondary: \`in ${args.path ?? args.glob}\` }`
- `glob` → `{ primary: <code>{args.pattern}</code> }`
- `websearch` → `{ primary: <code>"{args.query}"</code> }`
- `webfetch` → `{ primary: args.url }`
- `todowrite` → `{ primary: \`更新 ${count} 项\` }`
- 其他 → `{ primary: item.namespace ? \`${item.namespace}/${item.tool}\` : item.tool, secondary: summarizeArgs(args) }`

### 兼容性

`ToolCallCardShell` 是会话流内部组件，没有外部消费方（已 grep 确认仅 `SessionEntry` 用）。`title: ReactNode` → `header` 是不破坏外部契约的内部重构。

## 4. ReadCardBody（R3）

### 文件位置
`packages/app-web/src/features/session/ui/bodies/ReadCardBody.tsx`

### 数据来源（统一 normalize）

```ts
interface ReadPayload {
  text: string | null;       // 整段文本（拼接 contentItems 中所有 text 块）
  language: string;          // 从 path 扩展名推断
  startLine: number;         // 1-based 起始行
  totalLines: number;        // 抓到的行数
}

function normalizeReadItem(item): ReadPayload
```

适配三个上游：
- `fsRead` → `item.contentItems[].type === "text"` 拼接，`startLine = item.offset ?? 1`。
- `dynamicToolCall(read)` → 同上，`startLine` 从 `item.arguments.offset` 取。
- 其他兜底 → 直接 stringify（避免崩）。

### 渲染

```
┌──────────────────────────────────────────────┐
│ [复制] [全屏]                          238 行 │
├──────────────────────────────────────────────┤
│  12 │ import { useState } from "react";      │
│  13 │ ...                                    │
│  ...                                         │
│ ◢ 显示 24 / 238 行，展开全部                 │
└──────────────────────────────────────────────┘
```

- 行号列：`tabular-nums text-muted-foreground/60`，sticky-left 不影响。
- 内容列：复用现有 `agentdash-chat-code-block` 样式 + 一个简易 highlighter。
- highlighter：先用最简版本——按扩展名 map 到一组关键字着色（ts/js/tsx/jsx/json/md/py/sh），不做完整 AST。后续可换 prismjs（已有 npm 生态成熟方案）。MVP 不引入新依赖。
- 默认显示 24 行（`max-h` + 内部滚动），footer 提供"展开全部"切换为不限高 + 完整内容。
- "复制"按钮 copy raw text；"全屏"留位但 MVP 可不实现（feature-flag）。

## 5. DiffCardBody（R4）

### 文件位置
`packages/app-web/src/features/session/ui/bodies/DiffCardBody.tsx`

### 数据来源

```ts
interface DiffPayload {
  unified: string;       // unified diff 文本
  added: number;
  removed: number;
  language?: string;
}

function buildUnifiedDiff(opts: {
  before?: string; after?: string; rawDiff?: string; path?: string;
}): DiffPayload
```

适配：
- `fileChange.changes[].diff` → 直接 unified。
- `dynamicToolCall(edit | str_replace_editor)` → `old_string` / `new_string` 合成 unified（自实现简单 LCS 或者直接两段 ±）。
- `dynamicToolCall(applypatch)` → `arguments.patch` 当 unified。
- `dynamicToolCall(write)` → `new_string` / `content` 整段 `+`。

合成 unified 不必精确对齐行号，简单 `--- old\n+++ new\n@@ ...` + 全部 `-old / +new` 即可。后续可优化成基于行的 myers diff。

### 渲染

```
┌──────────────────────────────────────────────────┐
│ +12  -3                              [复制 diff] │
├──────────────────────────────────────────────────┤
│  10  10 │ const x = 1;                           │
│  11   - │ const old = 2;                         │
│      11 + │ const fresh = 2;                     │
│  12  12 │ const y = 3;                           │
│ ...                                              │
│ ◢ 折叠 / 展开全部                                │
└──────────────────────────────────────────────────┘
```

- 简单分行：`split("\n")`，前缀 `+` / `-` / 其它。
- 双列行号：旧/新各一列；context 行两列都填，`+` 行只填新列，`-` 行只填旧列。
- 着色：`+` 行 bg `success/8` text `success`，`-` 行 bg `destructive/8` text `destructive`。
- `@@ ...` hunk 头浅灰背景做小标签。
- 超过 40 行默认折叠。

### fileChange 多文件嵌套

`FileChangeCardBody` 现已是"按文件折叠"，每个 `change` block 内部把现有 `<pre>{change.diff}</pre>` 替换成 `<DiffCardBody payload={...} />` 即可。多文件总览的 `+N -M` 由 outer 计算。

## 6. registry 改造汇总

`toolCardRegistry.ts` 的 case 分支：

| ThreadItem | header.primary | body |
|---|---|---|
| commandExecution | `$ {command}` (code) | CommandExecutionCardBody |
| fileChange | first file path | FileChangeCardBody → 内含 DiffCardBody |
| mcpToolCall | `{server}/{tool}` | McpCardBody |
| webSearch | `Search` | WebSearchCardBody |
| imageView/Generation | `View`/`Generate image` | ImageCardBody |
| collabAgentToolCall | `{tool} agent` | CollabAgentCardBody |
| contextCompaction | `上下文压缩` | ContextCompactionCardBody |
| dynamicToolCall(read) | `Read` | **ReadCardBody** |
| dynamicToolCall(edit/str_replace_editor/applypatch/write) | `Edit`/`Write` | **DiffCardBody** |
| dynamicToolCall(grep/glob/...) | 现状 | DynamicToolCallCardBody (GenericJsonBody) |
| fsRead | `Read` | **ReadCardBody** |
| fsGrep | `Grep` | DynamicToolCallCardBody (保留 JSON) |
| fsGlob | `Glob` | DynamicToolCallCardBody (保留 JSON) |

`primary` 短词优先（动词 / 工具名），`file` chip 接管路径。

## 7. 测试策略

- **单测扩 `useSessionFeed.test.ts`**:
  - T15: `[tool_a, context_frame, tool_b]` 合并为单个 burst（CTX 被聚为 side group 与 burst 同存或前导）。
  - T16: `[tool_a, agent_msg(非空), tool_b]` 仍然分裂（保持 hard boundary）。
  - T17: 多个连续 context_frame 仍然合并到一个 side group。
  - T18: `[tool_a, context_frame, agent_msg, tool_b]` —— hard boundary 仍然分裂。
- 现有 T1–T14 全部保持通过（注意 T13/T14 的 platform 行为预期可能要更新）。
- 新增 `ReadCardBody.test.tsx` / `DiffCardBody.test.tsx` 极简快照或行为测试。
- 浏览器手测：参考 PRD AC7。

## 8. 开发顺序与回滚点

1. R1 合并修复（最小变动 + 单测）→ commit 1
2. R2 卡片 shell 抽象（不改 body，只迁 header）→ commit 2
3. R3 ReadCardBody → commit 3
4. R4 DiffCardBody → commit 4
5. 收尾：清理重复样式 / 轻微 polish → commit 5

每个 commit 都通过 lint + typecheck，便于中途回滚。回滚点 = 任一 commit 的前一个状态。
