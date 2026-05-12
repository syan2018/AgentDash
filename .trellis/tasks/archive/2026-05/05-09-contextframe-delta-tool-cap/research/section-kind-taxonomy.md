# Research: ContextFrame Section Kind Taxonomy — 拆分决策依据

- **Query**: 为 ContextFrame `CapabilityDeltaSection` 的细粒度拆分设计（方案 A/B/C 对比 + 推荐）提供依据
- **Scope**: 内部
- **Date**: 2026-05-09

## 1. 现有 section kind 清单（12 种）

权威定义：`crates/agentdash-spi/src/hooks/mod.rs:245-353`（Rust `enum ContextFrameSection`，`#[serde(tag = "kind", rename_all = "snake_case")]`）。前端镜像：`frontend/src/features/session/model/contextFrame.ts:17-29`。

| kind | 语义 | 构造位置 | 发送时机 |
|---|---|---|---|
| `bootstrap_context` | 启动动态上下文帧（project/user prefs/discovered guidelines） | `session/bootstrap_context_frame.rs` | owner bootstrap 首轮 |
| `capability_delta` | **当前大篮子**（6 类变化 + effective 快照） | `hub/runtime_context_transition.rs:558-575` | phase transition / 能力热更 |
| `tool_schema` | 完整工具列表快照 | `tool_schema_notice.rs:65-69` | 首次 / `ToolSchemaNoticeKind::Initial` |
| `tool_schema_delta` | 工具级增删（混合路径列表 + added_tools schema 详情） | `tool_schema_notice.rs:137-144`；由 `CapabilityStateDelta` 派生 | 与 `capability_delta` 成对发，同 frame |
| `workflow_context` | workflow slot 注入 | `hub/runtime_context_transition.rs:723-728`（injections） | phase transition |
| `hook_injection` | hook slot 注入 | 同 enum `HookInjection` 分支 | hook evaluate |
| `system_notice` | 通用系统提示 | — | 通用通道 |
| `workspace_surface` | 工作目录 + VFS mount **全量快照** | `surface_context_frames.rs:47-58` | **仅 `is_owner_bootstrap`**（`prompt_pipeline.rs:423-430`）|
| `skill_surface` | skill 全量列表 | `surface_context_frames.rs:113-120` | 仅 `is_owner_bootstrap` |
| `hook_runtime_surface` | hook 在位提示 + pending_action_count | `surface_context_frames.rs:157-162` | 仅 `is_owner_bootstrap` |
| `auto_resume` | auto-resume 提示 | `auto_resume_context_frame.rs` | 自动续跑路径 |
| `compaction_summary` | 压缩摘要 | `compaction_context_frame.rs` | compact 之后 |

关键观察：
- **只有 `capability_delta` + `tool_schema_delta` 是 delta 族**；其他 surface 系 (`workspace/skill/hook_runtime`) 是**快照**，当前**只在 bootstrap 发一次**，不随 phase 切换更新。
- `tool_schema_delta` 的 `added_tools` + `restored/blocked/removed_tool_paths` 已经完整覆盖"工具路径变化"，但 `capability_delta` 同时把 `unblocked_tool_paths / blocked_tool_paths / whitelisted_tool_paths` 再次带出——PRD 提到的重复（`unblocked ↔ restored`）就是这里制造的。

## 2. CapabilityDelta 大篮子的 6 类内容

定义：`crates/agentdash-spi/src/hooks/mod.rs:252-281`，构造器 `CapabilityDeltaFrameMetadata::from_delta` (`runtime_context_transition.rs:514-556`)。数据源统一是 `CapabilityStateDelta`（`session/capability_state.rs:63-70`）：

