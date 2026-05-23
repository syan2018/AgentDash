# AgentDash 架构 Review 汇总与重构问题文档

> 生成日期：2026-05-23  
> 来源：两份外部静态架构 review、当前工作区轻量核对。  
> 定位：这是后续拆分重构任务的输入文档，不替代 `.trellis/spec/` 中的架构契约。

## 1. 总体判断

两份 review 的核心判断高度一致：AgentDash 已经形成了 Agent Runtime 控制平面的主干，关键抽象包括 Session、ExecutionContext、VFS Mount、Relay、BackboneEnvelope、Lifecycle/Workflow、Plugin API 和前端工作台。当前阶段的主要工作不是继续增加概念，而是把已经出现的边界收紧，让后续 runtime、workflow、local/cloud hybrid、plugin marketplace 的功能增长有稳定承载面。

共同指向的问题是：系统进入了架构收敛期，复杂度集中在组合根、Session 拉起链路、Relay 时序、VFS/Workflow 大模块、前后端契约和 infrastructure/application 分层边界。优先级最高的重构应当先解决运行时正确性和依赖图可理解性，再做大规模模块拆分。

当前工作区与 review 快照有差异：`Cargo.lock`、`pnpm-lock.yaml`、`pnpm-workspace.yaml`、`scripts/`、`docs/`、`tests/` 已存在。因此“补齐仓库启动文件”不应再作为主问题，但“锁住可复现构建与 CI 校验”仍然可以作为工程质量事项继续跟踪。

## 2. 两份 Review 的共识

### 2.1 架构方向成立

两份 review 都认可当前方向：AgentDash 不是普通 chat app，而是围绕 Agent Runtime 的控制平面。领域事实、执行上下文、资源挂载、事件流、本机 relay 和 UI 投影已经有清晰雏形。

被共同认可的长期资产：

| 抽象 | 价值 |
| --- | --- |
| `ExecutionContext` | 将 session 级 who/where 与 turn 级 how/control 拆开，为 restore、多 connector、多 backend 留出空间。 |
| `VFS Mount` / `MountProvider` | 将 workspace、canvas、skill asset、lifecycle artifact 等统一为 agent 可访问资源。 |
| `BackboneEnvelope` | 作为 runtime event fact，让 UI、持久化和 relay 不直接耦合 agent loop 内部状态。 |
| `SessionBinding` | 把 Project / Story / Task / Session ownership 集中表达，适合继续强化为 session 归属事实源。 |
| `agentdash-agent-protocol` TS 生成 | 已经证明跨层协议生成可行，后续可扩展到更多 DTO。 |

### 2.2 最大复杂度集中在几个入口

两份 review 都认为以下区域已经承担过多职责：

| 区域 | 共同观察 |
| --- | --- |
| `agentdash-api/src/app_state.rs` | composition root 过重，repository、plugin、VFS、relay、session、auth、routine、background worker 全部集中装配。 |
| `agentdash-application/src/session` | Session pipeline 太长，construction、planning、tool/MCP/VFS、hook、capability、commit、stream ingestion 混在一起。 |
| `agentdash-domain/src/workflow/value_objects.rs` | Workflow contract、activity、lifecycle、run state、capability、validation 等语义层级聚集在同一文件。 |
| VFS 模块 | VFS 是关键边界，但 provider、tool、mutation、materialization、surface 解析继续增长后需要更清晰目录边界。 |
| 前端大 store/page | Zustand store、页面、DTO normalizer、stream hook 都有膨胀趋势。 |

### 2.3 应先修运行时正确性

两份 review 都给出“先小修关键时序，再做大拆分”的路线。当前核对后仍然成立的高优先级问题包括：

| 问题 | 当前核对 |
| --- | --- |
| Relay prompt sink 注册顺序 | `RelayAgentConnector::prompt` 仍然先 `relay_prompt().await`，后 `register_session_sink`，存在早到 session event 丢失窗口。 |
| backend disconnect pending 清理 | `BackendRegistry::unregister` 仍然保留 `retain(|_, _| true)`，pending command 可能等到 timeout。 |
| local prompt forwarder 去重 | `handle_prompt` 成功后每次 `tokio::spawn(forward_session_notifications)`，同一 relay session 多轮 prompt 仍需核对重复转发风险。 |

