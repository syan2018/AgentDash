# 设计：工具调用卡片信息架构重构

## 1. 总体架构

### 1.1 当前输入链路

```
┌──────────────────────────────────────────────────────────────────┐
│ AgentDash-owned tools / connectors                               │
│   structured tool facts                                          │
└──────────────────────┬───────────────────────────────────────────┘
                       │ AgentDashThreadItem
                       ▼
┌──────────────────────────────────────────────────────────────────┐
│ agentdash-agent-protocol::backbone                               │
│   Codex ThreadItem builders / AgentDash item notifications       │
└──────────────────────┬───────────────────────────────────────────┘
                       │ AgentDashThreadItem (Codex ThreadItem | AgentDash native item)
                       ▼
┌──────────────────────────────────────────────────────────────────┐
│ backbone-protocol → 前端 generated/backbone-protocol.ts           │
└──────────────────────┬───────────────────────────────────────────┘
                       │ BackboneEvent::ItemStarted/ItemCompleted
                       ▼
┌──────────────────────────────────────────────────────────────────┐
│ 前端 SessionEntry → ToolCallCardShell (header + 折叠 + 审批)      │
│   → 一级分发 (AgentDashThreadItem.type)                          │
│   → 二级分发 (dynamicToolCall 内按 tool 名)  ★ 新增              │
│   → renderer 注册表 / GenericJsonBody 兜底                       │
└──────────────────────────────────────────────────────────────────┘
```

legacy vibe-kanban 链路已经在后端任务中收束为 adapter 边界：

```text
vibe-kanban NormalizedEntry / ActionType
  -> agentdash-executor::adapters::vibe_kanban_legacy_log_mapper
  -> agentdash-agent-protocol::backbone::thread_item builders
  -> BackboneEnvelope
```

### 1.2 数据契约边界

- 前端工具卡只消费 Backbone stream 内的 `AgentDashThreadItem`。
- Codex Protocol 已有的 `ThreadItem`、状态 enum 与输出片段直接使用。
- Codex 不足表达的 AgentDash 自有工具事实，从
  `agentdash-agent-types::AgentDashNativeThreadItem` 做加法扩展。
- `ActionType` 只属于 vibe-kanban legacy adapter，不进入前端设计模型。
- P3 改动集中在 `packages/app-web/src/features/session/ui/` 与 `model/`。

## 2. 后端基线：Backbone/Codex 事实源

后端基线由 `.trellis/tasks/05-26-backend-tool-event-source-convergence` 提供。
本任务只记录 P3 前端可依赖的结果：

- `agentdash-agent-protocol::backbone::thread_item` 是 Codex `ThreadItem` 的集中构造
  API，处理 Codex 内部 path 类型和 serde wire shape。
- `AgentDashThreadItem` 是前端 item lifecycle 的统一输入：
  `ThreadItem | AgentDashNativeThreadItem`。
- `pi_agent::stream_mapper` 将 `shell_exec` 产出 Codex `CommandExecution`，将
  `fs_apply_patch` 产出 Codex `FileChange`，将 `fs_read` / `fs_grep` / `fs_glob`
  产出 AgentDash native item，其他工具继续走
  Codex `DynamicToolCall`。
- `vibe_kanban_legacy_log_mapper` 承接 `ActionType` 到 Codex `ThreadItem` 的 legacy
  输入转换，并保留 dynamic fallback 的 `content_items`。

### 2.1 前端依赖的 ThreadItem 形态

P3 renderer 面向以下 item 类型：

- `commandExecution`：命令执行，header 直接展示 `$ command`。
- `fileChange`：文件变更，按文件/patch change 渲染摘要和 diff。
  `fs_apply_patch` 后端已经进入该 Codex variant。
- `webSearch`：搜索 query 与 action。
- `mcpToolCall`：MCP server/tool 与参数。
- `imageView` / `imageGeneration`：图片路径、预览或 prompt。
- `collabAgentToolCall`：协作 agent 任务。
- `contextCompaction`：上下文压缩 lifecycle item。
- `dynamicToolCall`：Codex 没有专用 variant 或 connector 保留通用工具形态时的兜底，
  前端按 `tool` 名做二级摘要。
- `fsRead` / `fsGrep` / `fsGlob`：AgentDash native read/search/list item，优先按
  结构化字段摘要，body 仍保留原始 arguments 与 contentItems。

前端 P3 不读取 connector-private payload 来判断一级类型；所有一级类型以
`item.type` 为准。

## 3. 前端：渲染分发架构

### 3.1 模块布局

