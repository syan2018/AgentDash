# Quality Guidelines

> AgentDash 后端质量规范，包含跨层 DTO 契约。

---

## Overview

- **Linting**: Clippy (Rust)
- **格式化**: rustfmt
- **检查命令**: `cargo check`, `cargo clippy`
- **API DTO 原则**: AgentDash 业务 HTTP JSON 默认使用 `snake_case`

代码提交前必须通过格式化和基础检查；新增或修改跨层 DTO 时，必须同时核对前端类型与序列化输出是否一致。

---

## Forbidden Patterns

| 禁止模式 | 原因 | 替代方案 |
|----------|------|----------|
| `unwrap()` | 可能导致 panic | 使用 `?` 或 `match` |
| `panic!()` | 不可恢复错误 | 返回 `Result` |
| 裸 `std::sync::Mutex` | 可能死锁 | 使用 `tokio::sync::Mutex`（异步） |
| 业务 HTTP DTO 混用 `camelCase` / `snake_case` | 破坏前后端字段契约 | 统一 `#[serde(rename_all = "snake_case")]` |
| 前端 mapper 兼容"旧字段 + 新字段" | 掩盖后端契约错误 | 先修正后端 DTO |

---

## Required Patterns

- 异步函数使用 `async/await`
- 共享状态使用 `Arc<Mutex<T>>`
- 错误类型实现 `thiserror::Error`
- AgentDash 自有业务 HTTP DTO 字段名使用 `snake_case`
- 外部协议桥接数据保持上游协议原样，不在桥接层擅自改名

### 外部协议桥接例外

ACP 协议对象、第三方 SDK 透传、明确标注为"桥接层"的响应对象允许保留外部字段风格。

---

## Scenario: API JSON 字段命名统一

### Contracts

- Response JSON：所有层级 `snake_case`
- Request JSON：前端发 `snake_case`，后端按 `snake_case` 反序列化
- 不允许：顶层 `snake_case` 内层 `camelCase`、前端 `fooBar ?? foo_bar` 长期兼容

### Good / Bad

```rust
// ✅ Good
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProjectAgentSummaryResponse {
    pub display_name: String,
    pub shared_context_mounts: Vec<ProjectAgentMountResponse>,
}
```

```ts
// ❌ Bad — 后端契约失效，前端被迫双风格
const mountId = raw.mountId ?? raw.mount_id ?? "";

// ✅ Correct — 后端保证 snake_case
const mountId = String(raw.mount_id ?? "");
```

---

## Session 执行状态持久化

### 原则

Session 执行状态必须持久化在 `SessionMeta.last_execution_status`，不允许靠扫 JSONL 历史或读内存 map 推断。

```rust
// ❌ 扫 JSONL 历史推断状态
let history = store.read_all(session_id).await?;

// ❌ 纯内存 map
let running = sessions.lock().await.get(id).map(|r| r.running);

// ✅ 每次 turn 开始/结束时写入 meta
session_meta.last_execution_status = "running".to_string();
store.write_meta(&session_meta).await?;
```

### SessionMeta 写回约束

- `last_event_seq`、`last_execution_status`、`last_turn_id`、`last_terminal_message` 是事件投影字段，只能单调前进
- `save_session_meta()` 更新普通元信息时，底层必须按"合并"语义处理，保证投影字段不回退

### 冷启动 continuation

- `SessionHub` 中有 session 条目 / broadcaster ≠ 执行器仍有 live runtime
- 必须以 `connector.has_live_session(session_id)` 为准
- 仓储恢复：connector 支持原生恢复时用 `restored_session_state`，否则退化为 continuation `system_context`

### 合法值

`last_execution_status` 只有五个合法值：`idle` / `running` / `completed` / `failed` / `interrupted`。

---

## Session Context 注入架构

### system_context vs prompt_blocks

| 字段 | 用途 | 展示 |
|------|------|------|
| `system_context` | owner 级上下文，每轮随 system prompt 注入 Agent | 不出现在用户消息流 |
| `prompt_blocks` resource block | `agentdash://project-context/` URI，仅前端展示锚点 | 渲染为 AcpOwnerContextCard |

**禁止**：在 `prompt_blocks` 中放 instruction text block；在用户消息文本中暴露技术 slot 标识。

### PromptSessionRequest 新增字段

`PromptSessionRequest` 跨多个 crate，新增字段后必须同步：
1. `ExecutionContext` 添加字段
2. `hub.rs::start_prompt_with_follow_up` 填充
3. 所有 `PromptSessionRequest { ... }` 字面量构造处补充（api / executor 测试 / local）

---

## Code Review Checklist

- [ ] 无 `unwrap()` 或已标记为安全
- [ ] 错误处理完善
- [ ] 异步函数正确使用 `.await`
- [ ] 业务 HTTP DTO 输出为 `snake_case`

---

*更新：2026-04-14 — 精简 Session 持久化/Prompt Lifecycle 冗余描述，保留核心约束*
