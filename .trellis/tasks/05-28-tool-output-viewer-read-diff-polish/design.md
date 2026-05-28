# Design — 工具出参统一渲染与 Read/Diff 面板增强

## Current Data Flow

```text
Backbone item_completed
  -> AgentDashThreadItem
  -> renderToolCallCard(...)
  -> body renderer
      commandExecution       -> CommandExecutionCardBody
      fsRead / dynamic read  -> ReadCardBody
      dynamic edit/write     -> DiffCardBodyAuto
      fsGrep / fsGlob / MCP  -> GenericJsonBody / McpCardBody
```

当前缺口集中在 body renderer：普通 text 输出仍以 transport JSON 展示；Read 和 Diff 的专用面板还需要解析与 polish。

## Component Boundaries

新增组件建议：

```text
packages/app-web/src/features/session/ui/bodies/
  ToolOutputContentViewer.tsx
  toolOutputContent.ts
  readPayload.ts              # 可选：从 ReadCardBody 拆出 parser
```

保留并增强：

```text
ReadCardBody.tsx
DiffCardBody.tsx
diffPayload.ts
GenericJsonBody.tsx
McpCardBody.tsx
DynamicToolCallCardBody.tsx
FileChangeCardBody.tsx
```

`ToolOutputContentViewer` 只负责“输出内容”展示，不负责工具入参、工具状态、header、聚合。

## Output Normalization

定义一个 UI 内部 view model，例如：

```ts
type ToolOutputBlock =
  | { kind: "text"; text: string; source?: "content_item" | "mcp" }
  | { kind: "image"; imageUrl: string; mimeType?: string | null; label?: string }
  | { kind: "resource"; uri: string; label?: string; text?: string; mimeType?: string | null }
  | { kind: "json"; value: unknown; label?: string };
```

入口函数：

```ts
normalizeDynamicOutput(items: DynamicToolCallOutputContentItem[] | null): ToolOutputBlock[]
normalizeMcpOutput(content: JsonValue[] | null | undefined): ToolOutputBlock[]
```

规则：

- `inputText` -> `text`
- `inputImage` -> `image`
- MCP `{ type: "text", text }` -> `text`
- MCP `{ type: "image", data, mimeType }` -> `image`，拼成 data URL
- MCP `{ type: "resource_link", uri, name }` -> `resource`
- MCP `{ type: "resource", resource: { uri, text } }` -> `resource` 或 `text + resource metadata`
- 其它 -> `json`

`ToolOutputContentViewer` 渲染：

- 连续 text block 可以拼接为一个 text panel，中间用 `\n` 或空行分隔。
- image block 单独显示图片预览。
- json block 使用现有 `JsonTree`。
- resource block 使用轻量资源卡；有 text 时可展开 text。

## Read Parser

Read 工具返回文本的常见形态：

```text
file: path/to/file.ts
   1 | first line
   2 | second line
```

解析函数建议：

```ts
interface ParsedReadOutput {
  filePath?: string;
  lines: Array<{ lineNo: number; text: string }>;
  bodyText: string;
  rawText: string;
  parsedLineNumbers: boolean;
}

parseReadToolText(rawText: string, fallbackStartLine: number): ParsedReadOutput
```

算法：

1. 将 `rawText` 按行拆分。
2. 如果首个非空行匹配 `^file:\s*(.+)$`，记录 `filePath` 并从正文候选中移除。
3. 对正文候选逐行尝试匹配 `^\s*(\d+)\s*\|\s?(.*)$`。
4. 如果有足够证据说明这是行号格式（例如连续两行匹配，或全部非空行大多匹配），使用捕获到的行号与文本。
5. 不匹配的空行可作为空文本行保留；不匹配的非空行在解析模式下保守保留原文，避免丢内容。
6. 如果不满足行号格式，使用 fallbackStartLine 顺序编号。

展示：

- 顶部 metadata 显示 `filePath`、总行数。
- 代码区使用 parsed `lineNo` 和 `text`。
- 复制正文使用 `bodyText`；如果提供复制原文，用 `rawText`。

## Diff Renderer

`DiffCardBody` 已存在，增强重点：

- `DiffCardBodyAuto` 继续作为统一入口。
- `parseUnifiedDiff` 需要稳定处理：
  - `---` / `+++` meta 行
  - `@@ ... @@` hunk 行
  - `+` add / `-` remove / context
  - 无 hunk 的纯 `+/-` 文本
- `synthesizeFromOldNew` 保持简单替换 diff，不需要 LCS。
- toolbar 增加复制 diff。
- FileChange 多文件继续由 `FileChangeCardBody` 分文件组织，每个文件内嵌 `DiffCardBody`。

## GenericJsonBody And MCP

`GenericJsonBody` 建议改为：

```text
入参：JsonTree + CopyJsonButton
出参：ToolOutputContentViewer
未知结构化出参：ToolOutputContentViewer 内部 JSON fallback
```

`McpCardBody` 建议改为：

```text
入参：JsonTree
出参 content：ToolOutputContentViewer(normalizeMcpOutput(result.content))
structuredContent：JsonTree（存在时）
_meta：JsonTree（存在时）
error：现有错误块
```

这样普通 text MCP 不再被 JSON 树淹没，同时结构化结果仍保留调试能力。

## Testing Strategy

Focused tests 足够：

- `toolOutputContent.test.ts`：dynamic `inputText` / `inputImage`、MCP text/image/resource/json normalize。
- `ReadCardBody` parser test：`file:` + `N |`、无 file 标头、有行号、无行号 fallback。
- `diffPayload.test.ts`：unified diff、old/new 合成、空 diff。
- 组件 render test 可少量覆盖：GenericJsonBody 出参不再出现 `inputText` 字面 JSON，而出现 text 内容。

## Rollback

所有改动都在前端 session UI body 层；如有问题可以回退到 `GenericJsonBody` JSON fallback，不影响协议、后端或会话聚合。
