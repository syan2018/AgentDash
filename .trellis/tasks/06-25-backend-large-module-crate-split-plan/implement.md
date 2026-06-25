# 进一步拆分后端大模块 Implement Plan

## 全局执行规则

- 每个阶段先完成文件搬运、Cargo 声明、旧入口删除，再修复编译错误。
- 编译修复只允许处理路径、imports、Cargo 依赖、类型归属、trait wiring、constructor wiring、repository/input set 字段和测试路径。
- 禁止为了通过编译修改默认值、过滤条件、执行顺序、权限判断、错误映射、数据库读写、事件语义、projection 语义、runtime surface 语义、hook resolution、workflow orchestration 或 Shared Library install/publish 行为。
- 禁止用 no-op、空实现、吞错、降级返回、跳过校验、跳过持久化或删除断言来消除编译错误。
- 禁止为了规避 `agentdash-application` 依赖而重复实现已有 interface、DTO、repository trait、port、service contract、错误类型、helper 或业务算法。
- 每个阶段验收前运行禁止引用扫描。
- 每个阶段验收前检查行为 diff，确认业务逻辑只发生文件归属和调用路径变化。
- 每个阶段验收前检查新增 public types / traits / DTO / error enums / helper functions，确认没有复制已有接口或业务 helper。
- 每个阶段验收前删除目标模块在旧 crate 中的目录和 `pub mod`。
- 禁止通过 `agentdash-application` 给 workflow/hooks/shared_library 做转发。
- 每个阶段独立提交。完成该阶段 check review gate 后立即提交，不把多个阶段压成一个提交。
- 每个阶段提交只包含该阶段拥有的文件范围和必要集成文件；不得混入后续阶段预改动。

## 最终派发流程

### Dispatch 1: Contract Owner

目标：

- 完成 workflow agent node materialization port。
- 完成 hook projection/effect port。
- 定义 `SharedLibraryRepositorySet` 字段清单和 construction owner。
- 写入 worker ownership 清单。

拥有文件：

- `crates/agentdash-application-ports/**`
- 本任务的 Trellis 计划文件。

禁止：

- 搬运 workflow/hooks/shared_library 文件。
- 修改 API route。
- 修改 root `Cargo.toml` 之外的模块依赖。

提交：

- 独立提交，格式：`refactor(ports): 明确大模块拆分共享端口契约`

### Dispatch 2: Module Workers In Parallel

并行 worker：

- Workflow worker：完整搬运 workflow 到 `agentdash-application-workflow`。
- Hooks worker：完整搬运 hooks 到 `agentdash-application-hooks`。
- Shared Library worker：完整搬运 shared_library 到 `agentdash-application-shared-library`。

拥有文件：

- Workflow worker：
  - `crates/agentdash-application-workflow/**`
  - `crates/agentdash-application/src/workflow/**`
  - `crates/agentdash-application-lifecycle/src/workflow/**`
- Hooks worker：
  - `crates/agentdash-application-hooks/**`
  - `crates/agentdash-application/src/hooks/**`
- Shared Library worker：
  - `crates/agentdash-application-shared-library/**`
  - `crates/agentdash-application/src/shared_library/**`

禁止：

- 修改其他 worker 的目标 crate。
- 修改同一个 API route。
- 复制其他模块接口来解决编译错误。
- 修改业务行为来解决编译错误。

提交：

- 每个 worker 各自独立提交。
- Workflow 提交格式：`refactor(workflow): 拆分 Workflow application crate`
- Hooks 提交格式：`refactor(hooks): 拆分 Hooks application crate`
- Shared Library 提交格式：`refactor(shared-library): 拆分 Shared Library application crate`

### Dispatch 3: Integration Owner

目标：

- 统一处理 root `Cargo.toml`、workspace dependencies、API imports/bootstrap/routes。
- 删除 `agentdash-application` 和 `agentdash-application-lifecycle` 中目标模块旧入口。
- 执行禁止引用扫描。
- 执行 workspace check。

拥有文件：

- root `Cargo.toml`
- `Cargo.lock`
- `crates/agentdash-application/src/lib.rs`
- `crates/agentdash-application-lifecycle/src/lib.rs`
- `crates/agentdash-api/src/**`
- 必要的 composition adapter 文件。

禁止：

- 在 integration 阶段新增目标模块业务实现。
- 复制接口或 helper。
- 改业务语义。

