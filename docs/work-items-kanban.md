# AgentDash 工作项看板

> **生成日期**: 2026-02-26
> **基于**: 项目设计文档、参考项目分析、代码实现审查
> **目的**: 为后续开发提供完整的工作项清单，按优先级分组

---

## 项目进度概览

### 已完成 PRD（9 个）

| # | PRD | 日期 | 关键产出 |
|---|-----|------|----------|
| 1 | 项目初始化 | 02-25 | Rust + React 骨架、vibe-kanban 子模块 |
| 2 | ABCCraft UI 迁移 | 02-25 | Story/Task 前端组件、主题系统 |
| 3 | ACP 渲染迁移 | 02-25 | ACP SDK 集成、消息渲染组件 |
| 4 | 执行器集成层 | 02-25 | AgentConnector trait、ExecutorHub |
| 5 | 前端 Session MVP | 02-26 | Session 页面、Prompt 交互 |
| 6 | SSE + Fetch Streaming | 02-26 | 双通道流、Resume 机制 |
| 7 | Connector 增强 | 02-26 | 模型/模式选择、Discovery |
| 8 | Executor/Model 选择器 | 02-26 | WebSocket 选项发现、前端选择器 |
| 9 | 整洁架构重构 | 02-26 | Domain/Infrastructure 分层、Repository 模式 |

### 当前 Crate 状态

| Crate | 状态 | 文件数 | 说明 |
|-------|------|--------|------|
| `agentdash-domain` | ✅ 完成 | 14 | 实体、值对象、Repository traits |
| `agentdash-infrastructure` | ✅ 完成 | 6 | SQLite Repository 实现 |
| `agentdash-executor` | ✅ 大部分 | 7 | VibeKanban 完整、RemoteAcp 骨架 |
| `agentdash-api` | ✅ 完成 | 11 | 路由、流、错误处理 |
| `agentdash-coordinator` | ⚠️ 遗留 | 4 | 不在 workspace，待清理 |
| `agentdash-application` | ❌ 未创建 | 0 | 规划中 |

### 8 大模块完成度

```
02 State        ████░░░░░░  40%   [P1] Story/Task 基础 CRUD + StateChange
01 Connection   ███░░░░░░░  30%   [P2] Backend CRUD + 单连接器
05 Execution    █████░░░░░  50%   [P4] Agent 启动/取消/流 + Session 历史
03 Workspace    ██░░░░░░░░  20%   [P3] vibe-kanban worktree 隔离
08 View         █░░░░░░░░░  15%   [P7] 基础前端列表 + View CRUD
04 Orchestration░░░░░░░░░░   0%   [P5] 未开始
06 Injection    ░░░░░░░░░░   0%   [P6] 未开始
07 Validation   ░░░░░░░░░░   0%   [P6] 未开始
```

---

## 工作项清单

### 🔴 P0 — 基础修复与技术债务

> 这些是在继续新功能开发之前应优先解决的问题。

| ID | 工作项 | 模块 | 复杂度 | 说明 |
|----|--------|------|--------|------|
| P0-01 | 清理 agentdash-coordinator 遗留代码 | 基础 | S | 移除或整合到 infrastructure，当前与 BackendRepository 重复 |
| P0-02 | TaskRepository 补全 CRUD | 02-State | M | 当前只有 `list_by_story`，需添加 `create`、`get_by_id`、`update_status`、`delete` |
| P0-03 | StoryRepository 补全操作 | 02-State | M | 需添加 `get_by_id`、`update`、`delete`、`update_status` |
| P0-04 | 领域层单元测试 | 质量 | M | Story/Task 实体创建、状态迁移、值对象验证 |
| P0-05 | Clippy 警告修复 | 质量 | S | 修复当前存在的 minor clippy 警告 |
| P0-06 | 前端 index.html 标题修正 | 前端 | XS | 从 "frontend" 改为 "AgentDash" |
| P0-07 | 过时文档更新 | 文档 | S | directory-structure.md 仍写"技术栈未定"，需与实际对齐 |

