# agentdash-api God Module 拆解

## Goal

将 `agentdash-api` 中的 God Module 进行物理拆分，恢复模块内聚性。
跨 crate 迁移作为后续独立任务推进。

## Status: ✅ P0 已完成

### P0 成果：address_space_access.rs 拆分

**前后对比**：1 个 3361 行文件 → 8 个子模块文件

```
address_space_access/
├── mod.rs                  (19 行 re-export + 347 行集成测试)
├── inline_persistence.rs   (209 行) — InlineContentPersister / Overlay / DbPersister
├── relay_service.rs        (308 行) — RelayAddressSpaceService
├── runtime_provider.rs     (132 行) — RelayRuntimeToolProvider / SharedExecutorHubHandle
├── tools_workflow.rs       (202 行) — WorkflowArtifactReportTool
├── tools_companion.rs      (1309 行) — CompanionDispatch/Complete + 辅助函数 + 测试
├── tools_hook.rs           (197 行) — ResolveHookActionTool
└── tools_fs.rs             (471 行) — FS/Shell 6 个工具
```

**验证**：
- [x] `cargo check -p agentdash-api` 通过
- [x] `cargo check -p agentdash-application` 通过
- [x] `cargo check -p agentdash-local` 通过
- [x] `cargo test -p agentdash-api --lib` — 52 个测试全部通过
- [x] 零行为变更

## 后续任务（已创建独立 task，含完整 PRD）

### 架构决策摘要（已与用户确认，03-27 review 后修订）

| 决策点 | 结论 |
|--------|------|
| agent crate 定位 | 独立 Bounded Context，AgentTool SPI 属于跨层合同 |
| AgentTool SPI 归属 | 下沉到 connector-contract |
| ThinkingLevel 统一 | 唯一定义在 connector-contract，agent 和 application 均 re-export |
| Mount/AddressSpace 类型归属 | **统一到 domain**（作为共享值对象，MountCapability 已在此） |
| execution_hooks 归属 | application/hooks/（独立执行策略关注点） |
| runtime_bridge 归属 | mount 统一后重新评估（预计大幅缩减或消除） |
| workspace_resolution 归属 | application 层（抽象 BackendAvailability trait） |
| tool 实现归属 | application 对应子模块（按业务归属分散） |
| AppState 解构方向 | RepositorySet 下沉到 application |
| application → executor 解耦 | Task 4 延伸目标，通过 SessionExecutor trait 抽象 |

### 执行顺序（已与用户确认，03-27 review 后修订）

```
Step 1: AgentTool SPI 下沉到 connector-contract
        ├── agent 首次依赖 connector-contract
        ├── RuntimeToolProvider 去掉 feature gate
        └── ThinkingLevel 三重定义统一（含 application 中的第三份）
            │
Step 2: (并行)
        ├── execution_hooks → application/hooks/
        └── address space Phase 0: Mount/AddressSpace 类型统一到 domain
            + Phase 1: services → application/address_space/
            │
Step 3: tool 实现迁入 application 对应子模块
        ├── tools_fs → application/address_space/tools/
        ├── tools_workflow → application/workflow/tools/
        ├── tools_companion → application/task/tools/
        └── tools_hook → application/task/tools/
            │
Step 4: gateway 解构
        ├── RepositorySet 下沉到 application
        ├── runtime_bridge 评估与简化（mount 统一后预计大幅缩减）
        ├── workspace_resolution 迁入 application
        ├── gateway 核心逻辑迁入 application
        └── (延伸) application → executor 解耦（SessionExecutor trait 抽象）
```

### Task 列表

| # | Task | 优先级 | 复杂度 | 修订说明 |
|---|------|--------|--------|---------|
| 1 | **agent-tool-dependency-decoupling** | P0（第一步） | 中 | +application ThinkingLevel 清理 |
| 2 | **migrate-execution-hooks-to-application** | P1（Step 2 并行） | 低 | 无修订 |
| 3 | **migrate-address-space-services-to-application** | P1（Step 2+3） | 中 | +Phase 0 Mount 类型统一到 domain |
| 4 | **abstract-task-gateway-context** | P2（最后） | 高 | +Phase 6 application→executor 解耦延伸目标 |
