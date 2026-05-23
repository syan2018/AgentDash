# AgentDash 平台边界与工程治理静态 Review

## 总体判断

这个仓库的核心方向是清晰的：AgentDash 试图把 Agent Runtime、Connector、VFS、Lifecycle、Relay、Session Event Stream、前端工作台这些能力组合成一个“Agent 控制平面”。整体分层大致是：

```text
domain / protocol / spi
        ↓
application / executor / mcp / relay
        ↓
infrastructure
        ↓
api / local / tauri
        ↓
app-web
```

代码里已经有不少好的架构苗头：
`agentdash-agent-protocol` 独立承载事件协议并生成 TS 类型；`agentdash-domain` 试图沉淀纯领域模型；`agentdash-application` 里有 Session、VFS、Workflow 等应用服务；`agentdash-plugin-api` 抽象插件扩展点；前端也按 features/pages/stores/services 做了基本划分。

但当前最大的问题不是“缺功能”，而是**架构边界正在被组合复杂度侵蚀**。几个模块已经开始变成事实上的“大泥球入口”：`agentdash-api::app_state`、`agentdash-application::session`、`workflow/value_objects.rs`、前端若干 store/page 文件。继续往上堆功能会让修改成本越来越高，尤其是 Session Runtime、VFS、Workflow、Plugin、Auth、Stream 这几条线会互相缠住。

我会把建议分成两类：**必须尽快处理的结构风险**，以及**面向架构演进的模块化路线**。

---

## 1. 仓库完整性与工程启动问题

从 zip 内容看，当前仓库可能不是一个完整可复现的开发工作区。

我看到这些不一致：

```text
package.json 里引用 scripts/dev-joint.js 等脚本
但 zip 内没有 scripts/ 目录

package.json 使用 workspace:* 依赖
但 zip 内没有 pnpm-workspace.yaml

README 引用 docs/assets/readme-runtime-map.svg
但 zip 内没有 docs/ 目录

package scripts 引用 tests/e2e/...
但 zip 内没有 tests/ 目录

没有 Cargo.lock
没有 pnpm-lock.yaml
```

这对架构演进很关键，因为如果仓库不能稳定复现，后续任何重构都容易变成“凭感觉改，靠人工记忆补上下文”。

建议优先补齐：

```text
Cargo.lock
pnpm-lock.yaml
pnpm-workspace.yaml
scripts/
docs/
tests/
CI workflow
```

尤其是 Rust 侧用了 git dependency，比如 `codex`、`vibe-kanban`、patch 的 `tokio-tungstenite/tungstenite`，没有 `Cargo.lock` 会让构建结果非常容易漂移。

---

## 2. 模块地图

按当前代码结构，可以这么理解各模块职责：

| 模块                                | 当前职责                                             | 架构评价                                         |
| --------------------------------- | ------------------------------------------------ | -------------------------------------------- |
| `agentdash-domain`                | 领域实体、值对象、Workflow/Lifecycle/Shared Library 等模型   | 概念正确，但部分文件过大，领域模型和校验逻辑堆在一起                   |
| `agentdash-agent-protocol`        | Backbone 事件协议、TS 类型生成                            | 比较健康，建议扩展到更多 API/DTO 类型                      |
| `agentdash-agent-types`           | Agent 相关基础类型                                     | 可以承接更多 agent-facing 配置，减少 agent 对 domain 的依赖 |
| `agentdash-spi`                   | 服务提供接口、运行时抽象                                     | 是插件/运行时边界的关键层，值得继续收敛                         |
| `agentdash-plugin-api`            | 插件扩展点                                            | 扩展点设计偏多，但部分没有真正接入 host                       |
| `agentdash-agent`                 | Cloud Agent Runtime / agent loop                 | 核心能力清楚，但 `agent_loop.rs` 过大                  |
| `agentdash-executor`              | Agent Connector / 执行适配器                          | 与 runtime、domain、spi 都有关，是 runtime 桥接层       |
| `agentdash-application`           | Session、VFS、Workflow、Task、Hooks、Capability 等应用服务 | 目前是最大复杂度中心，需要拆出稳定边界                          |
| `agentdash-infrastructure`        | Postgres、Repository、持久化、迁移                       | 有迁移，也有 repo 内 DDL 初始化，schema 来源重复            |
| `agentdash-api`                   | Axum REST/NDJSON/WS、AppState、路由、Auth             | 入口层过重，组合根和业务编排混在一起                           |
| `agentdash-relay`                 | Cloud/local relay protocol types                 | 相对独立                                         |
| `agentdash-local` / `local-tauri` | 本地 runtime、Tauri 桥接                              | 更偏运行环境适配                                     |
| `agentdash-mcp`                   | MCP 能力                                           | 依赖 domain/spi，方向合理                           |
| `packages/app-web`                | 主前端应用                                            | 功能最集中，store/page/feature 文件过大                |
| `packages/core/ui/views`          | 共享包                                              | 目前利用率偏低，app-web 承载了大多数复杂度                    |

