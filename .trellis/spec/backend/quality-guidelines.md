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
| 在业务 HTTP DTO 上混用 `camelCase` / `snake_case` | 会破坏前后端字段契约，逼迫前端做双风格兼容 | 统一使用 `#[serde(rename_all = "snake_case")]` 或显式 `#[serde(rename = "...")]` |
| 让前端 mapper 兼容“旧字段 + 新字段”作为长期方案 | 掩盖后端契约错误，后续新增接口会继续扩散不一致 | 先修正后端 DTO，再把前端 mapper 收敛到单一字段风格 |

---

## Required Patterns

- 异步函数使用 `async/await`
- 共享状态使用 `Arc<Mutex<T>>`
- 错误类型实现 `thiserror::Error`
- AgentDash 自有业务 HTTP DTO 字段名使用 `snake_case`
- 外部协议桥接数据保持上游协议原样，不在桥接层擅自改名

### 外部协议桥接例外

以下场景允许保留外部字段风格，不受“业务 DTO 一律 `snake_case`”约束：

- ACP 协议对象
- 第三方 SDK / 上游服务直接透传的数据
- 明确声明为“桥接层”的响应对象

判断标准：

- 这是 AgentDash 自己定义的 REST 业务对象：用 `snake_case`
- 这是对外部协议的透传/包装：保持上游 schema，不另起一套命名

---

## Scenario: API JSON 字段命名统一

### 1. Scope / Trigger

- Trigger: 新增或修改 `crates/agentdash-api` 中的 REST 请求/响应 DTO
- Trigger: 前端 `frontend/src/types` 和 store mapper 需要直接消费业务 JSON
- Trigger: Project / Story / Session 等领域对象跨层流转时出现字段风格不一致

### 2. Signatures

- Rust:
  - `#[derive(Serialize, Deserialize)]`
  - `#[serde(rename_all = "snake_case")]`
- Frontend:
  - `api.get<Record<string, unknown>>()`
  - `mapXxx(raw): DomainType`

### 3. Contracts

- AgentDash 业务 HTTP Response JSON:
  - 顶层字段使用 `snake_case`
  - 嵌套对象字段也使用 `snake_case`
  - 数组元素中的对象字段也使用 `snake_case`
- AgentDash 业务 HTTP Request JSON:
  - 前端发送 `snake_case`
  - 后端按 `snake_case` 反序列化
- 不允许出现：
  - 顶层 `snake_case`、内层 `camelCase`
  - 同一路由不同分支返回不同字段风格
  - 前端靠 `fooBar ?? foo_bar` 长期兼容

### 4. Validation & Error Matrix

| 场景 | 期望 | 处理 |
|------|------|------|
| 新增业务 DTO | 输出全量 `snake_case` | 在 DTO 上声明 `rename_all = "snake_case"` |
| DTO 内嵌别的响应结构 | 内外层同样 `snake_case` | 复用的嵌套 DTO 也必须同风格 |
| 外部协议对象透传 | 保持外部 schema | 在代码中明确注释“桥接对象，不参与业务 DTO 命名规范” |
| 前端发现需要 `fooBar ?? foo_bar` | 视为后端契约缺陷 | 修正后端 DTO，并移除前端兼容 |

### 5. Good / Base / Bad Cases

#### Good

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProjectAgentSummaryResponse {
    pub display_name: String,
    pub shared_context_mounts: Vec<ProjectAgentMountResponse>,
}
```

```json
{
  "display_name": "项目默认 Agent",
  "shared_context_mounts": [
    { "mount_id": "spec", "display_name": "项目规范" }
  ]
}
```

#### Base

```rust
#[derive(Debug, Serialize)]
pub struct StorySessionDetailResponse {
    pub binding_id: String,
    pub session_id: String,
}
```
```
默认字段本身已经是 snake_case，可不额外声明 rename_all；
但一旦存在多词字段或嵌套 DTO，优先显式声明 rename_all = "snake_case"。
```

#### Bad

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAgentMountResponse {
    pub mount_id: String,
    pub display_name: String,
}
```

```json
{
  "context_snapshot": {
    "shared_context_mounts": [
      { "mountId": "spec", "displayName": "项目规范" }
    ]
  }
}
```

