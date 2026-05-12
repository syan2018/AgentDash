# Research: 后端 ContextFrame `CapabilityDelta` / `ToolSchemaDelta` Section schema

- **Query**: 摸清后端 `CapabilityDeltaSection` / `ToolSchemaDeltaSection` 的 Rust 定义、构造路径、消费者，为 schema 重构提供精确坐标
- **Scope**: 内部代码 + spec
- **Date**: 2026-05-09

---

## 1. Rust schema 定义位置

**唯一权威定义**：`crates/agentdash-spi/src/hooks/mod.rs`，`ContextFrameSection` 枚举（`#[serde(tag = "kind", rename_all = "snake_case")]`）。

| 变体 | 对应前端 type | 定义行 |
|---|---|---|
| `ContextFrameSection::CapabilityDelta { .. }` | `capability_delta` | `crates/agentdash-spi/src/hooks/mod.rs:252-281` |
| `ContextFrameSection::ToolSchema { tools }` | `tool_schema` | `crates/agentdash-spi/src/hooks/mod.rs:282-285` |
| `ContextFrameSection::ToolSchemaDelta { .. }` | `tool_schema_delta` | `crates/agentdash-spi/src/hooks/mod.rs:286-295` |

**注意**：Rust 端没有名为 `CapabilityDeltaSection` / `ToolSchemaDeltaSection` 的独立 struct。前端 TS 用了 `CapabilityDeltaSection` 这个 type alias 名字（`frontend/src/features/session/model/contextFrame.ts:45`），但 Rust 端是 enum variant inline struct。两者 wire 通过 serde tag `"kind"` 对齐。

### `ContextFrameSection::CapabilityDelta` 字段（`mod.rs:252-281`）

```rust
CapabilityDelta {
    added_capabilities: Vec<String>,        // capability key 增
    removed_capabilities: Vec<String>,      // capability key 减
    effective_capabilities: Vec<String>,    // 当前生效集合（全量快照）
    blocked_tool_paths: Vec<String>,        // ← 与 ToolSchemaDelta 重叠（同名）
    unblocked_tool_paths: Vec<String>,      // ← 与 ToolSchemaDelta.restored_tool_paths 语义重叠
    whitelisted_tool_paths: Vec<String>,    // ← 与 ToolSchemaDelta.restored_tool_paths 语义重叠
    removed_whitelist_paths: Vec<String>,   // 工具移出 include_only 白名单
    added_mcp_servers: Vec<String>,
    removed_mcp_servers: Vec<String>,
    changed_mcp_servers: Vec<String>,
    vfs_mounts_added: Vec<String>,
    vfs_mounts_removed: Vec<String>,
    default_mount_before: Option<String>,
    default_mount_after: Option<String>,
}
```

### `ContextFrameSection::ToolSchemaDelta` 字段（`mod.rs:286-295`）

```rust
ToolSchemaDelta {
    added_tools: Vec<RuntimeToolSchemaEntry>,  // 新增 / 恢复的工具完整 schema
    removed_tool_paths: Vec<String>,            // 工具被 include_only 排除（included_tool_paths.removed）
    restored_tool_paths: Vec<String>,           // excluded.removed ∪ included.added 的并集
    blocked_tool_paths: Vec<String>,            // ← 与 CapabilityDelta.blocked_tool_paths 同名同源
}
```

`RuntimeToolSchemaEntry` 字段：`name / description / parameters_schema / capability_key? / source? / tool_path?`（`mod.rs:355-367`）。

---

## 2. 构造路径（谁填充 `ContextFrame.sections`）

### 2.1 `RuntimeContextUpdateFrame` —— 同时塞两个 section 的唯一入口

`crates/agentdash-application/src/session/hub/runtime_context_transition.rs:346-430`

```rust
struct RuntimeContextUpdateFrame {
    capability_delta: CapabilityDeltaFrameMetadata,           // 必填
    tool_schema_delta: Option<ToolSchemaDeltaMetadata>,       // 选填
    workflow_context: Option<WorkflowContextMetadata>,
}

fn sections(&self) -> Vec<ContextFrameSection> {
    let mut sections = vec![self.capability_delta.section()];   // L410
    if let Some(tool_schema_delta) = &self.tool_schema_delta {
        sections.push(tool_schema_delta.section());              // L412
    }
    if let Some(workflow_context) = &self.workflow_context { ... }
    sections
}
```

