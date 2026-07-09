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

先实现 `workflow.rs` 和 guard，再按 guard 扫描结果补齐 `shared_library.rs`、`workspace_module.rs`、`skill.rs` 等模块，直到全仓 stateful test repository adapter 不再分散实现。

## Dependency Shape

`agentdash-test-support` 依赖：

- `agentdash-domain`
- `agentdash-application-ports`
- 只包含 port input/output 类型的低层 crate
- `tokio`、`chrono`、`uuid`、`serde_json`、`async-trait`

业务 crate 通过 `[dev-dependencies]` 引用 `agentdash-test-support`。生产 `[dependencies]` 不引用该 crate。这样测试 adapter 的 interface 对所有测试可见，同时不会进入生产组合根。

## Adapter Contracts

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

其他模块按扫描结果导出 canonical memory adapters：

- `shared_library.rs` 承载 `MemoryLibraryAssetRepository`。
- `workspace_module.rs` 承载 `MemoryCanvasRepository`、`MemoryProjectExtensionInstallationRepository` 和相关 fixture。
- `skill.rs` 承载 `MemorySkillAssetRepository`。
- 只服务单一错误路径的 adapter 使用 `Failing*` / `Rejecting*` 命名，并放在测试局部或 test-support 中表达失败语义。

测试需要额外暴露读取辅助时，优先提供小的 debug/query 方法，例如 `debug_list()`、`messages_for()`。这些方法表达测试观察需求，不改变 repository trait 自身。

## Quality Guard

新增脚本建议命名为 `scripts/check-test-support-boundaries.js`。检查范围覆盖 Rust 源码，识别普通业务测试中的 stateful repository adapter：

```text
struct (Memory|InMemory|Fake|Mock|Test).*Repository
impl .*Repository for (Memory|InMemory|Fake|Mock|Test).*
```

脚本应支持 allowlist，最终 allowlist 包括：

- `crates/agentdash-test-support/**`
- `crates/agentdash-infrastructure/**/Postgres*Repository`
- 明确命名的极小 failure adapter，例如 `Rejecting*Repository`、`Failing*Repository`

迁移过程中可以临时加入 legacy 文件，但最终验收时不保留“待迁移”类 allowlist。

质量门接入 `scripts/lib/quality-gates.js`：

- 新增 step：`test_support_guard`
- `pr_quick` 在 `backend_check` 前运行该 step
- `full_local` 通过 `pr_quick` 或直接 step 覆盖该检查

## Migration Strategy

迁移以 guard 驱动：先建立 guard 和 test-support，然后按扫描结果消除当前全仓重复实现。

1. 从 `agentdash-application-agentrun/src/test_support/workflow_repositories.rs` 迁移到 `agentdash-test-support/src/workflow.rs`。机械搬运使用脚本/命令行移动与替换，不用手写 patch 重抄实现。
2. 给 `agentdash-test-support` 添加自测，覆盖 current-frame tie-break、anchor create-once idempotency/conflict、delivery binding upsert、command receipt duplicate claim。
3. 接入静态 guard，用 guard 输出生成剩余迁移清单。
4. 更新 `agentdash-application-agentrun`、`agentdash-api` 及其他相关 crate 的 dev-dependency，测试改为 `use agentdash_test_support::*`。
5. 迁移 shared-library、workspace-module/canvas、extension installation、skill/vfs/lifecycle surface 等重复 adapter。
6. 删除或瘦身原 `agentdash-application-agentrun` crate-private test support。若保留，只做 re-export，不再拥有独立实现。
7. 收紧 guard allowlist 到最终集合，并以 guard clean 作为验收。

## Trade-Offs

- 单独 crate 比在 `agentdash-application` 下暴露 `test_support` 更清晰，因为 API、application-lifecycle、application-agentrun 等 crate 都能通过 dev-dependency 使用同一入口。
- 全仓清扫会增加一次性改动面，但能让 guard 立刻成为真实约束，不留下“以后再迁”的破窗。
- Memory adapter 不模拟数据库所有实现细节，只承载 repository trait 的可观察语义。生产唯一事实仍是 migration + Postgres adapter，test-support 只服务业务测试。

## Rollback

如果迁移导致大量测试不稳定，回滚点是：

- 保留 `agentdash-test-support` crate 和自测。
- 暂时恢复个别调用点的旧局部 adapter。
- guard allowlist 可以临时覆盖未迁移文件，但回滚后必须保留 guard 输出作为继续清扫清单。

回滚后仍保留规范和 crate 方向，因为它们定义的是长期收束目标。
