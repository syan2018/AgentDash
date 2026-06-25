# 进一步拆分后端大模块 Design

## 执行原则

本任务的拆分计划采用目标状态优先的重构方式：

1. 每个阶段先完成文件归位、Cargo 成员声明、旧模块入口删除。
2. 文件到达目标位置后，统一修复编译错误、imports、类型归属和测试。
3. 每个阶段都禁止用旧 `agentdash-application::<target>` 转发目标模块。
4. 每个阶段都必须在验收时证明旧引用关系没有重新出现。
5. 编译修复只允许修复模块边界造成的机械问题，禁止改业务行为来换取编译通过。

## 行为不变约束

每个阶段的修复动作只能属于以下类别：

- import path 修复。
- crate dependency / workspace member 修复。
- module visibility 修复。
- 类型归属修复。
- trait object wiring / constructor wiring 修复。
- repository/input set 的字段搬运和调用点改名。
- 测试模块路径与 fixture 路径修复。

每个阶段都禁止以下行为：

- 修改默认值。
- 修改过滤条件。
- 修改执行顺序。
- 修改权限判断。
- 修改错误类型映射语义。
- 修改数据库读写语义。
- 修改事件、projection、runtime surface、hook resolution、workflow orchestration、Shared Library install/publish 的业务语义。
- 用 no-op、空实现、吞错、降级返回、跳过校验、跳过持久化来消除编译错误。
- 删除测试断言来消除失败。
- 为了摆脱 `agentdash-application` 依赖，在目标 crate 内复制已有 interface、DTO、repository trait、port、service contract、错误类型、helper 或业务算法。

任何真实业务行为变更必须拆成单独任务，不能混入 crate 搬运阶段。

## 接口复用约束

当新 crate 需要访问原本位于 `agentdash-application` 的接口时，只允许以下处理：

- 将原接口完整迁入目标 crate。
- 将跨 crate 共享接口上移到 `agentdash-application-ports`。
- 将领域 repository trait / value object 保持或迁入 `agentdash-domain`。
- 将 provider/runtime service contract 保持或迁入 `agentdash-spi`。
- 将调用点改为依赖已有接口的新归属路径。

禁止以下处理：

- 在新 crate 里创建同名或近似同名 trait 来包一层原 trait。
- 复制 DTO 结构体，只为了避免引入正确依赖。
- 复制 repository trait 或 service contract，只为了让 Cargo 图暂时断开。
- 复制错误枚举并做一组平行转换来掩盖归属没拆清。
- 复制 helper 算法或 business mapper，只为了减少 import 修复。
- 以 adapter 名义重写已有业务逻辑。

每个阶段的 check review 必须对新增 public types、traits、DTO、error enums、helper functions 做复用审查，确认它们不是旧接口的重复实现。

涉及 `skill_asset` 的地方只作为 Shared Library 已有 Project asset / repository 相邻边界处理。

## 目标 Crates

- `agentdash-application-workflow`
- `agentdash-application-hooks`
- `agentdash-application-shared-library`

目标模块从 `agentdash-application` 或 `agentdash-application-lifecycle` 移出后，旧模块目录和旧 `pub mod` 入口必须删除。`agentdash-application` 不再为这三个目标模块提供转发 API。

## 禁止依赖关系

以下引用关系在对应阶段完成后不得存在：

- `agentdash-application-workflow -> agentdash-application`
- `agentdash-application-workflow -> agentdash-application-lifecycle`
- `agentdash-application-workflow -> agentdash-application-agentrun`
- `agentdash-application-hooks -> agentdash-application`
- `agentdash-application-hooks -> agentdash-application-lifecycle`
- `agentdash-application-hooks -> agentdash-application-runtime-session`
- `agentdash-application-hooks -> agentdash-application-agentrun`
- `agentdash-application-hooks -> agentdash-infrastructure`
- `agentdash-application-shared-library -> agentdash-application`
- `agentdash-infrastructure -> any agentdash-application-* crate`
- API/MCP/local 通过 `agentdash_application::workflow`
- API/MCP/local 通过 `agentdash_application::hooks`
- API/MCP/local 通过 `agentdash_application::shared_library`

允许方向：

```text
agentdash-api / agentdash-mcp / agentdash-local
  -> agentdash-application-workflow
  -> agentdash-application-hooks
  -> agentdash-application-shared-library

agentdash-application-lifecycle
  -> agentdash-application-workflow
  -> agentdash-application-ports
  -> agentdash-domain / agentdash-spi

agentdash-application-hooks
  -> agentdash-application-ports
  -> agentdash-domain / agentdash-spi

agentdash-application-shared-library
  -> agentdash-application-workflow
  -> agentdash-application-vfs
  -> agentdash-domain / agentdash-spi
```

## 模块引用矩阵

### Workflow

允许直接引用：

- `agentdash-domain`
- `agentdash-spi`
- `agentdash-agent-protocol`
- `agentdash-agent-types`
- `agentdash-application-ports`
- 技术依赖：`serde`、`serde_json`、`chrono`、`uuid`、`thiserror`、`async-trait`、`tokio` 测试依赖等。

必须通过 ports 引用：

