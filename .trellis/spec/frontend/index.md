# Frontend Development Guidelines

> Best practices for frontend development in this project.

---

## Overview

<!-- PROJECT-SPECIFIC-START: Frontend Overview -->
> **AgentDashboard 前端开发规范。**

### 项目总览

前端客户端（Client）负责以下核心职责：
- 管理 Project → Workspace → Story → Task 的完整领域模型
- 展示 Story 和 Task 的状态（看板/列表视图）
- 管理用户与多个后端的连接（模块01 Connection）
- 接收和展示 Agent 执行的实时状态推送（模块05 Execution）
- 提供 Project/Workspace/Story 创建的完整交互流程

---

### 核心领域模型层次

```
Project（项目）
├── Workspace（工作空间）  ← 物理目录/Git Worktree
├── Story（需求故事）      ← 包含结构化 StoryContext
│   └── Task（执行任务）   ← 绑定到 Workspace + AgentBinding
```

- **Project** 是顶层组织单元，关联一个 Backend
- **Workspace** 是物理工作目录（静态目录/Git Worktree/临时环境）
- **Story** 挂在 Project 下，包含 PRD/规格引用/资源列表
- **Task** 挂在 Story 下，可绑定 Workspace 和 Agent

### 核心UI概念

#### 侧边栏

侧边栏包含四个区域：
- **导航**：看板 / 会话 视图切换
- **项目选择器**：Project 列表 + 创建新项目
- **工作空间列表**：当前 Project 的 Workspace 列表 + 创建
- **后端连接**：后端连接状态展示

#### 看板（Dashboard）

以 Project 为中心的核心视图：
- 按 Project 加载 Story 列表（替代旧的按 Backend 加载）
- Story 状态卡片：显示当前状态、进度、关联 Task 数量
- Task 进度追踪：显示 Agent 执行状态和产物

#### 实时状态更新

Agent 执行是异步的，前端需要：
- 实时接收 Task 状态变更推送
- 流式显示 Agent 输出（类似 Claude Code 的输出流）
- 连接断线时的降级处理
<!-- PROJECT-SPECIFIC-END -->

This directory contains guidelines for frontend development. Fill in each file with your project's specific conventions.

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Module organization and file layout | To fill |
| [Component Guidelines](./component-guidelines.md) | Component patterns, props, composition | To fill |
| [Hook Guidelines](./hook-guidelines.md) | Custom hooks, data fetching patterns, ACP 事件归并契约 | ✅ 已更新 |
| [State Management](./state-management.md) | Local state, global state, server state | To fill |
| [Quality Guidelines](./quality-guidelines.md) | Code standards, forbidden patterns | To fill |
| [Type Safety](./type-safety.md) | Type patterns, validation | ✅ 已更新（含业务 API snake_case 映射边界） |

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

### 视图与状态解耦原则

视图（View）是用户自定义的组织方式，不影响底层状态：

```
✅ 正确：View 组件只读取 Story 数据，不修改其核心状态
❌ 错误：在视图组件中直接改变 Story/Task 的 status 字段
```

### Project 驱动原则

- 前端以 **Project** 为核心导航单元（不再以 Backend 为中心）
- Story 列表按 `projectId` 加载，不再按 `backendId`
- Workspace 列表按 `projectId` 加载
- Backend 信息保留在 Project 配置中，作为连接层使用

### 实时状态原则

- Story/Task 状态以后端为准，前端不要自行推断状态
- 状态变更通过后端 API，不要在前端直接修改
- 乐观更新需要有回滚机制

### 数据隔离原则

- Workspace 数据在 Store 中按 `projectId` 隔离（`workspacesByProjectId`）
- 切换 Project 时自动加载对应的 Workspace 和 Story

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