---

## 3. 最关键的架构风险

### P0：`agentdash-api/src/app_state.rs` 已经变成系统级 God Object

`AppState::new_with_plugins` 现在承担了太多职责：

```text
收集插件
构建 Postgres repositories
调用 repository initialize()
构建 RepositorySet
初始化 shared library seeds
构建 backend registry
构建 VFS / mount registry / mutation dispatcher
构建 Agent connector / Relay connector / Plugin connectors
构建 hook provider
构建 session runtime builder
注入 lifecycle orchestrator callback
构建 runtime gateway
执行 boot reconcile
解析 auth mode/provider
构建 terminal cancel coordinator
构建 cron scheduler / routine executor / audit bus
启动 stall detector / scheduler / cleanup task
```

这已经不是普通 `AppState`，而是**应用宿主、DI 容器、插件宿主、后台任务管理器、schema 初始化器、运行时编排器**全部混在一起。

建议拆成几个明确的 wiring 模块：

```text
agentdash-api/src/bootstrap/
  repositories.rs
  plugins.rs
  auth.rs
  vfs.rs
  session_runtime.rs
  background_workers.rs
  routes.rs
```

或者更进一步，新建一个 host crate：

```text
crates/agentdash-host/
  src/
    repository_wiring.rs
    plugin_host.rs
    runtime_wiring.rs
    background_workers.rs
    service_registry.rs
```

然后 `agentdash-api` 只负责：

```text
读取 config
调用 host build
挂载 routes
启动 HTTP server
处理 graceful shutdown
```

目标不是“为了拆而拆”，而是让每条能力线可以独立演进。现在任何人要动 Session、VFS、Plugin、Auth、Routine，都很容易被迫阅读 `AppState` 全量上下文。

---

### P0：数据库 schema 来源重复：migrations 和 repository initialize 并存

仓库里有大量 SQL migrations，同时很多 Postgres repository 仍然在 `initialize()` 中执行：

```text
CREATE TABLE IF NOT EXISTS
ALTER TABLE ADD COLUMN IF NOT EXISTS
CREATE INDEX IF NOT EXISTS
```

这会造成一个长期隐患：**最终 schema 到底由 migrations 决定，还是由 repository 启动时修补决定？**

对一个 pre-research 项目来说，尤其 AGENTS.md 里明确说不需要做兼容性包袱，这里建议更激进：

1. 保留一个唯一 schema 来源：`migrations/`。
2. 删除 repository 中的运行时 DDL。
3. 启动时只运行 migrations。
4. repository 假设 schema 已存在。
5. 如果需要本地 embedded Postgres，也让 embedded Postgres 走同一套 migration 流程。

这样好处很直接：

```text
启动失败更早暴露 schema 缺失
migration diff 更可信
repository 只负责数据访问，不负责改库
测试环境和真实环境一致
```

另外，当前 migrations 链里已经能看到一些兼容性债务清理迁移。既然还在研究阶段，可以考虑做一次 migration squash，形成新的 canonical `0001_init.sql`，除非你们已经有必须保留的真实历史数据。

---

### P0：插件 API 暴露的能力多于实际 host 接入能力

`agentdash-plugin-api` 定义了不少 extension points，例如：

```text
vfs_providers
source_resolvers
agent_connectors
auth_provider
external_service_clients
mount_providers
routine_trigger_providers
extra_skill_dirs
library_asset_seeds
on_init
on_shutdown
```

但 `agentdash-api` 目前实际收集和接入的只是其中一部分。比如 `source_resolvers`、`external_service_clients`、`routine_trigger_providers`、`on_shutdown` 没有完整 wiring。