### 6. Tests Required

- Response DTO 序列化测试：
  - 断言输出包含 `display_name` / `shared_context_mounts`
  - 断言不存在 `displayName` / `sharedContextMounts`
- Request DTO 反序列化测试：
  - 使用 `snake_case` payload 成功反序列化
- 前端集成验证：
  - 页面不再依赖 camel/snake 双读 mapper
  - 真实接口返回可直接映射到 `frontend/src/types`

### 7. Wrong vs Correct

#### Wrong

```ts
const mountId = raw.mountId ?? raw.mount_id ?? "";
const displayName = raw.displayName ?? raw.display_name ?? "";
```

原因：
后端契约已经失效，前端被迫吞下双风格字段，后续任何新增对象都可能重复踩坑。

#### Correct

```ts
const mountId = String(raw.mount_id ?? "");
const displayName = String(raw.display_name ?? "");
```

前提：
后端必须先保证业务 DTO 统一输出 `snake_case`。

---

## Session 执行状态持久化规范

### 原则

Session 的执行状态（idle / running / completed / failed / interrupted）必须持久化在 `SessionMeta.last_execution_status`，不允许靠扫 JSONL 历史或读内存 map 来推断。

**禁止模式：**

```rust
// ❌ 扫 JSONL 历史推断状态 — O(N) IO，重启后内存无状态
let history = store.read_all(session_id).await?;
for notification in history { /* 推断 terminal */ }

// ❌ 纯内存 map — 重启后全部丢失，无法持久查询
let running = sessions.lock().await.get(id).map(|r| r.running);
```

**正确模式：**

```rust
// ✅ 每次 turn 开始/结束时写入 meta
session_meta.last_execution_status = "running".to_string();
store.write_meta(&session_meta).await?;
// turn 结束后
meta.last_execution_status = terminal_kind.state_tag().to_string(); // completed/failed/interrupted
store.write_meta(&meta).await?;
```

### 启动恢复

进程重启时必须调用 `executor_hub.recover_interrupted_sessions()`，将残留的 `running` 状态修正为 `interrupted`。云端和本机两个 binary 的启动流程都必须调用。

### 合法值枚举

`last_execution_status` 只有五个合法值：`idle` / `running` / `completed` / `failed` / `interrupted`。查询时非法值应 `unreachable!`。

---

## Testing Requirements

## Code Review Checklist

- [ ] 无 `unwrap()` 或已标记为安全
- [ ] 错误处理完善
- [ ] 异步函数正确使用 `.await`
- [ ] 共享状态使用 `Arc`
- [ ] 业务 HTTP DTO 输出为 `snake_case`
- [ ] 外部协议桥接对象是否已明确标注例外边界

---

## Session Context 注入架构规范

### system_context vs prompt_blocks

Project / Story 伴随会话的 owner 级上下文（身份声明 + context markdown）必须通过 `system_context` 字段注入，不得出现在 `prompt_blocks` 的用户消息侧。

| 字段 | 用途 | 展示 |
|------|------|------|
| `PromptSessionRequest.system_context` | owner 级上下文，每轮随 system prompt 注入 Agent | 不出现在用户消息流 |
| `prompt_blocks` 中的 resource block | `agentdash://project-context/` 或 `agentdash://story-context/` URI，仅作前端展示锚点 | 渲染为 AcpOwnerContextCard |

**禁止行为**：

- 禁止在 `prompt_blocks` 中放 instruction text block（`## Instruction 你是...`），这属于 system 层信息
- 禁止在用户消息文本中暴露 `当前来源摘要：project_core(project), ...` 等技术 slot 标识

### PromptSessionRequest 新增字段时的规范

`PromptSessionRequest` 是跨多个 crate 的核心结构，新增字段后必须：
1. 在 `connector.rs::ExecutionContext` 同步添加对应字段
2. 在 `hub.rs::start_prompt_with_follow_up` 的 `ExecutionContext` 构造处填充
3. 在所有 `PromptSessionRequest { ... }` 字面量构造处补充新字段（当前分布在 `agentdash-api`、`agentdash-executor` 测试、`agentdash-local` 共三处）