提交：

- 独立提交，格式：`refactor(application): 收束大模块拆分集成入口`

### Dispatch 4: Check Owner

目标：

- 逐项执行 Phase 4 禁止引用扫描。
- 审查新增 public types / traits / DTO / error enums / helper functions。
- 审查行为 diff，确认没有编译性业务修改。
- 执行最终验证命令。

拥有文件：

- 只允许修复 check 发现的路径/import/test 路径问题。
- 不拥有业务逻辑文件的语义修改。

提交：

- 若 check 只确认通过，不提交。
- 若 check 修复机械问题，独立提交，格式：`fix(application): 修正大模块拆分集成引用`

## Phase 0: Baseline

执行：

- `cargo tree -p agentdash-application --depth 1 --edges normal`
- `cargo tree -p agentdash-application-lifecycle --depth 1 --edges normal`
- `cargo tree -p agentdash-application-agentrun --depth 1 --edges normal`
- `cargo tree -p agentdash-api --depth 1 --edges normal`
- `cargo tree -p agentdash-mcp --depth 1 --edges normal`
- `cargo tree -p agentdash-infrastructure --depth 1 --edges normal`
- `rg "agentdash_application::(workflow|hooks|shared_library)" crates/agentdash-api/src crates/agentdash-mcp/src crates/agentdash-local/src`

验收：

- 当前依赖图已记录。
- 当前旧引用已记录。

## Phase 0.5: 并行契约冻结

目标状态：

- 一个 owner 完成共享 contract 设计。
- 每个 implementation worker 有独占写入范围。
- 共享集成面由 integration owner 统一修改。

执行：

- 定义 workflow agent node materialization port 的名称、输入、输出、错误归属。
- 定义 hook projection/effect port 的名称、输入、输出、错误归属。
- 定义 `SharedLibraryRepositorySet` 字段清单和 construction owner。
- 写明 worker ownership：
  - Workflow worker：`crates/agentdash-application-workflow/**`、workflow 原目录搬运、workflow tests。
  - Hooks worker：`crates/agentdash-application-hooks/**`、hooks 原目录搬运、hooks tests。
  - Shared Library worker：`crates/agentdash-application-shared-library/**`、shared_library 原目录搬运、shared-library tests。
  - Integration owner：root `Cargo.toml`、workspace deps、`agentdash-application/src/lib.rs`、`agentdash-application-lifecycle/src/lib.rs`、API imports/bootstrap/routes、禁止引用扫描。

禁止：

- 多个 worker 同时修改 `agentdash-application-ports`。
- 多个 worker 同时修改 root `Cargo.toml`。
- 多个 worker 同时修改同一个 API route。
- worker 为了修本 crate 编译错误去复制其他模块接口。

验收：

- contract 文件已落到 `application-ports` 或对应目标 crate。
- 每个 worker 的写入范围互不重叠。
- 集成 owner 接管所有共享文件。
- 完成独立提交。

## Phase 1: Workflow 完整搬运

目标状态：

- 新 crate：`crates/agentdash-application-workflow`
- 旧目录不存在：
  - `crates/agentdash-application/src/workflow`
  - `crates/agentdash-application-lifecycle/src/workflow`
- 旧入口不存在：
  - `agentdash-application/src/lib.rs` 的 `pub mod workflow`
  - `agentdash-application-lifecycle/src/lib.rs` 的 `pub mod workflow`

先搬运：

- 新建 `crates/agentdash-application-workflow`。
- 移入 `crates/agentdash-application/src/workflow/**`。
- 移入 `crates/agentdash-application-lifecycle/src/workflow/**`。
- 移入 builtin workflow JSON assets。
- 在 `agentdash-application-ports` 新增 workflow agent node materialization port。
- 在 workflow crate 定义 `WorkflowApplicationError`。
- 在 workflow crate 定义 workflow repository/input set。
- 删除旧 workflow 目录和旧 `pub mod workflow`。

再修复：

- lifecycle orchestrator / dispatch service 改为直接调用 `agentdash-application-workflow`。
- `AgentNodeLauncher` 改为调用 materialization port。
- API workflow route 改为直接导入 `agentdash-application-workflow` 与 `agentdash-application-lifecycle`。
- `shared_library` 当前对 workflow builtin/template 的引用改为直接导入 `agentdash-application-workflow`。
- 所有 `crate::workflow`、`agentdash_application::workflow` 引用改到目标 crate。