### 2.4 分层边界需要收紧

两份 review 都提到 `agentdash-infrastructure -> agentdash-application` 的依赖方向会增加长期耦合压力。当前 `.trellis/spec/backend/architecture.md` 允许 Infrastructure 实现 Domain/Application 所需端口，说明这是当前基线；但随着系统变大，持久化 contract 下沉到 domain/spi/ports crate 会让 adapter 不必依赖完整 application orchestration。

## 3. 两份 Review 的差异

### 3.1 关注层级不同

第一份 review 更偏运行时链路和具体 bug 风险，尤其关注 Relay、Session pipeline、`list_executors` sync/async mismatch、local forwarder、protocol 拆分。

第二份 review 更偏全局工程治理和模块边界，强调 `AppState`、migrations/schema 来源、Plugin API host wiring、repository DDL、route composition、前端 contract 生成。

### 3.2 对 P0 的排序不同

第一份 review 的 P0 更偏“事件不丢、断连快失败、forwarder 不重复、加关键测试”。它把 `AppState` 拆分列为高优先级架构瓶颈，但不是第一个运行时 bug。

第二份 review 的 P0 更偏“工程地基与组合根”：AppState god object、schema 来源重复、Plugin API 暴露多于 host 接入。它把 migrations-only 和 PluginHost 收敛提得更靠前。

综合判断：下一轮最小闭环应先处理 Relay 三个 correctness 点，然后进入 AppState/bootstrap 拆分。schema 来源与 Plugin API 收敛需要先和当前 spec 基线对齐，因为它们牵涉数据库/插件长期契约。

### 3.3 对数据库策略的建议力度不同

第二份 review 建议 PostgreSQL 完全 migrations-only，并移除 repository runtime DDL。当前项目 spec 仍要求：

- PostgreSQL 新增 migration 文件；
- `CREATE TABLE IF NOT EXISTS` 保证新建库完整；
- SQLite 在 `initialize()` 中追加 `ALTER TABLE`。

因此数据库策略不宜直接作为单点重构执行，应先作为“schema 事实源收敛”专题设计：明确 PostgreSQL、SQLite、本机缓存和测试库各自的初始化契约，再更新 `.trellis/spec/backend/database-guidelines.md`。

### 3.4 对文档/仓库完整性的结论有旧快照差异

第二份 review 提到 zip 中缺少 `scripts/`、`pnpm-workspace.yaml`、lockfile、docs、tests。当前工作区这些文件已存在。后续文档引用这份 review 时，应把这一点标记为“历史快照问题”，避免重复排入 roadmap。

## 4. 重构问题清单

### P0：Relay 运行时正确性

目标是让 cloud/local relay 的 prompt 与 event stream 在时序上可靠。

| 问题 | 影响 | 建议方向 | 验证 |
| --- | --- | --- | --- |
| prompt 后注册 session sink | 本机可能在 `response.prompt` 前发送 notification，云端 sink 尚未注册导致事件丢失。 | `RelayAgentConnector::prompt` 先创建 channel 并注册 sink，再发送 `relay_prompt`；失败时 unregister；stream terminal/cancel/end 时清理。 | 增加测试：local 在 response 前发送 notification，cloud stream 能收到。 |
| unregister 不清 pending | backend 断连后，等待中的 command 只能等 timeout，UI/runtime 反馈慢且语义不清。 | pending 记录 `backend_id`，断连时移除该 backend 的 pending sender，使等待方立即得到 `ResponseDropped`。 | 增加测试：send_command pending 后 unregister backend，调用方立即失败。 |
| local forwarder 重复启动风险 | 同一 relay session 多轮 prompt 可能产生多个 receiver，导致事件重复转发或资源泄漏。 | local 维护 `session_id -> forwarder handle`，已有 forwarder 时复用；或提升到 session lifecycle 级 forwarding。 | 增加测试：同一 session 两轮 prompt，同一 notification 只转发一次。 |

### P1：AppState / Bootstrap 拆分

