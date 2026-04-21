# AgentDashboard 规范文档

> 项目开发规范和技术决策记录

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

主要规范：
- 目录结构（12 个 feature 模块、10 个 store）
- 组件规范（FSD 风格）
- Hook 规范（ACP 事件归并契约）
- 状态管理（Zustand 5）
- 质量规范
- 类型安全（snake_case 直接映射）

### 后端开发

详见 [后端开发指南](./backend/index.md)

通用开发规范：
- 目录结构（整洁架构分层、14 个 crate）
- 数据库指南（PostgreSQL + SQLite + SQLx）
- Repository 模式、错误处理、质量规范、日志规范

模块专属契约（按子系统分目录）：
- `session/` — 流式协议、Pi Agent 流式合并
- `hooks/` — Hook Runtime 跨层契约、Rhai 脚本引擎
- `workflow/` — Lifecycle Edge 设计
- `vfs/` — 统一 Address Space
- `capability/` — 工具能力管线、Plugin API、LLM Model Config

### 跨层契约

详见 [跨层契约索引](./cross-layer/index.md)

前后端共享的协议与序列化契约：
- ACP Meta Warp 跨层序列化契约

---

## 思维指南

详见 [思维指南索引](./guides/index.md)

- 跨层思考指南 — 数据流跨越层边界时的检查清单
- 代码复用思考指南 — 发现模式和减少重复

---

## 规范文档状态

### 项目级

| 文档 | 状态 |
|------|------|
| 项目总览 | ✅ 已定稿 |
| 技术选型 | ✅ 已更新（对齐实际技术栈） |
| 沟通规范 | ✅ 已定稿 |

### 前端

| 文档 | 状态 |
|------|------|
| 开发指南索引 | ✅ 已更新 |
| 目录结构 | ✅ 已更新（对齐实际 features/stores/pages） |
| 组件规范 | ✅ 已更新 |
| Hook 规范 | ✅ 已更新（含 ACP 事件归并契约） |
| 状态管理 | ✅ 已更新（对齐实际 13 个 store） |
| 质量规范 | ✅ 已更新 |
| 类型安全 | ✅ 已更新（含 snake_case 映射边界） |

### 后端 — 通用规范

| 文档 | 状态 |
|------|------|
| 开发指南索引 | ✅ 已更新 |
| 目录结构 | ✅ 已更新（整洁架构分层 + 演进记录） |
| 数据库指南 | ✅ 已更新（PostgreSQL + SQLx） |
| Repository 模式 | ✅ 已更新 |
| 错误处理 | ✅ 已更新（含 SSE/NDJSON 契约） |
| 领域类型化标准 | ✅ 已创建 |
| 质量规范 | ✅ 已更新（含 DTO 命名 + Session 持久化） |
| 日志规范 | ✅ 已更新 |

### 后端 — 模块专属契约

| 子系统 | 文档 | 状态 |
|--------|------|------|
| session/ | 流式协议 | ✅ 已拆分 |
| session/ | Pi Agent 流式合并 | ✅ 已拆分 |
| hooks/ | Execution Hook Runtime | ✅ 已更新 |
| hooks/ | Hook Script Engine | ✅ 已创建 |
| workflow/ | Lifecycle Edge 设计 | ✅ 已创建 |
| vfs/ | VFS Access | ✅ 已创建 |
| capability/ | 工具能力管线 | ✅ 已创建 |
| capability/ | Plugin API | ✅ 已创建 |
| capability/ | LLM Model Config | ✅ 已创建 |

### 跨层契约

| 文档 | 状态 |
|------|------|
| ACP Meta Warp | ✅ 已更新 |

### 思维指南

| 文档 | 状态 |
|------|------|
| 思维指南索引 | ✅ 已定稿 |

---

## 更新记录

| 日期 | 更新内容 |
|------|---------|
| 2026-04-21 | 目录重组：backend/ 模块专属契约按子系统分目录（session/hooks/workflow/vfs/capability），新增 cross-layer/ 存放跨层契约 |
| 2026-04-14 | 项目审计（第二轮）：修正 DomainError/ConnectorError 变体、更新 LLM 配置文件路径、补全 Rhai preset 清单、精简 ctx 对象复述、去重 database/repository 模板 |
| 2026-04-14 | 项目审计（第一轮）：修正数据库为 PostgreSQL、更新连接器架构、修复文件路径引用、精简 hook-runtime/hook-guidelines/quality-guidelines 过度规范、归档已完成任务 |
| 2026-03-29 | 全量审查与修复：tech-stack/database/quality/directory/state 等对齐实际代码 |
| 2026-03-29 | 拆分 directory-structure 演进日志、流式协议独立文件 |
| 2026-03-27 | 新增 API God Module 分解记录 |
| 2026-03-24 | 新增 Plugin API 规范 |
| 2026-03-20 | API/Application 解耦重构 |
| 2026-03-10 | 云端/本机双后端架构 + PiAgent 执行模型 |
| 2026-02-27 | Project/Workspace 领域模型重构 |
| 2026-02-26 | 整洁架构重构完成 |
| 2026-02-25 | 新增技术选型文档，记录 Rust+React 决策 |
| 2026-02-21 | 初始化项目文档框架 |

---

*所有文档使用中文编写*