禁止引用扫描：

- `rg "agentdash_application::workflow|crate::workflow" crates/agentdash-api/src crates/agentdash-mcp/src crates/agentdash-local/src crates/agentdash-application/src`
- `rg "agentdash_application_lifecycle::workflow|crate::workflow" crates/agentdash-application-lifecycle/src`
- `rg "agentdash_application_(lifecycle|agentrun)|agentdash_application::" crates/agentdash-application-workflow/src`

验证：

- `cargo check -p agentdash-application-ports`
- `cargo check -p agentdash-application-workflow`
- `cargo check -p agentdash-application-lifecycle`
- `cargo check -p agentdash-application`
- `cargo check -p agentdash-api`
- `cargo test -p agentdash-application-workflow`
- `cargo test -p agentdash-application-lifecycle`

验收：

- Workflow 文件全部位于 `agentdash-application-workflow`。
- lifecycle 没有 `src/workflow` 模块。
- application 没有 `src/workflow` 模块。
- workflow crate 不依赖 application/lifecycle/agentrun concrete crates。
- Workflow 行为 diff 只包含文件归属、crate path、port wiring 和 constructor wiring。
- Check review gate：新增 workflow public traits/DTO/errors/helpers 逐项确认，不得复制 application/lifecycle/agentrun 中已有接口；需要共享的接口必须位于 workflow crate 或 `application-ports`。
- 完成独立提交。

## Phase 2: Hooks 完整搬运

目标状态：

- New crate：`crates/agentdash-application-hooks`
- 旧目录不存在：`crates/agentdash-application/src/hooks`
- 旧入口不存在：`agentdash-application/src/lib.rs` 的 `pub mod hooks`

先搬运：

- 新建 `crates/agentdash-application-hooks`。
- 移入 hook policy 文件：
  - `rules.rs`
  - `rules/**`
  - `presets.rs`
  - `script_engine.rs`
  - `provider.rs`
  - `helpers.rs`
  - `snapshot_helpers.rs`
  - `active_workflow_contribution.rs`
  - hook preset `.rhai` assets
- 将 `active_workflow_snapshot.rs` 和 `owner_resolver.rs` 中的 repository-heavy 查询逻辑移到 lifecycle/application adapter 侧。
- 在 `agentdash-application-ports` 新增 hook projection/effect port。
- 删除旧 hooks 目录和旧 `pub mod hooks`。

再修复：

- hooks provider deps 改为 projection/effect port + `HookScriptEvaluator`。
- API bootstrap 直接构造 `agentdash-application-hooks` provider。
- API workflow route 的 hook preset/script endpoint 直接导入 hooks crate。
- runtime-session 和 agentrun 保持只依赖 `ExecutionHookProvider` trait object。

禁止引用扫描：

- `rg "agentdash_application::hooks|crate::hooks" crates/agentdash-api/src crates/agentdash-mcp/src crates/agentdash-local/src crates/agentdash-application/src`
- `rg "agentdash_application(_lifecycle|_runtime_session|_agentrun)|agentdash_infrastructure|agentdash_application::" crates/agentdash-application-hooks/src`

验证：

- `cargo check -p agentdash-application-ports`
- `cargo check -p agentdash-application-hooks`
- `cargo test -p agentdash-application-hooks`
- `cargo test -p agentdash-application-runtime-session hook`
- `cargo test -p agentdash-application-agentrun hook_runtime`
- `cargo check -p agentdash-api`
- `cargo check -p agentdash-application`

验收：

- Hooks 文件全部位于 hooks crate 或明确的 lifecycle/application adapter 位置。
- application 没有 `src/hooks` 模块。
- hooks crate 不依赖 application/lifecycle/runtime-session/agentrun/infrastructure。
- API 不再通过 `agentdash_application::hooks` 引用 hooks。
- Hooks 行为 diff 只包含文件归属、crate path、projection/effect port wiring 和 constructor wiring。
- Check review gate：hook projection/effect port 不得复制 lifecycle projection 的业务计算；hooks crate 不得复制 runtime-session/agentrun hook runtime contracts。
- 完成独立提交。

## Phase 3: Shared Library 完整搬运

目标状态：

- New crate：`crates/agentdash-application-shared-library`
- 旧目录不存在：`crates/agentdash-application/src/shared_library`
- 旧入口不存在：`agentdash-application/src/lib.rs` 的 `pub mod shared_library`

