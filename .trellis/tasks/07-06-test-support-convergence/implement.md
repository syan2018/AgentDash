# 测试仓储与测试基础设施收束实施计划

## Phase 0. 准备

- [ ] 确认工作区现有未提交改动，避免触碰并行会话文件。
- [ ] 阅读相关规范：
  - `.trellis/spec/backend/repository-pattern.md`
  - `.trellis/spec/backend/quality-guidelines.md`
  - `.trellis/spec/backend/directory-structure.md`
  - `.trellis/spec/guides/code-reuse-thinking-guide.md`
- [ ] 搜索当前重复实现，记录迁移前基线：
  - `rg -n "struct (Memory|InMemory|Fake|Mock|Test).*Repository" crates`
  - `rg -n "impl .*Repository for (Memory|InMemory|Fake|Mock|Test).*" crates`

## Phase 1. 新建 test-support crate

- [ ] 在 workspace members 添加 `crates/agentdash-test-support`。
- [ ] 创建 `Cargo.toml`，只引入测试 adapter 所需依赖。
- [ ] 创建 `src/lib.rs` 和 `src/workflow.rs`。
- [ ] 从 `agentdash-application-agentrun/src/test_support/workflow_repositories.rs` 迁移 workflow memory adapters。
- [ ] 将可见性改为 `pub`，并按 `agentdash_test_support::workflow::*` 导出。

## Phase 2. 固化 workflow adapter 语义

- [ ] 添加 `cargo test -p agentdash-test-support` 可运行的自测。
- [ ] 覆盖 `MemoryAgentFrameRepository::get_current` 的排序：同一 agent 下 revision 高者优先，同 revision 取 `created_at` 更新者。
- [ ] 覆盖 `MemoryRuntimeSessionExecutionAnchorRepository::create_once` 的幂等和 immutable conflict。
- [ ] 覆盖 `MemoryAgentRunDeliveryBindingRepository::upsert` 替换同 run/agent 当前 binding。
- [ ] 覆盖 `MemoryAgentRunCommandReceiptRepository::claim` 的 duplicate 与 digest conflict。

## Phase 3. 迁移首批调用点

- [ ] `crates/agentdash-application-agentrun/Cargo.toml` 添加 dev-dependency `agentdash-test-support`。
- [ ] 替换 `agentdash-application-agentrun` 中 `crate::test_support` 的 workflow adapter 引用。
- [ ] 删除或改薄 `crates/agentdash-application-agentrun/src/test_support`，避免保留第二份实现。
- [ ] `crates/agentdash-api/Cargo.toml` 添加 dev-dependency `agentdash-test-support`。
- [ ] 迁移 `crates/agentdash-api/src/routes/lifecycle_agents.rs` 局部 workflow repositories。
- [ ] 优先处理这些已发现热点：
  - `crates/agentdash-application-agentrun/src/agent_run/delivery_runtime_selection.rs`
  - `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs`
  - `crates/agentdash-application-agentrun/src/agent_run/mailbox/tests.rs`
  - `crates/agentdash-application-agentrun/src/agent_run/frame/launch_commit.rs`
  - `crates/agentdash-application-lifecycle/src/lifecycle/session_association.rs`
  - `crates/agentdash-api/src/routes/lifecycle_agents.rs`

## Phase 4. 添加静态 guard

- [ ] 创建 `scripts/check-test-support-boundaries.js`。
- [ ] 检查 Rust 文件中的 stateful repository adapter 定义和 impl。
- [ ] 支持 allowlist，初始仅覆盖本任务尚未迁移的 legacy 文件。
- [ ] 在 `package.json` 添加脚本，例如 `test-support:guard`。
- [ ] 在 `scripts/lib/quality-gates.js` 添加 `test_support_guard` step，并接入 `pr_quick`。
- [ ] 更新 `scripts/lib/quality-gates.test.js`，验证 manifest 和 gate 展开包含新 step。

## Phase 5. 更新规范

- [ ] 更新 `.trellis/spec/backend/repository-pattern.md`，记录测试 repository adapter 的归属和同步责任。
- [ ] 更新 `.trellis/spec/backend/quality-guidelines.md`，记录 quality gate 与新增 repository adapter 的检查要求。
- [ ] 文档描述集中维护的原因：stateful test adapter 承载 repository 可观察语义，集中维护能让生产 adapter 与测试 adapter 演进保持一致。

## Phase 6. 后续批次清单

- [ ] shared-library / marketplace：迁移 `MemoryLibraryAssetRepository`。
- [ ] workspace-module / canvas：迁移 `MemoryCanvasRepository`。
- [ ] extension runtime / workspace module：迁移 `MemoryProjectExtensionInstallationRepository`。
- [ ] skill / vfs / lifecycle surface：迁移 `MemorySkillAssetRepository`。
- [ ] 每迁移一个批次，同步收紧 guard allowlist。

## Validation

- [ ] `cargo test -p agentdash-test-support`
- [ ] `cargo test -p agentdash-application-agentrun`
- [ ] `cargo test -p agentdash-api lifecycle_agents`
- [ ] `node scripts/check-test-support-boundaries.js`
- [ ] `pnpm run quality:gates:check`
- [ ] `pnpm run quality:gates:test`
- [ ] `pnpm run check:quick`

## Risk Notes

- `agentdash-api` 当前没有 `[dev-dependencies]` 区块，添加 dev-dependency 时注意保持生产 dependency 不引用 test-support。
- `AgentFrameRepository::get_current` 的排序是本任务的首个语义校准点，迁移前后必须保留与 Postgres adapter 一致的行为。
- 工作区当前有并行未提交改动，执行时只改本任务文件和迁移目标文件，遇到无关 diff 不处理。
