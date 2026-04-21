# Session Owner 四字段收口成 Sum Type

## Goal

将散布在多个核心 struct 里的 `(owner_type, project_id, story_id: Option<Uuid>, task_id: Option<Uuid>)`
四字段并列替换为 sum type `SessionOwnerCtx`，消除"只有 owner_type=Task 时 task_id 才应 Some"
这类靠注释/约定维持的非法状态表达可能。

## Why（问题陈述）

### 现状

同一组 session owner 身份信息在下列结构体里以**四字段并列**的形式反复出现：

| 结构体 | 位置 |
|---|---|
| `CapabilityResolverInput` | [resolver.rs:20-26](crates/agentdash-application/src/capability/resolver.rs#L20) |
| `SessionPlan` | [session/plan.rs:69](crates/agentdash-application/src/session/plan.rs#L69) |
| `Runtime*`（VFS / context 上下文） | [runtime.rs:77-79](crates/agentdash-application/src/runtime.rs#L77) |
| `SessionBinding` | [session_binding/entity.rs:18](crates/agentdash-domain/src/session_binding/entity.rs#L18) |
| `HookContext`（owner_type: String） | [hooks.rs:31](crates/agentdash-spi/src/hooks.rs#L31) |

### 合法组合只有 3 种，但类型允许 $2^3 = 8$

| `owner_type` | `story_id` | `task_id` | 合法 |
|---|---|---|---|
| Project | None | None | ✓ |
| Project | Some | any | ✗（当前无类型约束） |
| Story | Some | None | ✓ |
| Story | None | any | ✗ |
| Story | Some | Some | ✗ |
| Task | Some | Some | ✓ |
| Task | None / Some+None | — | ✗ |

代码里靠 `if let Some(story_id) = input.story_id` + "根据 owner_type 分支"等运行时 pattern
维持一致性，任何一处漏判都会产出错误的 MCP 注入或上下文注入。

### 典型副作用

- `CapabilityResolver::build_platform_mcp_config` 在 `PlatformMcpScope::Task` 分支里
  必须连续 `input.task_id?` + `input.story_id?`——两次 `?` 隐藏了"Task owner 理应两者都有"的约束
- orchestrator / advance_node 构造 input 时手动写 `story_id: None, task_id: None`，
  一旦 owner 为 Task 却忘了同步 story/task id，resolver 会静默退化为 Project 级注入
- HookContext 里 owner_type 是 String，进一步放宽——非 `project/story/task` 的字符串通过类型系统毫无阻拦

## Requirements

### 新增领域类型

在 `agentdash-domain` 里新增（优先考虑 `session_binding` 模块，因为 SessionBinding 本身就承载此语义）：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionOwnerCtx {
    Project { project_id: Uuid },
    Story   { project_id: Uuid, story_id: Uuid },
    Task    { project_id: Uuid, story_id: Uuid, task_id: Uuid },
}

impl SessionOwnerCtx {
    pub fn owner_type(&self) -> SessionOwnerType { ... }
    pub fn project_id(&self) -> Uuid { ... }
    pub fn story_id(&self) -> Option<Uuid> { ... }
    pub fn task_id(&self) -> Option<Uuid> { ... }
}
```

提供 getter 保留现有 call-site "我只关心 story_id 是否存在"的读取口径，避免一次性重写全部 match。

### 替换目标结构体

- `CapabilityResolverInput.{owner_type, project_id, story_id, task_id}` → `owner_ctx: SessionOwnerCtx`
- `SessionPlan` / `SessionPlanInput` 相关字段同步收口
- `Runtime*` 三件套（runtime.rs:77-79）
- `SessionBinding` 内部存储：owner_type + 关联外键也可以折叠成 `SessionOwnerCtx`（序列化兼容性见下）
- `HookContext.owner_type: String` → 要么收成 enum SessionOwnerType，要么直接 SessionOwnerCtx

### 序列化兼容性

- DB 层：`session_bindings` 表现已有 `owner_type` 与 `owner_id`(project/story/task) 列，类型收口只改应用层反序列化逻辑，**不改表结构**（否则迁移风险过大）
- API DTO：对前端的 owner_type string contract 不变，新增 from_domain/to_domain 转换层
- serde：`#[serde(tag = "kind")]` 保证 JSON 可读性（可选择固化为内部 `{"kind":"task", "project_id":..., "story_id":..., "task_id":...}` 约定）

### 消费者改造分类

改造 callsite 分三档，分阶段收口：

1. **resolver & session plan**（本任务主体）：这里收口收益最大，也最容易
2. **workflow runtime**（orchestrator / advance_node）：构造 input 的硬编码 `story_id: None` 完全由 owner 变体自动推导
3. **hook context / session_binding 持久化**：涉及序列化与 DB 映射层，需要显式转换函数

## Acceptance Criteria

- [ ] `SessionOwnerCtx` 落在 `agentdash-domain`（或 spi）并由所有上层结构体复用
- [ ] `grep -rn "pub story_id: Option<Uuid>" crates/` 在 session 路径里归零（DTO 层除外）
- [ ] `CapabilityResolverInput` 构造处不再出现 `story_id: None, task_id: None` 硬编码
- [ ] resolver 所有现有测试绿；新增 2-3 个测试固定 Project/Story/Task 三种 owner 的 MCP 注入边界
- [ ] `SessionBinding` 序列化 JSON 不破坏现有 session_bindings 行数据（迁移前后 round-trip 相等）
- [ ] `cargo build` / `cargo test` / `cargo clippy` 全绿

## Definition of Done

- 所有 session 关键结构体收口到 `SessionOwnerCtx`
- Non-trivial callsite（workflow orchestrator / capability resolver / hook dispatcher）迁移完毕
- 测试矩阵覆盖三种 owner × 三类主要调用场景
- Spec `tool-capability-pipeline.md` 的 `CapabilityResolverInput` 小节同步更新

## Technical Approach

### 分阶段推进

**PR1 — 领域类型 + 只改 resolver**
- 新增 `SessionOwnerCtx` 与 getter
- 重构 `CapabilityResolverInput`、更新 12 个构造 callsite
- 不动 SessionBinding 持久化 & HookContext
- **这一步独立有收益：resolver 最关键的那组硬编码消失**

**PR2 — SessionPlan / Runtime / orchestrator**
- `SessionPlanInput` / `SessionPlan` 内部字段收口
- `Runtime*` 三件套收口
- orchestrator / advance_node 不再硬编码 `story_id: None`

**PR3 — SessionBinding + HookContext 持久化层**
- 需要 domain ↔ DB 的显式转换层（`owner_type + owner_id` ↔ `SessionOwnerCtx`）
- 需要评估对 HookContext 序列化契约的影响
- HookContext.owner_type: String → `SessionOwnerType` 强类型化

### 依赖注入

无新 repo / service 依赖，纯类型重构。

### 回归风险

- 序列化：JSON tag 格式必须稳定；`SessionBinding` 持久化 round-trip 测试覆盖
- capability visibility：Task 级特殊的 `allowed_owner_types` 判定必须与 `owner_ctx.owner_type()` 保持等价
- workflow projection：orchestrator 构造 resolver input 时的 owner 推导需要审查（当前是从 parent_bindings 读第一个 binding 的 owner_type，单一 Project fallback）

## Decision (ADR-lite)

**Context**：session owner 身份信息在 5+ 个结构体里以四字段并列形式反复出现，靠
文档约定维持合法性。PR1（workflow_ctx 收口）已经验证"收口分散字段"是可行路径，
且改动面可控。

**Decision**：
1. 引入 `SessionOwnerCtx` sum type，放在 domain 层由所有上层消费
2. 三阶段推进（resolver → plan/runtime → 持久化）
3. 保留现有 DB 表结构与 API JSON 约定，仅做应用层结构体收口

**Consequences**：
- + 非法状态组合在类型层面被排除
- + callsite 构造 input 时无须手动填 `story_id: None` 等噪音
- + `SessionBinding` 的 owner 解析从"运行时 match 三种 owner_type 字符串"退化为类型系统自动
- − 持久化层需要新增转换函数（改动面比 workflow_ctx 收口大一圈）
- − 序列化兼容性需要测试覆盖以防回归

## Out of Scope

- 修改 DB 表结构（session_bindings / hook_records 等），持久化层只改应用层映射
- 修改前端 owner_type 字符串约定
- 重构 `TaskSessionRuntimeInputs.executor_source + executor_resolution_error` 的 Enum 化（另走顺手改动，已在 main 分支提交）
- 重构 `agent_declared_capabilities: Option<Vec<String>>` 的 NotDeclared vs Declared 区分（低影响，单独任务）

## Technical Notes

- 硬编码扫描依据：`grep -rn "pub story_id: Option<Uuid>" crates/` + `grep -rn "story_id: None" crates/`
- 参考实现：本次 `workflow_ctx` 收口 commit `4cf8c94`
- 关联文件：
  - [resolver.rs](crates/agentdash-application/src/capability/resolver.rs)（主战场）
  - [session/plan.rs](crates/agentdash-application/src/session/plan.rs)
  - [runtime.rs](crates/agentdash-application/src/runtime.rs)
  - [session_binding/entity.rs](crates/agentdash-domain/src/session_binding/entity.rs)
  - [hooks.rs](crates/agentdash-spi/src/hooks.rs)
- 前置任务：
  - `04-20-builtin-workflow-admin`（visibility 规则语义定稿）
  - `04-20-session-workflow-context-wiring`（workflow_ctx 收口经验）
- Spec：`.trellis/spec/backend/tool-capability-pipeline.md` — PR1 完成后更新 Input 字段说明