| CAP 字段 | 来源 `CapabilityStateDelta` 字段 | 归宿语义 |
|---|---|---|
| `added_capabilities` / `removed_capabilities` | `CapabilityDelta.added/removed`（key 集合） | 能力键集合 |
| `effective_capabilities` | `input.capability_keys`（**全量快照**） | 快照（非 delta） |
| `blocked_tool_paths` / `unblocked_tool_paths` | `excluded_tool_paths.added/removed` | 工具路径状态 |
| `whitelisted_tool_paths` / `removed_whitelist_paths` | `included_tool_paths.added/removed` | 工具路径白名单 |
| `added_mcp_servers` / `removed_mcp_servers` / `changed_mcp_servers` | `mcp_servers: NamedEntityDelta`（字段 `changed: Vec<String>` 来自 `capability_state.rs:27-31`） | MCP server 生命周期 |
| `vfs_mounts_added/removed` + `default_mount_before/after` | `vfs: VfsSurfaceDelta` (`mounts: NamedEntityDelta`, `default_mount: DefaultMountDelta`) | VFS mount |

## 3. 方案对比

| 维度 | 方案 A（按语义拆 5 个细粒度 section） | 方案 B（并入已有 surface） | 方案 C（中间路线） |
|---|---|---|---|
| 新增 section kind 数 | +5 (`CapabilitySetDelta` / `ToolSurfaceDelta` / `McpServerDelta` / `VfsMountDelta` / `EffectiveCapabilitiesSnapshot`) | 0 | +1（`ToolSurfaceDelta`，CAP 缩小但保留） |
| 废弃 section | `capability_delta` / `tool_schema_delta` | `capability_delta` / `tool_schema_delta`（字段并入 `workspace_surface` / `tool_schema` / bootstrap 的 metadata） | `tool_schema_delta`；CAP 瘦身保留能力键/MCP/VFS |
| 后端 schema 变更面 | 大（spi enum、构造器、render_text、持久化事件） | 中（但 `workspace_surface` 从"仅 bootstrap"改为"per-turn 快照"—— 语义变化大） | 小 |
| 前端 renderer 增量 | +4（CapSet / ToolSurface / Mcp / VfsDelta / Effective） | +1 需改（`WorkspaceSurfaceBody` 扩展 mount delta 子区） | +1（ToolSurface）+ 瘦 CAP renderer |
| spec 一致性 | 与 `bundle-main-datasource.md` 的"按语义装载"方向一致 | 破坏 surface = 快照 / delta = 变化 的二元分层 | 混合：CAP 还在，TOOL 拆出 |
| 破坏性 | 高（前端 `CapabilityDeltaBody` 删除 + 类型 union 大改） | 中高（workspace_surface 契约从 snapshot 变为 snapshot+delta） | 中（CAP 字段瘦身、不全删） |
| WYSIWYG / 可读性 | 高：每个 section renderer 单一职责 | 低：surface 语义被污染（快照 + 变化混栈） | 中：CAP 还是多字段 |

---

### 方案 A 细节（与 PRD 已对齐）

```rust
// 伪代码（按语义独立 5 个 section）
pub enum ContextFrameSection {
    // ... 保留其他 ...

    /// 能力键集合变化（仅 added / removed，不带 effective 快照）
    CapabilitySetDelta {
        added: Vec<String>,     // 如 "file_read", "mcp:code_analyzer"
        removed: Vec<String>,
    },

    /// 当前生效能力键快照（从 effective_capabilities 分离）
    EffectiveCapabilitiesSnapshot {
        capabilities: Vec<String>,
    },

    /// 工具路径级变化（统一卡片）
    ToolSurfaceDelta {
        tools: Vec<ToolSurfaceEntry>,
    },

    McpServerDelta {
        added: Vec<String>,
        removed: Vec<String>,
        changed: Vec<String>,     // NamedEntityDelta.changed 已有
    },

    VfsMountDelta {
        added: Vec<String>,
        removed: Vec<String>,
        default_mount_before: Option<String>,
        default_mount_after: Option<String>,
    },
}

pub struct ToolSurfaceEntry {
    pub name: String,
    pub tool_path: String,
    pub capability_key: Option<String>,
    pub source: Option<String>,
    pub description: String,
    pub parameters_schema: serde_json::Value,   // added/restored 才有；blocked/removed 留空或 null
    pub state: ToolSurfaceState,
}

#[serde(rename_all = "snake_case")]
pub enum ToolSurfaceState {
    Added,              // 新能力导致新工具暴露（excluded_tool_paths.removed + capability added）
    Restored,           // excluded_tool_paths.removed 但 capability 已有
    Blocked,            // excluded_tool_paths.added
    Removed,            // capability 被移除 → included_tool_paths.removed
    WhitelistAdded,     // included_tool_paths.added
    WhitelistRemoved,   // included_tool_paths.removed（非 capability remove 导致）
}
```

