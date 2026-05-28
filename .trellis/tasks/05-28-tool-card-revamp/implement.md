# Implement — 工具卡片体验整体重做

## 执行清单（顺序执行，每步独立 commit）

### Step 1 · 修复跨轮合并（R1）

文件：
- `packages/app-web/src/features/session/model/useSessionFeed.ts`
- `packages/app-web/src/features/session/model/useSessionFeed.test.ts`

变更：
1. `classifyEntry` 引入 `soft_boundary` 第四类，返回类型改为 `"tool_like" | "hard_boundary" | "soft_boundary" | "neutral"`（原 `visible_boundary` 等同于 `hard_boundary`，重命名）。
2. 把以下事件从 `visible_boundary` 改为 `soft_boundary`：
   - `isContextFrameEvent(entry.event)`
3. `agent_message_delta`/`reasoning_*` 非空文本仍然 `hard_boundary`（保持现状）。
4. `aggregateEntries` 处理 `soft_boundary`：**不调用 flushToolGroup**，仅参与 side group：
   ```ts
   case "soft_boundary": {
     const sideKind = getSideGroupKind(entry);
     if (!sideKind) break;          // 防御：分类与 side group kind 不匹配则忽略
     if (activeSideGroup && sideGroupMatchesKind(activeSideGroup, sideKind)) {
       activeSideGroup.entries.push(entry);
     } else {
       flushSideGroup();
       activeSideGroup = createSideGroup(sideKind, entry);
     }
     break;
   }
   ```
5. `hard_boundary` 路径保持现行 `visible_boundary` 逻辑（flushToolGroup + side group 收纳 OR 直接 push）。
6. 单测：
   - 新增 T15 `[cmd_a, ctx, cmd_b]` 期望 `result.length === 2`：`AggregatedContextFrameGroup` + `AggregatedEntryGroup([cmd_a, cmd_b])`。具体顺序按实现产出，断言"工具组包含两条且 burst 仅一个"。
   - 新增 T16 `[cmd_a, msg("非空"), cmd_b]` 仍然分裂为 3 项。
   - 新增 T17 `[cmd_a, ctx, ctx, cmd_b]` —— burst 一个，CTX 合并为一个 side group。
   - 新增 T18 mixed: `[cmd_a, ctx, msg("非空"), cmd_b]` 应该分裂（hard boundary 优先）。
   - 跑 `pnpm -w test --filter @agentdash/app-web -- useSessionFeed`，确保 T1–T14 + T15–T18 全绿。

验证：`pnpm -w typecheck` + `pnpm -w lint --filter @agentdash/app-web` 通过。

**Commit 1**：`fix(session): context_frame 不再打散 tool burst`

### Step 2 · 抽 ToolCardHeader / 改 shell props（R2）

文件：
- 新建 `packages/app-web/src/features/session/ui/ToolCardHeader.tsx`
- 新建 `packages/app-web/src/features/session/ui/FilePathPill.tsx`
- 改 `packages/app-web/src/features/session/ui/ToolCallCardShell.tsx`
- 改 `packages/app-web/src/features/session/ui/toolCardRegistry.ts`
- 改 `packages/app-web/src/features/session/ui/SessionEntry.tsx`（如需调用 `header` 替代 `title`）

变更：
1. `FilePathPill`（通用 React 组件，**不**进 header model）：
   ```tsx
   export function FilePathPill({
     path,
     range,
   }: { path: string; range?: { from: number; to: number } | null }) {
     const { dir, base } = splitPath(path);  // 简单 helper：最后一个 "/" 或 "\\" 切分
     return (
       <span className="inline-flex min-w-0 items-baseline gap-1 font-mono">
         {dir && (
           <span
             className="truncate text-muted-foreground/60"
             dir="rtl"
             style={{ direction: "rtl", textAlign: "left", maxWidth: "240px" }}
           >
             {dir}/
           </span>
         )}
         <span className="truncate text-foreground">{base}</span>
         {range && (
           <span className="shrink-0 tabular-nums text-muted-foreground/60">
             L{range.from}-{range.to}
           </span>
         )}
       </span>
     );
   }
   ```
   - 父目录 `dir="rtl"` 让省略发生在前缀；basename 永远完整显示。
   - 不带 icon —— 当前项目不强制走 emoji，保持简洁。
