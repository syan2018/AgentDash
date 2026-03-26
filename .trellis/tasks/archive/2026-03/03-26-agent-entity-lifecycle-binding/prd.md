# Agent 独立实体 + Lifecycle 绑定重构

## 背景

当前 Agent 以 `AgentPreset` 嵌入在 `ProjectConfig.agent_presets` JSON 里，没有独立 ID/表/CRUD API。
`WorkflowAssignment` 按角色绑定 lifecycle 到 project，但其核心消费者 `resolve_assignment_and_ensure_run` 全仓零调用。
需要将 Agent 提升为独立实体，并以 Agent 级绑定替代角色级绑定。

## 设计决策

| 决策 | 结论 |
|------|------|
| Agent 与 Project 关系 | 多对多（junction table） |
| lifecycle_key 存放位置 | 关联表上，每个 Project 可给同一 Agent 配不同 lifecycle |
| config 管理 | Agent 有 base_config，关联表有 per-project config_override |
| 项目默认 Agent | 不存在"项目默认 Agent"概念，用户显式选择；Story/Task 有 default agent 标志在关联表上 |
| Lifecycle 运行时语义 | session 伴随的运行时状态实例，更换 workflow 是更新 step，不销毁重建 |
| 单 workflow 绑定 | 自动包装为单步 lifecycle（后端透明处理） |

## 数据模型

### agents 表（新建）

```sql
CREATE TABLE IF NOT EXISTS agents (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    agent_type TEXT NOT NULL,
    base_config TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

字段说明：
- `name`: 显示名（如 "Code Reviewer"）
- `agent_type`: 执行器类型（如 "PI_AGENT", "claude-code"）
- `base_config`: 默认配置 JSON（model_id, provider_id, mcp_servers, variant, thinking_level, permission_policy 等）

### project_agent_links 表（新建）

```sql
CREATE TABLE IF NOT EXISTS project_agent_links (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    agent_id TEXT NOT NULL REFERENCES agents(id),
    config_override TEXT,                    -- nullable JSON, 合并覆写 base_config
    default_lifecycle_key TEXT,              -- nullable, 此 agent 在此 project 的默认 lifecycle
    is_default_for_story INTEGER NOT NULL DEFAULT 0,
    is_default_for_task INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(project_id, agent_id)
);
```

### 移除

- `ProjectConfig.agent_presets` 字段
- `ProjectConfig.default_agent_type` 字段
- `AgentPreset` struct
- `workflow_assignments` 表

## API 设计

### Agent CRUD（顶层实体）

| Method | Path | 说明 |
|--------|------|------|
| GET | `/agents` | 列出所有 Agent（可选 ?project_id 过滤） |
| POST | `/agents` | 创建 Agent |
| GET | `/agents/{id}` | 获取 Agent 详情 |
| PUT | `/agents/{id}` | 更新 Agent base_config |
| DELETE | `/agents/{id}` | 删除 Agent（需无关联） |

### Project-Agent 关联

| Method | Path | 说明 |
|--------|------|------|
| GET | `/projects/{pid}/agents` | 列出项目关联的 Agent（合并 config，含 lifecycle 信息） |
| POST | `/projects/{pid}/agents` | 关联 Agent 到项目（body: agent_id, config_override?, default_lifecycle_key?） |
| PUT | `/projects/{pid}/agents/{agent_id}` | 更新关联（config_override, default_lifecycle_key, role defaults） |
| DELETE | `/projects/{pid}/agents/{agent_id}` | 解除关联 |

### Session（保留路径，改实现）

| Method | Path | 说明 |
|--------|------|------|
| POST | `/projects/{pid}/agents/{agent_id}/session` | 从关联表读取 Agent + lifecycle，auto-start run |

### Workflow 自动包装

PUT `/projects/{pid}/agents/{agent_id}` 支持两种绑定方式：
- `{ "default_lifecycle_key": "some-lifecycle" }` — 直接绑定 lifecycle
- `{ "default_workflow_key": "some-workflow" }` — 后端自动创建单步 lifecycle 并绑定

## 实施阶段

### Phase 0: 前端 Workflow/Lifecycle 删除 UI

**现状**：后端 DELETE API + 前端 service/store 已就绪，UI 缺删除按钮。

**改动**：
- `frontend/src/features/workflow/workflow-tab-view.tsx`：卡片增加删除按钮 + 确认对话框

**文件**：1 个

---

### Phase 1: Agent 独立实体（Domain + Infrastructure）

**新建**：
- `crates/agentdash-domain/src/agent/entity.rs` — Agent struct
- `crates/agentdash-domain/src/agent/repository.rs` — AgentRepository trait
- `crates/agentdash-domain/src/agent/mod.rs`
- `crates/agentdash-domain/src/project_agent_link/entity.rs` — ProjectAgentLink struct
- `crates/agentdash-domain/src/project_agent_link/repository.rs` — ProjectAgentLinkRepository trait
- `crates/agentdash-domain/src/project_agent_link/mod.rs`
- `crates/agentdash-infrastructure/src/persistence/sqlite/agent_repository.rs`
- `crates/agentdash-infrastructure/src/persistence/sqlite/project_agent_link_repository.rs`

**修改**：
- `crates/agentdash-domain/src/lib.rs` — 注册 agent、project_agent_link 模块
- `crates/agentdash-domain/src/project/value_objects.rs` — 移除 AgentPreset, agent_presets, default_agent_type
- `crates/agentdash-infrastructure/src/persistence/sqlite/mod.rs` — 注册新 repo
- `crates/agentdash-api/src/app_state.rs` — 注入新 repo

---

### Phase 2: Agent CRUD + 关联 API

**重构**：
- `crates/agentdash-api/src/routes/project_agents.rs` — 大幅重写
  - list/create/update/delete Agent
  - list/create/update/delete ProjectAgentLink
  - `open_project_agent_session` 改为从 agents + links 表查询
  - `resolve_project_agent_bridge` → `resolve_agent_from_link`
  - config 合并逻辑：`merge(agent.base_config, link.config_override)`
  - 自动包装 workflow → lifecycle
- `crates/agentdash-api/src/routes.rs` — 注册新路由

**新建**：
- `crates/agentdash-api/src/dto/agent.rs` — Agent 相关 DTO（可选，或在现有 dto 模块添加）

---

### Phase 3: Session 自动启动 Lifecycle Run

**修改** `open_project_agent_session`（新建 session 分支）：

```
1. create_session (已有)
2. create SessionBinding (已有)
3. 【新增】if link.default_lifecycle_key.is_some():
   a. LifecycleRunService::start_run(lifecycle_key, target_kind=Project, target_id=project.id)
   b. activate first step (if has workflow_key)
   c. failure → log warning, do NOT block session creation
