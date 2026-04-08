# Context Compaction 上下文压缩

> 状态：planning
> 参考：`references/pi-mono/packages/coding-agent/src/core/compaction/`

## 背景

当前 session 的消息历史无限增长，长对话下会：
1. 超出 LLM context window 限制导致请求失败
2. 导致 prompt 成本线性增长
3. 早期消息与当前任务无关但仍被全量发送

pi-coding-agent 通过 compaction 系统解决这个问题：
- 实时统计 token 用量
- 超过阈值时自动触发压缩（生成摘要 → 替换旧消息）
- 压缩前后有 hook 可介入
- 压缩记录作为特殊消息类型保留在历史中

---

## 设计

### 1. Token 计数

在 `AgentContext` 或 agent loop 层维护当前对话的 token 估算值：

```rust
pub struct ContextTokenStats {
    pub system_prompt_tokens: u32,
    pub messages_tokens: u32,
    pub total_tokens: u32,
    pub model_context_limit: u32,  // 从 model registry 获取
}
```

计数方式：使用 tiktoken 或模型提供商的 token counting API（精确），或基于字符数的快速估算（粗略，用于触发判断）。

触发阈值：`total_tokens >= model_context_limit * threshold_ratio`（默认 0.75）。

### 2. 压缩流程

```
1. 检测到 token 超阈值
2. 触发 BeforeCompact hook（可取消）
3. 确定 cut point（保留最近 N 条消息不压缩）
4. 对 cut point 之前的消息调用 LLM 生成摘要
5. 替换旧消息为 CompactionSummaryMessage
6. 触发 AfterCompact hook（通知）
7. 更新 token stats
```

**Cut point 策略**：保留最近的 `keep_last_n` 条消息（含完整 tool call / result 对），默认保留最近 20 条。cut point 必须在完整的 tool call/result 对边界处，不能截断中间。

### 3. 摘要生成

对需要压缩的消息段，向 LLM 发送专用 compaction prompt：

```
你是一个会话摘要助手。以下是一段 AI 编程助手与用户的对话历史，请生成一份简洁的结构化摘要，包含：
- 已完成的主要工作
- 做出的关键决策和原因
- 当前状态和待处理事项
- 重要的技术发现

对话历史：
{messages}
```

摘要结果写入 `CompactionSummaryMessage`。

### 4. CompactionSummaryMessage

在消息类型体系中新增：

```rust
pub struct CompactionSummaryMessage {
    pub role: String,           // "compaction_summary"
    pub summary: String,        // LLM 生成的摘要内容
    pub tokens_before: u32,     // 压缩前的 token 数
    pub tokens_after: u32,      // 压缩后的 token 数
    pub messages_compacted: u32, // 压缩了多少条消息
    pub timestamp: i64,
}
```

此消息**包含在**发给 LLM 的上下文中（让模型知道有历史摘要），但在前端以特殊样式渲染（显示压缩信息而非原始内容）。

### 5. 新增 HookTrigger

```rust
pub enum HookTrigger {
    // ...已有...

    /// 即将执行压缩，可取消
    BeforeCompact,

    /// 压缩已完成，通知
    AfterCompact,
}
```

**BeforeCompact payload**：
```json
{
  "tokens_current": 45000,
  "tokens_limit": 64000,
  "messages_to_compact": 85
}
```

**BeforeCompact resolution**：支持 `cancel: true`（跳过本次压缩），用于人工审查场景。

### 6. 自动 vs 手动压缩

- **自动**：token stats 检查在每次 `transform_context` 时触发（即每次 LLM 调用前）
- **手动**：通过 `/compact` slash command 或前端按钮立即触发
- 手动触发走同一套流程，只是跳过阈值检测

### 7. 配置项

```toml
[compaction]
enabled = true
threshold_ratio = 0.75      # token 使用率超过此值触发
keep_last_n = 20            # 保留最近 N 条消息不压缩
compaction_model = null     # null 表示使用与当前 session 相同的模型
```

配置通过 session settings 或 workspace config 读取。

---

## 实施顺序建议

1. Token 统计基础设施（`ContextTokenStats`，接入 `transform_context`）
2. `CompactionSummaryMessage` 消息类型 + 前端渲染
3. 压缩流程核心（cut point 检测、摘要生成、消息替换）
4. `BeforeCompact` / `AfterCompact` hooks
5. `/compact` 手动触发 slash command
6. 配置项读取

---

## 与 Session Tree & Branching 的关系

compaction 是 session tree 的前置依赖：
- session tree 里每个节点是一个"分支点"，compaction 需要知道当前在哪个分支
- 建议先完成 context-compaction，再做 session-tree-branching