2. `ToolCardHeader` 组件接收 `kind` + `ToolCardHeaderModel`，渲染 badge + primary（ReactNode 自由）+ secondary（灰色小字第二行）。
3. `ToolCallCardShellProps` 替换：
   - 删除 `title: ReactNode`。
   - 新增 `header: ToolCardHeaderModel`。
   - 删除 `kind.label` 重复副标题（不渲染 `<p>{kind.label}</p>`）。
4. **header model 极简：仅 `primary` + 可选 `secondary`，二者皆 ReactNode。各 renderer 自由构造。** 各分支迁移：
   - `commandExecution` → `{ primary: <code className="font-mono text-sm">{command}</code>, secondary: cwd ? \`cwd: ${cwd}\` : undefined }`
   - `fileChange` → `{ primary: <FilePathPill path={changes[0]?.path} />, secondary: \`${n > 1 ? \`+${n-1} 文件 · \` : ''}+${added} -${removed}\` }`
   - `mcpToolCall` → `{ primary: \`${server}/${tool}\`, secondary: summarizeArgs(arguments) }`
   - `webSearch` → `{ primary: <code>"{query}"</code> }`
   - `imageView` → `{ primary: <FilePathPill path={path} /> }`
   - `imageGeneration` → `{ primary: "图片生成" }`
   - `collabAgentToolCall` → `{ primary: \`${tool}\`, secondary: "协作 agent" }`
   - `contextCompaction` → `{ primary: "上下文压缩" }`
   - `dynamicToolCall` → `getDynamicToolHeader(item)`
   - `fsRead` → `{ primary: <FilePathPill path={item.path} range={rangeOf(offset, limit)} /> }`
   - `fsGrep` → `{ primary: <code>"{pattern}"</code>, secondary: target ? \`in ${target}\` : undefined }`
   - `fsGlob` → `{ primary: <code>{pattern}</code> }`
5. **MCP badge 改走 TOOL**：`packages/app-web/src/features/session/model/threadItemKind.ts` 的 `resolveKind` 中 `case "mcpToolCall": return KIND_REGISTRY.mcp;` 改成 `return KIND_REGISTRY.tool;`。`KIND_REGISTRY.mcp` 条目暂保留（避免影响 summary 路径），后续单独清理。
6. `getDynamicToolTitle` 重写为 `getDynamicToolHeader`（返回 `ToolCardHeaderModel`）：
   - `read` → `{ primary: <FilePathPill path={args.path} range={rangeOf(args.offset, args.limit)} /> }`
   - `write` / `edit` / `str_replace_editor` / `applypatch` → `{ primary: <FilePathPill path={args.file_path ?? args.path} /> }`
   - `grep` → `{ primary: <code>"{args.pattern}"</code>, secondary: \`in ${args.path ?? args.glob}\` }`
   - `glob` → `{ primary: <code>{args.pattern}</code> }`
   - `websearch` → `{ primary: <code>"{args.query}"</code> }`
   - `webfetch` → `{ primary: args.url }`
   - `todowrite` → `{ primary: \`更新 ${count} 项 todo\` }`
   - `askquestion`/`askuserquestion` → `{ primary: firstQuestionText, secondary: count > 1 ? \`+${count - 1}\` : undefined }`
   - 默认 → `{ primary: namespace ? \`${namespace}/${tool}\` : tool, secondary: summarizeArgs(arguments) }`
