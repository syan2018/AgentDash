# Backend Development Guidelines

> Best practices for backend development in this project.

---

## Overview

<!-- PROJECT-SPECIFIC-START: Backend Overview -->
> **AgentDashboard 后端开发规范。**

### 项目总览

后端服务负责以下核心职责：
- 维护 Story 和 Task 的状态存储（模块02 State）
- 管理用户与后端的连接会话（模块01 Connection）
- 调度 Agent 执行任务（模块05 Execution）
- 管理 Agent 执行的隔离环境（模块03 Workspace）
- 编排任务执行流程（模块04 Orchestration）
- 信息注入与验证（模块06 Injection, 模块07 Validation）

---

### 核心数据实体

后端需要管理的核心实体，遵循 Project → Workspace → Story → Task 的领域层次：

```
Project {
  id: Uuid
  name: String
  description: String
  backend_id: String          // 默认后端
  config: ProjectConfig       // Agent 预设、默认 Workspace 等
  created_at / updated_at
}

Workspace {
  id: Uuid
  project_id: Uuid            // 所属项目
  name: String
  container_ref: String       // 物理路径
  workspace_type: GitWorktree | Static | Ephemeral
  status: Pending | Preparing | Ready | Active | Archived | Error
  git_config: Option<GitConfig>
  created_at / updated_at
}

Story {
  id: Uuid
  project_id: Uuid            // 所属项目
  backend_id: String          // 执行后端
  title: String
  context: StoryContext        // PRD、规范引用、资源清单
  status: StoryStatus
  created_at / updated_at
}

Task {
  id: Uuid
  story_id: Uuid              // 归属的 Story
  workspace_id: Option<Uuid>  // 关联的 Workspace（外键）
  status: TaskStatus
  agent_binding: AgentBinding  // 结构化 Agent 绑定
  artifacts: Vec<Artifact>    // 执行产物
  created_at / updated_at
}
```

**实体关系**：
```
Project (1) → (*) Workspace
Project (1) → (*) Story
Story (1)   → (*) Task
Workspace (1) ← (*) Task（多 Task 可共享同一 Workspace）
```
<!-- PROJECT-SPECIFIC-END -->

This directory contains guidelines for backend development. Fill in each file with your project's specific conventions.

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Module organization and file layout | ✅ 已更新（整洁架构分层） |
| [Repository Pattern](./repository-pattern.md) | Repository trait 定义、依赖注入、测试 | ✅ 已创建 |
| [Database Guidelines](./database-guidelines.md) | ORM patterns, queries, migrations | To fill |
| [Error Handling](./error-handling.md) | Error types, handling strategies | To fill |
| [Quality Guidelines](./quality-guidelines.md) | Code standards, forbidden patterns | To fill |
| [Logging Guidelines](./logging-guidelines.md) | Structured logging, log levels | To fill |

---

## How to Fill These Guidelines

For each guideline file:

1. Document your project's **actual conventions** (not ideals)
2. Include **code examples** from your codebase
3. List **forbidden patterns** and why
4. Add **common mistakes** your team has made

The goal is to help AI assistants and new team members understand how YOUR project works.

<!-- PROJECT-SPECIFIC-START: Design Constraints -->
---

## 设计约束（编码前必读）

### 策略可插拔原则

后端的核心设计是**策略可插拔**，不要将实现细节硬编码：

```
✅ 正确：定义 StateManager 接口，实现可替换
❌ 错误：在业务逻辑中直接调用具体数据库 API
```

### 模块边界原则

- 每个模块只能调用接口，不能依赖其他模块的内部实现
- 状态变更必须通过 StateManager，不能直接操作存储
- 编排层不能直接操作执行层的 Agent

### 状态变更原则

- 所有状态变更必须记录 StateChange（不可省略）
- 变更必须包含 `reason` 字段说明原因
- 严禁直接覆盖状态而不记录历史

### ACP 流转换原则 (`normalized_to_acp.rs`)

- `NormalizedToAcpConverter` 必须对 `AssistantMessage`/`Thinking`/`UserMessage` 做**全局去重**（`emitted_agent`/`emitted_thought`/`emitted_user`），不能仅依赖 per-index delta
- `SystemMessage`、`TokenUsageInfo`、`NextAction` 不应作为 ACP 通知发射（ABCCraft 标准）
- `ErrorMessage`、`UserFeedback` 等一次性消息可直接发射，无需去重
- 新用户消息到达时重置 agent/thought 累积器（新 turn）

---

## 语言要求

> **必须使用中文**

- 所有与用户的交流必须使用中文
- 所有文档更新必须使用中文
- 代码注释必须使用中文
- 提交信息必须使用中文
<!-- PROJECT-SPECIFIC-END -->

---

**Language**: All documentation should be written in **English**.