- AgentRun frame materialization。
- runtime session creation。
- workflow agent node materialization。
- AgentRun runtime/resource surface query。
- lifecycle command/materialization effect。

禁止直接引用：

- `agentdash-application`
- `agentdash-application-lifecycle`
- `agentdash-application-agentrun`
- `agentdash-application-runtime-session`
- `agentdash-api`
- `agentdash-infrastructure`

已识别的高风险链路：

- `AgentNodeLauncher -> LifecycleDispatchService`：必须改成 workflow agent node materialization port。
- lifecycle `dispatch_service` 对 workflow reducer/activation 的调用：必须反向改成 lifecycle 依赖 workflow crate。
- `WorkflowApplicationError`：归属必须进入 workflow crate，不能在 lifecycle 和 workflow 平行定义两个错误枚举。
- workflow repository/input set：只能收敛实际需要的 repository traits，不能复制 domain repository traits。

### Hooks

允许直接引用：

- `agentdash-domain`
- `agentdash-spi`
- `agentdash-application-ports`
- 技术依赖：`serde`、`serde_json`、`chrono`、`uuid`、`thiserror`、`async-trait`、`tokio` 测试依赖等。

必须通过 ports 引用：

- active workflow projection facts。
- subject run context。
- fulfilled output-port map。
- execution log append effect。
- lifecycle/workflow hook target resolution。

禁止直接引用：

- `agentdash-application`
- `agentdash-application-lifecycle`
- `agentdash-application-runtime-session`
- `agentdash-application-agentrun`
- `agentdash-infrastructure`
- `agentdash-api`

已识别的高风险链路：

- `active_workflow_snapshot.rs` 中 repository-heavy projection 查询：必须成为 lifecycle/application adapter 对 hook projection port 的实现。
- `owner_resolver.rs` 中 subject owner/run context 查询：必须进入 projection port 实现侧。
- Rhai evaluator：只能通过 `agentdash-spi::HookScriptEvaluator` 注入，不能让 hooks crate 引用 infrastructure。
- hook runtime cache / runtime-session hook delegate：不属于 hooks crate，hooks crate 只提供 policy provider。

### Shared Library

允许直接引用：

- `agentdash-domain`
- `agentdash-spi`
- `agentdash-application-workflow`
- `agentdash-application-vfs`
- 技术依赖：`serde`、`serde_json`、`chrono`、`uuid`、`thiserror`、`async-trait`、`tokio` 测试依赖等。

必须通过本 crate 的 narrow input set 或外部 composition 引用：

- Shared Library install/publish/source-status 所需 repository traits。
- workflow builtin seed provider。
- integration embedded seed collection。
- extension package artifact repository records。

禁止直接引用：

- `agentdash-application`
- `agentdash-api`
- `agentdash-infrastructure`
- `skill_asset` service internals。

已识别的高风险链路：

- `install.rs` / `publish.rs` 对 broad `RepositorySet` 的依赖：必须替换成 `SharedLibraryRepositorySet`，且 `SharedLibraryRepositorySet` 只持有已有 repository trait 对象。
- `seed.rs` 对 `application::workflow` 的引用：必须改为 `agentdash-application-workflow` 提供的 seed DTO/provider。
- extension package archive validation/storage/install：不能搬入 shared-library crate。
- `skill_asset` install/publish：只复用 domain repository 和现有 Project asset 语义，不能复制或重写 `skill_asset` 业务逻辑。

## 前置未拆净风险

以下现象说明前置拆分没有完成，必须阻断阶段验收：

- 新 crate 为了不依赖 `agentdash-application` 增加了与旧模块同义的 trait/DTO/error/helper。
- 新 crate 出现对旧 application path 的引用后，用 wrapper/adaptor 包住旧路径继续调用。
- API/MCP/local 同时引用新 crate 和 `agentdash_application::<target>`。
- 旧 crate 仍保留目标模块 `pub mod`，即使目录里只剩 re-export。
- 新 crate 的 tests 通过复制 fixture/mapper/helper 绕开原有测试路径。
- 为了让 Cargo 图断开，业务算法被切成两份并分别维护。

## 并行边界

这些拆分可以并行推进，但不能无序并行。并行单位必须避开共享写面：

- Workflow、Hooks、Shared Library 的文件搬运可以由不同 worker 并行处理。
- `agentdash-application-ports` 的新增 port contract 必须先由一个 owner 完成并冻结名称、输入、输出和错误归属。
- root `Cargo.toml`、workspace dependencies、`agentdash-application/src/lib.rs`、`agentdash-api` bootstrap/routes 是共享集成面，必须由集成 owner 统一修改。
- API import 迁移必须按模块成片完成，不能让同一个 route 同时引用旧 application path 和新 crate path。
- 每个 worker 只拥有自己的目标 crate 和明确 adapter 文件；不得跨模块修别人的编译错误。

推荐并行模型：

1. Contract owner 先完成 `application-ports` 中 workflow materialization port 与 hook projection/effect port 的最终形状。
2. Workflow worker、Hooks worker、Shared Library worker 并行完成各自完整搬运和本 crate 内部修复。
3. Integration owner 统一处理 Cargo workspace、API imports、application 旧入口删除、禁止引用扫描和 workspace check。