### 🟠 P1 — State 模块完善（核心数据层）

> 模块 02 是所有其他模块的基础，必须优先完善。

| ID | 工作项 | 模块 | 复杂度 | 说明 |
|----|--------|------|--------|------|
| P1-01 | Story 状态迁移机制 | 02-State | L | 实现 `Created → ContextInjected → Decomposed → Orchestrating → Validating → Completed/Failed` 完整流转，包含状态守卫 |
| P1-02 | Task 状态迁移机制 | 02-State | L | 实现 `Pending → Assigned → Running → Validating → Completed/Failed` 完整流转 |
| P1-03 | StateChange 自动记录 | 02-State | M | 所有状态变更必须自动产生 StateChange 记录（含 reason），不可绕过 |
| P1-04 | Story 上下文管理 | 02-State | M | Story 的 `context` 字段结构化：PRD、规范文档引用、资源清单 |
| P1-05 | Task Artifacts 管理 | 02-State | M | Task 执行产物（代码变更、日志、文件）的结构化存储 |
| P1-06 | 状态快照能力 | 02-State | L | 可在任意时间点对状态进行快照，支持对比和回溯 |
| P1-07 | 前端 Story CRUD 完整流程 | 02-State / 前端 | L | 创建、编辑、删除、状态切换的完整 UI 交互 |
| P1-08 | 前端 Task 管理界面 | 02-State / 前端 | L | Task 创建、状态查看、Agent 绑定信息显示 |

### 🟡 P2 — Connection 模块完善（多后端支持）

> 模块 01 是 AgentDash 区别于 vibe-kanban 的核心特性。

| ID | 工作项 | 模块 | 复杂度 | 说明 |
|----|--------|------|--------|------|
| P2-01 | 多连接器注册机制 | 01-Connection | L | AppState 从单 connector 改为 ConnectorRegistry，支持多个连接器并存 |
| P2-02 | RemoteAcpConnector 实现 | 01-Connection | XL | 完成远程 ACP 后端的完整连接：认证、会话管理、流式推送 |
| P2-03 | 连接池与健康检查 | 01-Connection | M | 连接状态监测、自动重连、超时处理 |
| P2-04 | 后端动态添加/移除 | 01-Connection | M | 运行时添加/断开后端，不重启服务 |
| P2-05 | 前端多后端管理界面 | 01-Connection / 前端 | L | 后端列表、添加/编辑/删除、连接状态指示器 |
| P2-06 | 跨后端 Story 路由 | 01-Connection | M | Story 创建时指定目标后端，请求路由到正确连接器 |

### 🟢 P3 — Workspace 模块完善（执行环境隔离）

> 模块 03 确保 Task 执行的安全隔离。

| ID | 工作项 | 模块 | 复杂度 | 说明 |
|----|--------|------|--------|------|
| P3-01 | Workspace 生命周期管理 | 03-Workspace | L | `Pending → Preparing → Ready → Active → Paused → Destroying → Destroyed` |
| P3-02 | WorkspaceManager trait | 03-Workspace | M | 抽象接口：create、prepare、activate、pause、destroy |
| P3-03 | Git Worktree 策略实现 | 03-Workspace | M | 基于 vibe-kanban 的 worktree 管理，适配 AgentDash 的 WorkspaceManager |
| P3-04 | 环境模板系统 | 03-Workspace | M | 预定义执行环境配置模板（语言、工具链、权限级别） |
| P3-05 | Workspace 资源清理 | 03-Workspace | S | 定期清理已完成 Task 的 workspace 资源 |

### 🔵 P4 — Execution 模块完善（Agent 管理）

> 模块 05 在现有基础上完善多 Agent 管理能力。

