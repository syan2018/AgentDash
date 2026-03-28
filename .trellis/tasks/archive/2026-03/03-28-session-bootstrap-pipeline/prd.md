# Session 组装管道标准化 (SessionBootstrapPipeline)

## Goal

提取一个统一的 `SessionBootstrapPipeline`，将当前分散在 `task_execution_gateway.rs`、`story_sessions.rs`、`project_sessions.rs` 中的 session 组装逻辑收敛为一条共享管道，各 owner type 仅通过 policy 差异化配置，而非各自硬编码。

## 背景

当前 session 组装流程的共性步骤包括：
1. Resolve workspace binding（确定物理后端和根目录）
2. Build address space（基于 project/workspace policy 推导 mount 列表）
3. Bind MCP servers（解析运行时 MCP 服务列表）
4. Build session plan fragments（组装 system prompt 片段）
5. Compose PromptSessionRequest（填充后端注入字段）

这 5 步在 Task、Story、Project 三条路径中各实现了一遍，存在：
- 大量重复代码
- 不同路径的行为差异难以发现（例如 Task 路径会注入 `flow_capabilities`，Project 路径不会）
- 新增一种 owner type 需要复制整套流程

## Requirements

### 定义 Pipeline 抽象

在 `agentdash-application` 中（或靠近 session plan 的模块中）定义：

```rust
/// Session 启动所需的差异化策略
pub struct SessionBootstrapPolicy {
    pub owner_type: SessionOwnerType,
    pub owner_id: Uuid,
    pub project_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub is_continuation: bool,
    /// 是否注入 flow capabilities（仅 Task 需要）
    pub inject_flow_capabilities: bool,
    /// 额外的 context fragments（如 Story 的 PRD）
    pub extra_context: Vec<ContextFragment>,
}

/// Pipeline 产出的完整 session 运行时上下文
pub struct SessionBootstrapResult {
    pub workspace_root: Option<PathBuf>,
    pub address_space: ExecutionAddressSpace,
    pub mcp_servers: Vec<McpServer>,
    pub plan_fragments: Vec<ContextFragment>,
    pub flow_capabilities: Option<FlowCapabilities>,
    pub system_context: Option<String>,
}
```

### 实现统一 Pipeline

```rust
pub struct SessionBootstrapPipeline {
    // 所需的 service / repo 依赖
}

impl SessionBootstrapPipeline {
    pub async fn bootstrap(
        &self,
        policy: &SessionBootstrapPolicy,
        user_input: &UserPromptInput,
    ) -> Result<SessionBootstrapResult, SessionBootstrapError> {
        // 1. resolve workspace
        // 2. build address space
        // 3. bind MCP
        // 4. build plan fragments
        // 5. assemble result
    }
}
```

### 迁移各入口

- `task_execution_gateway.rs` → 构造 `SessionBootstrapPolicy { owner_type: Task, inject_flow_capabilities: true, ... }` 后调用 pipeline
- `story_sessions.rs` → 构造 `SessionBootstrapPolicy { owner_type: Story, extra_context: [story_prd], ... }` 后调用 pipeline
- `project_sessions.rs` → 构造 `SessionBootstrapPolicy { owner_type: Project, ... }` 后调用 pipeline

### 保持现有行为

Pipeline 的行为必须与现有三条路径完全一致，逐步确认：
- [ ] Task 路径：workspace resolve → address space → MCP → plan fragments + flow_capabilities
- [ ] Story 路径：workspace resolve → address space → MCP → plan fragments + story context
- [ ] Project 路径：workspace resolve → address space → MCP → plan fragments

## Acceptance Criteria

- [ ] `SessionBootstrapPipeline` 作为唯一的 session 组装入口
- [ ] Task/Story/Project 三条路径通过 `SessionBootstrapPolicy` 差异化，不再各自硬编码
- [ ] 各路径的最终行为与重构前完全一致（可通过 integration test 验证）
- [ ] 新增一种 owner type 只需定义新的 `SessionBootstrapPolicy` 构造方式
- [ ] `cargo check --workspace` 无错误

## Technical Notes

- 此 task 依赖 `owner-type-unify-prompt-split`，因为 Pipeline 使用 `SessionOwnerType` 而非三套枚举
- 此 task 也依赖 `PromptSessionRequest` 拆分，因为 Pipeline 产出的 `SessionBootstrapResult` 用于构造 `PromptSessionRequest`
- `workspace_resolution.rs` 中现有的 `resolve_workspace_binding` 可直接复用
- `build_derived_address_space` 和 `build_session_plan_fragments` 可直接作为 Pipeline 内部步骤

## 依赖

- 前置：`03-28-owner-type-unify-prompt-split`

## 优先级

P1 — 中优先级，整体收益最大但改动面也最广
