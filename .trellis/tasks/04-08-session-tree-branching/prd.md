# Session Tree & Branching

> 状态：planning（大型架构任务，建议排期前先完成 context-compaction）
> 参考：`references/pi-mono/packages/coding-agent/src/core/session-manager.ts`

## 背景

当前 session 的消息历史是线性的。一旦做出某个决策（如 agent 走了错误的路径），用户只能继续向前或从头开始，无法回溯到分叉点重试。

pi-coding-agent 实现了树形 session 结构：
- 每条消息/工具调用/模型变更都是一个有 id 的树节点
- 用户可以从任意节点"fork"出一个新分支
- 可以在不同分支间切换导航
- compaction 也以分支为粒度操作

## 核心设计

### 1. Session Entry（树节点）

```rust
pub struct SessionEntry {
    pub id: String,                         // UUID
    pub parent_id: Option<String>,          // 根节点为 None
    pub timestamp: i64,
    pub entry_type: SessionEntryType,
    pub content: SessionEntryContent,
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
    // 在指定节点追加子节点（正常对话流）
    fn append(&mut self, parent_id: &str, entry: SessionEntry) -> String;

    // 从指定节点 fork（创建同级新分支）
    fn fork(&mut self, from_entry_id: &str) -> ForkResult;

    // 获取从根到指定节点的完整路径（用于重建 AgentContext）
    fn path_to(&self, entry_id: &str) -> Vec<&SessionEntry>;

    // 切换当前激活分支
    fn switch_to(&mut self, entry_id: &str) -> Result<(), SessionTreeError>;

    // 获取当前激活节点
    fn current_head(&self) -> &SessionEntry;
}

pub struct ForkResult {
    pub new_branch_root_id: String,  // fork 点的新分支 ID
    pub summary: Option<String>,     // 可选的分支摘要（从 fork 点之前的历史提炼）
}
```

### 3. BranchSummaryMessage

当导航切换到历史节点时，注入分支摘要让模型了解"这条分支做过什么"：

```rust
pub struct BranchSummaryMessage {
    pub role: String,           // "branch_summary"
    pub summary: String,        // 该分支的摘要
    pub from_entry_id: String,  // 分支起点
    pub branch_label: Option<String>, // 用户为该分支起的名字
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

## 前置依赖

- `04-08-context-compaction`：compaction 需要知道当前分支，树的节点 ID 系统需要先建立

## 规模评估

**XL 级任务**。涉及：
1. 消息历史数据模型的根本性变更（线性 → 树形）
2. `AgentContext` 重建逻辑（path_to → messages）
3. 持久化层重构
4. 前端分支树 UI
5. 所有现有依赖线性历史的逻辑需要适配

建议拆分为多个 PR 实施，先实现内存中的树结构，再做持久化，最后做前端 UI。

## 待讨论

- [ ] 是否支持分支命名？（用户为每个 fork 起名，便于管理）
- [ ] 分支历史的 retention policy？（无限保留 vs 自动清理 N 天前的死分支）
- [ ] 前端 UI 是否做树形可视化，还是简化为"分支列表"？
- [ ] fork 时是否自动生成分支摘要（需额外 LLM 调用），还是懒加载？
