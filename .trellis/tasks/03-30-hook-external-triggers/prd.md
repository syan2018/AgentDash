# Hook 引擎扩展：Session 状态容器 + 外部触发入口

> 状态：待讨论（parking）
> 前置：`03-30-workflow-hook-simplification` 已完成后端+前端精简

## 背景

当前 Hook 引擎有两个限制：

1. **无状态**：每次规则评估只能看到当前 snapshot + 当前 query，没有跨事件的记忆。无法实现"改了 10 个文件未提交就提醒"这类需要累积观察的行为。
2. **仅 Agent 生命周期触发**：10 种 `HookTrigger` 全部是 Agent 内部事件（SessionStart → BeforeStop → SessionTerminal 等），外部系统无法主动向活跃 session 注入信息。

本 PRD 将这两个方向统一规划。

---

## 核心设计：规则引擎的三层模型

### 第一层：无状态规则（已有）

```
事件 → matches(snapshot, query) → apply(resolution)
```

- 没有跨事件的记忆
- 每次评估只看当前 snapshot + 当前 query
- **现有能力**：BeforeTool 路径重写、BeforeStop checklist gate、supervised 审批、SubagentResult 回流

### 第二层：有状态规则（本次新增 — Session 状态容器）

```
事件 → matches(snapshot, query, state) → apply(resolution, state)
```

- `state` 是 session 级的通用 KV 容器
- 规则的 `matches` 可以**读** state 决定是否命中
- 规则的 `apply` 可以**写** state 记录信息，也可以写 resolution 注入上下文
- state 生命周期 = session 生命周期，不持久化

### 第三层：外部触发（本次新增 — 新 HookTrigger 类型）

```
外部系统 → 构造 HookEvaluationQuery → 投递到目标 session 的 runtime → 规则评估
```

- 触发源不是当前 Agent 的生命周期
- 与第一层/第二层共享同一套规则引擎
- 区别仅在于 trigger 枚举值和 payload 结构

---

## 设计 A：Session 状态容器（`HookSessionState`）

### 数据结构

```rust
/// Session 级有状态容器，生命周期 = session
/// 规则引擎可读可写，session 结束即丢弃
pub struct HookSessionState {
    entries: HashMap<String, serde_json::Value>,
}

impl HookSessionState {
    pub fn get(&self, key: &str) -> Option<&serde_json::Value>;
    pub fn set(&mut self, key: &str, value: serde_json::Value);
    pub fn remove(&mut self, key: &str);
    pub fn keys(&self) -> impl Iterator<Item = &str>;
}
```

### 规则引擎签名变化

```rust
// 当前
pub(crate) struct HookEvaluationContext<'a> {
    pub(crate) snapshot: &'a SessionHookSnapshot,
    pub(crate) query: &'a HookEvaluationQuery,
}

// 变为
pub(crate) struct HookEvaluationContext<'a> {
    pub(crate) snapshot: &'a SessionHookSnapshot,
    pub(crate) query: &'a HookEvaluationQuery,
    pub(crate) state: &'a HookSessionState,          // 只读引用
}

// apply 签名变化
pub(crate) struct NormalizedHookRule {
    key: &'static str,
    trigger: HookTrigger,
    matches: fn(&HookEvaluationContext<'_>) -> bool,
    apply: fn(&HookEvaluationContext<'_>, &mut HookResolution, &mut HookSessionState),
    //                                                          ^^^^^^^^^^^^^^^^^^^^^^^^ 新增
}
```

### 状态容器的宿主

`HookSessionState` 由 `HookSessionRuntime` 持有（和 snapshot、diagnostics、trace 同级）：

```rust
pub struct HookSessionRuntime {
    session_id: String,
    provider: Arc<dyn ExecutionHookProvider>,
    snapshot: RwLock<SessionHookSnapshot>,
    state: RwLock<HookSessionState>,        // 新增
    diagnostics: RwLock<Vec<HookDiagnosticEntry>>,
    trace: RwLock<Vec<HookTraceEntry>>,
    pending_actions: RwLock<Vec<HookPendingAction>>,
    revision: AtomicU64,
}
```

### 示例：10 个文件未提交提醒