**两个 section 来自同一份 `CapabilityStateDelta`** —— 这是重复的根因：

- `CapabilityDeltaFrameMetadata::from_delta`（`runtime_context_transition.rs:514-556`）从 `state_delta.excluded_tool_paths.removed` 取 `unblocked_tool_paths`，从 `state_delta.included_tool_paths.added` 取 `whitelisted_tool_paths`。
- `ToolSchemaDeltaMetadata::from_tools_and_state_delta`（`tool_schema_notice.rs:85-135`）把同样的 `excluded_tool_paths.removed` ∪ `included_tool_paths.added` 收集为 `restored_tool_paths`，并把 `excluded_tool_paths.added` 写进 `blocked_tool_paths`。

→ **同一个 `CapabilityState` excluded/included path 字段被两个 section 字面照搬**。这不是上游数据巧合一致，而是 wire 层硬编码的双投影。

### 2.2 调用 `RuntimeContextUpdateFrame` 的地方

| 路径 | 文件 | 备注 |
|---|---|---|
| Live apply（phase 切换 / 即时生效） | `runtime_context_transition.rs:276-294`（`build_live_context_frame`） | 通过 `apply_mode = "live"` |
| Pending → applied_on_next_turn（下一 turn 注入） | `runtime_context_transition.rs:240-249`（`build_context_frame`） | 通过 `apply_mode = "applied_on_next_turn"` |

两条路径最终都走 `context_frame::build_context_frame(&metadata)` →
`crates/agentdash-application/src/session/context_frame.rs:28-43` 包装成 `ContextFrame { sections, ... }`。

### 2.3 `ToolSchemaDelta` 的独立路径

`ToolSchemaDeltaMetadata::section()` 只在 `RuntimeContextUpdateFrame::sections()` 内部被推入；当前**没有**独立的 ToolSchemaDelta-only frame 入口（`tool_schema_notice.rs` 里的 `ToolSurfaceContextFrame` 走的是 `ContextFrameSection::ToolSchema`，即初始化全量 schema，不是 delta）。

### 2.4 `ToolSchema`（initial bootstrap，无 delta）

`crates/agentdash-application/src/session/tool_schema_notice.rs:65-69`

由 `enqueue_tool_schema_notice(ToolSchemaNoticeKind::Initial, tools)` 触发，唯一调用点：`crates/agentdash-application/src/session/prompt_pipeline.rs:453-455`（session 首次 prompt 时）。

### 2.5 持久化与广播

`SessionHub::emit_context_frame` → `persist_context_frame_direct` → `SessionMetaUpdate { key: "context_frame", value: <序列化 ContextFrame JSON> }`（`crates/agentdash-application/src/session/hub/facade.rs:329, 632, 661`）。
即：每一个 ContextFrame 整体 JSON 被写入 session journal，作为前端、回放、审计的唯一权威源（见 `.trellis/spec/backend/hooks/execution-hook-runtime.md:141` 与 `.trellis/spec/frontend/type-safety.md:177`）。

---

## 3. 消费者清单

### 3.1 后端内部消费者

**没有**任何后端代码读取 `ContextFrameSection::CapabilityDelta` / `ToolSchemaDelta` 的字段做 audit / telemetry / hook trace。`grep CapabilityDeltaSection` 在 `crates/` 下零命中（除了定义文件本身）。`hub/tests.rs:1218` 仅在测试桩里构造 `ContextFrameSection::ToolSchema`。`hub/runtime_context_transition.rs:871` 是测试断言匹配。

→ **后端纯 producer，section 字段无后端 reader**。可以放心改 schema，不会破坏后端逻辑。

### 3.2 前端消费者（CapabilityDeltaSection 字段）

