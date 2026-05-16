# AgentDashboard 规范文档

> 项目开发规范和技术决策记录。

---

## 必读文档（按顺序）

| 顺序 | 文档 | 内容 |
|------|------|------|
| 1 | [项目总览](./project-overview.md) | 核心概念（Story/Task）、系统架构、设计原则 |
| 2 | [技术选型](./tech-stack.md) | 前后端技术栈、Crate 结构、版本信息 |
| 3 | [沟通规范](./communication.md) | 语言要求（中文）、Git 提交规范 |

---

## 开发指南

### 前端开发

详见 [前端开发指南](./frontend/index.md)

主要规范：目录结构、组件规范、Hook 规范（事件聚合契约）、状态管理（Zustand 5）、类型安全（snake_case 直接映射）

### 后端开发

详见 [后端开发指南](./backend/index.md)

通用规范：目录结构（整洁架构分层）、数据库指南、Repository 模式、错误处理、质量规范

模块专属契约：
- `session/` — launch 主线、runtime 状态、ExecutionContext 投影、Bundle 主数据面、流式协议
- `hooks/` — Hook Runtime 跨层契约、Rhai 脚本引擎
- `workflow/` — Lifecycle Edge 设计
- `vfs/` — 统一 VFS
- `capability/` — 工具能力管线、Plugin API、LLM Model Config

### 跨层契约

详见 [跨层契约索引](./cross-layer/index.md)

### 思维指南

详见 [思维指南索引](./guides/index.md)

---

*所有文档使用中文编写*
