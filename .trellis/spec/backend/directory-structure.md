# Directory Structure

> 后端代码的整洁架构分层组织方式。

---

## Crate 一览

```
crates/
├── agentdash-agent-runtime-contract/ # AgentDash-owned Runtime IDs/commands/events/profiles/errors
├── agentdash-agent-runtime-wire/     # transport-neutral Runtime request/response/event framing
├── agentdash-agent-runtime-test-support/ # Runtime/Driver共享conformance behavior harness
├── agentdash-api/               # Interface Layer — HTTP 路由、DTO、中间件
├── agentdash-application/       # Application Layer — 剩余用例编排与 composition adapters
├── agentdash-application-ports/ # Application Boundary Ports — API/local 实现、application 消费的纯端口
├── agentdash-application-workflow/ # Application Layer — Workflow catalog/compiler/orchestration runtime
├── agentdash-application-hooks/ # Application Layer — Hook policy provider 与 script surface
├── agentdash-application-shared-library/ # Application Layer — Shared Library seed/install/publish use cases
├── agentdash-domain/            # Domain Layer — 实体、值对象、Repository 接口
├── agentdash-infrastructure/    # Infrastructure Layer — PostgreSQL/SQLite 持久化
├── agentdash-workspace-module/  # Workspace Module Boundary — module contract 与 Canvas 子模块业务
├── agentdash-executor/          # Infrastructure Layer — 连接器、LLM Bridge
├── agentdash-platform-spi/               # 平台能力、工具与 Hook SPI
├── agentdash-agent/             # Agent Loop 引擎（纯 loop + bridge trait）
├── agentdash-agent-types/       # Agent 领域通用类型
├── agentdash-llm-provider/      # Provider catalog、凭据解析与Agent Core LLM bridges
├── agentdash-agent-protocol/    # Backbone Protocol + 外部协议 adapter
├── agentdash-mcp/               # MCP Server 实现
├── agentdash-relay/             # WebSocket Relay 协议
├── agentdash-local/             # 本机后端
└── agentdash-local-tauri/       # Tauri 桌面端封装；通过 external/sidecar 连接 Dashboard API
```

> 具体的文件级目录结构请直接查看代码库，不在 spec 中维护逐文件列表。

---

## 整洁架构分层

**核心原则**：依赖方向始终向内，外层依赖内层。

```
Interface Layer (agentdash-api)
    ↓ depends on
Application Layer (agentdash-application)
    ↓ depends on
Domain Layer (agentdash-domain)
    ↑ implemented by
Infrastructure Layer (agentdash-infrastructure, agentdash-executor)

Agent 子系统（独立于主分层）：
agentdash-agent-runtime-contract ← agentdash-agent-runtime-wire
agentdash-agent-runtime-contract ← agentdash-agent-runtime-test-support

现有Agent执行链在后续工作包中逐步切换到Managed Runtime与Integration Driver Host；新Runtime Contract不反向依赖application、domain repository、旧protocol、vendor或transport。
```

### 分层职责

| 分层 | Crate | 职责 | 允许依赖 |
|------|-------|------|----------|
| **Interface** | `agentdash-api` | HTTP 路由、DTO、中间件、错误映射 | application, domain |
| **Application** | `agentdash-application` | 剩余用例编排与 composition adapters：session / context / task / story / repository set wiring | domain, spi, split application crates |
| **Application Split Crates** | `agentdash-application-workflow`, `agentdash-application-hooks`, `agentdash-application-shared-library` | 大型 application use case 边界：workflow 编排、hook policy、Shared Library seed/install/publish | domain, spi, application-ports；只在用例所有权明确时依赖 sibling application crate |
| **Application Ports** | `agentdash-application-ports` | application 边界 port、transport trait、轻量 DTO/error | domain, relay, agent-protocol |
| **Workspace Module Boundary** | `agentdash-workspace-module` | Workspace Module 业务边界：module identity、presentation URI、operation contract、runtime tool provider，以及 `canvas` 子模块中的 Canvas 管理/runtime/VFS/visibility 业务服务 | domain, application-ports, application-vfs, runtime-gateway |
| **Domain** | `agentdash-domain` | 实体、值对象、Repository 接口、领域事件 | 无外部业务库 |
| **Infrastructure** | `agentdash-infrastructure`, `agentdash-executor` | 持久化实现、连接器、WebSocket 中继 | domain |
| **Agent Types** | `agentdash-agent-types` | 跨层共享类型（Message/Tool/Context/Delegate） | serde, async-trait |
| **Agent Engine** | `agentdash-agent` | Agent Loop 引擎、LlmBridge trait | agent-types, domain |
| **LLM Provider Adapter** | `agentdash-llm-provider` | Provider catalog、账户凭据解析、模型发现与provider protocol bridge；向Agent Core产出固定provider/model的`LlmBridge` | agent, domain, spi |
| **Agent Runtime Contract** | `agentdash-agent-runtime-contract` | canonical Runtime IDs、commands、events、snapshots、profiles、availability、errors与Driver SPI | serde、schema/TS生成、结构化错误与async trait |
| **Agent Runtime Wire** | `agentdash-agent-runtime-wire` | typed request/response/notification/ack、protocol revision与critical frame violation | agent-runtime-contract、serde、schema/TS生成 |
| **Agent Runtime Test Support** | `agentdash-agent-runtime-test-support` | 可被Runtime/Host/Adapter复用的行为一致性测试 | agent-runtime-contract、test runtime |