目标是降低组合根复杂度，让服务构造顺序和循环依赖显式化。

建议先保持外部 `AppState` 字段和 route 行为稳定，只拆内部 wiring：

```text
agentdash-api/src/bootstrap/
  repositories.rs
  plugins.rs
  auth.rs
  vfs.rs
  relay.rs
  session.rs
  routines.rs
  background_workers.rs
```

可进一步沉淀为：

```text
RepositoryBootstrap
PluginBootstrap
VfsKernel
RelayKernel
SessionKernel
RoutineKernel
AuthKernel
RuntimeGatewayKernel
```

验收重点：

- `AppState::new_with_plugins` 只表达构造顺序和组合结果。
- 每个 kernel 的输入/输出结构清晰，能单独单测关键 wiring。
- 延迟注入点有名字和原因，后续能被 staged builder 或显式 init graph 替换。

### P2：Session Launch Pipeline 阶段化

目标是把 session 拉起从“长流程函数”收敛为可审计的阶段。

建议目标链路：

```text
LaunchCommand
  -> SessionConstruction
  -> TurnPreparation
  -> ConnectorLaunch
  -> TurnCommit
  -> TurnEventIngestion
```

与既有 session refactor plan 对齐时，可进一步形成：

```text
LaunchCommand -> LaunchPlanner -> LaunchPlan -> LaunchExecutor / TurnSupervisor
```

验收重点：

- plan/prepare/start/commit/attach 的副作用边界清楚。
- connector accepted、turn_started、bootstrap commit 的顺序有显式 policy。
- HTTP、Task、Workflow、Routine、Companion、Local relay 都通过同一 launch service 进入。
- `SessionHub` 逐步退化为 orchestration facade，而不是继续吸纳新业务。

### P3：持久化与分层边界收敛

目标是让 infrastructure 实现窄端口，而不是依赖完整 application orchestration。

建议方向：

```text
domain / agent-protocol / relay
        ↓
spi / ports / persistence-contract
        ↓
application

adapters:
  infrastructure
  executor
  first-party-plugins

composition roots:
  api
  local
  local-tauri
```

可先从最小 port 下沉开始：

- `SessionPersistence`
- terminal effect outbox persistence
- runtime event persistence
- audit persistence
- repository-facing record DTO

验收重点：

- `agentdash-infrastructure` 不需要 import 大量 `agentdash_application::session::*` orchestration 类型。
- application service 接收真实需要的 port/bundle，而不是默认拿完整 `RepositorySet`。
- `.trellis/spec/backend/architecture.md` 与 `repository-pattern.md` 同步更新新的依赖基线。

### P4：VFS / Workflow / Relay Protocol 模块边界

目标是按领域语义拆大文件，先做目录级边界，再考虑 crate 级拆分。

建议拆分：

| 区域 | 建议边界 |
| --- | --- |
| Workflow value objects | `contract.rs`、`lifecycle_def.rs`、`activity_def.rs`、`run_state.rs`、`tool_capability.rs`、`hook_rule.rs`、`mount_directive.rs`、`validation.rs` |
| VFS | `core`、`providers`、`tools`、`mutation`、`materialization`、`surface` |
| Relay protocol | `handshake`、`prompt`、`workspace`、`tool`、`mcp`、`terminal`、`session_event`、`capabilities` |
| Agent loop | `turn`、`tool_call`、`event_mapping`、`cancellation`、`prompt`、`output` |

验收重点：

- 顶层 `mod.rs` 可以保留 re-export，降低调用方改动。
- 拆分不改变 JSON 协议形态和数据库 schema。
- 每次拆分都配合 `rg` 检查重复路径/类型/mapper 是否可以同步收敛。

### P5：前后端契约与前端 feature 收拢

目标是降低 DTO drift，让前端 store/hook/page 不再承担全部副作用。

建议方向：