先搬运：

- 新建 `crates/agentdash-application-shared-library`。
- 移入：
  - `service.rs`
  - `external_marketplace.rs`
  - `seed.rs`
  - `install.rs`
  - `publish.rs`
- 在 shared-library crate 定义 `SharedLibraryRepositorySet`。
- 将 workflow builtin seed 输入改为来自 `agentdash-application-workflow` 的 DTO/provider。
- 删除旧 shared_library 目录和旧 `pub mod shared_library`。

再修复：

- broad `RepositorySet` 到 `SharedLibraryRepositorySet` 的 construction code 放在 application/API composition 侧。
- API shared_library route 直接导入 shared-library crate。
- API marketplace route 直接导入 shared-library crate。
- API bootstrap/integrations 直接导入 shared-library seed/service types。
- VFS 常量/helper 直接来自 `agentdash-application-vfs`。
- extension package archive/storage/install 继续留在 extension package 边界。

禁止引用扫描：

- `rg "agentdash_application::shared_library|crate::shared_library" crates/agentdash-api/src crates/agentdash-mcp/src crates/agentdash-local/src crates/agentdash-application/src`
- `rg "agentdash_application::|crate::repository_set::RepositorySet" crates/agentdash-application-shared-library/src`
- `rg "agentdash_application::workflow|crate::workflow" crates/agentdash-application-shared-library/src`

验证：

- `cargo check -p agentdash-application-shared-library`
- `cargo test -p agentdash-application-shared-library`
- `cargo check -p agentdash-api`
- `cargo check -p agentdash-application`
- `cargo check -p agentdash-infrastructure`
- `pnpm run contracts:check`
- `pnpm run migration:guard`，仅在 schema/migration 文件发生变更时运行。

验收：

- Shared Library 文件全部位于 shared-library crate。
- application 没有 `src/shared_library` 模块。
- shared-library crate 不依赖 application。
- API 不再通过 `agentdash_application::shared_library` 引用 shared_library。
- Shared Library 行为 diff 只包含文件归属、crate path、repository set wiring、seed provider wiring 和 constructor wiring。
- Check review gate：`SharedLibraryRepositorySet` 不得复制 repository traits；workflow seed DTO/provider 不得复制 workflow template 业务结构；shared-library crate 不得复制 extension package 或 skill_asset install/publish 逻辑。
- 完成独立提交。

## Phase 4: 旧目标引用清零

执行：

- `rg "agentdash_application::(workflow|hooks|shared_library)" crates`
- `rg "pub mod (workflow|hooks|shared_library)" crates/agentdash-application/src crates/agentdash-application-lifecycle/src`
- `rg "agentdash_application::" crates/agentdash-application-workflow/src crates/agentdash-application-hooks/src crates/agentdash-application-shared-library/src`
- `cargo check --workspace`
- `pnpm run contracts:check`

验收：

- 目标模块没有任何 `agentdash-application` 旧转发入口。
- 新 crate 没有反向依赖旧 application 转发层。
- infrastructure 没有 application 依赖。
- 没有为了通过编译而引入的业务行为修改。
- 没有为了规避 application 依赖而引入的重复接口、重复 DTO、重复 repository trait、重复 port、重复错误枚举或重复业务 helper。
- 完成独立提交。

## 风险文件

- `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs`
- `crates/agentdash-application-lifecycle/src/workflow/orchestration/agent_node_launcher.rs`
- `crates/agentdash-application-lifecycle/src/workflow/orchestration/executor_launcher.rs`
- `crates/agentdash-application/src/hooks/provider.rs`
- `crates/agentdash-application/src/hooks/active_workflow_snapshot.rs`
- `crates/agentdash-application/src/hooks/owner_resolver.rs`
- `crates/agentdash-application/src/shared_library/install.rs`
- `crates/agentdash-application/src/shared_library/publish.rs`
- `crates/agentdash-application/src/shared_library/seed.rs`
- `crates/agentdash-application/src/repository_set.rs`
- `crates/agentdash-api/src/routes/workflows.rs`
- `crates/agentdash-api/src/bootstrap/session.rs`
- `crates/agentdash-api/src/bootstrap/repositories.rs`
- `crates/agentdash-api/src/routes/shared_library.rs`
- `crates/agentdash-api/src/routes/marketplace.rs`
