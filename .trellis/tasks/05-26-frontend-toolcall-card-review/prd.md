# 前端工具调用卡片信息架构重构 review

## 背景与问题

当前会话流里，工具调用卡片在用户实际使用的 executor 链路下表现存在以下问题：

1. **卡片折叠态没有"请求摘要"**。`SessionToolCallCard` 折叠态 header 只展示
   `getThreadItemTitle(item)` 返回的简单字符串：`fileChange` 显示第一个文件路径、`mcpToolCall`
   显示 `server/tool`、`dynamicToolCall` 仅显示 `tool` 名。"读哪个文件的哪几行"、
   "Edit 加减多少行"、"Grep 什么 pattern"、"生图 prompt 是什么" 等关键参数全部
   藏在折叠区里——用户必须点开每一条才能知道 agent 在做什么。
   `CommandExecutionCard` 已经把 `$ command` 直接亮在 header，是体面对照，但只对
   `commandExecution` variant 生效。

2. **多数 ThreadItem variant 在当前链路下根本不命中**。验证发现两个 executor 适配
   `crates/agentdash-executor/src/adapters/normalized_to_backbone.rs:224` 和
   `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:219` 把所有
   工具调用统一编码为 `ThreadItem::DynamicToolCall`。
   `commandExecution` / `fileChange` / `mcpToolCall` / `webSearch` / `imageView` /
   `imageGeneration` / `collabAgentToolCall` 这些 variant 仅在 application 层与
   持久化层有处理路径，**当前 executor 不会向 backbone 发送它们**。
   因此用户可见的工具卡几乎只有"通用 dynamicToolCall 卡"一种形态，前端按
   `ThreadItem.type` 拆 renderer 这条线本身路由不到流量。

3. **upstream 已经 normalize 出语义类型，但被丢弃**。`vibe-kanban` 的
   `executors::logs::ActionType` 上游已经把工具语义分化成
   `FileRead { path }` / `FileEdit { path, changes }` / `CommandRun { command, result }` /
   `Search { query }` / `WebFetch { url }` / `Tool { tool_name, arguments, result }` /
   `TaskCreate { description, subagent_type }` / `AskUserQuestion` / `Other` 等。
   适配器没有把这些还原到 ThreadItem 的对应 variant，导致前端看不到分化，也违反了
   ThreadItem 自身的 variant 设计意图。

4. **前端胖卡 + JSON 平铺无可读性**。`SessionToolCallCard` 的 `extractDetailContent`
   把 `arguments` / `result` 整段 `JSON.stringify(..., null, 2)` 塞进 `<pre>`，
   `webSearch` / `imageView` / `imageGeneration` / `collabAgentToolCall` 没有
   detail 分支——展开后只剩一个 8 字符 ID 兜底；`fileChange` 把所有 diff 拼成大字符串
   塞进同一个 `<pre>`，没有按文件折叠、没有加减行计数。

5. **存在重复与死代码**。
   - `packages/app-web/src/components/acp/tool-call.tsx::ToolCallView` 仅在自身被
     grep 命中，是 ThreadItem 重构前的孤儿
   - `SessionToolCallCard.compact` 模式无任何调用点传 `compact={true}`
   - `extractDetailContent` 内 `commandExecution` 分支被 `SessionEntry` 路由分流，
     永远走不到
   - `kind` 字符串映射散落在 `types.ts::getThreadItemKind` /
     `SessionToolCallCard::getKindConfig` / `SessionEntry::buildKindSummary` 三处，
     新增工具种类需同步改三处

## Goal

让会话流的工具调用卡片在 **折叠态就传达"agent 正在干什么"**，并通过双线推进
（connector 语义还原 + 前端按语义分发渲染）解决根因。在不丢失任何调试信息的
前提下减少 UI 占位，清理孤儿与冗余代码。

## Requirements

### 后端 / connector 侧（双线之一）

