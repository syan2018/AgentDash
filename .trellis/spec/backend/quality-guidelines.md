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

### SessionMeta 写回约束

- `SessionMeta.last_event_seq`、`last_execution_status`、`last_turn_id`、`last_terminal_message` 属于**事件投影字段**，只能随事件追加单调前进，不能被旧快照整块写回覆盖。
- `save_session_meta()` 若用于更新 `executor_session_id`、`companion_context`、`visible_canvas_mount_ids` 等普通元信息，底层持久化层必须按“合并”语义处理，至少保证：
  - `last_event_seq` 不回退
  - 已落库的 terminal 状态不被旧的 `running` / `idle` 覆盖
  - 不相关的普通字段不会因为旧快照缺值被清空
- 如果需要更新事件投影字段，优先通过 `append_event()` 产生对应事件，而不是手工改 meta。

### 启动恢复

进程重启时必须调用 `executor_hub.recover_interrupted_sessions()`，将残留的 `running` 状态修正为 `interrupted`。云端和本机两个 binary 的启动流程都必须调用。

### 冷启动 continuation 判定

- `SessionHub` 中“是否已有 session 条目 / broadcaster / backlog 订阅”**不等于**执行器仍有 live runtime。
- continuation 是否可直接续跑，必须以 connector 级 `has_live_session(session_id)` 为准；否则打开旧会话页面、补写历史事件、或仅建立订阅时，都可能把冷启动误判成热续跑。
- 只有以下两种情况可以跳过仓储恢复：
  - 执行器原生 follow-up token / `executor_session_id` 仍然可用
  - connector 明确报告该 session 在当前进程内仍有 live runtime
- 其余“无 live runtime 但已有历史事件”的场景，一律视为 repository rehydrate。

### 仓储恢复优先级

- repository rehydrate 不是单一策略：
  - 若 connector 支持原生仓储恢复，应优先把 `session_events` 重建成消息历史，放入 `ExecutionContext.restored_session_state`
  - 若 connector 不支持原生恢复，才退化为 continuation `system_context` 文本
- owner 上下文（project/story/task）在冷启动恢复时仍属于 `system_context`，但不得再次把 owner resource block 当成新的用户消息注入。
- `agentdash://project-context/*` / `story-context/*` / `task-context/*` 这类 owner 展示块在恢复消息历史时必须过滤，否则会把首轮 bootstrap 内容重复回灌给模型。

### 场景：Session Prompt Lifecycle 与 Repository Rehydrate

#### 1. Scope / Trigger

- 触发点：
  - `POST /api/sessions/:id/prompt`
  - 已存在 session 在页面 reopen 后再次发送 prompt
  - 后端或本地执行器重启后，用户继续同一条 owner session
- 该场景属于 infra/cross-layer 契约，必须同时约束：
  - API route 生命周期判定
  - Session 仓储恢复模式
  - Connector 恢复能力
  - 前端 reopen 后的实际行为

#### 2. Signatures

- 生命周期判定入口：
  - `crates/agentdash-application/src/session/types.rs`
  - `resolve_session_prompt_lifecycle(...) -> SessionPromptLifecycle`
- Connector 能力：
  - `AgentConnector::supports_repository_restore(executor: &str) -> bool`
  - `AgentConnector::has_live_session(session_id: &str) -> bool`
- 执行上下文：
  - `ExecutionContext.restored_session_state: Option<RestoredSessionState>`
- 持久化字段：
  - `sessions.bootstrap_state`
  - `session_events(session_id, event_seq, notification_json, ...)`

#### 3. Contracts

- 前端 contract：
  - 会话页发送请求时只提交 `promptBlocks`
  - owner bootstrap 的判定与注入不得由前端手写
- API contract：
  - `OwnerBootstrap`：route 负责附加 owner resource block 展示锚点 + `system_context`
  - `RepositoryRehydrate(SystemContext)`：只补 continuation `system_context`，不再追加 owner resource block
  - `RepositoryRehydrate(ExecutorState)`：通过 `ExecutionContext.restored_session_state` 下发消息历史
  - `Plain`：视为热续跑，不做额外 bootstrap / rehydrate
- Repository contract：
  - `build_restored_session_messages(session_id)` 必须按 `session_events` 重建 user / assistant / tool_call / tool_result
  - 必须过滤 `agentdash://project-context/*` / `story-context/*` / `task-context/*`
- Migration contract：
  - PostgreSQL migration 必须显式创建 `sessions.bootstrap_state`
  - 不允许只依赖 repository `initialize()` 的运行时补列

#### 4. Validation & Error Matrix

| 场景 | 期望 | 错误表现 |
|------|------|----------|
| broadcaster/backlog 已存在，但执行器无 live runtime | 进入 `RepositoryRehydrate` | 误判 `Plain`，导致恢复链路被跳过 |
| connector 支持仓储恢复 | 下发 `restored_session_state` | 退化成 continuation 文本，丢失 tool / assistant 历史 |
| connector 不支持仓储恢复 | 使用 continuation `system_context` | 直接报不支持，无法继续旧会话 |
| owner resource block 未过滤 | 模型历史中无重复 owner bootstrap | reopen/restart 后再次注入 project/story/task 上下文 |
| migration 缺 `bootstrap_state` | 启动恢复与查询正常 | 启动期 warn/失败：`字段 "bootstrap_state" 不存在` |

#### 5. Good / Base / Bad Cases

- Good：
  - 首次 owner prompt 只出现一次 owner context block
  - reopen 同一 session 后继续发送，不新增第二份 owner context block
  - restart 后 reopen 同一 session，再发 prompt，owner context block 总数保持不变
- Base：
  - connector 不支持 `restored_session_state` 时，允许用 continuation 文本兜底继续会话
- Bad：
  - 以 `SessionHub.sessions` / broadcaster 是否存在来判断热续跑
  - 将 owner resource block 当普通历史消息重建回执行器
  - 只在 repository `initialize()` 里补 `bootstrap_state`，不做 migration

#### 6. Tests Required

- `cargo test -p agentdash-application session::hub::tests -- --nocapture`
  - 断言 session 仓储历史可重建，且 owner block 不被重复回灌
- `cargo test -p agentdash-api session_prompt_lifecycle -- --nocapture`
  - 断言 route 生命周期判定符合 `OwnerBootstrap / RepositoryRehydrate / Plain`
- `cargo test -p agentdash-executor prompt_restores_repository_messages_before_new_user_prompt -- --nocapture`
  - 断言支持恢复的 connector 会先消费仓储消息，再追加新用户 prompt
- 手工前端验证：
  - 首次 prompt
  - reopen 后再次 prompt
  - restart 后 reopen 再次 prompt
  - 三步都要确认 owner context 卡片不重复出现

#### 7. Wrong vs Correct

##### Wrong

```rust
if session_hub.has_session(session_id) {
    SessionPromptLifecycle::Plain
} else {
    SessionPromptLifecycle::OwnerBootstrap
}
```

##### Correct

```rust
if connector.has_live_session(session_id).await {
    SessionPromptLifecycle::Plain
} else {
    resolve_session_prompt_lifecycle(meta, connector.supports_repository_restore(executor))
}
```

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
