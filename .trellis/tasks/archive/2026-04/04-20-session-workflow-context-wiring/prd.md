# Session 创建链路装配工作流上下文

## Goal

清除 session 创建路径上所有 `has_active_workflow: false, workflow_capabilities: None` 的硬编码，替换为从 **agent link / session owner 实际绑定的 lifecycle** 解析出的真实工作流上下文；使得 `CapabilityResolver` 在 session 创建时就能正确授予 workflow 相关 capability（而不是等到 workflow run 推进时才迟到地注入）。

本任务是 `04-20-builtin-workflow-admin` 的直接 follow-up：PR1 引入的 `workflow_can_grant: true` 在当前代码里**完全触发不到**，因为没有任何 caller 同时传 `workflow_capabilities: None + has_active_workflow: true`。这是 session 创建 ↔ workflow 激活链路的系统性设计债。

## Why（问题陈述）

### 观察到的症状

- 把 agent link 的 `default_lifecycle_key` 设为 `builtin_workflow_admin` 后，session 开启时 agent **拿不到** `workflow_management` 工具集。
- 即便 workflow run 被 `auto_start_lifecycle_run` 异步启动，capability 的注入点已经错过 session bootstrap。

### 根因

`CapabilityResolver::resolve` 的分支：

```rust
let mut effective_caps = if input.workflow_capabilities.is_some() {
    BTreeSet::<ToolCapability>::new()  // 用 workflow 声明的集合
} else {
    default_visible_capabilities(...)  // 走 visibility rule
};
```

两条分支的激活需要 caller 正确传入 `has_active_workflow` 和 `workflow_capabilities`。但 session 创建路径 **7 处都写死为 `false / None`**——也就是总是走 `default_visible_capabilities`，且 `has_active_workflow=false` 让 `workflow_can_grant` 永远无法命中。

结果：
- `workflow_management` 对非自声明的 agent 永远不可见
- `workflow`（需要 workflow cluster）也受影响

### 硬编码清单

| 文件 | 行号 | Owner | 当前值 |
|---|---|---|---|
| `crates/agentdash-api/src/routes/acp_sessions.rs` | 1192 | Story | `false / None` |
| `crates/agentdash-api/src/routes/acp_sessions.rs` | 1320 | **Project** | `false / None` |
| `crates/agentdash-api/src/routes/story_sessions.rs` | 445-446 | Story | `false / None` |
| `crates/agentdash-api/src/routes/project_sessions.rs` | 227-228 | Project | `false / None` |
| `crates/agentdash-application/src/routine/executor.rs` | 488-489 | Routine | `false / None` |
| `crates/agentdash-application/src/task/gateway/turn_context.rs` | 130 | Task | `workflow_capabilities: None`（`has_active_workflow` 已接通） |
| `crates/agentdash-application/src/task/session_runtime_inputs.rs` | 60-61 | Task | `has_active_workflow: workflow.is_some()` 接通，`workflow_capabilities: None` 半缺 |

### 正确装配时机

`orchestrator.rs:502` 和 `advance_node.rs:393` 在 **workflow run 推进**时已正确传真实值。但这只覆盖"切步骤"的运行时事件；**session 初始化**路径缺同样装配逻辑。

## Requirements

### 抽辅助函数

在 `agentdash-application` 内新增：

```rust
pub struct SessionWorkflowContext {
    pub has_active_workflow: bool,
    pub workflow_capabilities: Option<Vec<String>>,
}

pub async fn resolve_session_workflow_context(
    repos: &SessionWorkflowRepos,
    owner: SessionWorkflowOwner,
) -> Result<SessionWorkflowContext, WorkflowApplicationError>;
```

- `SessionWorkflowOwner`: enum 区分 `Project { agent_link }` / `Story { story_id, agent_link? }` / `Task { task_id, lifecycle_run_id? }` / `Routine { routine_id }`
- 实现：
  - 解析 owner → default_lifecycle_key（或 active lifecycle_run）
  - 解析 lifecycle.entry_step_key（或当前 active step）→ step 的 workflow_key
  - 调 `compute_effective_capabilities(workflow.contract.baseline_caps?, step.capabilities)` 计算 effective set
  - 返回 `(has_active_workflow=true, workflow_capabilities=Some(effective))`
- 无 lifecycle 绑定时返回 `(false, None)`（保持向后兼容默认行为）

### 清理 7 处硬编码

每个 caller 改为：

```rust
let workflow_ctx = resolve_session_workflow_context(&repos, owner_variant).await?;
CapabilityResolverInput {
    ...
    has_active_workflow: workflow_ctx.has_active_workflow,
    workflow_capabilities: workflow_ctx.workflow_capabilities,
    ...
}
```

### 端到端集成测试

新增 `capability::pipeline_tests::session_creation_with_default_lifecycle_grants_workflow_management`：
- 构造一个 Project + agent link with `default_lifecycle_key = "builtin_workflow_admin"`
- 走 session 创建路径
- 断言 resolver 输出的 `effective_capabilities` 包含 `workflow_management`
- 断言 `platform_mcp_configs` 包含 `PlatformMcpScope::Workflow`

### 回归测试

- 现有 `capability/pipeline_tests.rs` 3 个测试保持绿
- 现有 `resolver.rs` 所有单测保持绿
- 对 `workflow_capabilities: None + has_active_workflow: true` 这条之前无人调用的分支加**专项测试**，固定 `workflow_can_grant` 的 OR 语义（否则 PR1 的改动永远被视为无效代码）

## Acceptance Criteria