## 4. MCP 与 Tool 的粒度关系

查 `runtime_context_transition.rs` + `tool_schema_notice.rs`：

- `ToolSchemaDeltaMetadata::from_tools_and_state_delta` (`tool_schema_notice.rs:85-135`) 把**已经构建完成的 `tools: &[DynAgentTool]`** 按 `state_delta.tool_capabilities.added`（capability）与 `restored_paths`（tool_path）过滤，生成 `added_tools` entries。这条路径**不直接**消费 `state_delta.mcp_servers`；**MCP server 增删与工具增删在同一条 `apply_live_runtime_context_transition` 中一起发生**：
  - MCP preset 变更 → `CapabilityState` 变更 → `replace_current_capability_state` 重建 `assembled_tools` → `ToolSchemaDeltaMetadata` 自动包含受影响的具体 MCP 工具 entries。
  - **同时** `capability_delta` 的 `added/removed/changed_mcp_servers` 字段从 `mcp_servers: NamedEntityDelta` 派生。
- 所以一次 MCP reload 当前会产生：
  - 1 条 `capability_delta`（带 `added_mcp_servers + blocked/unblocked_tool_paths`）
  - 1 条 `tool_schema_delta`（带 `added_tools` 具体 schema）
  - **重复源**：同一组工具路径在 `capability_delta.unblocked_tool_paths` 和 `tool_schema_delta.restored_tool_paths` 同时出现（`ToolSchemaDeltaMetadata.restored_tool_paths` 就取自 `excluded_tool_paths.removed + included_tool_paths.added`，见 `tool_schema_notice.rs:97-103`）。
- 方案 A 下的正确分层：
  - **`McpServerDelta` 只报告 server 生命周期**（名字级），不展开工具
  - **`ToolSurfaceDelta` 负责所有工具路径级变化**（包括 MCP 下挂的工具，通过 `source` 字段区分 platform / mcp）
  - 一次 MCP reload 预期：1 条 `McpServerDelta` + 1 条 `ToolSurfaceDelta`（后者已包含每个工具 entry）；**不再发 `capability_delta` 的 `*_tool_paths` 字段**，源头去重。

## 5. VFS mount 与 workspace_surface 的重叠

- `WorkspaceSurfaceFrame` (`surface_context_frames.rs:14-28`) 已经携带**全量快照**：`working_directory` + `default_mount` + `mounts: Vec<RuntimeWorkspaceMountEntry>`（每个 mount 带 `provider / root_ref / capabilities`）。
- 构造位置 `prompt_pipeline.rs:423-430` 受 `is_owner_bootstrap` 门控 → **phase transition 里 VFS mount 变更目前不会触发 `workspace_surface` 再发**。这就是为什么 phase transition 期间 VFS 变更被塞进 `capability_delta.vfs_mounts_added/removed`。
- 两种修法方向：
  - **方向一（方案 B 风格）**：把 `workspace_surface` 升级为"snapshot-on-change"，每次 VFS mount 变化重发一次 `workspace_surface`（以全量快照形式）。前端渲染最省力（已经会画 mount 卡片），但破坏了 surface 当前"bootstrap-only"契约；且需要在 `apply_live_runtime_context_transition` 里判断 VFS 是否有变化再重发 `workspace_surface`。
  - **方向二（方案 A 风格）**：新增独立 `VfsMountDelta` 专做 delta 展示，`workspace_surface` 仍然只在 bootstrap 发一次。优点：快照 vs delta 责任分离，符合现有分层；缺点：前端需要写一个单独的 `VfsMountDeltaBody` renderer，但内容简单（mount id 字符串列表 + default_mount 前后对比）。
