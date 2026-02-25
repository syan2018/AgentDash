# PRD：AgentDashboard 项目初始化

**任务ID：** project-init
**创建日期：** 2026-02-25
**优先级：** P1（所有后续开发的基础）
**负责人：** 待定

---

## 一、背景与目标

### 1.1 背景

AgentDashboard 目前处于**框架定义阶段**，已完成：
- 核心概念设计（Story/Task 双层模型）
- 三层架构设计（编排层/状态层/连接层）
- 八大模块的接口概念定义
- Spec 文档体系建立
- **技术选型确定**：Rust (Axum + SQLx) + React (Vite)，引入 **Coordinator（中控层）** 解决多后端看板同步。
- **协议选型**：采用 **Agent Client Protocol / MCP** + **RPC & NDJSON** 实现流式推送与状态恢复。

### 1.2 目标

完成项目初始化，搭建 Rust + React 的技术骨架，引入 `vibe-kanban` 作为基础库，实现包含中控层与后端通信的原型，使后续各模块的开发可以并行展开。

---

## 二、技术选型（已确定）

详见 [`.trellis/spec/tech-stack.md`](../spec/tech-stack.md)

### 核心决策

| 层级 | 技术 | 说明 |
|------|------|------|
| 后端 (Backend) | Rust + Axum + SQLx | 性能优先，单机/局域网数据源 |
| 中控 (Coordinator) | Supabase / SQLite | 存储连接凭证、全局看板、用户偏好 |
| 前端 (Client) | React + TypeScript + Vite | 生态成熟，类型安全 |
| 协议 | RPC (JSON-RPC) + NDJSON | 指令交互与流式推送（带恢复机制） |
| 数据标准 | Agent Client Protocol / MCP | 统一 Agent 产物的语义 |

### 与 vibe-kanban 的关系

- **作为 git submodule 引入至 `third_party/`**，作为基础能力库。
- 复用范围：Workspace 管理、Execution 生命周期、部分 UI 基础。
- 扩展点：Story/Task 模型、中控层路由、跨后端 View 聚合。

---

## 三、需要完成的工作

### Phase 1：项目结构初始化

#### 1.1 Monorepo 结构

```
AgentDash/
├── .gitmodules                  # vibe-kanban submodule (third_party/)
├── Cargo.toml                   # Rust workspace (含 crates/ 和 third_party/ 成员)
├── package.json                 # npm 根配置（前端依赖）
├── crates/                      # 核心业务 crates
│   ├── agentdash-coordinator/   # [新增] 中控逻辑（偏好/连接）
│   ├── agentdash-state/         # [新增] Story/Task 状态存储
│   └── agentdash-api/           # [新增] API 服务入口（RPC + Streaming）
├── third_party/
│   └── vibe-kanban/             # [submodule]
└── frontend/                    # React 前端
```

#### 1.2 配置子模块路径

确保 Rust Workspace 能够引用 `third_party/vibe-kanban/crates` 中的特定 crate。

---

### Phase 2：核心协议与后端骨架

#### 2.1 RPC 与 Resume 机制

后端需要实现基础的 RPC 框架，并支持 `since_id` 参数。

```rust
// 示例：状态恢复接口
async fn get_events_from(cursor: String) -> Vec<StateChange>;
```

#### 2.2 agentdash-coordinator 实现（最小版本）

管理后端列表（Endpoint, Token）及视图配置。

```rust
pub struct BackendConfig {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub auth_token: String,
}
```

---

### Phase 3：前端看板骨架

#### 3.1 多后端 Store

```typescript
// frontend/src/stores/coordinatorStore.ts
interface CoordinatorState {
  backends: BackendConfig[];
  globalViews: ViewConfig[];
  currentStoryId: string | null;
}
```

#### 3.2 统一状态流接收

实现基于 `Agent Client Protocol` 的解析层。

---

## 四、子任务拆解

| # | 子任务 | 优先级 | 预估 |
|---|--------|--------|------|
| 1 | 创建项目基础结构（Cargo workspace + third_party） | P0 | 0.5天 |
| 2 | 实现 agentdash-coordinator（后端管理与视图存储） | P1 | 1天 |
| 3 | 实现 agentdash-state（Story/Task SQLite 存储） | P1 | 1天 |
| 4 | 实现 agentdash-api（RPC 指令集 + NDJSON 流） | P1 | 1天 |
| 5 | 前端看板框架（多后端隔离 Store + Resume 客户端） | P1 | 1天 |
| 6 | 集成 MCP/Agent Protocol 类型定义 | P2 | 0.5天 |

---

## 五、验收标准

- [ ] `cargo build` 成功引用 `third_party/vibe-kanban` 中的模块
- [ ] 后端支持通过 RPC 命令恢复（Resume）断开连接期间的状态变更
- [ ] 前端可配置多个后端连接，并能在统一看板展示 Story
- [ ] 遵循 Agent Client Protocol 定义的 Task 产物结构
- [ ] 所有代码注释和文档使用中文

---

*更新时间：2026-02-25 - 已同步 Coordinator & Resume 设计决策*