```

**文件**：
- `crates/agentdash-api/src/routes/project_agents.rs`

---

### Phase 4: 前端 Agent 实体迁移

**类型**：
- `frontend/src/types/index.ts` — 新增 Agent, ProjectAgentLink 类型；移除 AgentPreset；更新 ProjectConfig

**Store**：
- `frontend/src/stores/projectStore.ts` — 新增 Agent CRUD + Link CRUD actions；移除 presets 相关逻辑

**Editor**：
- `frontend/src/features/project/agent-preset-editor.tsx` → 重命名/重构为 agent-editor.tsx
  - 支持创建独立 Agent
  - 支持设置 per-project config override
  - 新增 lifecycle/workflow 选择器（下拉列表 from workflowStore）

**Views**：
- `frontend/src/features/project/project-agent-view.tsx` — 适配新 API
- `frontend/src/features/agent/agent-tab-view.tsx` — 适配新 Agent 列表结构

---

### Phase 5: 会话面板显示 Lifecycle 状态

**改动**：
- `frontend/src/features/agent/agent-tab-view.tsx` — 面包屑区域增加 lifecycle run 状态 badge
- 可复用 `frontend/src/features/workflow/task-workflow-panel.tsx` 的查询和展示逻辑

---

### Phase 6: 清理

**移除 WorkflowAssignment 全链路**：
- Domain: `WorkflowAssignment` entity, `WorkflowAssignmentRepository` trait
- Infrastructure: `workflow_assignments` 表 + SqliteWorkflowAssignmentRepository
- API: `list_project_workflow_assignments`, `create_project_workflow_assignment` 路由
- Application: `catalog.rs` 中 `assign_to_project`
- Frontend: `WorkflowAssignment` 类型、store 的 `assignmentsByProjectId`、service 的 assignment 函数、tab-view/task-panel 的 assignment UI

**移除死代码**：
- `crates/agentdash-application/src/workflow/assignment_resolution.rs` — 整文件删除
- `crates/agentdash-application/src/workflow/binding.rs` — 已是空壳，删除

**移除旧 Agent 机制**：
- `ProjectConfig.agent_presets`, `ProjectConfig.default_agent_type`
- `AgentPreset` struct
- `build_preset_bridge`, "default" agent key 解析逻辑

## 风险与注意

1. **ProjectConfig.default_agent_type 移除影响**：Story/Task 的 agent 分配依赖此字段，需迁移到 `project_agent_links.is_default_for_story/task`
2. **前端改动面广**：agent-preset-editor 重构 + projectStore 大量修改 + types 变更
3. **Lifecycle Run 动态更新 step**：当前 `LifecycleRun.step_states` 在创建时固定，需支持运行中更新
