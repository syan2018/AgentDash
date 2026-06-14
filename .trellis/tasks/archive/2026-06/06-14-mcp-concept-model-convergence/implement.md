# MCP 概念模型收束实施计划

## Preconditions

- 规划经用户确认后再执行 `task.py start`。
- 实现前读取 backend、frontend、cross-layer 相关 spec。
- 每个阶段以删除无价值概念为目标，不新增兼容路径。
- 第一阶段由主会话先锁定目标数据结构和模块归属；锁定完成后再派 subagent 做模块级重构。

## Implementation Checklist

0. Contract lock：先写好目标数据结构和模块归属
   - 明确最终保留的类型名、模块名、public re-export 边界。
   - 在 Rust 中固定 runtime surface、summary surface、wire adapter 的目标归属。
   - 在 TS 中固定 cloud preset contract type、local runtime config type 和 shared editor props 的边界。
   - 删除或封禁任何会让 summary 反向生成 runtime declaration 的入口。
   - 该阶段完成前不派模块 subagent 做大范围替换。

1. 建立 MCP 概念清单
   - 枚举当前 `Mcp*` / `RuntimeMcp*` / `mcp_*` 数据结构。
   - 标注每个结构归属：事实源、边界 DTO、展示投影、可删除遗留。
   - 将目标命名落到 Rust / TS 代码改动清单。

2. 收束 Rust transport / relay adapter
   - 新增或移动一个权威 MCP adapter 模块。
   - 合并 `RuntimeMcpServer -> McpServerRelay`。
   - 合并 `McpTransportConfig <-> McpTransportConfigRelay`。
   - 删除 application/api/local 中重复 match 逻辑。

3. 收束执行面 runtime model
   - 将执行面 MCP 类型固定为 `RuntimeMcpServer`。
   - 将 application 当前 `RuntimeMcpServer` 改名为 summary/view。
   - 删除有损反向转换入口。
   - 校准 session plan、bootstrap、context contribution 对 summary 类型的使用。

4. 清理 Agent MCP 引用路径
   - 保留 `mcp_preset_keys -> McpPreset -> RuntimeMcpServer` 主路径。
   - 删除 inline agent MCP server 路径。
   - 校准 capability resolver、activity activation、owner bootstrap。

5. 校准 AgentFrame / Capability projection
   - 确认 `mcp_surface_json` 只序列化 runtime surface。
   - 校准 `project_capability_state_from_frame` 与 `capability_state_to_frame_surfaces`。
   - 覆盖 runtime capability replay 的 MCP server set effect。

6. 收束前端 MCP 类型
   - Cloud MCP Preset 表单使用 generated contract 类型。
   - Local runtime 设置使用 local config wrapper。
   - `McpTransportConfigEditor` 改为显式抽象 props 或由调用侧 adapter 包装。
   - 删除结构兼容带来的隐式类型借用。

7. Marketplace 模板边界
   - 确认 `mcp_server_template` 只安装成 `McpPreset`。
   - 删除任何让 template 参与 runtime/capability/relay 的路径。

8. 数据库与生成文件
   - 如目标模型删除字段，新增 migration。
   - 更新 repository 映射。
   - 重新生成 TS contracts。

9. 文档与 spec
   - 更新相关 spec，记录目标分层原因。
   - 不记录被删除实现的历史细节。

## Subagent Dispatch Plan

Contract lock 通过后再按模块派发 subagent，每个 subagent 的验收标准都是删除旧结构和旧转换，而不是包兼容层。

| Subagent Track | Files / Modules | Deliverable |
| --- | --- | --- |
| Session / Capability | `agentdash-application/src/capability`, `session`, `workflow/frame_*` | MCP runtime surface 单一化，AgentFrame projection 和 replay 只围绕锁定类型 |
| Relay / Local Runtime | `agentdash-relay`, `agentdash-api/src/relay`, `agentdash-local` | 统一 adapter 接管所有 MCP transport/server wire 转换 |
| Frontend | `packages/app-web`, `packages/core`, `packages/views/mcp-shared` | cloud/local MCP transport 类型显式分流，shared editor 不混用事实源 |
| Marketplace | `shared_library`, marketplace UI display | `mcp_server_template` 只产出 `McpPreset`，不参与 runtime/capability |
| Verification / Spec | `.trellis/spec`, generated contracts, targeted tests | specs 记录目标结构，生成文件和测试验证无重复概念回流 |

需要 Trellis child tasks 时，在 Contract lock 后创建；父任务保留总体目标、分派计划和最终集成 review。

## Validation Commands

```powershell
cargo test -p agentdash-application mcp
cargo test -p agentdash-api mcp
cargo test -p agentdash-local mcp
cargo test -p agentdash-executor mcp
pnpm typecheck
pnpm test -- mcp
```

最终验证根据实际改动范围补充 `cargo test --workspace` 或相关前端包测试。

## High-Risk Files