- 将 TS 生成从 Backbone protocol 扩展到 Workflow、Session、VFS、Shared Library、MCP Preset、ProjectAgent 等 DTO。
- 对 generated type 增加 drift check。
- 抽通用 NDJSON stream client，让 Project stream 与 Session stream 共享 transport。
- `useSessionStream` 拆成 transport、normalizer、reducer、React hook。
- `workflowStore.ts`、`storyStore.ts` 按 `api.ts / normalize.ts / reducer.ts / selectors.ts / store.ts` 收拢。
- 大页面按 feature 目录拆分，但保留已有设计系统和交互基线。

验收重点：

- 手写 enum/string normalizer 明显减少。
- store 中纯状态转换可独立单测。
- 前端 runtime surface、session event、workflow DTO 与后端契约同源。

## 5. 建议的执行顺序

### 第一阶段：Relay correctness 闭环

范围小、收益高，适合作为下一轮实际 coding task：

1. relay prompt 先注册 session sink；
2. backend disconnect 清理对应 pending requests；
3. local session notification forwarder 去重；
4. 补三类时序测试；
5. 将 relay 时序不变量沉淀到 `.trellis/spec/cross-layer/desktop-local-runtime.md` 或 relay appendix。

### 第二阶段：Bootstrap 拆分

先拆 `AppState::new_with_plugins` 内部结构，不改变 HTTP API 和 service 行为：

1. 提取 repository/plugin/vfs/relay/session/auth/routine/background bootstrap 模块；
2. 建立 `ServiceSet` 或 kernel output 结构；
3. 给延迟注入点命名并集中记录原因；
4. 增加 bootstrap 层 architecture test 或 import boundary check。

### 第三阶段：Session pipeline 阶段化

承接既有 [Session 拉起流程定向 Review](../2026-05-16-zip-static-review/session-launch-refactor-plan.md)：

1. 建立不可变 `LaunchPlan` 或阶段结果结构；
2. 分离 construction / preparation / connector launch / commit / ingestion；
3. 统一 HTTP、Task、Workflow、Routine、Companion、Local relay 入口；
4. 补 owner bootstrap、connector failure、并发 prompt、turn terminal 的测试矩阵。

### 第四阶段：分层与 schema 策略专题

先形成设计，再执行迁移：

1. 讨论 PostgreSQL 是否走 migrations-only；
2. 明确 SQLite 本机会话缓存的初始化策略；
3. 决定 persistence contract 放在 domain、spi 还是新 crate；
4. 更新 `.trellis/spec/backend/database-guidelines.md` 与 repository pattern；
5. 分批移除 infrastructure 对 application orchestration 类型的依赖。

### 第五阶段：模块拆分与契约生成

在前几阶段边界稳定后进行：

1. 拆 Workflow / VFS / Relay protocol / Agent loop 大模块；
2. 扩展 TS DTO 生成；
3. 前端 feature store、stream hook、mapper 收拢；
4. 增加 generated drift check 与跨层契约测试。

## 6. 后续拆任务建议

可以拆成这些独立 Trellis task：

| 任务 | 目标 | 优先级 |
| --- | --- | --- |
| Relay 时序正确性修复 | 解决 sink 注册、pending 清理、local forwarder 去重并补测试 | P0 |
| AppState Bootstrap 拆分 | 将 `new_with_plugins` 拆为 kernel/bootstrap 模块 | P1 |
| Session LaunchPlan 阶段化 | 统一 session launch 入口与阶段结果 | P1 |
| Database Schema 事实源决策 | 明确 migrations、repository initialize、SQLite 初始化策略 | P1 |
| Persistence Ports 下沉 | 降低 infrastructure 对 application 的依赖 | P2 |
| Workflow/VFS/Relay 模块拆分 | 按语义边界拆大文件，保持行为不变 | P2 |
| Frontend Contract Generation | 扩展 TS 类型生成并减少手写 normalizer | P2 |

## 7. 判断标准

后续每个重构任务应优先满足以下标准：

1. 先锁 correctness，再拆结构。
2. 优先缩短关键链路的隐式状态窗口，例如 relay event、turn commit、bootstrap commit。
3. 每个模块只接收它真实需要的 port/bundle。
4. 跨层 JSON / NDJSON / DTO 尽量同源生成或有 drift check。
5. 文档更新应沉淀架构原因和当前契约；任务过程、历史错误形态留在 review/task 文档，不进入 spec 主线。
