# 测试仓储与测试基础设施收束设计

## Architecture

新增 `crates/agentdash-test-support` 作为后端测试支撑 crate。它的职责是集中维护跨 crate 可复用的测试 adapter 和 fixture builder，让测试跨过业务 module 的 interface 断言行为，而不是在每个测试模块里重新实现 repository seam。

推荐目录：

```text
crates/agentdash-test-support/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── workflow.rs
    ├── shared_library.rs
    ├── workspace_module.rs
    └── skill.rs
```

第一轮只实现并迁移 `workflow.rs`，后续批次再补 `shared_library.rs`、`workspace_module.rs`、`skill.rs`。

## Dependency Shape

`agentdash-test-support` 依赖：

- `agentdash-domain`
- `agentdash-application-ports`
- 必要时依赖低层 application crate 中只包含 port input/output 类型的 crate
- `tokio`、`chrono`、`uuid`、`serde_json`、`async-trait`

业务 crate 通过 `[dev-dependencies]` 引用 `agentdash-test-support`。生产 `[dependencies]` 不引用该 crate。这样测试 adapter 的 interface 对所有测试可见，同时不会进入生产组合根。

## Workflow Adapter Contract

`workflow.rs` 导出 canonical memory adapters：

- `MemoryLifecycleRunRepository`
- `MemoryLifecycleAgentRepository`
- `MemoryAgentFrameRepository`
- `MemoryRuntimeSessionExecutionAnchorRepository`
- `MemoryAgentRunDeliveryBindingRepository`
- `MemoryAgentRunCommandReceiptRepository`
- `MemoryAgentRunLineageRepository`
- `MemoryAgentRunMailboxRepository`
- `MemoryProjectAgentRepository`
- `MemoryProjectBackendAccessRepository`
- `MemoryAgentRunForkMaterialization`

这些 adapter 的实现以生产 repository 的可观察语义为准。首个必须固化的契约是 `AgentFrameRepository::get_current`：current frame 先比较 `revision`，同 revision 比较 `created_at`，与 Postgres `ORDER BY revision DESC, created_at DESC LIMIT 1` 对齐。

测试需要额外暴露读取辅助时，优先提供小的 debug/query 方法，例如 `debug_list()`、`messages_for()`。这些方法表达测试观察需求，不改变 repository trait 自身。

## Quality Guard

新增脚本建议命名为 `scripts/check-test-support-boundaries.js`。检查范围覆盖 Rust 源码，识别普通业务测试中的 stateful repository adapter：

```text
struct (Memory|InMemory|Fake|Mock|Test).*Repository
impl .*Repository for (Memory|InMemory|Fake|Mock|Test).*
```

脚本应支持 allowlist，初始 allowlist 包括：

- `crates/agentdash-test-support/**`
- `crates/agentdash-infrastructure/**/Postgres*Repository`
- 明确命名的极小 failure adapter，例如 `Rejecting*Repository`、`Failing*Repository`
- 当前批次尚未迁移的已知 legacy 文件，迁移完成一个删除一个 allowlist 条目

质量门接入 `scripts/lib/quality-gates.js`：

- 新增 step：`test_support_guard`
- `pr_quick` 在 `backend_check` 前运行该 step
- `full_local` 通过 `pr_quick` 或直接 step 覆盖该检查

## Migration Strategy

第一轮以替换可复用 workflow repository 为主：

1. 从 `agentdash-application-agentrun/src/test_support/workflow_repositories.rs` 移植到 `agentdash-test-support/src/workflow.rs`。
2. 给 `agentdash-test-support` 添加自测，覆盖 current-frame tie-break、anchor create-once idempotency/conflict、delivery binding upsert、command receipt duplicate claim。
3. 更新 `agentdash-application-agentrun` dev-dependency，测试改为 `use agentdash_test_support::workflow::*`。
4. 更新 `agentdash-api` dev-dependency，迁移 `routes/lifecycle_agents.rs` 局部 workflow memory repositories。
5. 删除或瘦身原 `agentdash-application-agentrun` crate-private test support。若保留，只做 re-export 或文件级过渡，不再拥有独立实现。
6. 接入静态 guard。对同任务内尚未迁移的 legacy 重复实现使用显式 allowlist，并在 `implement.md` 中列出清理顺序。

## Trade-Offs

- 单独 crate 比在 `agentdash-application` 下暴露 `test_support` 更清晰，因为 API、application-lifecycle、application-agentrun 等 crate 都能通过 dev-dependency 使用同一入口。
- 第一轮不一次性迁移所有重复 fake，可以降低 blast radius；质量 guard 会让新增重复停止增长，后续批次再逐步减少 allowlist。
- Memory adapter 不模拟数据库所有实现细节，只承载 repository trait 的可观察语义。生产唯一事实仍是 migration + Postgres adapter，test-support 只服务业务测试。

## Rollback

如果迁移导致大量测试不稳定，回滚点是：

- 保留 `agentdash-test-support` crate 和自测。
- 暂时恢复个别调用点的旧局部 adapter。
- guard allowlist 覆盖未迁移文件。

回滚后仍保留规范和 crate 方向，因为它们定义的是长期收束目标。