- [ ] 7 处硬编码全部替换为 `resolve_session_workflow_context(...)` 调用
- [ ] `grep -rn "has_active_workflow: false" crates/` 仅命中测试代码和辅助函数默认返回
- [ ] 新增 `session_creation_with_default_lifecycle_grants_workflow_management` 端到端测试绿
- [ ] 手动 demo：Project session 绑定 `builtin_workflow_admin` → session 第一个 turn 就能看到 `list_workflows / get_workflow / upsert_workflow` 等工具
- [ ] `cargo build` / `cargo test` / `cargo clippy` / frontend test 全绿
- [ ] PR1 引入的 `workflow_can_grant: true` 有专项测试覆盖运行期生效路径

## Definition of Done

- 所有硬编码替换完成，`resolve_session_workflow_context` 在各 session 类型下都被覆盖
- 集成测试锁定"绑定 lifecycle → session 创建 → capability 自动授予"链路
- spec（`tool-capability-pipeline.md`）更新"装配时机"章节，明确 session 创建时机与 workflow run 推进时机的 capability 交付契约
- `resolve_agent_default_lifecycle` 等既有 helper 复用而非重复实现

## Technical Approach

### 分阶段推进（避免一次性改 7 处）

**PR1 — 抽象 + Project session 打通**
- 新增 `resolve_session_workflow_context` 及 `SessionWorkflowOwner::Project` 分支
- 改 `acp_sessions.rs:1320` + `project_sessions.rs:227`（Project 两处）
- 跑端到端集成测试验证 `builtin_workflow_admin` 绑定链路
- **这一步完成后用户的实际场景就能 work**

**PR2 — 扩展到 Story / Task / Routine**
- 实现其余 `SessionWorkflowOwner` 分支
- 改剩余 5 处硬编码
- 补各 owner 类型的集成测试

**PR3 — 收口 + 一致性审查**
- 删除所有遗留的 `has_active_workflow: false` 默认表达
- 更新 spec 的装配时机章节
- CI grep 守卫：禁止在非测试代码里硬编码两个字段

### 依赖注入

`SessionWorkflowRepos` 至少需要：
- `ProjectAgentLinkRepository`（查 default_lifecycle_key）
- `LifecycleDefinitionRepository`（查 lifecycle + entry_step_key）
- `WorkflowDefinitionRepository`（查 workflow contract baseline caps）
- `LifecycleRunRepository`（Task owner 查当前 active run/step）

这些 repo 大多已在 AppState 里注册，复用即可。

### 回归风险

- 改变 `has_active_workflow` 默认值可能影响其他 well-known capability 的 visibility（如 `CAP_WORKFLOW` 的 `workflow_can_grant: true`）。需要**回归矩阵测试**：覆盖所有 well-known capability × (有/无 workflow 绑定) × (有/无 agent 声明) 组合，确保迁移后判定结果与迁移前"在实际业务场景下"一致。

## Decision (ADR-lite)

**Context**: `CapabilityVisibilityRule` 支持基于工作流授予 capability 的语义（`workflow_can_grant`），但 session 创建路径从未装配过 workflow 上下文，导致该语义永远不生效。根因是"session 创建 ↔ workflow 激活"在时序和数据流上解耦，capability 注入分散到 7 个 callsite 各自 ad-hoc 处理。

**Decision**:
1. 抽一个**单一入口**函数 `resolve_session_workflow_context`，集中所有 owner 类型的 workflow 上下文解析逻辑
2. 所有 session 创建 callsite 统一调用该入口，不再允许硬编码
3. 分 3 个 PR 推进：抽象 + Project 打通 → 扩展其他 owner → 收口

**Consequences**:
- + `CapabilityVisibilityRule` 的设计意图在运行时真正生效
- + Session 创建和 workflow 推进两个装配时机的 capability 语义一致
- + 未来新增 session 类型 / 新能力时有明确装配规范
- − 改动面大（7 个 callsite + 新 repo 依赖），分阶段推进降低一次性风险
- − 需要回归矩阵测试确保 well-known capability 迁移无歧义

## Out of Scope

- 修改 `CapabilityResolver` 本身的判定逻辑（OR/AND 模型在 04-20-builtin-workflow-admin 已定稿）
- 新增 `workflow_management_read/_write` 细粒度 capability（另立任务）
- 前端提供 agent 能力配置入口（visibility rule 不再依赖 agent 声明后本需求消失）
- Workflow run 推进路径的 capability resolve（已由 orchestrator 正确处理）
- Multi-agent 共享 lifecycle / lifecycle handover 等高级场景

## Technical Notes

- 硬编码扫描依据：`grep -rn "has_active_workflow:" crates/` + `grep -rn "workflow_capabilities:" crates/`
- 关联文件：
  - [resolver.rs](crates/agentdash-application/src/capability/resolver.rs)
  - [tool_capability.rs](crates/agentdash-spi/src/tool_capability.rs)
  - [orchestrator.rs:496-507](crates/agentdash-application/src/workflow/orchestrator.rs#L496)（正确示例）
  - [plan_builder.rs:143-151](crates/agentdash-application/src/session/plan_builder.rs#L143)（透传层）
- 关联任务：
  - `04-20-builtin-workflow-admin`（本任务的前置）
  - `04-20-dynamic-capability-followup`（能力链路收尾，有部分重叠但聚焦动态切换）
  - `04-15-workflow-dynamic-lifecycle-context`（lifecycle 上下文模型，提供 step capability 动态解析依据）
- 相关 Spec：`.trellis/spec/backend/tool-capability-pipeline.md` — 现缺"装配时机"章节