```rust
// 规则 A: AfterTool — 记录被修改的文件
NormalizedHookRule {
    key: "state:track_modified_files",
    trigger: HookTrigger::AfterTool,
    matches: |ctx| {
        ctx.query.tool_name.as_deref()
            .is_some_and(|name| name.ends_with("write") || name.ends_with("edit"))
    },
    apply: |ctx, _resolution, state| {
        let file_path = extract_tool_arg(ctx.query.payload.as_ref(), "file_path")
            .unwrap_or("unknown");
        let mut files: Vec<String> = state.get("modified_files")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        if !files.contains(&file_path.to_string()) {
            files.push(file_path.to_string());
        }
        state.set("modified_files", serde_json::to_value(&files).unwrap());
    },
}

// 规则 B: AfterTool — git commit 时清空
NormalizedHookRule {
    key: "state:clear_on_commit",
    trigger: HookTrigger::AfterTool,
    matches: |ctx| {
        ctx.query.tool_name.as_deref().is_some_and(|n| n.ends_with("shell_exec"))
            && extract_tool_arg(ctx.query.payload.as_ref(), "command")
                .is_some_and(|cmd| cmd.contains("git commit"))
    },
    apply: |_ctx, _resolution, state| {
        state.set("modified_files", serde_json::json!([]));
    },
}

// 规则 C: AfterTurn — 检查阈值并注入提醒
NormalizedHookRule {
    key: "state:uncommitted_files_warning",
    trigger: HookTrigger::AfterTurn,
    matches: |ctx| {
        ctx.state.get("modified_files")
            .and_then(|v| v.as_array())
            .is_some_and(|files| files.len() >= 10)
    },
    apply: |_ctx, resolution, _state| {
        resolution.injections.push(HookInjection {
            slot: "constraint".to_string(),
            content: "你已修改超过 10 个文件但尚未提交。建议先 git commit 保存进度，避免丢失工作。".to_string(),
            source: "builtin:uncommitted_files_warning".to_string(),
        });
    },
}
```

### 设计要点

| 点 | 决策 |
|----|------|
| 生命周期 | session 级，不持久化 |
| Schema | 自由 KV（`HashMap<String, Value>`），由规则约定 key 语义 |
| 可观测性 | state 快照可通过 `HookSessionRuntimeAccess` 暴露给 trace/前端 |
| 并发安全 | `RwLock<HookSessionState>`，evaluate 时先获取读锁做 matches，再获取写锁做 apply |
| 防循环 | state 写入不会自动触发新的评估；如需"state 变更触发规则"应在下一个自然事件时检查 |

---

## 设计 B：外部触发入口

### 新增 HookTrigger 类型

```rust
pub enum HookTrigger {
    // 现有（Agent 生命周期）
    SessionStart, UserPromptSubmit, BeforeTool, AfterTool,
    AfterTurn, BeforeStop, SessionTerminal,
    BeforeSubagentDispatch, AfterSubagentDispatch, SubagentResult,

    // 新增（外部触发）
    ExternalMessage,      // 跨 Agent/Session 消息回流、人工注入、webhook
    StateChange,          // 业务实体状态变更
    ConditionMet,         // 内置条件触发
}
```

### 触发入口

统一通过 `HookSessionRuntimeAccess.evaluate()` 提交，外部系统只需构造正确的 `HookEvaluationQuery`：

```rust
// 示例：CI 通知构建失败
runtime.evaluate(HookEvaluationQuery {
    session_id: "sess-123".to_string(),
    trigger: HookTrigger::ExternalMessage,
    payload: Some(json!({
        "source": "ci",
        "event": "build_failed",
        "message": "CI build #456 failed: 3 test failures",
        "url": "https://ci.example.com/builds/456",
    })),
    ..Default::default()
}).await?;

// 示例：Task 状态转移
runtime.evaluate(HookEvaluationQuery {
    session_id: "sess-123".to_string(),
    trigger: HookTrigger::StateChange,
    payload: Some(json!({
        "entity_type": "task",
        "entity_id": "...",
        "old_status": "implementing",
        "new_status": "reviewing",
    })),
    ..Default::default()
}).await?;
```

### 外部触发 + 状态容器联动

外部触发也可以读写 state，实现更复杂的跨系统协调：

