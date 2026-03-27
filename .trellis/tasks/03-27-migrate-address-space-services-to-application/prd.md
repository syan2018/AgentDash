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

- [ ] `cargo check -p agentdash-application` 通过
- [ ] `cargo check -p agentdash-api` 通过
- [ ] `RelayAddressSpaceService` 在 application crate 中可独立测试
- [ ] 所有 tool 实现在 application crate 中可独立测试
- [ ] api 层的 `address_space_access/mod.rs` 通过 re-export 保持对外 API 不变
- [ ] api 层 `address_space_access/` 总行数 < 50

## Technical Notes

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
