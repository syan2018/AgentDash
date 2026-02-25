# 前端开发指南

> AgentDashboard 前端开发规范。**开始编码前必须阅读项目总览。**

---

## 项目总览

**首先阅读：** [`../project-overview.md`](../project-overview.md)

前端客户端（Client）负责以下核心职责：
- 展示 Story 和 Task 的状态（看板/列表/树状等视图，模块08 View）
- 管理用户与多个后端的连接（模块01 Connection）
- 接收和展示 Agent 执行的实时状态推送（模块05 Execution）
- 提供 Story 创建、上下文注入、任务拆解的交互界面

---

## 核心UI概念

### 看板（Dashboard）

这是系统的核心视图，需要满足：
- 同时展示多个后端的连接状态
- 多视图支持：看板视图、列表视图、树状视图（模块08 View）
- Story 状态卡片：显示当前状态、进度、关联 Task 数量
- Task 进度追踪：显示 Agent 执行状态和产物

### 实时状态更新

Agent 执行是异步的，前端需要：
- 实时接收 Task 状态变更推送
- 流式显示 Agent 输出（类似 Claude Code 的输出流）
- 连接断线时的降级处理

### 多后端管理

用户可连接多个后端，前端需要：
- 清晰展示每个后端的连接状态
- 支持在不同后端的 Story/Task 之间切换
- 连接失败时的重连UI

---

## 必读文档

1. [**技术选型**](../tech-stack.md) - 前后端技术栈决策记录
2. [**项目总览**](../project-overview.md) - 核心概念和架构

## 规范文档索引

| 指南 | 描述 | 状态 |
|------|------|------|
| [目录结构](./directory-structure.md) | 模块组织和文件布局 | 待完善 |
| [组件规范](./component-guidelines.md) | 组件模式、Props、组合 | 待完善 |
| [Hook规范](./hook-guidelines.md) | 自定义Hook、数据获取模式 | 待完善 |
| [状态管理](./state-management.md) | 本地/全局/服务器状态 | 待完善 |
| [质量规范](./quality-guidelines.md) | 代码标准、禁止模式 | 待完善 |
| [类型安全](./type-safety.md) | 类型模式、验证 | 待完善 |

---

## How to Fill These Guidelines

For each guideline file:

1. Document your project's **actual conventions** (not ideals)
2. Include **code examples** from your codebase
3. List **forbidden patterns** and why
4. Add **common mistakes** your team has made

The goal is to help AI assistants and new team members understand how YOUR project works.

## 设计约束（编码前必读）

### 视图与状态解耦原则

视图（View）是用户自定义的组织方式，不影响底层状态：

```
✅ 正确：View 组件只读取 Story 数据，不修改其核心状态
❌ 错误：在视图组件中直接改变 Story/Task 的 status 字段
```

### 实时状态原则

- Story/Task 状态以后端为准，前端不要自行推断状态
- 状态变更通过后端 API，不要在前端直接修改
- 乐观更新需要有回滚机制

### 多后端隔离原则

- 不同后端的数据在 Store 中需要按 backendId 隔离
- 跨后端操作（如复制 Story）需要明确的用户确认

---

## 语言要求

> **必须使用中文**

- 所有与用户的交流必须使用中文
- 所有文档更新必须使用中文
- 代码注释必须使用中文
- 提交信息必须使用中文