不能并行拆散的部分：

- `agentdash-application-ports` contract 不能多人同时改。
- `agentdash-api/src/routes/workflows.rs` 不能由 workflow 和 hooks worker 同时改。
- `agentdash-application/src/lib.rs` 和 root `Cargo.toml` 不能由多个 worker 各自追加。
- `RepositorySet` 到 narrow input set 的 adapter 不能由 shared_library worker 和 integration owner 各写一份。

## Workflow 目标状态

新建 `agentdash-application-workflow`，完整接管 workflow 定义、catalog、builtin templates、graph/script compiler、preflight、orchestration reducer、orchestration executor launcher、AgentCall/Function/HumanGate ready-node execution。

必须迁入：

- `crates/agentdash-application/src/workflow/**`
- `crates/agentdash-application-lifecycle/src/workflow/**`

必须删除：

- `crates/agentdash-application/src/workflow/**`
- `crates/agentdash-application-lifecycle/src/workflow/**`
- `agentdash-application/src/lib.rs` 中的 `pub mod workflow`
- `agentdash-application-lifecycle/src/lib.rs` 中的 `pub mod workflow`

必须新增或改造：

- `agentdash-application-ports` 中的 workflow agent node materialization port。
- workflow crate 自有 `WorkflowApplicationError`。
- workflow crate 自有 repository/input set，不能使用 `agentdash_application::repository_set::RepositorySet`。
- lifecycle crate 提供 workflow materialization port 实现，供 workflow crate 通过 trait object 调用。

Lifecycle 保留职责：

- lifecycle run command service。
- lifecycle orchestrator 作为 session terminal / lifecycle command bridge。
- lifecycle read model、surface projector、VFS provider。
- lifecycle 对 workflow reducer/launcher 的调用。

Workflow 不依赖 AgentRun concrete crate。AgentRun 相关效果通过 application ports 表达。

## Hooks 目标状态

新建 `agentdash-application-hooks`，完整接管 hook policy、preset registry、hook script service、`ExecutionHookProvider` implementation。

必须迁入 `agentdash-application-hooks`：

- `crates/agentdash-application/src/hooks/rules.rs`
- `crates/agentdash-application/src/hooks/rules/**`
- `crates/agentdash-application/src/hooks/presets.rs`
- `crates/agentdash-application/src/hooks/script_engine.rs`
- `crates/agentdash-application/src/hooks/provider.rs`
- `crates/agentdash-application/src/hooks/helpers.rs`
- `crates/agentdash-application/src/hooks/snapshot_helpers.rs`
- `crates/agentdash-application/src/hooks/active_workflow_contribution.rs`
- hook preset `.rhai` assets

必须迁出到 lifecycle/application adapter 侧：

- `active_workflow_snapshot.rs` 中的 repository-heavy active workflow 查询逻辑。
- `owner_resolver.rs` 中的 subject owner / run context 查询逻辑。
- execution log append 的具体 repository effect。

必须删除：

- `crates/agentdash-application/src/hooks/**`
- `agentdash-application/src/lib.rs` 中的 `pub mod hooks`
- `agentdash-application` 对 hooks public surface 的转发。

必须新增或改造：

- `agentdash-application-ports` 中的 hook projection/effect port。
- lifecycle/application adapter 对该 port 的实现。
- hooks crate 自有错误类型与 API-facing admin/script service。

Runtime-session 和 AgentRun 继续只接收 `agentdash_spi::hooks::ExecutionHookProvider` trait object，不依赖 hooks crate。

## Shared Library 目标状态

新建 `agentdash-application-shared-library`，完整接管 Shared Library application use cases。

必须迁入：

- `crates/agentdash-application/src/shared_library/service.rs`
- `crates/agentdash-application/src/shared_library/external_marketplace.rs`
- `crates/agentdash-application/src/shared_library/seed.rs`
- `crates/agentdash-application/src/shared_library/install.rs`
- `crates/agentdash-application/src/shared_library/publish.rs`

必须删除：

- `crates/agentdash-application/src/shared_library/**`
- `agentdash-application/src/lib.rs` 中的 `pub mod shared_library`
- `agentdash-application` 对 shared_library public surface 的转发。

必须新增或改造：

- `SharedLibraryRepositorySet`，只包含 install/publish/source-status 实际使用的 repository traits。
- broad `RepositorySet` 到 `SharedLibraryRepositorySet` 的 construction code；该 code 不能位于 shared-library crate 内。
- workflow builtin seed DTO/provider，由 `agentdash-application-workflow` 提供。
- API bootstrap 负责组合 shared-library builtin seeds、workflow builtin seeds、integration embedded seeds。

不迁入：

- API routes / DTO mapping。
- concrete PostgreSQL repositories。
- extension package archive validation/storage/install use cases。
- workflow builtin template ownership。
- `skill_asset` service internals。

## Research

- `research/workflow-crate-boundary.md`
- `research/hooks-crate-split-boundary.md`
- `research/shared-library-crate-boundary-review.md`
- `research/backend-crate-split-dependency-topology.md`