---

## 添加新模块的步骤

1. **Domain Layer**：`agentdash-domain/src/<module>/` 下创建 `entity.rs` + `repository.rs` + `value_objects.rs`
2. **Infrastructure Layer**：`agentdash-infrastructure/src/persistence/postgres/` 下创建 `<module>_repository.rs`
3. **Interface Layer**：在 `app_state.rs` 添加 trait 对象，在 `routes/` 添加路由
4. **Application Layer**（复杂业务时）：创建用例模块，路由改为调用用例

> **禁止跨层依赖**：API 层不能直接访问 Repository 的具体实现。

`agentdash-local-tauri` 不作为 API composition root；它通过 external origin 或 `agentdash-server` sidecar process 连接 Dashboard API，原因是桌面壳负责本机能力与进程生命周期，而 HTTP route、AppState、migration 与业务 API ownership 属于 `agentdash-api`/`agentdash-server`。

`agentdash-application-ports` 只承载 API/local 实现、application 消费的纯端口，原因是 transport trait 需要被 interface/runtime composition root 实现，同时又不能让 API 反向依赖 application 内部编排模块。Domain 仍不依赖 contracts、protocol DTO 或 application ports。

跨多个 application 入口共享的 command / intent / typed modifier 应放入 `agentdash-application-ports` 的业务 namespace，并在 namespace 内按主合同、modifier、outcome 或 error 拆文件。`launch` namespace 使用 `command.rs` 与 `modifier.rs`，原因是启动来源入口很多，但进入 frame construction / launch planning 前应共享同一套边界合同。

大型 application facade 拆 owner 时，owner 文件放在 facade 同级的业务子目录，并由 `mod.rs` 做 crate-private re-export。`agentdash-application-lifecycle/src/lifecycle/dispatch/` 使用这种布局，原因是 public facade 需要保持用例入口清晰，而 run/orchestration、runtime materialization、subject association、relation/gate 和 reducer bridge 的副作用策略需要各自拥有可 review 的文件边界。

`agentdash-domain::canvas` 承载 Canvas 实体、值对象、repository trait、runtime observation / interaction snapshot contract 与 embedded Canvas skill bundle。这样 infrastructure 可以只实现 domain trait，不需要依赖 workspace-module。

`agentdash-workspace-module` 是 Workspace Module 业务边界：Canvas 作为 `agentdash-workspace-module::canvas` 子模块承载 mount/module/presentation identity、Canvas 管理/runtime/VFS/visibility 业务服务、operation keys、runtime tool provider 与 Workspace Module descriptor/presentation 组装。它通过 domain repository trait 和 application ports 连接外部能力。

Workspace Module 与运行中 Agent 的协作端口使用AgentRun语义命名。运行坐标通过`AgentRunRuntimeTarget/Binding`解析；workspace-module不接触Driver source identity。HTTP authorization、route mapping、Postgres adapter与composition仍属于API/application/infrastructure边界。

---

## 命名规范

| 类型 | 规范 | 示例 |
|------|------|------|
| Crate | `agentdash-<layer>` | `agentdash-domain` |
| 实体 struct | PascalCase | `Story`, `Task` |
| Repository trait | `<Entity>Repository` | `StoryRepository` |
| Repository 实现 | `<Tech><Entity>Repository` | `PostgresStoryRepository` |
| 值对象 | PascalCase 描述性 | `StoryStatus`, `TaskStatus` |
| 目录 | 小写单数 | `agentdash-domain/src/story/` |