这类“不完全生效的扩展点”后续会很危险，因为它会让插件作者以为某个能力稳定可用，但 host 并没有真正管理生命周期。

建议二选一：

```text
方案 A：删掉暂未接入的 extension points
方案 B：补齐 PluginHost，让每个 extension point 都有明确注册、生命周期、shutdown 语义
```

我更建议当前阶段采用方案 A + 小步补齐。pre-research 阶段 API 不需要为了未来可能性提前暴露太多接口。

---

### P1：`RepositorySet` 太大，服务边界被它稀释

`agentdash-application/src/repository_set.rs` 是一个非常大的仓库集合。它作为 composition root 的便利对象可以理解，但如果它被广泛传入 leaf service，就会导致服务的真实依赖不清晰。

建议把它收敛成多个小 repository bundle：

```text
ProjectRepos
SessionRepos
WorkflowRepos
AssetRepos
RuntimeRepos
AuthRepos
VfsRepos
RoutineRepos
```

或者更进一步，让每个 application service 只接收自己需要的 trait port。

当前可以先做一个折中：

```rust
pub struct RepositorySet {
    pub project: ProjectRepos,
    pub session: SessionRepos,
    pub workflow: WorkflowRepos,
    pub asset: AssetRepos,
    pub runtime: RuntimeRepos,
    pub auth: AuthRepos,
}
```

然后逐步把服务构造函数里的 `RepositorySet` 替换成更小的 bundle。这个改动会显著改善测试可读性，也能减少模块误用 repository 的概率。

---

## 4. 各模块重构建议

## 4.1 `agentdash-application::session`

这是当前最核心、也最需要保护边界的模块。它现在包含了：

```text
session state
runtime registry
turn control
eventing
hooks
capability
construction
prompt pipeline
launch
assembler
control effects
runtime builder
tool services
continuation
```

模块很多，说明你们已经意识到 Session 是一个复杂聚合。但问题是它仍然集中在一个巨大 `session` 命名空间里，而且 `mod.rs` re-export 很多东西，容易形成“只要跟 session 有关都往里面塞”的趋势。

建议把 Session 拆成几个稳定子域：

```text
session-core
  SessionMeta
  SessionEvent
  TurnState
  RuntimeHandle
  Registry

session-launch
  SessionConstruction
  OwnerResolution
  CapabilityAssembly
  PromptPipeline
  Assembler

session-runtime
  RuntimeBuilder
  RuntimeControl
  TurnSupervisor
  ConnectorBridge

session-capability
  CapabilityState
  CapabilityProjection
  CapabilityReplay

session-hooks
  HookDelegate
  HookRuntime
  HookEvents

session-effects
  TerminalEffects
  RuntimeCommands
  ToolEffects
```

短期不一定要立刻拆 crate，可以先在 `agentdash-application/src/session/` 下按这些目录重排，并建立少数 facade：

```rust
pub trait SessionLauncher { ... }
pub trait SessionRuntimeControl { ... }
pub trait SessionEventWriter { ... }
pub trait SessionCapabilityProjector { ... }
```

长期可以考虑：

```text
crates/agentdash-session
crates/agentdash-session-runtime
```

尤其 `assembler.rs`、`hook_delegate.rs`、`runtime_builder`、`control` 这类文件，应该避免继续线性膨胀。

---

## 4.2 `agentdash-application::vfs`

VFS 是 AgentDash 里很重要的架构资产。它连接了：

```text
agent tools
UI surfaces
inline/canvas/skill/lifecycle providers
mount registry
materialization
mutation dispatcher
relay service
rewrite/apply_patch
```

概念上很棒，但现在 `vfs` 已经超过 1 万行，里面几类职责混在一起：

```text
路径/URI/挂载模型
provider 抽象
provider 实现
agent callable tools
mutation queue
materialization
surface resolution
relay
```

建议拆成：

```text
vfs-core
  path.rs
  types.rs
  mount.rs
  provider_trait.rs

vfs-surface
  surface.rs
  binding_resolver.rs
  capability resolution

vfs-providers
  provider_inline.rs
  provider_canvas.rs
  provider_lifecycle.rs
  provider_skill_asset.rs

vfs-mutation
  mutation_dispatcher.rs
  mutation_queue.rs
  apply_patch.rs
  rewrite.rs

vfs-materialization
  materialization.rs
  inline_persistence.rs

vfs-tools
  read.rs
  write.rs
  list.rs
  search.rs
  patch.rs
```

