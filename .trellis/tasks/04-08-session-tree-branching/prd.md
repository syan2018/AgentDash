# Session Tree & Branching

> 状态：planning（大型架构任务）
> 前置：`04-08-context-compaction`（已完成）、`04-09-agentmessage-identity-projection`（P1/P2 已完成）
> 参考：`references/pi-mono/packages/coding-agent/src/core/session-manager.ts`

## 背景

当前 session 的消息历史是线性的。一旦做出某个决策（如 agent 走了错误的路径），用户只能继续向前或从头开始，无法回溯到分叉点重试。

pi-coding-agent 实现了树形 session 结构：
- 每条消息/工具调用/模型变更都是一个有 id 的树节点
- 用户可以从任意节点"fork"出一个新分支
- 可以在不同分支间切换导航
- compaction 也以分支为粒度操作

## 已有基建（来自 identity-projection 任务）

`04-09-agentmessage-identity-projection` P1/P2 已落地的基础设施可直接复用：

| 类型 | 位置 | 与 branching 的关系 |
|------|------|-------------------|
| `MessageRef { turn_id, entry_index }` | `agentdash-agent-types/src/message.rs` | **fork 锚点** — fork 操作可引用 `MessageRef` 指定分叉点，无需独立 entry ID |
| `ProjectedTranscript` / `ProjectedEntry` | `agentdash-agent-types/src/projection.rs` | **分支路径投影** — `path_to(entry_id)` 的输出可直接复用 `ProjectedTranscript` |
| `ProjectionKind` | 同上 | 可扩展 `BranchSummary` variant，标记分支摘要消息 |
| `CompactionSummary.compacted_until_ref` | `message.rs` | **分支感知压缩** — compaction 按 ref 而非计数切割，分支间不串扰 |
| `build_projected_transcript_from_events` | `continuation.rs` | **分支重建** — 从事件流重建某条分支的投影 transcript |

### 关键设计衔接

1. **SessionEntry.id vs MessageRef**：原 PRD 设计了独立的 `SessionEntry.id: String (UUID)`。可以考虑复用 `MessageRef` 作为节点引用，避免两套 ID 体系。`MessageRef { turn_id, entry_index }` 已在 persistence 层天然稳定，且 continuation.rs 已为每条恢复消息分配 ref。

2. **BranchSummaryMessage → ProjectionKind 扩展**：原 PRD 的 `BranchSummaryMessage` 可以作为 `AgentMessage` 的新 variant（类似 `CompactionSummary`），同时在 `ProjectionKind` 中增加 `BranchSummary` 变体标记其投影来源。

3. **P3 runtime 携带 ref**：branching 的 `fork` / `switch_to` 操作需要 runtime 内部的消息有稳定引用。这正是 identity-projection P3 的驱动场景。建议 branching 实施时优先推进 P3（`AgentState.messages` 升级为 `Vec<ProjectedEntry>`），使 fork 点定位和分支路径重建能直接在 runtime 完成。

## 核心设计

### 1. Session Entry（树节点）

```rust
/// 树节点 — 每条消息/事件对应一个节点
pub struct SessionEntry {
    /// 节点引用 — 复用 MessageRef 而非独立 UUID
    pub ref_id: MessageRef,
    /// 父节点（根节点为 None）
    pub parent_ref: Option<MessageRef>,
    pub timestamp: i64,
    pub entry_type: SessionEntryType,
    /// 节点对应的投影消息
    pub projected: ProjectedEntry,
}

pub enum SessionEntryType {
    UserMessage,
    AssistantMessage,
    ToolCall,
    ToolResult,
    ModelChange,
    CompactionSummary,
    BranchSummary,
}
```

### 2. Session Tree 操作

