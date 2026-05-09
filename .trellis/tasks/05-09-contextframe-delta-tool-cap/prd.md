# ContextFrame frame 职责拆分 + ToolSchemaDelta 瘦身（全栈）

## Goal

修正 ContextFrame 后端的两个职责混装 / 数据双投影问题：
1. `RuntimeContextUpdateFrame` 把 `workflow_context` 和能力/工具 delta 强行打包进同一个 frame，导致前端在"能力变更篮子"里看到 Workflow Context Update——**workflow_context 必须从 RuntimeContextUpdateFrame 剥离，作为独立 frame 发出**。
2. 同一份 `CapabilityStateDelta.excluded/included_tool_paths` 被同时投影到 `CapabilityDeltaSection.{unblocked,blocked,whitelisted}_tool_paths` 和 `ToolSchemaDeltaSection.{restored,blocked,removed}_tool_paths`，前端渲染同一工具两次（CAP 说"工具解除屏蔽"，TOOL 说"恢复"）——**ToolSchemaDelta 瘦身为只承载 `added_tools`，所有 path-only 变化归口 CAP**。

前端聚合规则不变：同 turn 内多个 frame 仍然合并到一个 Stream shell 里以多 frame tab 形式展示，视觉上用户依然只看到一张 CTX 卡。

## Core Design Principles（用户校准）

1. **CAP 是能力变更的标准汇聚 section**：能力键 / 工具权限路径 / MCP / VFS 挂载都归 CAP；后续新增的能力维度 delta 也继续往 CAP 扩展字段（而不是新建 section）。CAP 是稳定的汇聚点，只是目前很多维度还没开始塞。与之配对的 `workspace_surface` 只管全量快照，二者职责正交。
2. **TOOL 只管 schema 注入**：移除/屏蔽/恢复（path-only）走 CAP；TOOL 的 `tool_schema_delta` 仅在有真正新增工具 schema 需要发给 Agent 时出现，否则**整个 section 不发**。这样去重在数据源头解决，前端无需 dedup。
3. **frame 一职一责**：`runtime_context_update` 只装能力/工具 delta；`workflow_context` 走自己的 frame kind。一次 phase 切换可能连发 N 个 frame，前端聚合层负责把它们绑在同一张 Stream shell 上。

## What I already know（来自 research）

研究文件：
- [`research/backend-capability-delta-schema.md`](research/backend-capability-delta-schema.md) — schema 定义、构造路径、消费者、风险
- [`research/section-kind-taxonomy.md`](research/section-kind-taxonomy.md) — section 拆分方案对比（已被用户校准作废，但 workspace_surface bootstrap-only 等副发现仍引用）

### 关键摸底结论

