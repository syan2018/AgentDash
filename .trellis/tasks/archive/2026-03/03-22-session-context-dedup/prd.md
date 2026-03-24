# Session Context Builder 统一管线重构

## 背景

当前 session context 构建逻辑在 6 个独立位置重复实现：

**A 类（只读查询，前端展示用）**：
1. `task_execution.rs::build_task_session_context_response`
2. `story_sessions.rs::build_story_session_context_response`
3. `project_sessions.rs::build_project_session_context_response`

**B 类（写操作 bootstrap，启动 session 用）**：
4. `acp_sessions.rs::build_story_owner_prompt_request`
5. `acp_sessions.rs::build_project_owner_prompt_request`
6. `task_execution_gateway.rs::start_task_turn`

每个位置都独立执行：加载关联实体 → 解析 executor config → 构建 address space → 注入 MCP servers → 计算 tool visibility + runtime policy → 组装 DTO。

## 目标

1. 统一 DTO：3 个独立 context snapshot 类型合并为 1 个 `SessionContextSnapshot`，用 enum 区分 owner 级别差异
2. 共享管线：context 计算 + address space 构建 + MCP 注入 + prompt block 组装 + working dir 注入全部提取到 `agentdash-application` 层
3. 附带清理：`normalize_optional_string` 等重复工具函数统一提取

## 统一 DTO 设计

```rust
#[derive(Debug, Serialize)]
pub struct SessionContextSnapshot {
    pub executor: SessionExecutorSummary,
    pub project_defaults: SessionProjectDefaults,
    pub effective: SessionEffectiveContext,
    #[serde(flatten)]
    pub owner_context: SessionOwnerContext,
}

#[derive(Debug, Serialize)]
#[serde(tag = "owner_level", rename_all = "snake_case")]
pub enum SessionOwnerContext {
    Task { story_overrides: SessionStoryOverrides },
    Story { story_overrides: SessionStoryOverrides },
    Project {
        agent_key: String,
        agent_display_name: String,
        shared_context_mounts: Vec<ProjectAgentMount>,
    },
}
```

## 执行计划

### Phase 1: 提取共享类型和工具到 application 层

**范围**：
- 在 `agentdash-application` 新建 `session_context` 模块
- 迁移 `SessionProjectDefaults`、`SessionStoryOverrides`、`SessionEffectiveContext`、`SessionExecutorSummary` 类型
- 迁移 `normalize_optional_string`、`build_session_executor_summary` 工具函数
- 定义 `SessionContextSnapshot` + `SessionOwnerContext` 统一 DTO

### Phase 2: 实现共享 context 构建管线

**范围**：
- 实现 `build_session_context(input, address_space, mcp_servers) -> SessionContextSnapshot`
- 3 个 A 类查询端点改为调用共享管线
- 删除 3 处旧的 context builder 函数

### Phase 3: 提取 bootstrap 共享管线

**范围**：
- 提取 address space 构建、MCP server 注入、working dir / workspace root 注入到共享层
- ACP 的 `build_story_owner_prompt_request` / `build_project_owner_prompt_request` 和 `task_execution_gateway::start_task_turn` 改用共享管线子步骤
- 清理 bootstrap 端重复代码

### Phase 4: 前端类型统一

**范围**：
- 删除 `TaskSessionContextSnapshot`、`StorySessionContextSnapshot`、`ProjectSessionContextSnapshot` 接口
- 统一为 `SessionContextSnapshot`（带 `owner_level` 判别字段）
- 更新所有前端消费点

### Phase 5: 清理

**范围**：
- 删除废弃类型定义
- 删除重复的 `normalize_optional_string` 定义（backends.rs / workflows.rs / project_agents.rs）
- 确认编译通过和测试通过