```rust
pub trait SessionTree {
    /// 在指定节点追加子节点（正常对话流）
    fn append(&mut self, parent_ref: &MessageRef, entry: SessionEntry) -> MessageRef;

    /// 从指定节点 fork（创建同级新分支）
    fn fork(&mut self, from_ref: &MessageRef) -> ForkResult;

    /// 获取从根到指定节点的完整路径（用于重建 AgentContext）
    fn path_to(&self, ref_id: &MessageRef) -> ProjectedTranscript;

    /// 切换当前激活分支
    fn switch_to(&mut self, ref_id: &MessageRef) -> Result<(), SessionTreeError>;

    /// 获取当前激活节点
    fn current_head(&self) -> &SessionEntry;
}

pub struct ForkResult {
    pub new_branch_root: MessageRef,
    pub summary: Option<String>,
}
```

### 3. BranchSummaryMessage

当导航切换到历史节点时，注入分支摘要让模型了解"这条分支做过什么"：

```rust
// AgentMessage 新增 variant
BranchSummary {
    summary: String,
    from_ref: MessageRef,       // 分支起点
    branch_label: Option<String>,
    timestamp: Option<u64>,
}
```

对应 `ProjectionKind` 扩展：

```rust
pub enum ProjectionKind {
    Transcript,
    CompactionSummary,
    BranchSummary,  // 新增
}
```

### 4. 新增 HookTrigger

```rust
pub enum HookTrigger {
    // ...已有...

    /// 即将 fork，可取消，可自定义摘要
    BeforeFork,

    /// fork 完成后通知
    AfterFork,

    /// 即将导航到历史节点，可取消
    BeforeTreeNavigation,

    /// 导航完成后通知
    AfterTreeNavigation,
}
```

### 5. 用户触发

- `/fork` slash command：从当前节点 fork 出新分支
- `/branches` slash command：列出所有分支
- 前端：消息历史旁显示分支树可视化，支持点击切换

### 6. 持久化

Session tree 序列化到 session 存储，结构为 JSON 树。每次 append/fork/switch 操作后持久化。

---

## 实施路径（建议）

### Phase A：P3 Runtime 携带 ref

- `AgentState.messages` 从 `Vec<AgentMessage>` 升级为 `Vec<ProjectedEntry>`
- `AgentContext.messages` 同步升级
- Bridge 边界做 `map(|e| &e.message)` 降级
- Compaction 引擎改为操作 `&[ProjectedEntry]`，原生产出 `compacted_until_ref`
- 影响文件：`agent.rs`、`agent_loop.rs`、`context.rs`、`decisions.rs`、`compaction/mod.rs`、`rig_bridge.rs`

### Phase B：树结构 + fork/switch

- 定义 `SessionEntry`、`SessionTree` trait
- 实现内存树结构
- `path_to` 返回 `ProjectedTranscript`
- fork 和 switch 操作

### Phase C：持久化 + 事件集成

- Session tree 持久化到 DB
- fork/switch 产出 `SessionNotification` 事件
- Hook trigger 集成

### Phase D：前端 UI

- 分支树可视化
- fork/switch 交互

## 前置依赖

- ~~`04-08-context-compaction`~~：已完成
- ~~`04-09-agentmessage-identity-projection` P1/P2~~：已完成
- `04-09-agentmessage-identity-projection` P3：runtime 携带 ref（建议作为 Phase A）

## 规模评估

**XL 级任务**。涉及：
1. ~~消息身份模型~~ → 已有 `MessageRef` + `ProjectedTranscript`
2. Runtime 消息升级（Phase A，中等规模）
3. 树结构数据模型和操作（Phase B，中等规模）
4. 持久化层适配（Phase C，中等规模）
5. 前端分支树 UI（Phase D，大规模）

建议按 Phase A → B → C → D 顺序实施，每个 Phase 独立可交付。

## 待讨论

- [ ] 是否支持分支命名？（用户为每个 fork 起名，便于管理）
- [ ] 分支历史的 retention policy？（无限保留 vs 自动清理 N 天前的死分支）
- [ ] 前端 UI 是否做树形可视化，还是简化为"分支列表"？
- [ ] fork 时是否自动生成分支摘要（需额外 LLM 调用），还是懒加载？
- [ ] `SessionEntry.ref_id` 直接复用 `MessageRef` 还是包装一层 `EntryId` 预留扩展空间？