| 字段 | 渲染位置 |
|---|---|
| `added_capabilities` | `SectionRenderers.tsx:202`（"+ 能力"） |
| `removed_capabilities` | `SectionRenderers.tsx:209`（"− 能力"） |
| `effective_capabilities` | `SectionRenderers.tsx:248-249`（折叠面板） |
| `blocked_tool_paths` | `SectionRenderers.tsx:210`（"− 工具屏蔽"） |
| `unblocked_tool_paths` | `SectionRenderers.tsx:203`（"+ 工具解除屏蔽"） ← 与 TOOL section 冲突 |
| `whitelisted_tool_paths` | `SectionRenderers.tsx:204`（"+ 工具加入白名单"） ← 与 TOOL section 冲突 |
| `removed_whitelist_paths` | `SectionRenderers.tsx:211`（"− 工具移出白名单"） |
| `added_mcp_servers` / `removed_mcp_servers` / `changed_mcp_servers` | `SectionRenderers.tsx:205, 212, 216` |
| `vfs_mounts_added/removed` | `SectionRenderers.tsx:206, 213` |
| `default_mount_before/after` | `SectionRenderers.tsx:219-242` |
| 总数（badge count） | `SectionRenderers.tsx:93-110` |

ToolSchemaDeltaSection 字段：`SectionRenderers.tsx:318-351`（restored="恢复" / blocked="屏蔽" / removed="移除" / added_tools 完整 schema）。

→ 前端 `ContextFrameStream.tsx:172-187` 也读 `kind === "capability_delta"` 与 `"tool_schema_delta"` 做 stream 聚合。

### 3.3 前端测试快照

| 文件 | 引用 |
|---|---|
| `frontend/src/features/session/ui/ContextFrameCard.test.tsx:13, 35, 135, 150` | 包含同时含两个 section 的固定输入；测试 dedup |
| `frontend/src/features/session/ui/SessionEntry.context-frame.test.tsx:54` | `kind: "capability_delta"` 装配 |
| `frontend/src/features/session/ui/SessionSystemEventCard.test.tsx:29` | `kind: "tool_schema_delta"` 装配 |

### 3.4 协议生成链路

仓库**没有**配置 ts-rs / typeshare / specta（grep `crates/` 零命中）。前端 `contextFrame.ts` 是手写镜像，不存在自动同步。改 Rust schema 必须**同步手改 TS 类型 + parser + 渲染**。

---

## 4. 现有 spec 约束

| spec | 关键约束 | 行号 |
|---|---|---|
| `.trellis/spec/backend/capability/tool-capability-pipeline.md` | 工具 schema runtime context 收敛到 `ContextFrameSection::ToolSchemaDelta`，只发送 CapabilityStateDelta 影响到的 delta；**section 字段列表未硬编码**，仅约束"必须能与能力来源对应" | L243-253, L523 |
| `.trellis/spec/backend/session/execution-context-frames.md` | 不涉及 section 字段，仅描述 ExecutionContext / Bundle | 全文 |
| `.trellis/spec/backend/session/bundle-main-datasource.md` | 不涉及 CapabilityDelta / ToolSchemaDelta 字段（grep 无命中） | — |
| `.trellis/spec/frontend/type-safety.md:177, 186` | 约定 `SessionMetaUpdate { key: "context_frame" }` 是前端权威源；`tool_schema_delta` section 是 runtime 工具变化主视图 | L177-186 |

→ **现有 spec 没有把 `CapabilityDelta` 的 14 个字段列表显式锁死**。重构前需要在 `tool-capability-pipeline.md` 增补「section schema 拆分」章节，明确新 section 的字段契约。

---

## 5. 破坏性改造风险评估

### 5.1 测试 / 快照影响面

- Rust：`runtime_context_transition.rs:781-893`（`live_context_frame_includes_tool_schema_delta_only`）+ `tool_schema_notice.rs:467-507`（`tool_schema_notice_includes_full_parameter_schema`）+ `hub/tests.rs:1200-1239`（`emit_context_frame_persists_agent_visible_frame` 与 `is_platform_session_meta_update("context_frame")`）+ `hub/tests.rs:1642`（`compaction_summary` 分支断言）。
- TS：上述 §3.3 三个 test 文件的 fixture 必须重写。
- 没有发现 `*.snap` / `insta` 快照文件涉及这两个 section（仓库未使用 insta 快照对 ContextFrame 做断言）。