R1. **`normalized_to_backbone.rs::tool_use_envelopes` 按 `ActionType` 分发到对应
    ThreadItem variant**。映射矩阵：

    | ActionType                  | ThreadItem variant        | 备注 |
    |-----------------------------|---------------------------|------|
    | `CommandRun`                | `CommandExecution`        | 把 `command` / `cwd` / `result.exit_code` / `result.output` 还原 |
    | `FileEdit`                  | `FileChange`              | `changes` 直接映射；status 由 ToolStatus 推导 |
    | `FileRead`                  | `DynamicToolCall`("Read") | ThreadItem 暂无 fileRead variant，保持 dynamic 但 tool 名规范化 |
    | `Search`                    | `WebSearch`               | `query` 透传；`action` 暂留 None |
    | `WebFetch`                  | `DynamicToolCall`("WebFetch") | 同上，无专用 variant |
    | `Tool`                      | `DynamicToolCall`         | 保持现状，`tool_name` / `arguments` / `result` 完整透传 |
    | `TaskCreate`                | `CollabAgentToolCall`     | `description` 作 prompt；`subagent_type` 作 sub agent name |
    | `AskUserQuestion`           | `DynamicToolCall`("AskUserQuestion") | 留待后续如需独立 variant 再升级 |
    | `Other`                     | `DynamicToolCall`("Other") | 显式兜底 |
    | `PlanPresentation` / `TodoManagement` | (已 SessionMetaUpdate) | 保持现状 |

R2. **`pi_agent/stream_mapper.rs` 同步对齐**。如果 pi backend 自己的工具语义来源
    无法对齐 `ActionType`，至少要把 Bash/Read/Edit/Grep/Glob 这些常见 tool 名
    转成对应 ThreadItem variant；其余继续用 `DynamicToolCall`。

R3. **不破坏 executor 已有契约**。R1/R2 不改 `ActionType` 上游、不改
    `ThreadItem` schema、不改前端生成的 `backbone-protocol.ts` 类型定义。
    仅调整适配器内部分发逻辑。

R4. **不影响 application 层与 persistence 层已有的 ThreadItem 处理路径**。
    `journey/tool_calls.rs` / `task/artifact.rs` / `session/continuation.rs` /
    `session_repository.rs` 现有 match arms 对新流量必须仍然正确。

### 前端侧（双线之二）

R5. **抽 `ToolCallCardShell`**。把 header（kind badge + title + status dot +
    时长 + 折叠箭头）、审批操作、错误展示、折叠容器从 `SessionToolCallCard` 内
    抽出，作为所有工具卡共享的外层。

R6. **建立按 `ThreadItem.type` 分发的 renderer 注册表**：
    - `commandExecution` → 复用 `CommandExecutionCard`（已体面，整合到 shell 形态）
    - `fileChange` → `FileChangeCardBody`：每个 change 一个折叠行，header 摘要
      `+N -M` 加减行数 + 文件名；展开按文件分块显示 unified diff（暂用 monospace
      `<pre>` 染色，语法高亮留作后续）
    - `mcpToolCall` → `McpCardBody`：header 摘要 `{server}/{tool}({key1=v1, …})`
      抽前 1-2 个标量参数；body 入参出参分区
    - `webSearch` → `WebSearchCardBody`：header 摘要 `搜索: "{query}"`（截断）
    - `imageView` / `imageGeneration` → `ImageCardBody`：缩略图 + path / prompt
    - `collabAgentToolCall` → `CollabAgentCardBody`：subAgent 名 + 任务首句
    - `contextCompaction` → 现有 SessionToolCallCard 兜底（已 acceptable）
    - `dynamicToolCall` → `DynamicToolCallCardBody`：按 `tool` 名做二级分发，
      命中已知工具走对应 summarizer（见 R7），未命中走 R8 通用兜底

R7. **`dynamicToolCall` 已知工具 summarizer**（仅 header 摘要，body 复用通用兜底
    或专用形态）：

    | tool 名（大小写无关） | header 摘要规则 |
    |--------------------|---------------|
    | `Read`             | `Read {path}{:line-range?}` |
    | `Write`            | `Write {path}（N 行）` |
    | `Grep`             | `Grep "{pattern}" in {path/glob?}` |
    | `Glob`             | `Glob {pattern}` |
    | `WebFetch`         | `Fetch {url}` |
    | `WebSearch`        | `Search "{query}"` |
    | `TodoWrite`        | `更新 N 项 todo` |
    | `AskUserQuestion`  | 首个 question |

R8. **GenericJsonBody 兜底**（针对未注册的 `dynamicToolCall.tool` 与未知
    `mcpToolCall`）：
    - "入参 / 出参" 双分区
    - 用可折叠 key-value 树渲染，默认展开顶层 2 层；叶子标量直接显示，对象/数组
      显示 `{N keys}` / `[N items]` 折叠摘要，点开再展开
    - 提供"复制原始 JSON"按钮
    - 不再把 `JSON.stringify(..., null, 2)` 整段塞 `<pre>`