尤其 `vfs/tools/fs.rs` 不建议继续作为一个超大工具文件。Agent-callable FS tools 最好拆成多个命令 handler，并共享一个权限/路径解析服务。

另一个重要建议：把 VFS 的权限检查、可写性检查、mount capability 检查放在 VFS service 边界统一处理，不要散落在 route/provider/tool handler 里。VFS 是一个很适合做“能力安全边界”的模块。

---

## 4.3 `agentdash-domain::workflow`

`workflow/value_objects.rs` 目前承载了太多东西：

```text
WorkflowContract
CapabilityConfig
MountDirective
HookRule
ToolCapabilityPath
Lifecycle node/edge/port
ActivityDefinition
ActivityExecutorSpec
TransitionCondition
ArtifactBinding
ActivityAttempt state
LifecycleRunStatus
validation functions
```

这不是“文件太长”那么简单，而是不同语义层级混在一起：

```text
工作流契约
生命周期拓扑定义
活动定义
运行时状态
工具能力声明
校验规则
```

建议拆成：

```text
workflow/
  contract.rs
  lifecycle_def.rs
  activity_def.rs
  run_state.rs
  tool_capability.rs
  hook_rule.rs
  mount_directive.rs
  validation.rs
  mod.rs
```

`mod.rs` 可以继续 re-export，保证调用方改动小：

```rust
pub use contract::*;
pub use lifecycle_def::*;
pub use activity_def::*;
pub use run_state::*;
```

这个拆分会直接改善两个方向：

1. 后续 Workflow/Lifecycle 引擎演进时不必频繁碰同一个巨型文件。
2. TS 类型生成、API DTO、前端 normalizer 可以按领域切分。

---

## 4.4 `agentdash-api`

### 路由组合建议拆分

当前路由挂载是显式的，这点不错；但所有 route merge/nest 都集中起来后，`routes.rs` 会越来越难维护。

建议改成每个领域导出自己的 router：

```rust
pub fn project_routes(state: AppState) -> Router<AppState> { ... }
pub fn asset_routes(state: AppState) -> Router<AppState> { ... }
pub fn workflow_routes(state: AppState) -> Router<AppState> { ... }
pub fn session_routes(state: AppState) -> Router<AppState> { ... }
pub fn vfs_routes(state: AppState) -> Router<AppState> { ... }
pub fn runtime_routes(state: AppState) -> Router<AppState> { ... }
pub fn auth_routes(state: AppState) -> Router<AppState> { ... }
```

主路由只做：

```rust
Router::new()
    .nest("/api/projects", project_routes())
    .nest("/api/workflows", workflow_routes())
    .nest("/api/sessions", session_routes())
    .nest("/api/vfs", vfs_routes())
```

### bootstrap 模块不要依赖 routes helper

`agentdash-api/src/bootstrap/session_construction_bootstrap.rs` 的方向是对的：把 session construction 从 route handler 里抽出来。

但它现在还 import 了 route 模块里的 helper，例如 project agent / task execution 相关 helper。这是一个边界味道：bootstrap/use-case 层不应该依赖 HTTP route 模块。

建议把这些 helper 移到：

```text
agentdash-application/src/session/construction/
```

或者 API 内部独立的：

```text
agentdash-api/src/bootstrap/resolvers.rs
```

核心原则是：route handler 是最外层，不能被 bootstrap/application 反向依赖。

---

## 4.5 `agentdash-agent` / `agentdash-executor` / `agentdash-agent-protocol`

`agentdash-agent-protocol` 是当前比较健康的一块。它独立描述 Backbone 事件协议，并生成前端类型。这条路值得扩大。

建议：

1. 把更多后端 DTO 使用 `ts-rs` 或统一 schema 生成。
2. 不要只生成 Backbone protocol，也生成 Workflow Contract / Session DTO / VFS DTO。
3. 加一个 CI check，确保生成文件没有 drift。
4. root package 里增加明确脚本，例如：

```json
{
  "scripts": {
    "protocol:generate": "cargo run -p agentdash-agent-protocol --bin generate_ts",
    "protocol:check": "cargo run -p agentdash-agent-protocol --bin generate_ts -- --check"
  }
}
```