```
packages/app-web/src/features/session/
├── model/
│   ├── threadItemKind.ts      ★ 新增：单一 kind/icon/label 注册表
│   └── types.ts               改：移除 getThreadItemKind 内字面量，转引 threadItemKind
└── ui/
    ├── ToolCallCardShell.tsx  ★ 新增：共享 shell（header/folding/approval/error）
    ├── toolCardRegistry.ts    ★ 新增：ThreadItem.type → renderer 映射
    ├── dynamicToolRenderers.ts ★ 新增：dynamicToolCall.tool → summarizer 注册表
    ├── bodies/                ★ 新增子目录
    │   ├── FileChangeCardBody.tsx
    │   ├── McpCardBody.tsx
    │   ├── WebSearchCardBody.tsx
    │   ├── ImageCardBody.tsx
    │   ├── CollabAgentCardBody.tsx
    │   ├── DynamicToolCallCardBody.tsx
    │   ├── GenericJsonBody.tsx
    │   └── jsonTree/JsonTree.tsx   折叠树组件
    ├── CommandExecutionCard.tsx    保留，内部改为复用 ToolCallCardShell
    ├── SessionToolCallCard.tsx     ★ 整体替换为 router shell（极薄）或删除
    └── SessionEntry.tsx            改：转向 toolCardRegistry 调用
```

### 3.2 ToolCallCardShell 接口

```typescript
interface ToolCallCardShellProps {
  kind: ThreadItemKind;          // 来自 threadItemKind 注册表
  title: ReactNode;              // 一行摘要（请求摘要）— renderer 提供
  status: DisplayStatus;
  isPendingApproval?: boolean;
  sessionId?: string;
  itemId: string;
  durationMs?: number;
  defaultExpanded?: boolean;     // 审批/失败时默认展开
  children: ReactNode;           // 展开后的 body
}
```

shell 内部负责：
- header 一行：`[badge][title][status dot+label][duration?][▼]`
- 审批按钮 / declined 提示
- approvalError 容器
- 折叠态隐藏 children；展开态加 `border-t` + padding 渲染 children

renderer 自身只关心：返回 title (ReactNode) + body (ReactNode)。

### 3.3 一级分发：toolCardRegistry

```typescript
type CardRenderer = (item: ThreadItem, ctx: CardContext) => {
  title: ReactNode;
  body: ReactNode;
};

const REGISTRY: Partial<Record<ThreadItem["type"], CardRenderer>> = {
  commandExecution: renderCommandExecution,    // → CommandExecutionCard 内部
  fileChange:       renderFileChange,
  mcpToolCall:      renderMcpToolCall,
  webSearch:        renderWebSearch,
  imageView:        renderImage,
  imageGeneration:  renderImage,
  collabAgentToolCall: renderCollabAgent,
  contextCompaction: renderContextCompaction,
  dynamicToolCall:  renderDynamicToolCall,    // 内部二级分发
};
```

`SessionEntry` 把 `commandExecution` 的特例分支去掉，统一走 registry。
`CommandExecutionCard` 不再独立顶层组件——其视觉迁入 `renderCommandExecution`
返回的 body，header 由 shell 统一处理。

### 3.4 二级分发：dynamicToolRenderers

```typescript
interface DynamicToolRenderer {
  /** header summary，单行；返回 null 落到 default summarizer */
  summarize(item: DynamicToolCall): ReactNode | null;
  /** body 渲染；不实现则用 GenericJsonBody */
  body?(item: DynamicToolCall): ReactNode;
}

const DYNAMIC_RENDERERS: Record<string, DynamicToolRenderer> = {
  read:       { summarize: ReadSummarizer },
  write:      { summarize: WriteSummarizer },
  grep:       { summarize: GrepSummarizer },
  glob:       { summarize: GlobSummarizer },
  webfetch:   { summarize: WebFetchSummarizer },
  websearch:  { summarize: WebSearchSummarizer },
  todowrite:  { summarize: TodoWriteSummarizer, body: TodoWriteBody },
  askuserquestion: { summarize: AskUserQuestionSummarizer, body: AskUserQuestionBody },
};

function renderDynamicToolCall(item: DynamicToolCall) {
  const key = item.tool.toLowerCase();
  const renderer = DYNAMIC_RENDERERS[key];
  const title = renderer?.summarize(item)
    ?? defaultDynamicSummary(item);   // {namespace}/{tool}
  const body = renderer?.body?.(item)
    ?? <GenericJsonBody arguments={item.arguments} contentItems={item.contentItems} />;
  return { title, body };
}
```

### 3.5 GenericJsonBody / JsonTree

```typescript
interface JsonTreeProps {
  data: unknown;
  /** 默认展开层数；0 = 全折叠 */
  defaultDepth?: number;
}
```

实现要点：
- 标量：直接显示 (string 用引号包，超长截断 + tooltip)
- 对象：`{ N keys }` 折叠占位；展开后逐 key 缩进显示
- 数组：`[ N items ]` 折叠占位；展开后按索引列
- "复制"按钮：复制原始 `JSON.stringify(value, null, 2)`
- 自适应：value 中含 `unified_diff` / `path` 等已知 key 时，可走特化展开（后续优化点；本期先走通用）

GenericJsonBody 简单包：
```tsx
<div className="space-y-2">
  <Section label="入参"><JsonTree data={item.arguments} defaultDepth={2} /></Section>
  {item.contentItems && (
    <Section label="出参"><JsonTree data={item.contentItems} defaultDepth={1} /></Section>
  )}
</div>
```

