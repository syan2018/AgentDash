# Directory Structure

> How backend code is organized in this project.

---

## Overview

<!--
Document your project's backend directory structure here.

Questions to answer:
- How are modules/packages organized?
- Where does business logic live?
- Where are API endpoints defined?
- How are utilities and helpers organized?
-->

<!-- PROJECT-SPECIFIC-START: AgentDashboard Backend Structure -->
> **AgentDashboard 后端代码的组织方式。**
> **注意：当前为概念阶段，技术栈未定，目录结构仅为参考设计。**

### 设计原则

按照项目的**策略可插拔**原则，目录结构应体现模块边界：
- 每个模块独立目录，模块间通过接口交互
- 接口定义与实现分离
- 策略（Strategy）作为可替换组件
<!-- PROJECT-SPECIFIC-END -->

---

## Directory Layout

```
<!-- Replace with your actual structure -->
src/
├── ...
└── ...
```

<!-- PROJECT-SPECIFIC-START: Directory Tree -->
### 实际目录布局（Rust / Axum）

```
crates/
├── agentdash-api/src/           # HTTP API 服务器（主入口）
│   ├── main.rs                  # Axum 启动入口
│   ├── app_state.rs             # AppState（store, coordinator, executor_hub, connector）
│   ├── routes.rs                # 路由注册（所有 /api/* 路由）
│   ├── rpc.rs                   # ApiError 统一错误处理
│   ├── stream.rs                # 全局事件流（SSE + NDJSON）
│   ├── routes/                  # 路由处理函数
│   │   ├── health.rs
│   │   ├── backends.rs
│   │   ├── stories.rs
│   │   ├── acp_sessions.rs      # ACP 会话流（SSE/NDJSON/WS）
│   │   └── discovery.rs         # 执行器发现 API
│   └── executor/                # 执行层
│       ├── mod.rs               # 导出 AgentConnector, ExecutorHub 等
│       ├── connector.rs         # AgentConnector trait + ConnectorType + ConnectorCapabilities
│       ├── hub.rs               # ExecutorHub（会话管理 + 广播）
│       ├── adapters/
│       │   └── normalized_to_acp.rs  # vibe-kanban 日志 → ACP 通知转换
│       └── connectors/
│           ├── mod.rs
│           ├── vibe_kanban.rs   # 本地执行器连接器（LocalExecutor）
│           └── remote_acp.rs    # 远程 ACP 后端连接器（骨架）
├── agentdash-state/             # 状态存储（Story, Task, StateChange）
└── agentdash-coordinator/       # 后端/连接管理
```

### 关键 API 端点

| 路径 | 方法 | 说明 |
|------|------|------|
| `/api/agents/discovery` | GET | 执行器发现（返回连接器信息、执行器列表、权限策略） |
| `/api/sessions/{id}/prompt` | POST | 启动 ACP 会话执行 |
| `/api/sessions/{id}/cancel` | POST | 取消会话 |
| `/api/acp/sessions/{id}/stream` | GET | ACP 会话流（SSE） |
| `/api/acp/sessions/{id}/stream/ndjson` | GET | ACP 会话流（NDJSON） |
| `/api/events/stream` | GET | 全局事件流（SSE） |

### 连接器架构

```
AgentConnector trait
├── connector_id()          → &str
├── connector_type()        → ConnectorType (LocalExecutor | RemoteAcpBackend)
├── capabilities()          → ConnectorCapabilities
├── get_preset_configs()    → Vec<PresetConfig>
├── prompt()                → ExecutionStream
└── cancel()                → ()

实现：
├── VibeKanbanExecutorsConnector  → LocalExecutor（通过 vibe-kanban executors crate）
└── RemoteAcpConnector            → RemoteAcpBackend（骨架，待实现）
```
<!-- PROJECT-SPECIFIC-END -->

---

## Module Organization

<!-- How should new features/modules be organized? -->

<!-- PROJECT-SPECIFIC-START: Module Guidelines -->
### 每个模块的标准结构

```
modules/<module-name>/
├── interfaces/         # 接口/类型定义（稳定，不轻易改变）
│   └── index          # 导出接口
├── strategies/         # 可替换策略实现
│   ├── <strategy-a>/
│   └── <strategy-b>/
└── index               # 模块入口，注册策略
```

### 模块依赖方向

```
api → orchestration → state
                   ↓       ↓
              injection  execution → workspace
                              ↓
                          validation
```

> **禁止跨层依赖：** api层不能直接访问 state 层内部实现
<!-- PROJECT-SPECIFIC-END -->

---

## Naming Conventions

<!-- File and folder naming rules -->

<!-- PROJECT-SPECIFIC-START: Naming Rules -->
> **注意：技术栈确定后，根据所选语言的约定调整命名规范。**

- **模块目录**：小写短横线（kebab-case），如 `state-manager/`
- **接口文件**：描述性名称，如 `StateManager`, `ConnectionManager`
- **策略实现**：`<技术>-<功能>`，如 `sqlite-state-store`, `worktree-workspace`
- **实体类型**：PascalCase，如 `Story`, `Task`, `StateChange`
<!-- PROJECT-SPECIFIC-END -->

---

## Examples

<!-- Link to well-organized modules as examples -->

<!-- PROJECT-SPECIFIC-START: Current Status -->
### 当前状态

> 技术栈未确定，上述为概念性目录设计。
> 确定技术栈后，在此文件更新实际目录结构。

**需要讨论决定：**
- [ ] 后端语言选择（Node.js / Python / Go / Rust / ...）
- [ ] 框架选择
- [ ] 存储方案（影响 state 模块目录结构）
- [ ] 构建工具和项目结构约定
<!-- PROJECT-SPECIFIC-END -->
