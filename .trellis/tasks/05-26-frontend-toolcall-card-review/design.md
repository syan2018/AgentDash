# 设计：工具调用卡片信息架构重构

## 1. 总体架构

### 1.1 双线分层

```
┌──────────────────────────────────────────────────────────────────┐
│ executors crate (vibe-kanban)                                    │
│   NormalizedEntry { entry_type: ToolUse { tool_name, action_type } } │
└──────────────────────┬───────────────────────────────────────────┘
                       │ ActionType (FileRead/FileEdit/CommandRun/Search/…)
                       ▼
┌──────────────────────────────────────────────────────────────────┐
│ agentdash-executor 适配器层                                       │
│   normalized_to_backbone.rs::tool_use_envelopes                  │
│   pi_agent/stream_mapper.rs                                      │
│   → 按 ActionType 分发到对应 ThreadItem variant ★ 新增           │
└──────────────────────┬───────────────────────────────────────────┘
                       │ ThreadItem (CommandExecution/FileChange/WebSearch/DynamicToolCall…)
                       ▼
┌──────────────────────────────────────────────────────────────────┐
│ backbone-protocol → 前端 generated/backbone-protocol.ts           │
└──────────────────────┬───────────────────────────────────────────┘
                       │ BackboneEvent::ItemStarted/ItemCompleted
                       ▼
┌──────────────────────────────────────────────────────────────────┐
│ 前端 SessionEntry → ToolCallCardShell (header + 折叠 + 审批)      │
│   → 一级分发 (ThreadItem.type)                                   │
│   → 二级分发 (dynamicToolCall 内按 tool 名)  ★ 新增              │
│   → renderer 注册表 / GenericJsonBody 兜底                       │
└──────────────────────────────────────────────────────────────────┘
```

### 1.2 数据契约边界

- `ActionType` 是上游 normalize 的语义类型，**不动**
- `ThreadItem` schema **不动**——这次只是把已有 variant 用起来
- `BackboneEvent` / 前端生成 `backbone-protocol.ts` **不动**
- 改动集中在两个位置：
  - 后端：`agentdash-executor` 内部分发逻辑
  - 前端：`packages/app-web/src/features/session/ui/` 与 `model/`

## 2. 后端：ActionType → ThreadItem 映射

### 2.1 映射函数位置

新增 `crates/agentdash-executor/src/adapters/threaditem_mapping.rs`，提供：

```rust
pub(crate) fn action_type_to_thread_item(
    action_type: &ActionType,
    tool_name: &str,
    tool_status: &ToolStatus,
    item_id: String,
    raw_content: &str,
) -> codex::ThreadItem
```

`tool_use_envelopes` 调用此函数替代原地的 `ThreadItem::DynamicToolCall { ... }`
直接构造。

### 2.2 各分支映射规则

```rust
match action_type {
    ActionType::CommandRun { command, result, category: _ } => {
        ThreadItem::CommandExecution {
            id: item_id,
            command: command.clone(),
            cwd: cwd_from_context_or_default(),  // 从 entry context 推导，缺省 "."
            status: command_status_from_tool_status(tool_status),
            exit_code: result.as_ref().and_then(|r| r.exit_code),
            // aggregated_output: result.as_ref().map(|r| r.output.clone()),
            // 其余字段按 ThreadItem::CommandExecution 当前 schema 取舍
            ...
        }
    }

    ActionType::FileEdit { path, changes } => {
        ThreadItem::FileChange {
            id: item_id,
            changes: changes
                .iter()
                .map(|fc| convert_file_change(path, fc))  // 把 executors::FileChange 转 codex::FileUpdateChange
                .collect(),
            status: patch_apply_status_from_tool_status(tool_status),
        }
    }

    ActionType::Search { query } => {
        ThreadItem::WebSearch {
            id: item_id,
            query: query.clone(),
            action: None,
        }
    }

    ActionType::TaskCreate { description, subagent_type, .. } => {
        ThreadItem::CollabAgentToolCall {
            id: item_id,
            tool: subagent_type.clone().unwrap_or_else(|| "task".into()),
            // 其余字段按 ThreadItem::CollabAgentToolCall 当前 schema 取舍
            ...
        }
    }

    // 没有专用 variant 的语义保持 DynamicToolCall，但保留 tool_name
    ActionType::FileRead { path } => dynamic_tool_call(item_id, "Read", json!({ "path": path }), tool_status),
    ActionType::WebFetch { url } => dynamic_tool_call(item_id, "WebFetch", json!({ "url": url }), tool_status),
    ActionType::AskUserQuestion { questions } => dynamic_tool_call(item_id, "AskUserQuestion", json!(questions), tool_status),
    ActionType::Other { description } => dynamic_tool_call(item_id, "Other", json!({ "description": description }), tool_status),

    // 通用 Tool 与 PlanPresentation/TodoManagement 保持原有路径
    ActionType::Tool { tool_name, arguments, result } => dynamic_tool_call(item_id, tool_name, ...),
    ActionType::PlanPresentation { .. } | ActionType::TodoManagement { .. } => unreachable!(),
}
```