### 3.6 FileChangeCardBody（差异化示例）

```tsx
function FileChangeCardBody({ item }: { item: FileChange }) {
  return (
    <div className="space-y-2">
      {item.changes.map((c) => (
        <FileChangeBlock key={c.path} change={c} />
      ))}
    </div>
  );
}

function FileChangeBlock({ change }: { change: FileUpdateChange }) {
  const [expanded, setExpanded] = useState(false);
  const { added, removed } = useMemo(() => countDiffLines(change.diff), [change.diff]);
  return (
    <div className="rounded border border-border">
      <button onClick={() => setExpanded(v => !v)} className="…">
        <span className="font-mono text-xs">{change.path}</span>
        <span className="text-xs text-success">+{added}</span>
        <span className="text-xs text-destructive">-{removed}</span>
      </button>
      {expanded && (
        <pre className="agentdash-chat-code-block max-h-96">{change.diff}</pre>
      )}
    </div>
  );
}
```

`countDiffLines` 解析 unified diff 的 `+` / `-` 行（排除 `+++`/`---` 头）。

### 3.7 各 summarizer 摘要规则

| Renderer | 摘要规则 | arguments 字段来源 |
|---------|---------|-------------------|
| Read    | `Read {path}{:offset–offset+limit}?` | `path`, `offset`, `limit` |
| Write   | `Write {path}（{lineCount} 行）` | `file_path`, `content` |
| Grep    | `Grep "{pattern}"{ in {path/glob}}?` | `pattern`, `path`/`glob` |
| Glob    | `Glob {pattern}` | `pattern` |
| WebFetch| `Fetch {url}` | `url` |
| WebSearch | `Search "{query}"` | `query` |
| TodoWrite | `更新 {N} 项 todo` | `todos.length` |
| AskUserQuestion | `提问 {questions[0].question}{ (+{N-1})}` | `questions[]` |
| Read（fileChange）  | header 不进二级，直接走 FileChangeCardBody 的 path 摘要 | — |
| 默认 dynamic        | `{namespace}/{tool}` 或 `{tool}` | — |

所有摘要要做长度截断（>~80 字符尾部 `…`），保留 tooltip 显示完整。

### 3.8 kind 注册表（threadItemKind.ts）

```typescript
export type ThreadItemKind =
  | "execute" | "edit" | "read" | "search" | "fetch"
  | "image" | "collab" | "context" | "tool" | "mcp" | "other";

interface KindMeta {
  kind: ThreadItemKind;
  badge: string;        // "RUN" / "EDIT" / "READ" / ...
  label: string;        // 中文 label
}

export const KIND_REGISTRY: Record<ThreadItemKind, KindMeta> = { ... };

export function resolveKind(item: ThreadItem): KindMeta {
  switch (item.type) {
    case "commandExecution": return KIND_REGISTRY.execute;
    case "fileChange":       return KIND_REGISTRY.edit;
    case "mcpToolCall":      return KIND_REGISTRY.mcp;
    case "webSearch":        return KIND_REGISTRY.search;
    case "imageView":
    case "imageGeneration":  return KIND_REGISTRY.image;
    case "collabAgentToolCall": return KIND_REGISTRY.collab;
    case "contextCompaction":   return KIND_REGISTRY.context;
    case "dynamicToolCall":  return resolveDynamicKind(item.tool);  // Read/Grep/Glob → read/search
    default: return KIND_REGISTRY.other;
  }
}

function resolveDynamicKind(tool: string): KindMeta {
  switch (tool.toLowerCase()) {
    case "read":      return KIND_REGISTRY.read;
    case "write":     return KIND_REGISTRY.edit;
    case "grep":
    case "glob":
    case "websearch": return KIND_REGISTRY.search;
    case "webfetch":  return KIND_REGISTRY.fetch;
    default:          return KIND_REGISTRY.tool;
  }
}
```

`buildKindSummary`、shell badge、SessionEntry 都从这里取，不再各自维护映射。

## 4. 兼容性与回滚

### 4.1 部署时序

项目当前未上线，P3 不按兼容 rollout 设计。实施顺序直接以当前后端基线为准：

1. 后端已经产出 Backbone/Codex `ThreadItem`。
2. P3 建立前端 shell 与 registry，先保持视觉平移。
3. P4 逐个 renderer 替换 body 与摘要。

### 4.2 回滚

P3/P4 每个阶段保持单 commit 可回退。回退单位是前端 renderer 或 shell
重构提交，不引入运行时 feature flag。

## 5. Open Questions

- **OQ1**: P3 的 `ToolCallCardShell` 是否保留旧 `SessionToolCallCard` 作为
  临时 `LegacyDetailView`，还是直接把现有 detail 内容迁入 registry body。
- **OQ2**: GenericJsonBody 的复制按钮粒度是分区复制还是整条 item 复制；优先保证
  展开后完整原始值可见。