`agentdash-agent` 当前依赖 `agentdash-domain`，但它真正需要的可能只是 ThinkingLevel、Agent 配置、协议上下文这类 agent-facing 类型。建议逐步把这些类型移到 `agentdash-agent-types` 或 `agentdash-spi`，减少 agent runtime 对完整 domain 的依赖。

`agent_loop.rs` 也建议拆：

```text
agent_loop/
  mod.rs
  turn.rs
  tool_call.rs
  event_mapping.rs
  cancellation.rs
  prompt.rs
  output.rs
```

目标是让 agent loop 的“状态机逻辑”和“事件/协议转换”分开。

---

## 4.6 `agentdash-infrastructure`

基础设施层整体职责明确：Postgres runtime、repository 实现、migrations。

但有两个明显问题。

### 第一，infrastructure 依赖 application

比如某些 repository 实现会 import `agentdash_application::session::...` 或 shared library digest 相关类型。这说明 repository contract/type 有一部分定义在 application 层，导致 infrastructure 被迫依赖整个 application crate。

这不是绝对错误，但长期会导致循环压力和编译膨胀。

建议把持久化需要的窄接口/record 类型抽出来：

```text
agentdash-persistence-contract
```

或者放到 domain/spi 里：

```text
agentdash-domain::session_persistence
agentdash-spi::repositories
```

让关系变成：

```text
application -> persistence traits
infrastructure -> persistence traits
api/host -> wire application + infrastructure
```

而不是：

```text
infrastructure -> application
```

### 第二，embedded Postgres shutdown 有风险

`PostgresRuntime::Drop` 里使用 `tokio::spawn` 去 stop embedded postgres。这类异步资源释放放在 `Drop` 里容易不可靠：运行时关闭时可能 spawn 不了，或者任务还没执行完进程就退出。

建议改成显式 shutdown：

```rust
impl PostgresRuntime {
    pub async fn shutdown(self) -> Result<()> { ... }
}
```

然后在 API server graceful shutdown 时调用。

另外，stale process cleanup 里 non-Windows 分支似乎还在匹配 `.theseus.*postgres`，但当前路径已经是 `.agentdash/embedded-postgres/...`。这类历史命名残留应该尽早清掉。

---

## 4.7 Plugin / Routine

插件系统是 AgentDash 后续扩展能力的关键，但目前处于“接口看起来很大，host 实现偏少”的状态。

建议先明确插件系统的阶段目标：

```text
阶段 1：只支持 auth provider / vfs provider / agent connector / library seed
阶段 2：支持 routine trigger provider
阶段 3：支持 external service clients / source resolvers / shutdown lifecycle
```

当前如果某些扩展点没有 host wiring，就不要暴露在 public trait 里。

`RoutineExecutor::new` 里有参数名带 `_`，比如 `_vfs_service`、`_connector`、`_platform_config`，说明构造函数签名和实际实现已经漂移。这里建议尽快清理：

```text
不用的参数先删除
要用的参数就真正接入执行逻辑
```

Routine 执行状态也建议区分：

```text
dispatch_status: prompt/session 是否成功派发
run_status: agent session 是否实际完成
```

现在如果“派发成功就算 completed”，后续做自动化、定时任务、审计时会很难解释。

---

## 4.8 前端 `packages/app-web`

前端的主问题是：`app-web` 承载了几乎所有复杂度，而 `core/ui/views` 这些共享包相对很小。

几个明显的大文件：

```text
SettingsPage.tsx
ProjectSettingsPage.tsx
workspace-list.tsx
workflow/ui/activity-inspector.tsx
workflowStore.ts
storyStore.ts
workspace-layout.tsx
SkillCategoryPanel
McpPresetCategoryPanel
```

建议前端按“领域 feature + 纯状态 reducer + API contract”重构。

### Store 建议

现在 zustand store 同时做：

```text
API 调用
DTO normalizing
状态变更
选择器逻辑
副作用
错误处理
```

建议拆成：

```text
features/workflow/
  api.ts
  contract.ts
  normalize.ts
  reducer.ts
  store.ts
  selectors.ts
  components/
```

尤其 `workflowStore.ts`、`storyStore.ts` 这种文件，应该把纯状态转换逻辑抽出来做单元测试。

### Session stream 建议

`useSessionStream.ts` 和 `streamTransport.ts` 做了不少正确的事情：先 hydrate 历史事件，再接 NDJSON incremental stream，以 raw events 为事实源，批量 flush，terminal events 特殊处理。