### 5.2 客户端兼容性

- **前端（dashboard）**：仓库内唯一前端，与后端同 monorepo 同 release 同步发布，**无前向兼容需求**，可破坏式改。
- **Tauri / VSCode 扩展**：`grep CapabilityDeltaSection` 在 `crates/` / `frontend/` 之外零命中，没有外部 client。
- **Relay / vibe_kanban connector**：见 `execution-context-frames.md:101-105`，它们消费的是 `rendered_system_prompt` 字符串而非 ContextFrameSection 结构化字段，不受影响。

### 5.3 持久化 / 事件回放

- `SessionMetaUpdate { key: "context_frame", value: <ContextFrame JSON> }` 写入 session journal（`facade.rs:329, 661`）。
- 历史 session 加载时，`parseContextFrame`（`contextFrame.ts:169-313`）会对未知 `kind` 返回 `null` 而被过滤；旧持久化的 `capability_delta` / `tool_schema_delta` JSON 在新 schema 下可能被丢弃或字段缺失。
- **建议**：要么保留 `capability_delta` 旧 variant 做向后兼容反序列化（`#[serde(other)]` 或多 variant 并存），要么承认"老 session frame 在新 UI 上展示降级"，由 PRD 拍板。

### 5.4 上游数据耦合

`CapabilityStateDelta`（`crates/agentdash-application/src/session/capability_state.rs:63-81`）是上游唯一真源，含 `tool_capabilities / tool_clusters / excluded_tool_paths / included_tool_paths / mcp_servers / vfs` 六个子 delta。新 section 拆分必须保持**同一个 `state_delta` 输入** → **多个 section 输出**的纯函数映射，避免重复投影同一字段。

---

## 6. 关键文件一览（绝对路径）

- `d:/ABCTools_Dev/AgentDashboard/crates/agentdash-spi/src/hooks/mod.rs`（schema 定义 L243-353）
- `d:/ABCTools_Dev/AgentDashboard/crates/agentdash-application/src/session/hub/runtime_context_transition.rs`（构造主入口 L240-693）
- `d:/ABCTools_Dev/AgentDashboard/crates/agentdash-application/src/session/tool_schema_notice.rs`（ToolSchemaDelta 元数据 L77-169，Initial bootstrap L171-210）
- `d:/ABCTools_Dev/AgentDashboard/crates/agentdash-application/src/session/context_frame.rs`（builder L28-43）
- `d:/ABCTools_Dev/AgentDashboard/crates/agentdash-application/src/session/capability_state.rs`（CapabilityStateDelta L63-81）
- `d:/ABCTools_Dev/AgentDashboard/crates/agentdash-application/src/session/hub/facade.rs`（持久化 L329, 632, 661）
- `d:/ABCTools_Dev/AgentDashboard/frontend/src/features/session/model/contextFrame.ts`（TS schema 镜像 L45-83, parser L208-243）
- `d:/ABCTools_Dev/AgentDashboard/frontend/src/features/session/ui/contextFrame/SectionRenderers.tsx`（渲染 L199-351）
- `d:/ABCTools_Dev/AgentDashboard/.trellis/spec/backend/capability/tool-capability-pipeline.md`（spec L232-265, 523）
- `d:/ABCTools_Dev/AgentDashboard/.trellis/spec/frontend/type-safety.md`（前端契约 L177-186）

## 7. 结论：重叠根因

`ContextFrameSection::CapabilityDelta.{blocked_tool_paths, unblocked_tool_paths, whitelisted_tool_paths, removed_whitelist_paths}` 与 `ContextFrameSection::ToolSchemaDelta.{blocked_tool_paths, restored_tool_paths, removed_tool_paths}` 是**同一份 `CapabilityStateDelta` 在 `RuntimeContextUpdateFrame::sections()` 内被两次投影**（`runtime_context_transition.rs:524-535` 与 `tool_schema_notice.rs:97-126`），不是数据巧合。重构方向（细分 section）只需在该 frame builder 内重新决定字段归属，上游 `CapabilityStateDelta` 无需改动。