- `crates/agentdash-domain/src/mcp_preset/*`
- `crates/agentdash-spi/src/connector/mod.rs`
- `crates/agentdash-spi/src/platform/tool_capability.rs`
- `crates/agentdash-application/src/mcp_preset/runtime.rs`
- `crates/agentdash-application/src/capability/resolver.rs`
- `crates/agentdash-application/src/runtime_bridge.rs`
- `crates/agentdash-application/src/session/capability_state.rs`
- `crates/agentdash-application/src/workflow/frame_builder.rs`
- `crates/agentdash-application/src/relay_connector.rs`
- `crates/agentdash-api/src/relay/mcp_relay_impl.rs`
- `crates/agentdash-local/src/mcp_client_manager.rs`
- `crates/agentdash-local/src/handlers/relay_mcp_servers.rs`
- `crates/agentdash-relay/src/protocol/mcp.rs`
- `packages/core/src/local-runtime/index.ts`
- `packages/views/src/mcp-shared/McpTransportConfigEditor.tsx`
- `packages/app-web/src/features/mcp-shared/*`
- `packages/app-web/src/generated/mcp-preset-contracts.ts`

## Review Gates

- Gate 0: Contract lock 完成，目标类型与模块归属已在代码中固定。
- Gate 1: Rust adapter 合并后，重复转换点消失。
- Gate 2: 执行面 runtime model 与 summary model 分离后，无有损反向转换。
- Gate 3: 前端 cloud/local MCP transport 类型不再隐式混用。
- Gate 4: 全链路 MCP preset 启用、relay probe/list/call、AgentFrame projection 通过定向测试。

## Progress

### 2026-06-14 Gate 0/1/2/3 Contract Lock

- Rust runtime surface 固定为 `agentdash_spi::RuntimeMcpServer`。
- Application 展示摘要固定为 `agentdash_application::runtime::McpServerSummary`，`runtime_bridge` 只保留 `RuntimeMcpServer -> McpServerSummary` 单向投影。
- Relay wire server 固定为 `agentdash_relay::McpServerRelay`，MCP transport/server wire 转换集中到 `agentdash_application::mcp_relay_adapter`。
- `agentdash-api`、`agentdash-local`、`agentdash-application::relay_connector` 已删除重复 MCP transport/server match 转换。
- `McpTransportConfigEditor` 使用 `McpTransportConfigEditorValue` / `McpTransportConfigEditorEntry`，不再 import local-runtime MCP transport 类型。
- Inline agent MCP server 路径已删除，custom `mcp:<key>` 只从 project MCP Preset 解析为 `RuntimeMcpServer`。

已通过检查：

```powershell
cargo check -p agentdash-relay -p agentdash-spi -p agentdash-application-ports -p agentdash-application -p agentdash-api -p agentdash-local -p agentdash-executor
pnpm --filter @agentdash/views typecheck
pnpm --filter app-web typecheck
pnpm run contracts:check
cargo test -p agentdash-application -p agentdash-local -p agentdash-executor -p agentdash-relay --no-run
```

### 2026-06-14 Marketplace / Shared Library 边界收束

- 已审计 `mcp_server_template` / `McpServerTemplate` / `AgentMcpDependencyTemplate` / `AgentMcpSlotTemplate` / `McpTransportTemplate` 引用。
- `mcp_server_template` 安装路径保持为 `McpServerTemplatePayload -> McpPreset`，`InstallLibraryAssetOutput` 只返回 `McpPreset { id }`。
- Agent 模板 MCP 依赖路径只预安装依赖模板为 Project MCP Preset，并向 ProjectAgent 配置写入 `mcp_preset_keys`；未发现模板进入 `RuntimeMcpServer`、`McpServerRelay`、`CapabilityState`、`AgentFrame` 或 `mcp_surface_json`。
- `AgentMcpDependencyInstallPlan` 已收束命名为 `AgentMcpPresetInstallPlan`，强调安装产物是 Project MCP Preset。

已通过检查：

```powershell
cargo check -p agentdash-domain -p agentdash-application -p agentdash-contracts
```

### 2026-06-14 Final Quality Gate

- `docs/relay-protocol.md` 已同步到 `McpServerRelay { name, transport }` payload，`mcp.list_tools` / `mcp.call_tool` 不再使用旧 `server_name` 示例。
- 当前代码、packages、docs、spec、当前任务和相关活跃任务中，旧类型名与旧 inline MCP server 路径 scoped search 无命中。
- Shared Library / Marketplace MCP template 仍只停留在 publish/install/display 边界，没有进入 runtime、capability、AgentFrame 或 relay dispatch。
- 本次没有 MCP 数据库 schema 改动；不需要 migration。

已通过最终检查：

```powershell
cargo fmt --check
cargo check -p agentdash-relay -p agentdash-spi -p agentdash-application-ports -p agentdash-application -p agentdash-domain -p agentdash-contracts -p agentdash-api -p agentdash-local -p agentdash-executor
pnpm --filter @agentdash/views typecheck
pnpm --filter app-web typecheck
pnpm run contracts:check
cargo test -p agentdash-application -p agentdash-local -p agentdash-executor -p agentdash-relay mcp
python ./.trellis/scripts/task.py validate ./.trellis/tasks/06-14-mcp-concept-model-convergence
```
