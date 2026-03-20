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

## Testing Requirements

关键 API 需要：

1. 正常流程测试
2. 错误处理测试
3. 跨层契约测试

涉及 DTO 变更时，至少补一条字段命名断言，避免回归为混合风格。

---

## Code Review Checklist

- [ ] 无 `unwrap()` 或已标记为安全
- [ ] 错误处理完善
- [ ] 异步函数正确使用 `.await`
- [ ] 共享状态使用 `Arc`
- [ ] 业务 HTTP DTO 输出为 `snake_case`
- [ ] 外部协议桥接对象是否已明确标注例外边界
