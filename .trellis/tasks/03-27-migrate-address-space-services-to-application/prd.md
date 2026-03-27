# address space services + tool 实现迁入 application 层

## Goal

将 `address_space_access/` 中的 service 模块和 tool 实现全部迁入 `agentdash-application`，
让 application 拥有完整的 address space 访问能力和 runtime tool 实现。

## Background

在 P0 拆分和 AgentTool SPI 下沉到 connector-contract 之后，application 层可以：
1. 直接实现 `AgentTool` trait（来自 connector-contract）
2. 持有 address space service 实现

### 可迁移模块清单

| 文件 | 行数 | 类型 | 阻塞 |
|------|------|------|------|
| `inline_persistence.rs` | 232 | service | 无（纯 domain repo 操作） |
| `relay_service.rs` | 342 | service | 无（纯 application service） |
| `tools_workflow.rs` | 221 | tool impl | SPI 下沉后可迁 |
| `tools_companion.rs` | 1407 | tool impl | SPI 下沉后可迁 |
| `tools_hook.rs` | 218 | tool impl | SPI 下沉后可迁 |
| `tools_fs.rs` | 494 | tool impl | SPI 下沉后可迁 |
| `runtime_provider.rs` | 132 | tool builder | SPI 下沉后可迁 |

### Tool 迁移目标

Tool 实现按业务归属迁入 application 对应子模块：

```
application/src/
├── address_space/
│   ├── inline_persistence.rs   ← 从 api 迁入
│   ├── relay_service.rs        ← 从 api 迁入
│   └── tools/
│       ├── mod.rs              ← runtime_provider 逻辑
│       ├── fs.rs               ← tools_fs（MountsListTool, FsReadTool 等）
│       └── shell.rs            ← ShellExecTool（如果需要单独拆分）
├── workflow/
│   └── tools/
│       └── artifact_report.rs  ← tools_workflow（WorkflowArtifactReportTool）
├── task/
│   └── tools/
│       ├── companion.rs        ← tools_companion（CompanionDispatch/Complete）
│       └── hook_action.rs      ← tools_hook（ResolveHookActionTool）
```

## Requirements

### Phase 0: Mount/AddressSpace 类型统一到 domain（前置基建）

**问题**：`application::RuntimeMount` 和 `connector-contract::ExecutionMount` 字段完全一致，
存在双向转换 boilerplate（`to_execution_mount()` / `From<&ExecutionMount>`）。
`RuntimeAddressSpace` ↔ `ExecutionAddressSpace` 同理。`MountCapability` 已在 domain 中。

**方案**：将 Mount/AddressSpace 核心结构定义移入 `agentdash-domain`，消除重复。

1. 在 `domain/src/common/` 新增 `mount.rs`（或扩展现有 `mount_capability.rs`）：
   - 定义 `Mount`（即当前 `ExecutionMount` / `RuntimeMount` 的统一体）
   - 定义 `AddressSpace`（即当前 `ExecutionAddressSpace` / `RuntimeAddressSpace` 的统一体）
   - `MountCapability` 已在此，保持不动

2. `connector-contract`：
   - 删除 `ExecutionMount` / `ExecutionAddressSpace` 的独立定义
   - 改为 `pub use agentdash_domain::common::{Mount as ExecutionMount, AddressSpace as ExecutionAddressSpace}`
     （或直接使用新名称，视迁移友好度决定）

3. `application`：
   - 删除 `RuntimeMount` / `RuntimeAddressSpace` 的独立定义
   - 改为 re-export（`pub use agentdash_domain::common::{Mount as RuntimeMount, AddressSpace as RuntimeAddressSpace}`）
   - 删除所有 `to_execution_mount()` / `From<&ExecutionMount>` 转换代码

4. `executor`：通过 connector-contract re-export 自动获得新类型，无额外变更

**验证**：`cargo check` 全 crate 通过，零转换 boilerplate

### Phase 1: Service 迁移（可与 SPI 下沉并行）

1. 迁移 `inline_persistence.rs` → `application/src/address_space/inline_persistence.rs`
2. 迁移 `relay_service.rs` → `application/src/address_space/relay_service.rs`
3. 更新 `application/src/address_space/mod.rs` 导出新模块
4. api 侧改为 re-export

### Phase 2: Tool 迁移（SPI 下沉完成后）

5. 迁移 `tools_fs.rs` → `application/src/address_space/tools/fs.rs`
6. 迁移 `tools_workflow.rs` → `application/src/workflow/tools/artifact_report.rs`
7. 迁移 `tools_companion.rs` → `application/src/task/tools/companion.rs`
8. 迁移 `tools_hook.rs` → `application/src/task/tools/hook_action.rs`
9. 迁移 `runtime_provider.rs` → `application/src/address_space/tools/mod.rs`（或 provider.rs）
10. api 层 `address_space_access/` 仅保留 re-export 的 `mod.rs`

## Acceptance Criteria

- [ ] Mount/AddressSpace 核心结构在 `domain/src/common/` 中定义，其他 crate 为 re-export
- [ ] `application` 和 `connector-contract` 中零 mount 转换 boilerplate
- [ ] `cargo check -p agentdash-application` 通过
- [ ] `cargo check -p agentdash-api` 通过
- [ ] `RelayAddressSpaceService` 在 application crate 中可独立测试
- [ ] 所有 tool 实现在 application crate 中可独立测试
- [ ] api 层的 `address_space_access/mod.rs` 通过 re-export 保持对外 API 不变
- [ ] api 层 `address_space_access/` 总行数 < 50

## Technical Notes

- **Phase 0 命名策略**：统一后的权威类型建议命名为 `Mount` / `AddressSpace`（中性名称），
  各 crate 按需使用 type alias（`ExecutionMount = Mount`、`RuntimeMount = Mount`）保持
  迁移期间的兼容性，后续逐步统一为直接使用 `Mount`
- **domain 新增依赖**：domain 当前是零运行时依赖的纯类型 crate，Mount/AddressSpace
  只用到 serde + serde_json + schemars，与 domain 现有依赖一致，不引入新外部依赖
- `RelayAddressSpaceService` 使用 `regex::Regex` 做 inline 搜索，
  需确认 application 的 Cargo.toml 加上 `regex` 依赖
- `tools_companion.rs` 依赖 `SharedExecutorHubHandle`（来自 runtime_provider），
  迁移时需确保 ExecutorHub 的引用路径正确
- `tools_companion.rs` 依赖 `agent_client_protocol` 和 `agentdash_acp_meta`，
  需确认 application 的 Cargo.toml 包含这些依赖
- 迁移后 api 层的 `address_space_access/` 可大幅简化

## Dependency Chain

```
前置：03-27-agent-tool-dependency-decoupling (SPI 下沉，Phase 2 的前置)
前置：03-27-api-god-module-decomposition (P0 文件拆分，已完成)
并行：03-27-migrate-execution-hooks-to-application (无冲突)
后续：03-27-abstract-task-gateway-context (减少 api 层职责后更容易推进)
```