| ID | 工作项 | 模块 | 复杂度 | 说明 |
|----|--------|------|--------|------|
| P4-01 | Task-Agent 一对一绑定 | 05-Execution | L | Task 创建时分配 Agent，维护绑定关系、PID、状态 |
| P4-02 | 多 Agent 并行执行 | 05-Execution | XL | 支持同时运行多个 Agent 进程，资源调度与限流 |
| P4-03 | Agent 生命周期管理 | 05-Execution | L | 创建、启动、暂停、恢复、终止的完整管理 |
| P4-04 | 执行结果结构化捕获 | 05-Execution | M | 从 Agent 输出提取代码变更、文件修改、测试结果等结构化 artifacts |
| P4-05 | Session 历史持久化优化 | 05-Execution | M | 从 JSONL 文件迁移到 SQLite，支持检索和分页 |
| P4-06 | 前端实时状态仪表盘 | 05-Execution / 前端 | L | 展示所有运行中 Agent 的状态、进度、资源使用 |

### 🟣 P5 — Orchestration 模块（编排引擎）

> 模块 04 是 AgentDash 的智能核心。

| ID | 工作项 | 模块 | 复杂度 | 说明 |
|----|--------|------|--------|------|
| P5-01 | OrchestrationStrategy trait | 04-Orchestration | M | 策略接口定义：decompose、plan、schedule |
| P5-02 | Story 拆解策略 — 手动模式 | 04-Orchestration | M | 用户手动将 Story 拆解为 Tasks |
| P5-03 | Story 拆解策略 — AI PM 模式 | 04-Orchestration | XL | 使用 Agent 分析 Story 自动生成 Task 列表 |
| P5-04 | Task 依赖管理 | 04-Orchestration | L | 定义 Task 间依赖关系，生成执行 DAG |
| P5-05 | ExecutionPlan 生成与管理 | 04-Orchestration | L | 根据依赖图生成执行计划，支持并行度控制 |
| P5-06 | 策略运行时切换 | 04-Orchestration | M | 同一 Story 可在执行过程中切换编排策略 |
| P5-07 | 前端编排可视化 | 04-Orchestration / 前端 | XL | Task 依赖图可视化、执行计划时间线 |

### ⚪ P6 — Injection & Validation（注入与验证）

> 模块 06/07 提升 Agent 执行质量。

| ID | 工作项 | 模块 | 复杂度 | 说明 |
|----|--------|------|--------|------|
| P6-01 | Injector trait 框架 | 06-Injection | M | 注入源、注入点、注入策略的抽象接口 |
| P6-02 | PRD/Spec 注入器 | 06-Injection | M | 从 Story context 提取相关 PRD 和规范文档注入到 Task |
| P6-03 | 项目上下文注入器 | 06-Injection | M | 注入项目结构、技术栈、编码规范等 |
| P6-04 | 历史执行注入器 | 06-Injection | L | 注入前序 Task 的执行结果和经验 |
| P6-05 | Validator trait 框架 | 07-Validation | M | 验证规则、验证模式、验证结果的抽象接口 |
| P6-06 | 脚本验证器 | 07-Validation | M | 运行 lint/test/typecheck 等脚本验证执行结果 |
| P6-07 | Agent 审查验证器 | 07-Validation | L | 使用 Agent 审查代码变更质量 |
| P6-08 | Ralph Loop 集成 | 07-Validation | L | 类似 Trellis 的循环验证机制：执行 → 验证 → 修复 → 再验证 |

### ⬜ P7 — View 模块（用户视图组织）

> 模块 08 提供灵活的 Story 组织方式。

| ID | 工作项 | 模块 | 复杂度 | 说明 |
|----|--------|------|--------|------|
| P7-01 | 视图类型框架 | 08-View | M | 列表、树、看板、时间线四种基础视图 |
| P7-02 | 看板视图 | 08-View / 前端 | L | 按状态分列的经典看板拖拽视图 |
| P7-03 | 树形视图 | 08-View / 前端 | M | Story → Task 树形展示 |
| P7-04 | 自定义分组与筛选 | 08-View / 前端 | M | 按标签、后端、状态等维度分组 |
| P7-05 | 跨后端聚合视图 | 08-View / 前端 | L | 在一个视图中聚合多个后端的 Stories |
| P7-06 | 时间线视图 | 08-View / 前端 | L | 按时间轴展示 Story/Task 进度 |