### 2.3 cwd / exit_code 等缺失字段

- `ActionType::CommandRun` 没带 `cwd`。从 `NormalizedEntry` 上下文（如果有）推导，
  否则填 `"."`（与 `CommandExecutionCard` 当前接受的占位一致）
- `ActionType::CommandRun.result.output` 流式输出在当前 envelope 路径里要走
  `CommandExecutionOutputDeltaNotification`，本次先不接管，老路径继续工作
  （即 R1 仅产 `ThreadItem::CommandExecution` 的 started/completed envelope，
  output delta 仍然由其他路径推送，或在 completed 时一次性塞 `aggregatedOutput`）
- 状态转换辅助函数对每种 ThreadItem 子状态独立实现
  （`CommandExecutionStatus` / `PatchApplyStatus` / `DynamicToolCallStatus`），
  不复用同一个枚举

### 2.4 pi_agent/stream_mapper.rs

`pi_agent` 自己的工具语义不必走 `ActionType`——它是直接对接 pi 协议的 raw 流。
对常见 tool 名做硬编码白名单：

```rust
match tool_name.to_lowercase().as_str() {
    "bash" | "shell" | "execute_shell" => construct_command_execution(...),
    "edit" | "str_replace_editor" | "apply_patch" => construct_file_change(...),
    "websearch" | "search" => construct_web_search(...),
    _ => construct_dynamic_tool_call(...),  // 现有路径
}
```

不强求 pi_agent 与 normalized_to_backbone 完全对齐，至少 Bash/Edit/Search 三类
能命中专用 variant。

### 2.5 测试

- `crates/agentdash-executor/src/adapters/normalized_to_backbone.rs` 已有
  `convert_event_to_envelopes` 测试套（如未有则新增），覆盖：
  - `CommandRun` → `CommandExecution`
  - `FileEdit{Edit}` → `FileChange`
  - `Search` → `WebSearch`
  - `Tool { tool_name="Read" }` → `DynamicToolCall(tool="Read")`
  - 未知 `Other` → `DynamicToolCall(tool="Other")`
- `pi_agent/stream_mapper.rs` 已有 connector_tests，新增 1-2 个 case 验证白名单
- 一律不打断 application 层 / persistence 层已有的 ThreadItem variant 测试

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

后端先到位 → 旧前端：
- 旧前端会继续走 `SessionToolCallCard`，但 ThreadItem.type 多样化
  （commandExecution/fileChange/...），旧 SessionEntry 已有特例分支
  （commandExecution → CommandExecutionCard），新分发出来的 fileChange / webSearch
  会落到 SessionToolCallCard 通用路径——视觉一致，**功能不退化**

新前端先到位 → 旧后端：
- 新前端注册表里 `dynamicToolCall` 二级分发已经覆盖 Read/Grep/Edit/...
  常见工具，单凭 `tool` 名就能给出体面摘要——**视觉甚至比双线齐到还要前置生效**
- `commandExecution` / `fileChange` 等 renderer 暂时收不到流量，无害

### 4.2 回滚

- 后端：`tool_use_envelopes` 内部的 dispatch 函数加 feature flag
  `dispatch_thread_item_variants`（默认 on，回滚时 off 退回纯 DynamicToolCall）
- 前端：toolCardRegistry 与 dynamicToolRenderers 是新文件；shell 与旧
  `SessionToolCallCard` 暂时并存，SessionEntry 通过一个 `useNewToolCardLayout`
  常量切换；切换稳定后下一个 commit 再删旧组件

## 5. Open Questions

- **OQ1**: `ThreadItem::CommandExecution.cwd` 在 `ActionType::CommandRun` 没带 cwd
  时填什么？建议从 `NormalizedEntry` 关联的 session/turn metadata 取，否则填 "."；
  实施时验证一下 `NormalizedEntry` 是否带 cwd
- **OQ2**: `ThreadItem::FileChange` 的 schema 与 `executors::FileChange` 子枚举
  （Write/Delete/Rename/Edit）的字段对齐方式——具体到代码层面要把
  `FileChange::Write { content }` / `Delete` / `Rename { new_path }` 的语义如何
  映射到 codex `FileUpdateChange`，可能需要在 mapping 函数里造 unified_diff 字符串
- **OQ3**: 双线 feature flag 是否真的需要？如果 PR 切得足够小，每一步都能独立合并，
  可以省掉 flag。在 implement.md 里再决议