但 hook 现在还是太大。建议拆成：

```text
session-event-reducer.ts
session-event-normalizer.ts
session-stream-client.ts
useSessionStream.ts
```

`useSessionStream` 只负责 React 生命周期和订阅，事件处理交给纯 reducer。这样测试也会轻很多。

### API contract 建议

前端 `services/workflow.ts` 里有大量手写 enum normalizer，例如 run status、hook trigger、target kind 等。这些值明显和 Rust domain 类型强相关。

既然后端已经用了 `ts-rs` 生成 Backbone protocol，建议扩展到：

```text
WorkflowContract
LifecycleDefinition
ActivityDefinition
Session DTO
VFS DTO
Shared Library DTO
```

这会减少前后端 enum drift，也能降低 normalizer 文件复杂度。

### Stream 抽象建议

前端至少有 Project Event Stream 和 Session Stream 两套 NDJSON over fetch 逻辑。建议提取一个通用客户端：

```ts
createNdjsonStreamClient<T>({
  url,
  cursor,
  parse,
  onEvent,
  onHeartbeat,
  onReconnect,
  signal,
})
```

然后 Project stream / Session stream 只提供 parse 和事件处理。后端也可以同步形成统一 stream cursor/backpressure 规范。

---

## 5. Auth / 安全边界建议

这块如果只是本地 pre-research 可以先不做重安全，但架构上要尽早避免错方向。

我看到 token 支持从 query param 读取：

```text
?token=...
```

这不建议保留。URL token 很容易进入浏览器历史、代理日志、错误日志。建议只保留：

```text
Authorization: Bearer ...
```

或者服务端管理的 cookie。

另外，前端同时使用 localStorage 和 cookie 存 token，cookie 由 JS 设置就无法是 HttpOnly。后续如果要走更安全的方案，建议统一成：

```text
server-set HttpOnly Secure SameSite cookie
```

如果现在还只是 local/personal 模式，也建议在代码里明确标记为 dev/personal auth strategy，不要让它悄悄变成默认生产模型。

---

## 6. 推荐的演进路线

我建议不要一上来大拆 crate。先做“不改变行为的边界整理”，然后再移动核心能力。

### Phase 1：先修工程可复现和组合根

目标：降低后续重构风险。

建议事项：

```text
补齐 pnpm-workspace.yaml / lockfiles / scripts / CI

把 AppState::new_with_plugins 拆成 bootstrap 子模块

把 routes.rs 拆成 domain routers

删除 repository initialize() 里的 DDL，统一走 migrations

清理 RoutineExecutor 未使用构造参数

清理 plugin-api 中未接入的 extension points

加 architecture tests：
  - bootstrap/application 不允许 import routes
  - application 不允许依赖 api
  - domain 不允许依赖 application/infrastructure/api
  - infrastructure 不直接依赖大 application 模块，或者只允许指定 persistence contract
```

这里最值得优先做的是 `AppState` 和 migrations，因为它们是后续所有模块演进的地基。

---

### Phase 2：拆核心领域大模块

目标：让 Session / VFS / Workflow 可持续演进。

建议事项：

```text
拆 workflow/value_objects.rs

拆 session 子域：
  core
  launch
  runtime
  capability
  hooks
  effects

拆 VFS：
  core
  providers
  tools
  mutation
  materialization
  surface

把 API bootstrap helper 从 routes 移走

把 agent_loop.rs 拆为状态机、事件映射、工具调用、取消控制
```

这一阶段尽量只做文件级/模块级拆分，不急着拆 crate。先让代码边界在目录结构上可见。

---

### Phase 3：契约生成和前后端同步

目标：减少前后端类型漂移。

建议事项：

```text
扩展 ts-rs 生成：
  Workflow DTO
  Session DTO
  VFS DTO
  Shared Library DTO

前端移除大量手写 enum normalizer

增加 generated type drift check

抽出通用 NDJSON stream client

把 useSessionStream 改成 hook + reducer + transport 三层
```

这一阶段可以显著减少前端维护压力。尤其 Workflow 和 Session 这两类对象后续肯定会持续变化，手写 normalizer 会越来越痛。

---

### Phase 4：再考虑 crate 级拆分

等模块边界稳定后，再做 crate 化：

