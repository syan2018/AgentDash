# 测试仓储与测试基础设施收束

## Goal

把后端测试里的临时 Repository adapter 收束成项目级测试基础设施，让需要状态语义的测试复用同一套 canonical memory repository，并通过质量门阻止后续在业务测试里继续复制同类仓储实现。

第一轮执行目标是先处理已经影响测试可信度的 AgentRun / workflow repository adapter，再把 shared-library、workspace-module / canvas / extension installation 等重复热点纳入后续迁移清单。任务完成后，新测试应该自然从统一入口构造依赖，而不是在生产文件尾部拼临时仓储。

## Background

- 当前存在可复用但 crate-private 的 AgentRun workflow 测试仓储：[crates/agentdash-application-agentrun/src/test_support/workflow_repositories.rs](../../../crates/agentdash-application-agentrun/src/test_support/workflow_repositories.rs)。它只在 `agentdash-application-agentrun` 的 `#[cfg(test)] pub(crate)` 模块下可用，跨 crate 测试无法引用。
- API route 测试复制了同类 workflow memory repository：[crates/agentdash-api/src/routes/lifecycle_agents.rs](../../../crates/agentdash-api/src/routes/lifecycle_agents.rs)。这说明重复不是偶发，而是共享入口不可达导致的自然结果。
- `AgentFrameRepository::get_current` 已出现测试 adapter 语义漂移。生产 Postgres 查询按 `revision DESC, created_at DESC` 取 current frame；`agentdash-application-agentrun` 的 test support 按 `(revision, created_at)`；API 局部 fake 只按 `revision`。这会让测试证明和生产不同的行为。
- `LibraryAssetRepository`、`ProjectExtensionInstallationRepository`、`CanvasRepository`、`SkillAssetRepository` 也存在多处局部 in-memory/fake 实现，后续应按同一模式收束。
- 后端现有规范定义了生产 Repository 边界，但还没有定义测试 adapter 的归属和复用契约：[.trellis/spec/backend/repository-pattern.md](../../spec/backend/repository-pattern.md)。

## Requirements

### R1. 建立唯一测试 adapter 归属

新增或启用一个 workspace Rust crate 作为后端测试支撑入口，建议命名为 `agentdash-test-support`。该 crate 只作为其他 crate 的 dev-dependency 使用，承载跨 crate 共享的 memory repository、fixture builder 和必要的失败 adapter。

### R2. 第一轮收束 AgentRun / workflow repository

第一轮必须迁移这些已高频重复且存在语义漂移风险的 repository adapter：

- `MemoryLifecycleRunRepository`
- `MemoryLifecycleAgentRepository`
- `MemoryAgentFrameRepository`
- `MemoryRuntimeSessionExecutionAnchorRepository`
- `MemoryAgentRunDeliveryBindingRepository`
- `MemoryAgentRunMailboxRepository`
- `MemoryAgentRunCommandReceiptRepository`
- `MemoryProjectAgentRepository`
- `MemoryProjectBackendAccessRepository`

这些 adapter 的行为必须对齐生产 repository 的可观察语义，尤其是 `AgentFrameRepository::get_current` 的 current-frame 排序。

### R3. 迁移首批调用点

首批迁移以能消除语义漂移和继续复制的风险为准，至少覆盖：

- `agentdash-application-agentrun` 内当前直接使用 `crate::test_support` 的 AgentRun tests。
- `agentdash-api/src/routes/lifecycle_agents.rs` 中复制的 workflow memory repository。
- 同一 crate 内已经局部重写 `LifecycleRunRepository` / `AgentFrameRepository` / `RuntimeSessionExecutionAnchorRepository` / `AgentRunDeliveryBindingRepository` 的测试热点，优先处理与 runtime surface、delivery selection、fork、mailbox 相关的文件。

### R4. 引入测试基础设施质量门

仓库质量门新增一个快速静态检查，用于识别普通业务测试中新增的 stateful repository adapter。检查应纳入 `pr_quick` 和 `full_local`，并提供清晰错误信息指向 `agentdash-test-support`。

允许的例外要表达为明确归属，而不是散落在业务测试里：

- 生产 adapter，例如 `Postgres*Repository`。
- `agentdash-test-support` 中的 canonical memory adapter。
- 单一错误分支使用的极小 failure adapter，例如只返回固定错误的 adapter。

### R5. 更新项目规范

更新后端 Repository / Quality 规范，记录测试 adapter 的归属、同步责任和质量门。规范应说明为什么 stateful repository adapter 需要集中维护：它们承载生产 repository 的可观察语义，分散实现会让测试语义漂移。

### R6. 形成后续迁移清单

本任务不要求一次性清理全仓所有重复测试仓储，但必须在 `implement.md` 中列出后续批次和优先级，至少包括：

- shared-library / marketplace 的 `MemoryLibraryAssetRepository`。
- workspace-module / canvas 的 `MemoryCanvasRepository`。
- extension runtime / workspace module 的 `MemoryProjectExtensionInstallationRepository`。
- skill / vfs / lifecycle surface 的 `MemorySkillAssetRepository`。

## Acceptance Criteria

- [ ] Workspace 中存在 `agentdash-test-support` 或等价的测试支撑 crate，并只通过 dev-dependency 被业务 crate 使用。
- [ ] AgentRun / workflow memory repository 从统一入口导出，首批迁移点不再保留复制的同类 stateful repository。
- [ ] `AgentFrameRepository::get_current` 的 test-support 实现与生产 Postgres current-frame 排序一致，并有聚焦测试覆盖同 revision 下按 `created_at` 取最新的语义。
- [ ] `agentdash-api/src/routes/lifecycle_agents.rs` 不再定义局部 `MemoryLifecycleRunRepository`、`MemoryAgentFrameRepository`、`MemoryRuntimeSessionExecutionAnchorRepository`、`MemoryAgentRunDeliveryBindingRepository`。
- [ ] 新增静态 guard 并接入 `pr_quick` / `full_local`，能在普通业务测试里新增 stateful `*Repository` fake 时失败。
- [ ] 后端规范记录测试 repository adapter 的集中归属和同步责任。
- [ ] `cargo test -p agentdash-test-support`、被迁移 crate 的相关 Rust tests、`pnpm run quality:gates:check` 通过。

## Out Of Scope

- 不把所有前端 mock / Vitest helper 纳入本任务。
- 不为生产 Repository 增加兼容性 adapter。
- 不调整数据库 schema 或 migration。
- 不把所有旧测试一次性搬出 inline `mod tests`；只在迁移相关文件时顺手拆分过大的测试模块。