- 推荐 **方向二**：VFS mount delta 量少结构简单，不值得为它打破 surface 契约；并且同理适用于 MCP（当前 MCP 没有 `mcp_surface` 快照 section，如果要做完整对称，未来可以加）。

## 6. ToolSurfaceState 枚举候选 + 漏项排查

候选（来自 PRD）：`added` / `restored` / `blocked` / `removed` / `whitelist_added` / `whitelist_removed`。

查代码有哪些真正的"工具级变化源"：
- `CapabilityStateDelta.excluded_tool_paths.added` → **Blocked**（`capability_state.rs:66`）
- `CapabilityStateDelta.excluded_tool_paths.removed` → **Restored**
- `CapabilityStateDelta.included_tool_paths.added` → **WhitelistAdded**
- `CapabilityStateDelta.included_tool_paths.removed` → **WhitelistRemoved**
- `CapabilityStateDelta.tool_capabilities.added` → **Added**（capability 新增 → 新工具暴露；需与 excluded/included delta 区分）
- `CapabilityStateDelta.tool_capabilities.removed` → **Removed**（capability 被移除 → 工具下线）

漏项检查：
- **`schema_changed`**：`grep schema_changed` 无匹配；`ToolSchemaDeltaMetadata` 也不检测 schema 版本差异。当前代码不产出此状态，**不需要**。
- **`capability_key_changed`**：同样无匹配。一个工具的 capability_key 变更在当前架构不单独广播（只会以 added + removed 成对表达）。不需要。

正交性检查（Q2 白名单 vs blocked/restored）：
- `excluded_tool_paths` 和 `included_tool_paths` 在 `CapabilityState` 是**独立的两个集合**（见 `capability_state.rs:63-70` 与 `excluded_tool_paths()` / `included_tool_paths()` 方法）。同一个 tool_path 理论上可以同时出现在 both delta；实际运行时通常互斥（白名单是"强制暴露"，blocked 是"强制屏蔽"），但 schema 不限制。
- **结论**：state 字段应允许一条 `ToolSurfaceEntry` 逻辑上拥有多个状态标签。两种实现：
  1. **单状态 + 优先级**（推荐，简单）：同一 tool 当同时命中多分类时，取优先级 `Added > Restored > WhitelistAdded > WhitelistRemoved > Blocked > Removed`；理由：新增类是"出现在视图内"，移除类是"从视图消失"，同时出现时展示最强的正向信号。
  2. **状态列表 `Vec<ToolSurfaceState>`**：schema 严格，但前端 badge 要合并渲染，复杂度升高；不推荐。

## 7. 推荐方案

**推荐方案 A**（完全拆分），理由：

1. **源头去重（PRD 核心目标）**：TOOL / CAP 单源化，只有 `ToolSurfaceDelta` 一条通道承载工具路径变化，彻底消除 `unblocked ↔ restored` 的双发。
2. **责任分离清晰**：每个 section renderer 对应单一语义，前端 L2 折叠即可承载；不需要在 CAP body 里穿插 6 块子区。
3. **与现有 delta 框架对齐**：`CapabilityStateDelta` 的 5 大子字段（tool_capabilities / excluded_tool_paths / included_tool_paths / mcp_servers / vfs）自然映射 5 个 section；新增 `EffectiveCapabilitiesSnapshot` 承担快照（从 delta 里剥离）。
4. **不破坏 surface 契约**：`workspace_surface` 仍是 bootstrap-only 快照；phase transition 期间的 VFS 变化通过 `VfsMountDelta` 补齐。
5. **空 section 省略天然生效**：每个 delta section 构造器在全空时返回 `None`，`RuntimeContextUpdateFrame::sections` 直接跳过，满足 PRD "空 section 不发"。

### 推荐 Rust 伪代码骨架