1. **重复根因 wire 层硬编码**：`RuntimeContextUpdateFrame::sections()` 在 [`runtime_context_transition.rs:409-418`](crates/agentdash-application/src/session/hub/runtime_context_transition.rs#L409-L418) 同时调用 `capability_delta.section()` + `tool_schema_delta.section()` + `workflow_context.section()`。其中 `CapabilityDeltaFrameMetadata::from_delta`（[`L514-556`](crates/agentdash-application/src/session/hub/runtime_context_transition.rs#L514-L556)）和 `ToolSchemaDeltaMetadata::from_tools_and_state_delta`（[`tool_schema_notice.rs:85-135`](crates/agentdash-application/src/session/tool_schema_notice.rs#L85-L135)）从同一个 `state_delta.excluded_tool_paths.removed` ∪ `included_tool_paths.added` 取数据并双向投影。
2. **后端零 reader**：`grep CapabilityDeltaSection` 在 `crates/` 下除定义和测试外零命中——后端没有任何业务逻辑读取这两个 section 的字段，schema 改动对后端逻辑零风险。
3. **无协议生成链路**：仓库未配置 ts-rs / typeshare / specta，前端 TS 类型是手写镜像，必须同步手改。
4. **持久化兼容性**：旧 session journal 里的 `SessionMetaUpdate { key: "context_frame" }` JSON 含老 variant 结构。前端 `parseContextFrame` 对未知 kind 返回 null 直接过滤，老 frame 在新 UI 上"展示降级"——可接受。
5. **workspace_surface 是 bootstrap-only 快照，VFS 运行时 delta 归 CAP 是合理分层**：workspace_surface 管全量 mount 快照（首次注入），CAP 管运行时 delta（和 MCP / capability key 同级）。本任务保持这种分工不动。

## Decisions（brainstorm 已定）

| # | 话题 | 决定 |
|---|------|------|
| Q1 | CAP 大篮子是否保留 | **保留并作为能力变更的长期汇聚点**。能力键 / 工具权限 / MCP / VFS 仍归 CAP，后续能力维度扩展继续在 CAP 加字段 |
| Q2 | 工具路径双投影去重 | **TOOL 瘦身**：`ToolSchemaDelta` 删除 `restored_tool_paths` / `blocked_tool_paths` / `removed_tool_paths` 三个字段，只保留 `added_tools`。path-only 变化全部由 CAP 表达 |
| Q3 | TOOL section 何时发送 | **仅在 `added_tools` 非空时发**。某次 phase 切换若只有屏蔽/解除屏蔽（没有真正新增 schema），则该 frame 的 `sections` 数组里没有 TOOL section |
| Q4 | workflow_context 在 RuntimeContextUpdateFrame 里 | **拆出**。`workflow_context` 字段从 `RuntimeContextUpdateFrame` 删除，作为独立 frame（kind=`workflow_context`），由独立的 `emit_context_frame` 入口发出 |
| Q5 | 前端聚合规则 | **不变**。同 turn 内多个 frame（runtime_context_update + workflow_context 等）继续聚合到一张 Stream shell 的多 frame tab |
| Q6 | 持久化兼容 | **接受降级**。旧 session journal 里的老结构 frame 在新 parser 下未知字段被忽略，不做向后兼容反序列化 |
| Q7 | spec 同步 | **必做**。`tool-capability-pipeline.md` 增补章节明确新 frame 职责切分和 ToolSchemaDelta 瘦身契约 |

## Open Questions（无 — 可以进入实施）

## Requirements

### 后端（`crates/agentdash-spi` + `crates/agentdash-application`）

- [ ] [`crates/agentdash-spi/src/hooks/mod.rs`](crates/agentdash-spi/src/hooks/mod.rs) `ContextFrameSection::ToolSchemaDelta` 删除 `removed_tool_paths` / `restored_tool_paths` / `blocked_tool_paths` 字段，只保留 `added_tools`
- [ ] [`crates/agentdash-application/src/session/tool_schema_notice.rs`](crates/agentdash-application/src/session/tool_schema_notice.rs) `ToolSchemaDeltaMetadata` 同步瘦身：`from_tools_and_state_delta` 不再消费 `state_delta` 的 path 字段，只根据"哪些 tool 是真正新增的 schema"构造 `added_tools`；构造结果为空时返回 None（外层 `RuntimeContextUpdateFrame` 不挂载）
- [ ] [`crates/agentdash-application/src/session/hub/runtime_context_transition.rs`](crates/agentdash-application/src/session/hub/runtime_context_transition.rs) 
  - `RuntimeContextUpdateFrame` struct 删除 `workflow_context: Option<WorkflowContextMetadata>` 字段
  - `sections()` 不再 push workflow_context section
  - 新增 `WorkflowContextFrame { workflow_context: WorkflowContextMetadata }`，自有 kind=`workflow_context` 的独立 frame 类型
  - `build_live_context_frame` / `build_context_frame` 在 workflow_context 非空时**额外**构造一个 `WorkflowContextFrame` 并通过 emit 链路发出（具体 emit 入口由 implement 子代理摸清）
- [ ] CAP delta 字段保持原状（`unblocked / blocked / whitelisted / removed_whitelist_tool_paths` 等都保留），仅作为唯一的工具权限信息来源
- [ ] `cargo build` / `cargo test` 全绿；现有 [`runtime_context_transition.rs:781-893`](crates/agentdash-application/src/session/hub/runtime_context_transition.rs#L781-L893) / [`tool_schema_notice.rs:467-507`](crates/agentdash-application/src/session/tool_schema_notice.rs#L467-L507) / [`hub/tests.rs:1200-1239`](crates/agentdash-application/src/session/hub/tests.rs#L1200-L1239) 测试同步更新

### 前端（`frontend/src/features/session/`）

- [ ] [`frontend/src/features/session/model/contextFrame.ts`](frontend/src/features/session/model/contextFrame.ts) `ToolSchemaDeltaSection` interface 删除 `restored_tool_paths` / `blocked_tool_paths` / `removed_tool_paths`，只保留 `added_tools`
- [ ] `parseSection` 跟随删除对应字段
- [ ] [`frontend/src/features/session/ui/contextFrame/SectionRenderers.tsx`](frontend/src/features/session/ui/contextFrame/SectionRenderers.tsx)
  - `ToolSchemaDeltaBody` 删除上半段的 path DiffLine（`restored / blocked / removed` 三个 DiffLine 块），只渲染 `added_tools` 工具卡片列表
  - `sectionHint` 的 `tool_schema_delta` 分支改为只统计 `added_tools.length`
  - `toolSchemaDeltaAffectedCount` 函数删除（不再有路径维度）
- [ ] [`frontend/src/features/session/ui/ContextFrameStream.tsx`](frontend/src/features/session/ui/ContextFrameStream.tsx) `summarizeRuntimeUpdate` 中的 `tool_schema_delta` 分支去掉对 `restored / blocked / removed` 的统计（CAP 已经数过这些了），只数 `added_tools.length` 计入新增
- [ ] 测试 fixture 同步：[`ContextFrameCard.test.tsx`](frontend/src/features/session/ui/ContextFrameCard.test.tsx) 的 `sampleNotice` 删除多余 path 字段；[`SessionEntry.context-frame.test.tsx`](frontend/src/features/session/ui/SessionEntry.context-frame.test.tsx) 同步
- [ ] `pnpm lint` / `pnpm typecheck` / `pnpm test` 全绿

### Spec

- [ ] [`.trellis/spec/backend/capability/tool-capability-pipeline.md`](.trellis/spec/backend/capability/tool-capability-pipeline.md) 增补"frame 职责切分"章节，明确：
  - `RuntimeContextUpdateFrame` 只承载能力/工具 delta，不再装 workflow_context
  - `ToolSchemaDelta` 仅含 `added_tools`，path-only 变化归 CAP
  - **CapabilityDelta 是能力变更的长期汇聚 section**，新增维度继续在此扩展字段
  - workflow_context 是独立 frame kind

## Acceptance Criteria

- [ ] 任意 ContextFrame 中，同一 tool_path 不再同时出现在 CAP 和 TOOL 两个 section（后端 schema 保证）
- [ ] kind=`runtime_context_update` 的 frame 的 sections 数组**不含** workflow_context section
- [ ] 当 `state_delta` 仅有工具屏蔽/解除屏蔽（无 added_tools）时，frame 的 sections 不含 tool_schema_delta section
- [ ] workflow_context 仍作为独立 frame 发到前端，前端 Stream shell 在同 turn 把它和 runtime_context_update frame 合并展示为多 frame tab
- [ ] 前端 / 后端测试全绿
- [ ] 不引入新依赖（npm 或 crate）

## Definition of Done

- 后端 `cargo build && cargo test` 全绿
- 前端 `pnpm lint && pnpm typecheck && pnpm test` 全绿
- spec 已增补
- 视觉走查：典型 phase 切换场景（仅屏蔽工具 / 仅新增工具 / 同时切换 workflow）三类 frame 的渲染符合预期

## Out of Scope

- ContextFrame 历史向前兼容反序列化
- 其他 section（bootstrap_context / hook_injection / system_notice / auto_resume / compaction_summary）的结构调整
- 前端 L3 折叠违规（`EffectiveCapabilitiesBlock` / `CompactedUntilRefBlock` / `ToolSchemaItem` 的 useState 折叠）—— 留作独立任务

## Technical Notes

### Frame 拆分的 emit 入口

`SessionHub::emit_context_frame` → `persist_context_frame_direct` → `SessionMetaUpdate { key: "context_frame", value: <ContextFrame JSON> }`（[`facade.rs:329, 632, 661`](crates/agentdash-application/src/session/hub/facade.rs#L329)）。

新增 `WorkflowContextFrame` 走相同入口，只是上游构造逻辑变成两次 emit（runtime_context_update + workflow_context），而不是一次 emit 内含三个 section。具体 emit 调用点在 `build_live_context_frame` / `build_context_frame` 处增补。

### 数据契约一览（瘦身后）

```
CapabilityDelta {
  added_capabilities, removed_capabilities, effective_capabilities,
  blocked_tool_paths, unblocked_tool_paths,
  whitelisted_tool_paths, removed_whitelist_paths,
  added_mcp_servers, removed_mcp_servers, changed_mcp_servers,
  vfs_mounts_added, vfs_mounts_removed,
  default_mount_before, default_mount_after,
}  ← 字段全部保留

ToolSchemaDelta {
  added_tools: Vec<RuntimeToolSchemaEntry>,  ← 唯一字段
}  ← 删除 restored_tool_paths / blocked_tool_paths / removed_tool_paths

WorkflowContextFrame { workflow_context: WorkflowContextMetadata }  ← 新增独立 frame kind
```