R9. **kind 元数据集中**。新增 `packages/app-web/src/features/session/model/threadItemKind.ts`，
    单一注册表导出 `{ kind, icon, label }`，废弃 `getThreadItemKind` /
    `getKindConfig` / `buildKindSummary` 三处分散映射。

R10. **删除孤儿 / 死代码**：
    - 删 `packages/app-web/src/components/acp/tool-call.tsx`
    - 删 `SessionToolCallCard.compact` 模式分支与 `compact` prop（确认无外部使用后）
    - 删 `extractDetailContent` 内 `commandExecution` 分支
    - 经过 R6 的 shell 拆分后，老的胖 `SessionToolCallCard` 整体可下线

### 通用

R12. **不引入新的样式或视觉系统**。沿用现有 `border-border` / `bg-background` /
    `bg-secondary/...` / status `text-success/warning/destructive/primary` 体系，
    所有 renderer 视觉一致。

R13. **保留所有原始信息可见性**。任何"摘要化"的字段必须能在展开后看到完整原始值
    （diff、JSON、URL、path 等）。这是兜底原则，避免 UI 优化掩盖调试信息。

## Acceptance Criteria

- [ ] AC1: `cargo test -p agentdash-executor` 通过；新增至少 2 个测试覆盖
      `ActionType::CommandRun` → `ThreadItem::CommandExecution` 与
      `ActionType::FileEdit` → `ThreadItem::FileChange` 的映射
- [ ] AC2: 用一个集成 fixture（或现有 e2e）验证：通过通用 executor 通道发起
      bash/read/edit/grep/glob 五种调用，前端会话流分别命中
      `CommandExecutionCard` / `DynamicToolCallCardBody[Read]` /
      `FileChangeCardBody` / `DynamicToolCallCardBody[Grep]` /
      `DynamicToolCallCardBody[Glob]` 五种 renderer
- [ ] AC3: 折叠态 header 在 5 种工具下都能直接展示请求摘要：
      `$ <command>` / `Read <path>` / `+N -M in <file>` / `Grep "<pattern>" in <glob>` /
      `Glob <pattern>`，无需点开
- [ ] AC4: 任意未注册 `tool` 名（用 `tool="UnknownXyz"` 构造）落到 GenericJsonBody，
      且默认展开后能看到 "入参/出参" 分区与折叠树
- [ ] AC5: `packages/app-web/src/components/acp/tool-call.tsx` 已删除；全仓 grep
      `ToolCallView`、`SessionToolCallCard.compact` 无残留命中
- [ ] AC6: `kind` 元数据全仓只在 `threadItemKind.ts` 一处定义；
      `getThreadItemKind` / `getKindConfig` / `buildKindSummary` 内不再有重复
      switch 字面量
- [ ] AC7: 类型检查、`pnpm -C packages/app-web lint`、`pnpm -C packages/app-web test`
      全绿；后端 `cargo clippy --workspace` 无新增警告

## Out of Scope

- 不改 `ThreadItem` schema（不新增 `fileRead` / `webFetch` 等 variant），由后续
  任务评估
- 不做 diff 语法高亮、JSON 树高级查看器（搜索/路径过滤等），现阶段只做基础可折叠
- 不动 `SessionMessageCard` / `SessionPlanCard` / `SessionTaskEventCard` 等
  非工具调用卡（用户已确认本轮聚焦工具调用）
- 不重做 turn-message 显示，agent_message 已经独立绘制
- 不动 `tests/` 目录下与会话流无关的 e2e
- 不动 `CommandExecutionCard` 的 promote-to-terminal 行为

## Notes

- "外层完全没有请求摘要"在用户原话中指的是**单个工具卡折叠态 header 缺请求参数
  摘要**，不是聚合卡 / turn 摘要。设计核心因此落在"每个工具的 summarizer"上
- "实际上重复"指 `acp/tool-call.tsx::ToolCallView`、`compact` 死路径、kind 三处
  字面量映射
- 双线推进的耦合：前端 R6/R7 的注册表覆盖范围，与后端 R1 的映射结果直接对应。
  前端先到一步会出现"已注册的 renderer 收不到对应 type"的现象（仍然 fallback 到
  dynamic），后端先到一步会出现"前端 dynamic 分支被分流走了部分流量但 renderer
  尚未拆好"的中间态——两边都需要保留 fallback 路径，避免任一阶段中间态崩
