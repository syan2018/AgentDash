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

### 架构决策摘要（已与用户确认）

| 决策点 | 结论 |
|--------|------|
| agent crate 定位 | 独立 Bounded Context，AgentTool SPI 属于跨层合同 |
| AgentTool SPI 归属 | 下沉到 connector-contract |
| execution_hooks 归属 | application/hooks/（独立执行策略关注点） |
| runtime_bridge 归属 | executor 层 |
| workspace_resolution 归属 | application 层（抽象 BackendAvailability trait） |
| tool 实现归属 | application 对应子模块（按业务归属分散） |
| AppState 解构方向 | RepositorySet 下沉到 application |

### 执行顺序（已与用户确认）

```
Step 1: AgentTool SPI 下沉到 connector-contract
        ├── agent 首次依赖 connector-contract
        ├── RuntimeToolProvider 去掉 feature gate
        └── ThinkingLevel 统一
            │
Step 2: (并行)
        ├── execution_hooks → application/hooks/
        └── address space services → application/address_space/
            │
Step 3: tool 实现迁入 application 对应子模块
        ├── tools_fs → application/address_space/tools/
        ├── tools_workflow → application/workflow/tools/
        ├── tools_companion → application/task/tools/
        └── tools_hook → application/task/tools/
            │
Step 4: gateway 解构
        ├── RepositorySet 下沉到 application
        ├── runtime_bridge 迁入 executor
        ├── workspace_resolution 迁入 application
        └── gateway 核心逻辑迁入 application
```

### Task 列表

| # | Task | 优先级 | 复杂度 |
|---|------|--------|--------|
| 1 | **agent-tool-dependency-decoupling** | P0（第一步） | 中 |
| 2 | **migrate-execution-hooks-to-application** | P1（Step 2 并行） | 低 |
| 3 | **migrate-address-space-services-to-application** | P1（Step 2+3） | 中 |
| 4 | **abstract-task-gateway-context** | P2（最后） | 高 |
