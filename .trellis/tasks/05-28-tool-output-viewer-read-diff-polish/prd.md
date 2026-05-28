# 工具出参统一渲染与 Read/Diff 面板增强

## Goal

让会话工具卡片展示“用户真正关心的工具输出”，而不是暴露 `contentItems` / MCP `content` 的传输 JSON 形态。

当前多数 PI_AGENT / native 工具出参已经归一为 `contentItems: [{ type: "inputText", text }]`，少量场景可能是 `inputImage`；MCP 工具则是 `result.content: JsonValue[]` 加 `structuredContent` / `_meta`。因此前端应提供统一的输出 viewer：文本输出直接用可复制、可折叠、可换行的文本框展示；图片输出可预览；结构化输出保留 JSON fallback。

同时补强两个专用面板：

- Read 返回文本目前常见形态是 `file: path\n   1 | ...`，现有 `ReadCardBody` 会把 `file:` 标头与行号前缀当作正文。需要解析出文件标头和行号，正文区域只显示文件内容。
- Diff 面板已有 `DiffCardBody` / `diffPayload.ts` 雏形，但需要确认 `fileChange` 与 `dynamicToolCall(edit/write/applypatch/str_replace_editor)` 都走专用 diff 渲染，并补齐复制、折叠、染色、空 diff 等体验。

## Confirmed Facts

- `packages/app-web/src/generated/backbone-protocol.ts` 中 `DynamicToolCallOutputContentItem` 只有：
  - `{ type: "inputText"; text: string }`
  - `{ type: "inputImage"; imageUrl: string }`
- `fsRead` / `fsGrep` / `fsGlob` / `dynamicToolCall` 都使用 `contentItems` 承载出参。
- `mcpToolCall.result` 形态是 `{ content: JsonValue[]; structuredContent: JsonValue | null; _meta: JsonValue | null }`。
- `DynamicToolCallCardBody` 当前已经把 `read` 分发到 `ReadCardBody`，把 `edit` / `str_replace_editor` / `write` / `applypatch` 分发到 `DiffCardBodyAuto`。
- `GenericJsonBody` 当前仍把 `contentItems` 作为 JSON 树展示；`McpCardBody` 也通过 `GenericJsonBody` 展示 `result.content`。
- `ReadCardBody` 当前只拼接 `inputText`，按 `offset` 起始行号重新编号，没有解析工具返回文本里的 `file: ...` 和 `N |` 前缀。

## Requirements

### R1 · 统一工具输出 viewer

- 新增统一输出组件，建议命名为 `ToolOutputContentViewer`。
- 支持 `DynamicToolCallOutputContentItem[]`：
  - `inputText`：拼接为文本输出，提供折叠预览、复制、行数展示、可换行等基础能力。
  - `inputImage`：显示图片预览；图片与文本混合时按原顺序展示。
- 支持 MCP `result.content: JsonValue[]`：
  - 识别 `{ type: "text", text }` / `{ type: "image", data, mimeType }` / `{ type: "resource" ... }` / `{ type: "resource_link" ... }` 等常见 MCP content block。
  - 可转成文本的块优先用文本 viewer 展示。
  - 不能安全识别的块保留 JSON fallback。
- `GenericJsonBody` 保留“入参 JSON”分区，但“出参”优先使用统一输出 viewer；只有结构化或未知输出才显示 JSON 树。
- `McpCardBody` 需要展示普通 content 输出，并在存在 `structuredContent` / `_meta` 时显示独立结构化分区。

### R2 · Read 返回文本解析与展示

- `ReadCardBody` 应解析如下常见返回文本：

  ```text
  file: packages/app-web/src/foo.ts
     1 | import ...
     2 | ...
  ```

- 文件标头拆出到内容面板顶部 metadata，不作为代码正文。
- 行号前缀拆掉，正文列只显示真实文件内容。
- 行号使用工具返回文本里的行号；解析失败时回退到现有 `offset ?? 1` 重新编号。
- 解析应允许空行、宽度不同的行号、无空格的 `1 |text` 形态。
- 多个 `inputText` 块拼接后再解析；如果没有 `file:` 标头但行号前缀明显存在，也应识别行号。
- 保留复制原文和复制正文的取舍：MVP 至少复制正文；如实现代价低，可同时提供“复制正文 / 复制原始输出”。

### R3 · Diff 专用面板补强

- `fileChange` 的每个 change 必须使用 `DiffCardBody` 或等价专用 diff renderer，不再裸 `<pre>`。
- `dynamicToolCall(edit | str_replace_editor)` 使用 `old_string` / `new_string` 合成 diff。
- `dynamicToolCall(write)` 视为整体新增内容。
- `dynamicToolCall(applypatch)` 使用 `patch` 字段直接解析。
- Diff 面板展示：
  - `+N -M` 统计。
  - 双列旧/新行号。
  - `+` / `-` / context / hunk header 差异染色。
  - 超长折叠与展开。
  - 复制 diff。
- 空 diff 或无法解析时要有稳定空态，不渲染破碎面板。

### R4 · 视觉与交互

- 普通文本输出不再以 JSON 树展示 `[{ type: "inputText", text: "..." }]`。
- 文本、Read、Diff 输出都要复用一致的 toolbar 语义：行数 / 字符数、复制、展开/折叠。
- 长文本不撑破卡片宽度；移动端和窄面板保持可读。
- 组件实现保持在 `packages/app-web/src/features/session/ui/bodies/` 附近，不引入新的全局状态。

## Out Of Scope

- 不修改后端协议或 generated TS 类型。
- 不引入大型 diff / syntax highlight 依赖；如需高亮，先使用轻量 class 或现有样式。
- 不重做工具卡片 header / 聚合逻辑。
- 不改变模型实际收到的 tool result 内容。

## Acceptance Criteria

- [ ] AC1：`fsGlob` / `fsGrep` / unknown `dynamicToolCall` 的文本出参显示为文本输出框，而不是 JSON 树。
- [ ] AC2：`inputImage` 出参至少能显示图片预览或明确附件提示；不会 JSON dump data URL。
- [ ] AC3：MCP `content` 中的 text block 使用文本 viewer；`structuredContent` / `_meta` 仍可查看。
- [ ] AC4：Read 返回 `file: ...` 时，文件路径显示在 metadata，正文不包含 `file:` 行。
- [ ] AC5：Read 返回 `N |` 行号前缀时，正文不包含 `N |`，UI 行号与原始返回行号一致。
- [ ] AC6：Read 解析失败时仍能稳定展示文本，不丢内容。
- [ ] AC7：`fileChange` 展开后看到专用 diff 面板，带 +/- 染色和双列行号。
- [ ] AC8：`dynamicToolCall(edit/write/applypatch/str_replace_editor)` 展开后看到专用 diff 面板。
- [ ] AC9：文本输出、Read、Diff 都支持折叠/展开和复制。
- [ ] AC10：补充 focused unit tests，覆盖 output viewer、Read 行号解析、Diff 基本解析。
- [ ] AC11：`pnpm --filter app-web run typecheck`、`pnpm --filter app-web lint`、`pnpm --filter app-web test` 通过。

## Notes For Implementer

- 当前任务是 `05-28-tool-card-revamp` 的子任务，目标是补齐出参 viewer / Read / Diff 的体验闭环。
- 用户偏好：项目还在预研期，不需要兼容旧实现；应收束到最正确、最清晰的前端结构。