```rust
// 规则：收到 CI 失败通知 → 记录到 state + 注入提醒
NormalizedHookRule {
    key: "external:ci_failure_notice",
    trigger: HookTrigger::ExternalMessage,
    matches: |ctx| {
        extract_payload_str(ctx.query.payload.as_ref(), "source") == Some("ci")
            && extract_payload_str(ctx.query.payload.as_ref(), "event") == Some("build_failed")
    },
    apply: |ctx, resolution, state| {
        let message = extract_payload_str(ctx.query.payload.as_ref(), "message")
            .unwrap_or("CI build failed");
        state.set("last_ci_failure", ctx.query.payload.clone().unwrap_or_default());
        resolution.injections.push(HookInjection {
            slot: "context".to_string(),
            content: format!("## CI 构建失败通知\n{message}\n请优先处理构建问题。"),
            source: "external:ci".to_string(),
        });
    },
}
```

---

## 动机场景（更新）

### 第二层场景（有状态规则 — 通过 state 容器实现）

| 场景 | state key | 写入时机 | 检查时机 |
|------|-----------|---------|---------|
| N 个文件未提交提醒 | `modified_files` | AfterTool(Write/Edit) | AfterTurn |
| 连续失败提醒 | `consecutive_failures` | AfterTool(失败时+1) | AfterTurn |
| Token 消耗预警 | `token_usage` | AfterTurn | AfterTurn |
| Companion 回流跟踪 | `pending_reviews` | SubagentResult | BeforeStop |

### 第三层场景（外部触发 — 通过新 trigger 类型实现）

| 场景 | Trigger | Payload 关键字段 |
|------|---------|-----------------|
| CI 通知 | ExternalMessage | `source: "ci"`, `event`, `message` |
| 人工前端注入 | ExternalMessage | `source: "user"`, `message` |
| Webhook 回调 | ExternalMessage | `source: "webhook"`, 自定义 |
| Task 状态转移 | StateChange | `entity_type`, `entity_id`, `old_status`, `new_status` |
| Story 状态更新 | StateChange | 同上 |
| WorkspaceBinding 变化 | StateChange | `entity_type: "binding"`, `status` |
| 定时器到期 | ConditionMet | `condition: "session_timeout"`, `elapsed_ms` |
| 资源预警 | ConditionMet | `condition: "token_budget"`, `usage`, `limit` |

---

## 待讨论

### 状态容器相关
- [ ] state 是否需要支持 TTL（自动过期的 key）？
- [ ] state 变更是否需要记录到 trace 中？（倾向是：作为 diagnostic 记录变更的 key 和新值摘要）
- [ ] 是否需要"state 变更触发重新评估"能力？还是只在下一个自然事件时检查？
- [ ] state 的 key 命名约定？（建议 `domain:key` 格式，如 `files:modified`、`ci:last_failure`）

### 外部触发相关
- [ ] 三种外部触发类型的粒度是否合适？是否需要合并/拆分？
- [ ] 每种触发的 payload schema 具体约定？
- [ ] 外部触发的投递入口：API endpoint？内部 channel？两者都要？
- [ ] ExternalMessage 是否需要和现有的 SubagentResult 合并？（倾向不合并——SubagentResult 有特定的 adoption_mode 语义）
- [ ] ConditionMet 由谁负责检测条件？runtime 轮询还是外部推送？
- [ ] 前端需要展示外部触发事件和 state 变更吗？

---

## 实施建议

建议分两个 PR：

### PR 1：Session 状态容器
1. `HookSessionState` 数据结构
2. `HookEvaluationContext` / `NormalizedHookRule` 签名扩展
3. `HookSessionRuntime` 集成 state
4. 1-2 个示例规则（如 uncommitted files warning）
5. 单元测试

### PR 2：外部触发入口
1. `HookTrigger` 新增枚举值
2. 投递入口（API 或内部 channel）
3. 1-2 条示例规则
4. 与 state 容器的联动示例
5. 单元测试

---

## 参考

- 本任务原始讨论见 `03-30-workflow-hook-simplification/prd.md` 的 P7 章节
- Claude Code hook 系统参考见 `03-30-workflow-hook-simplification/ref-claude-code-hooks.md`
  - CC 的 `TeammateIdle`、`TaskCreated`、`TaskCompleted` 可作为跨 Agent 触发的参考
  - CC 的 `FileChanged`、`ConfigChange` 可作为状态变更触发的参考
  - CC 的 `Stop` hook 的 `stop_hook_active` 防循环机制可参考