6. `summarizeArgs(args)`：取入参第 1-2 个有意义字段，`key: shortValue` 格式拼接，长度限到 ~80 字。是 dynamic 兜底场景与 mcp 的副标题来源，统一抽到 `bodies/argSummary.ts`。
7. `ToolCallCardShell` 内部把 button 区域的 title 渲染改为 `<ToolCardHeader ... />`。
8. `SessionEntry.tsx::SingleEntry` 中 `item_started/item_completed` 分支把 `card.title` 替换为 `card.header`，传给 shell。
9. **彻底删掉**：
   - `ToolCallCardShell` 里 `<p className="text-xs text-muted-foreground">{kind.label}</p>` 这一行
   - `truncate(path, 60)` 在 dynamic / fsRead / fsGrep 标题中的字符层截断（路径交给 FilePathPill 中段省略）
   - 各分支 primary 中 "Read"/"Edit"/"Search" 等与 badge 重复的 verb 前缀

验证：浏览器手测一遍 commandExecution / fsRead / fileChange / dynamic edit 等渲染没塌。`pnpm -w lint && pnpm -w typecheck`。

**Commit 2**：`refactor(session): 抽 ToolCardHeader 统一卡片标题区`

### Step 3 · ReadCardBody（R3）

文件：
- 新建 `packages/app-web/src/features/session/ui/bodies/ReadCardBody.tsx`
- 新建 `packages/app-web/src/features/session/ui/bodies/readPayload.ts`（normalize 函数）
- 改 `packages/app-web/src/features/session/ui/toolCardRegistry.ts`：fsRead + dynamicToolCall(read) 改用 ReadCardBody

变更：
1. `readPayload.ts` 实现 `normalizeReadItem(item: AgentDashThreadItem): ReadPayload | null`：
   - 抽 `text` 自 `item.contentItems`（type=text 节拼接）或 dynamic 的 result。
   - 推断 `language` 自 `path` 扩展名（最简映射表：`.ts/.tsx -> typescript, .js/.jsx -> javascript, .py -> python, .json -> json, .md -> markdown, .sh -> shell, default -> plaintext`）。
   - `startLine` 自 `offset ?? 1`；`totalLines = text.split("\n").length`。
2. `ReadCardBody`：
   - 默认 `expanded = false`，预览高度 24 行（`max-h-[24em]` 或 `max-h-96`）。
   - 顶 toolbar：`{totalLines} 行` + 复制按钮（沿用 `JsonTree.tsx::CopyJsonButton` 写法）。
   - 内容区：`<pre>` 双列 — 行号 col + 代码 col，`grid grid-cols-[auto_1fr]` 或 absolute positioning。
   - 简易 highlighter：MVP 用 `<code className={\`language-\${language}\`}>{text}</code>` + 现有项目 css class（如已配 prism）。**若项目尚无 highlighter，本期跳过着色，仅做行号+等宽体**——不阻塞主交付。
   - 折叠/展开切换。
3. `toolCardRegistry.ts`：
   - `case "fsRead"`：`body: <ReadCardBody item={item} />`
   - dynamic `read` 分支：在 dynamic 内 switch 出 read 用 `ReadCardBody`，其他保持 `DynamicToolCallCardBody`。
4. 单测：`ReadCardBody.test.tsx` 极简——给一段 6 行内容，渲染应该出现行号 1–6 + 文本。

验证：浏览器手测 Read 工具调用面板。

**Commit 3**：`feat(session): Read 类工具加专用预览面板`

### Step 4 · DiffCardBody（R4）

文件：
- 新建 `packages/app-web/src/features/session/ui/bodies/DiffCardBody.tsx`
- 新建 `packages/app-web/src/features/session/ui/bodies/diffPayload.ts`（合成 unified diff）
- 改 `packages/app-web/src/features/session/ui/bodies/FileChangeCardBody.tsx`（用 DiffCardBody 替换 `<pre>{diff}</pre>`）
- 改 `packages/app-web/src/features/session/ui/toolCardRegistry.ts`：dynamicToolCall(edit/str_replace_editor/applypatch/write) 改用 DiffCardBody