```text
crates/
  agentdash-domain
  agentdash-contracts
  agentdash-session
  agentdash-vfs
  agentdash-workflow
  agentdash-runtime
  agentdash-application
  agentdash-infrastructure
  agentdash-host
  agentdash-api
```

其中：

```text
agentdash-contracts
```

可以承载跨后端/前端的 DTO、协议、TS 生成。

```text
agentdash-host
```

承载插件宿主、service wiring、background worker lifecycle。

```text
agentdash-application
```

只保留 use-case 编排，不再塞所有子系统细节。

---

## 7. 我会优先落地的 10 个改动

按收益/风险排序，我建议先做这些：

1. **补齐工作区文件和锁文件**：`pnpm-workspace.yaml`、`Cargo.lock`、`pnpm-lock.yaml`、缺失 scripts。
2. **拆 `AppState::new_with_plugins`**：先拆成 repositories/plugins/vfs/session/auth/background workers。
3. **统一 schema 来源**：repository 不再运行 DDL，只跑 migrations。
4. **拆 routes composition**：每个业务域一个 router builder。
5. **移动 bootstrap 中对 routes helper 的依赖**：避免 bootstrap/use-case 反向依赖 HTTP route。
6. **拆 `workflow/value_objects.rs`**：按 contract/lifecycle/activity/run_state/validation 拆。
7. **拆 `vfs/tools/fs.rs`**：read/write/list/search/patch 分离，统一 capability check。
8. **拆前端 `workflowStore.ts` / `useSessionStream.ts`**：API、normalizer、reducer、hook 分离。
9. **清理 plugin-api 未接入扩展点**：要么删，要么完整接入 PluginHost。
10. **扩展 TS 类型生成**：从 Backbone protocol 扩展到 Workflow/Session/VFS DTO。

---

## 8. 一个目标架构草图

比较理想的依赖关系可以变成这样：

```text
                    ┌────────────────────┐
                    │ agentdash-contracts│
                    │ protocol / DTO / TS│
                    └─────────┬──────────┘
                              │
┌────────────────────┐ ┌──────▼───────┐ ┌────────────────────┐
│ agentdash-domain   │ │ agentdash-spi │ │ agentdash-plugin-api│
│ pure domain model  │ │ ports/traits  │ │ extension contracts │
└─────────┬──────────┘ └──────┬───────┘ └─────────┬──────────┘
          │                   │                   │
          ▼                   ▼                   ▼
┌─────────────────────────────────────────────────────────────┐
│ session / workflow / vfs / executor / mcp / agent runtime   │
└─────────────────────────────┬───────────────────────────────┘
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ application use-cases                                       │
│ launch session / mutate vfs / run workflow / manage assets  │
└─────────────────────────────┬───────────────────────────────┘
                              ▼
┌────────────────────┐ ┌────────────────────┐
│ infrastructure     │ │ host               │
│ postgres/repos     │ │ wiring/plugins/bg  │
└─────────┬──────────┘ └─────────┬──────────┘
          │                      │
          ▼                      ▼
        ┌──────────────────────────┐
        │ api / local / tauri      │
        │ transport adapters       │
        └────────────┬─────────────┘
                     ▼
              packages/app-web
```

核心原则是：

```text
domain/protocol/spi 稳定
application 只做 use-case
host 只做 wiring/lifecycle
api 只做 transport
frontend 依赖 generated contracts
```

---

## 最后给一个浓缩结论

AgentDash 的方向和基本分层是成立的，但现在已经到了需要“收边界”的阶段。最需要优先处理的不是继续抽象新概念，而是把已经存在的概念固定下来：

```text
Session 是 runtime 聚合，不是所有业务逻辑的收纳箱
VFS 是能力安全边界，不只是文件工具集合
Workflow 是领域模型，不应该塞在一个 value_objects 巨型文件里
API 是 transport，不应该承担系统 wiring 和业务编排
Plugin API 暴露什么，就必须 host 真正支持什么
DB schema 只能有一个事实来源
前后端 DTO 应该生成，而不是靠手写 normalizer 对齐
```

如果只选三个优先级最高的重构点，我会选：

```text
1. 拆 AppState/bootstrap
2. migrations-only，移除 repository DDL 初始化
3. 拆 Session/VFS/Workflow 三个核心大模块的目录边界
```

这三件事做好之后，后面的前端契约生成、插件宿主、stream 抽象、crate 级拆分都会顺很多。