```rust
// crates/agentdash-spi/src/hooks/mod.rs （替换 CapabilityDelta / ToolSchemaDelta 两个分支）
pub enum ContextFrameSection {
    CapabilitySetDelta {
        added: Vec<String>,
        removed: Vec<String>,
    },
    EffectiveCapabilitiesSnapshot {
        capabilities: Vec<String>,
    },
    ToolSurface {                              // 替换 ToolSchema + ToolSchemaDelta
        tools: Vec<ToolSurfaceEntry>,
        // 初始化快照 state 全为 Initial；delta 时 state 反映变化
    },
    McpServerDelta {
        added: Vec<String>,
        removed: Vec<String>,
        changed: Vec<String>,
    },
    VfsMountDelta {
        added: Vec<String>,
        removed: Vec<String>,
        default_mount_before: Option<String>,
        default_mount_after: Option<String>,
    },
    // ... 其他 surface / injection / notice 保持不变 ...
}

pub struct ToolSurfaceEntry {
    pub name: String,
    pub description: String,
    pub parameters_schema: serde_json::Value,
    pub capability_key: Option<String>,
    pub source: Option<String>,           // "platform:*" / "platform_mcp:*" / "mcp:*"
    pub tool_path: Option<String>,
    pub state: ToolSurfaceState,
}

pub enum ToolSurfaceState {
    Initial,           // 首次 bootstrap tool schema 快照
    Added,
    Restored,
    Blocked,
    Removed,
    WhitelistAdded,
    WhitelistRemoved,
}
```

### 构造侧改造点

- `CapabilityDeltaFrameMetadata::from_delta` (`runtime_context_transition.rs:514-556`) 分解为 5 个独立 metadata builder，每个返回 `Option<ContextFrameSection>`。
- `RuntimeContextUpdateFrame::sections` (`runtime_context_transition.rs:409-418`) 聚合这些 `Option` 用 `filter_map` 拼装，空 section 自动省略。
- `ToolSchemaDeltaMetadata` 并入 `ToolSurfaceDelta`，统一使用 `ToolSurfaceEntry` 承载（初始化快照用 `state: Initial`，phase transition 用具体状态）。

## 8. Caveats / 其他

- **向后兼容**：PRD 已明确"破坏性全栈改造"，不需要并行字段。但持久化 event payload（`emit_capability_state_changed` 的 JSON）独立于 ContextFrameSection；若历史事件持久化需要可读（`RuntimeContextTransition::event_payload` in `capability_state.rs:101-158`），注意 event 结构是否需要同步调整。
- **Relay / vibe_kanban 消费者**：`execution-context-frames.md` §3.1 明确这两个 connector 目前**只消费 `rendered_system_prompt`**，不结构化消费 ContextFrame。因此 section kind 重构对它们无影响。
- **PiAgent 消费**：也不结构化消费 ContextFrame（通过 `bundle_id` 热更）。安全。
- **未找到**：没有跨 crate（非前端）的结构化消费 `CapabilityDeltaSection` 字段的代码；grep `unblocked_tool_paths` 仅在 frontend + spi + application 三处出现。

## 相关文件锚点

| File | 作用 |
|---|---|
| `crates/agentdash-spi/src/hooks/mod.rs:245-353` | ContextFrameSection enum 权威定义 |
| `crates/agentdash-spi/src/hooks/mod.rs:442-461` | `CapabilityDelta { added, removed }` key 集合 delta |
| `crates/agentdash-application/src/session/capability_state.rs:12-81` | `CapabilityStateDelta` 结构（SetDelta / NamedEntityDelta / VfsSurfaceDelta） |
| `crates/agentdash-application/src/session/hub/runtime_context_transition.rs:496-695` | `CapabilityDeltaFrameMetadata` 构造 / render |
| `crates/agentdash-application/src/session/tool_schema_notice.rs:76-169` | `ToolSchemaDeltaMetadata` 构造 / render |
| `crates/agentdash-application/src/session/surface_context_frames.rs:14-168` | `workspace_surface` / `skill_surface` / `hook_runtime_surface` 快照 |
| `crates/agentdash-application/src/session/prompt_pipeline.rs:423-442` | surface frame enqueue（is_owner_bootstrap 门控） |
| `frontend/src/features/session/model/contextFrame.ts:17-161` | TypeScript 镜像 |
| `frontend/src/features/session/ui/contextFrame/SectionRenderers.tsx:199+` | `CapabilityDeltaBody` 待删 |