变更：
1. `diffPayload.ts`：
   - `parseUnifiedDiff(diff: string): DiffLine[]` —— 行级解析 +/- / context / `@@`。
   - `synthesizeFromOldNew(oldText: string, newText: string): string` —— 输出一个最简 unified：
     ```
     --- old
     +++ new
     @@
     -<old line 1>
     -<old line 2>
     +<new line 1>
     +<new line 2>
     ```
     不做按行 LCS，全部当替换；后续可以升级。
2. `DiffCardBody`：
   - props `{ payload: DiffPayload }`。
   - 渲染双列行号 + 内容；`+`/`-`/context 三种 row 样式。
   - top toolbar：`+N -M` + 复制 raw diff 按钮。
   - >40 行折叠。
3. `FileChangeCardBody.FileChangeBlock` 内 `<pre>{change.diff}</pre>` 替换为 `<DiffCardBody payload={parseUnifiedDiff(change.diff)} />`。
4. registry dynamic 分支：在已有 switch 中加分支：
   ```ts
   case "edit":
   case "str_replace_editor": {
     const oldText = str(args, "old_string") ?? "";
     const newText = str(args, "new_string") ?? "";
     return { ..., body: <DiffCardBody payload={fromOldNew(oldText, newText)} /> };
   }
   case "applypatch": {
     const patch = str(args, "patch") ?? "";
     return { ..., body: <DiffCardBody payload={parseUnifiedDiff(patch)} /> };
   }
   case "write": {
     const content = str(args, "content") ?? str(args, "new_string") ?? "";
     return { ..., body: <DiffCardBody payload={fromOldNew("", content)} /> };
   }
   ```
5. 单测：`DiffCardBody.test.tsx` 渲染一段 `+a\n-b` 应该出现 success 与 destructive 行；`diffPayload.test.ts` 覆盖 `synthesizeFromOldNew` 与 `parseUnifiedDiff` 基本路径。

验证：浏览器手测 fileChange + dynamic edit / applypatch / write。

**Commit 4**：`feat(session): Edit/Write/applypatch 走统一 diff 渲染`

### Step 5 · 清理与 polish

- 删除 `ToolCallCardShell` 里弃用的 `title` 相关代码（如有遗留 import）。
- `truncate` 工具函数若不再被使用就删除。
- `kind.label` 字段如果 KIND_REGISTRY 完全无人消费，保留（聚合摘要还在用 `summaryVerb / summaryUnit`，label 仍可能用于 a11y）—— 不要清得过激。
- 整体过一遍浏览器，确认审批 / 失败 / 取消行为不受影响。
- 跑 `pnpm -w lint && pnpm -w typecheck && pnpm -w test --filter @agentdash/app-web`。

**Commit 5**：`chore(session): 工具卡片改造收尾清理`

## Validation 命令

```bash
# 单测（含新增）
pnpm -w test --filter @agentdash/app-web -- useSessionFeed
pnpm -w test --filter @agentdash/app-web

# 类型 + 风格
pnpm -w typecheck
pnpm -w lint --filter @agentdash/app-web

# 浏览器手测：启动开发环境，发起一次 read/edit 序列，确认 P0~P3 改造可见
```

## Review Gates

- Step 1 完成后：单测产出可独立 review，确认幽灵 boundary 修复方向无误。
- Step 2 完成后：浏览器一眼能看出标题样式翻新；停一下 review 视觉，确认无回归。
- Step 3/4：每个 body 改完都浏览器复现一次目标体验。
- Step 5：整体回归 + 提交。

## Rollback Points

- 每个 commit 即一个 rollback point。最危险的是 Step 1（动核心聚合），可以用 `git revert <commit-1>` 安全回退；Step 2-5 都是新增/局部改动，回退影响小。