### 🔧 P8 — Application Layer 与质量保障

> 代码质量、测试、文档的长期投资。

| ID | 工作项 | 模块 | 复杂度 | 说明 |
|----|--------|------|--------|------|
| P8-01 | 创建 agentdash-application crate | 架构 | L | 提取 Use Case 层：CreateStory、DecomposeStory、CreateTask 等 |
| P8-02 | 事务管理 | 架构 | M | 跨 Repository 操作的事务一致性 |
| P8-03 | 前端测试框架搭建 | 质量 | M | Vitest + React Testing Library + MSW |
| P8-04 | 后端集成测试 | 质量 | L | Repository 集成测试、API 端到端测试 |
| P8-05 | E2E 测试 | 质量 | XL | Playwright/Cypress 端到端测试 |
| P8-06 | 项目 README | 文档 | S | 项目简介、快速开始、架构图 |
| P8-07 | API 文档 | 文档 | M | OpenAPI/Swagger 规范或等效文档 |
| P8-08 | 部署文档 | 文档 | M | 构建、配置、部署指南 |
| P8-09 | 响应式前端布局 | 前端 | M | 移动端适配 |

---

## 推荐开发顺序

基于模块间的依赖关系和优先级，推荐以下开发路径：

```
Phase 1: 基础巩固
├── P0-01 ~ P0-07  清理技术债务
├── P1-01 ~ P1-03  State 核心能力
└── P0-04          领域层测试

Phase 2: 数据层完善
├── P1-04 ~ P1-06  State 高级能力
├── P1-07 ~ P1-08  前端 Story/Task 完整流程
└── P2-01           多连接器注册

Phase 3: 执行层增强
├── P4-01 ~ P4-03  Agent 管理
├── P3-01 ~ P3-03  Workspace 管理
└── P4-04 ~ P4-05  执行结果捕获

Phase 4: 多后端与并行
├── P2-02 ~ P2-06  多后端支持
├── P4-02           多 Agent 并行
└── P4-06           实时仪表盘

Phase 5: 智能编排
├── P5-01 ~ P5-02  编排框架 + 手动拆解
├── P5-03 ~ P5-05  AI PM + 依赖管理
└── P5-06 ~ P5-07  策略切换 + 可视化

Phase 6: 质量闭环
├── P6-01 ~ P6-04  上下文注入
├── P6-05 ~ P6-08  验证机制
└── P8-01 ~ P8-02  Application 层

Phase 7: 视图与体验
├── P7-01 ~ P7-06  多种视图
├── P8-03 ~ P8-05  测试体系
└── P8-06 ~ P8-09  文档与响应式
```

---

## 参考项目利用计划

| 参考来源 | 可复用内容 | 对应工作项 |
|---------|-----------|-----------|
| **vibe-kanban** (third_party/) | Workspace 管理、Git worktree、Executor 生命周期、部分 UI | P3-01~03, P4-01~03 |
| **agent-client-protocol** (third_party/) | ACP 协议定义、Session 语义 | P2-02, P4-04 |
| **ABCCraft** (references/) | 前端 UI 范式、ACP 实现参考 | P1-07~08, P7-02 |
| **Trellis** (references/analysis/) | Hook 注入、Ralph Loop、Multi-Agent Pipeline | P6-01~04, P6-08, P5-03 |
| **胡渊鸣实践** (references/) | 任务队列循环、上下文管理、闭环反馈 | P5-04~05, P6-04, P6-08 |

---

## 工作量估算

| 复杂度 | 预估工时 | 工作项数量 |
|--------|----------|-----------|
| XS | 0.5 天 | 1 |
| S | 1 天 | 5 |
| M | 2-3 天 | 21 |
| L | 3-5 天 | 17 |
| XL | 5-10 天 | 5 |
| **合计** | **约 150-200 工作天** | **49 项** |

> 注：以上估算基于单人（AI 辅助）开发效率，实际可通过 Multi-Agent Pipeline 并行缩短。
