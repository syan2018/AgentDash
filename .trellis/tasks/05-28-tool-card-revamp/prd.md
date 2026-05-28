# 工具卡片体验整体重做

## Goal

会话流里的工具调用卡片当前体验有三个相互独立的问题，本任务一次解决：

1. **工具组自动合并失效** —— `mounts_list`/`Read`/`canvas_start` 这种连续工具序列，即使中间没有任何 agent message / approval 等真正的"用户可见边界"，也被 CTX context_frame 之类的"幽灵 boundary"打散，每个工具独立显示。
2. **Read / Edit 无专用渲染** —— Read 类工具（`fsRead` 与 `dynamicToolCall(read)`）只走 `GenericJsonBody` 把 `contentItems` 当 JSON 树展示；Edit 类工具走 `dynamicToolCall(edit/str_replace_editor/applypatch)` 时连 diff 都没有，纯字段打印。
3. **卡片标题样式粗糙** —— `Read /a/very/long/path` 路径尾部截断丢文件名、副标题与 badge 语义重复、无文件 chip / icon、无视觉分层。

并且现有 `ToolCallCardShell` 已经是一个壳，但标题部分各 renderer 各自硬编码 `code/string/path` 不统一。本任务顺手把"工具卡片标题区"抽成一个统一的可复用组件。

## Requirements

### R1 · 修复跨轮合并

**根因**：[useSessionFeed.ts `classifyEntry`](packages/app-web/src/features/session/model/useSessionFeed.ts#L154-L159) 把 `context_frame`、可渲染 platform 系统事件统统归为 `visible_boundary` —— 一旦碰到就 `flushToolGroup()`。但这些事件在 UI 上要么并入 CTX side group、要么 `parseContextFrame` 失败渲染 null，**幽灵 boundary 让工具组被拆，肉眼却看不到原因**。

- R1.1 `context_frame` 事件**不再**是 tool-burst 的硬边界。它要么走独立 side group 但不 flush tool group，要么直接降级为 neutral（具体方案见 design.md）。
- R1.2 任何"分类成 visible_boundary 但渲染时返回 null"的 entry 不能继续作为 boundary —— 引入"实际可见性"的判定，或在分类阶段对齐渲染规则。
- R1.3 `agent_message_delta` 的 boundary 行为保留现状（非空文本 = boundary），不在本任务调整。
- R1.4 工具组合并的核心 invariant：**没有"真正肉眼可见的非工具内容"出现时，连续 tool_like entries 必须合并为一个 burst。**

### R2 · 统一卡片 shell（标题模板）

- R2.1 抽出 `ToolCardHeader` 通用模板，承接当前 `ToolCallCardShell` 内 button 的 header 渲染逻辑。
- R2.2 标题不再是 `string | ReactNode` 自由格式，而是结构化 props：
  ```
  { kind, primary: 动词/工具名, file?: { path, range? }, badges?, suffix? }
  ```
  允许 renderer 提供主标签（如命令/MCP `server/tool`）+ 可选 file pill + 可选行范围 + 可选后缀。
- R2.3 删除现有 header 中与 badge 重复的副标题 `kind.label`（"读取"/"编辑"），副标题改用更有信息量的内容（cwd / range / args 摘要等）或省略。
- R2.4 文件路径渲染抽出 `FilePathPill`：basename 主显，父目录置灰小字 + 中段省略，**绝不在尾部截断丢失文件名**。
- R2.5 现有 commandExecution / mcp / search / fileChange / dynamic / fs* 全部迁移到新 header。

### R3 · Read 专用 body

- R3.1 新增 `ReadCardBody`（替代 `fsRead` 与 `dynamicToolCall(read)` 的 `GenericJsonBody` 兜底）。
- R3.2 从 `contentItems`（或 dynamicToolCall 的 result）抽取文本内容，按扩展名做基础语法着色（复用项目内已有的 highlighter 或 prism 都行）。
- R3.3 行号从 `offset` 起算（fsRead 有原生字段；dynamic read 从 `arguments.offset` 取）。
- R3.4 默认折叠到 ~24 行预览，"展开全部"按钮；超长内容必须能展开到全文，不要靠 max-h overflow 被吞。
- R3.5 footer 显示读取行数 / 字节数等元信息（如可获取）。

### R4 · Edit 专用 body（diff renderer）

- R4.1 新增 `DiffCardBody`，让 `fileChange` 与 `dynamicToolCall(edit/str_replace_editor/applypatch/write)` 都走它。
- R4.2 数据接入：
  - `fileChange` 直接用 `change.diff`。
  - `dynamicToolCall(edit/str_replace_editor)` 从 `arguments.old_string` / `new_string` 合成 unified diff。
  - `dynamicToolCall(applypatch)` 直接用 `arguments.patch`（如已是 diff 文本）。
  - `dynamicToolCall(write)` 视为整体 add（无 old），渲染所有行为 `+`。
- R4.3 Diff 渲染：每行 +/- 着色（复用 success / destructive token）、行号双列、连续 hunk 头加分隔。
- R4.4 总览：`+N -M` 行数计数；超长 diff 默认折叠（>40 行），"展开全部"。
- R4.5 多文件场景（fileChange 有多 changes）保留现有"按文件折叠 + 文件 pill"结构，每个文件 body 即新的 DiffCardBody。

### R5 · 不在范围

- 不调整 ThreadItem schema / 后端协议。
- 不引入大型 diff 库（如 `react-diff-viewer`），自实现 unified diff 渲染足够。
- 不动思考组（thinking group）渲染。
- 不动审批 / 错误 / 计划等非工具卡片。

## Acceptance Criteria

- [ ] **AC1**：在你截图同形态的会话（mounts_list → CTX → Read → canvas_start）下，工具被合并为一个 tool burst，CTX 不再充当幽灵 boundary。
- [ ] **AC2**：模拟"context_frame 渲染返回 null"的场景，`useSessionFeed.test.ts` 新增单测覆盖：CTX 不影响前后 tool burst 合并；连续 tool_like + 多种 platform 事件混合时合并行为符合 R1.4 invariant。
- [ ] **AC3**：所有工具卡片的 header 由统一 `ToolCardHeader` 渲染；`ToolCallCardShell` 不再直接接收 `title: ReactNode`，改为结构化 props。
- [ ] **AC4**：长路径（>60 字符）渲染时 basename 完整可见，不再尾部截断。删除 `kind.label` 重复副标题。
- [ ] **AC5**：fsRead 与 dynamicToolCall(read) 在浏览器里展开后看到行号 + 语法高亮 + 折叠预览，而不是 JSON 树。
- [ ] **AC6**：fileChange 与 dynamicToolCall(edit/str_replace_editor/applypatch/write) 在浏览器里展开后看到 +/- 着色的 diff，而不是字符串字段或纯文本块。
- [ ] **AC7**：浏览器手测一遍真实会话场景，确认改造不影响审批 / 失败 / 取消 等已有交互；现有 `useSessionFeed.test.ts` 单测继续全绿。
- [ ] **AC8**：`pnpm -w lint` / `pnpm -w typecheck` / `pnpm -w test --filter @agentdash/app-web` 通过。

## Notes

- 工作量目标"整体不大"——所有改动集中在 `packages/app-web/src/features/session/`，不跨包。
- Read/Edit body 是**新建文件**而非改造 GenericJsonBody；GenericJsonBody 保留作为 dynamic 兜底。
- 任何 ThreadItem schema 字段疑问以 `packages/app-web/src/generated/backbone-protocol` 为准。
