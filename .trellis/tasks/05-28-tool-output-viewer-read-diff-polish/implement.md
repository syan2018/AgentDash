# Implement — 工具出参统一渲染与 Read/Diff 面板增强

## Step 0 · Baseline Check

- 读取相关文件：
  - `packages/app-web/src/features/session/ui/bodies/GenericJsonBody.tsx`
  - `packages/app-web/src/features/session/ui/bodies/ReadCardBody.tsx`
  - `packages/app-web/src/features/session/ui/bodies/DiffCardBody.tsx`
  - `packages/app-web/src/features/session/ui/bodies/diffPayload.ts`
  - `packages/app-web/src/features/session/ui/bodies/DynamicToolCallCardBody.tsx`
  - `packages/app-web/src/features/session/ui/bodies/FileChangeCardBody.tsx`
  - `packages/app-web/src/features/session/ui/bodies/McpCardBody.tsx`
  - `packages/app-web/src/generated/backbone-protocol.ts`
- 确认当前 `DynamicToolCallCardBody` 已经分发 read/edit/write/applypatch，避免重复重构。

## Step 1 · 新增统一输出 normalize 与 viewer

文件：

- 新建 `packages/app-web/src/features/session/ui/bodies/toolOutputContent.ts`
- 新建 `packages/app-web/src/features/session/ui/bodies/ToolOutputContentViewer.tsx`

实现：

- `normalizeDynamicOutput(items)`：
  - `inputText` -> text block
  - `inputImage` -> image block
- `normalizeMcpOutput(content)`：
  - 支持 text/image/resource/resource_link 常见块
  - 未知块转 json block
- `ToolOutputContentViewer`：
  - 拼接并渲染 text block
  - 支持复制、行数/字符数、折叠/展开
  - image block 显示图片
  - json block 用 `JsonTree`

验证：

- 添加 normalize 单测。
- 运行 `pnpm --filter app-web test -- toolOutputContent`。

## Step 2 · GenericJsonBody / MCP 接入 viewer

文件：

- `GenericJsonBody.tsx`
- `McpCardBody.tsx`

实现：

- `GenericJsonBody` 的入参仍用 `JsonTree`。
- `GenericJsonBody` 的出参改用 `ToolOutputContentViewer`，由调用方可传 dynamic 或 generic unknown；推荐先支持 `contentItems?: unknown` 并在组件内部识别。
- `McpCardBody` 不再只把 `result.content` 传给 `GenericJsonBody`：
  - arguments -> 入参 JSON
  - `result.content` -> `ToolOutputContentViewer`
  - `structuredContent` / `_meta` -> 单独 JSON section
  - error -> 保留现有错误块

验证：

- 补 render test：dynamic `[{type:"inputText", text:"hello"}]` 不显示 `inputText` JSON 字面量，显示 `hello`。
- 手看 fsGlob/fsGrep/MCP 普通 text 输出。

## Step 3 · Read parser 抽出并接入

文件：

- 可新建 `readPayload.ts` 或在 `ReadCardBody.tsx` 内拆 helper 后导出测试。
- `ReadCardBody.tsx`

实现：

- 实现 `parseReadToolText(rawText, fallbackStartLine)`。
- 解析 `file: path` 标头并拆到 metadata。
- 解析 `N | text` 行号前缀。
- 解析成功时正文不包含 `file:` 或 `N |`。
- 解析失败时保留现有 fallback 行号逻辑。
- Toolbar 至少提供复制正文；如果成本低，提供复制原始输出。

验证：

- parser 单测覆盖：
  - `file:` + `1 |`
  - 只有 `1 |`
  - 无行号文本 fallback
  - 空行与宽行号
- render test 或 snapshot 覆盖正文不包含 `file:`。

## Step 4 · Diff 面板补强和全路径确认

文件：

- `DiffCardBody.tsx`
- `diffPayload.ts`
- `FileChangeCardBody.tsx`
- `DynamicToolCallCardBody.tsx`

实现：

- 确认 `FileChangeCardBody` 每个 change 使用 `DiffCardBody`。
- 确认 dynamic `edit` / `str_replace_editor` / `write` / `applypatch` 都使用 `DiffCardBodyAuto`。
- 增加复制 diff。
- 确认 `+` / `-` / context / hunk / meta 行染色清晰。
- 处理空 diff：显示“无差异”。

验证：

- `diffPayload.test.ts` 覆盖 parse 和 synthesize。
- 组件测试覆盖 `+a` / `-b` 出现对应行。

## Step 5 · Polish And Regression

- 检查窄宽度下文本不撑破卡片。
- 检查大型输出折叠不会造成卡片过高。
- 检查 Read / Diff / Generic / MCP 的 copy 按钮语义一致。
- 删除不再需要的 JSON 出参展示分支或重复 helper。

## Validation Commands

```powershell
pnpm --filter app-web run typecheck
pnpm --filter app-web lint
pnpm --filter app-web test
```

Focused during development:

```powershell
pnpm --filter app-web test -- toolOutputContent
pnpm --filter app-web test -- ReadCardBody
pnpm --filter app-web test -- diffPayload
```

## Review Checklist

- 普通 `inputText` 出参不再显示为 JSON 树。
- Read 面板顶部显示 file metadata，代码正文干净。
- Diff 面板在 fileChange 与 dynamic edit/write/applypatch 中都可见。
- MCP text 输出可读，structuredContent 仍可检查。
- 没有修改 generated contract。
