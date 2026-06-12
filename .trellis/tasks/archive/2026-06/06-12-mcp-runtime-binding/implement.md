# AgentRun MCP 运行时绑定实施计划

## Pre-Dev Context

实现前先加载：

- `.trellis/spec/backend/architecture.md`
- `.trellis/spec/backend/capability/architecture.md`
- `.trellis/spec/backend/capability/capability-dimension-pipeline.md`
- `.trellis/spec/backend/vfs/architecture.md`
- `.trellis/spec/backend/database-guidelines.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- `.trellis/spec/cross-layer/desktop-local-runtime.md`

实现必须先执行：

```powershell
python ./.trellis/scripts/task.py start .trellis/tasks/06-12-mcp-runtime-binding
```

## Implementation Checklist

### 1. Domain And Persistence

- [ ] 在 `crates/agentdash-domain/src/mcp_preset/value_objects.rs` 增加 runtime binding 值对象。
- [ ] 在 `McpPreset` entity 增加 `runtime_binding: Option<McpRuntimeBindingConfig>`。
- [ ] 更新 `new_user` / `new_builtin` 构造函数签名或 builder，使 builtin/user 都能携带 runtime binding。
- [ ] 新增 migration：`0012_mcp_preset_runtime_binding.sql`。
- [ ] 更新 `PostgresMcpPresetRepository` 的 `COLS`、insert、update、row mapping、测试。
- [ ] 更新 builtin MCP preset definition / shared-library MCP template 安装路径，确保字段缺省为 None 且不丢失。

### 2. Contracts And Generated Types

- [ ] 在 `crates/agentdash-contracts/src/mcp_preset.rs` 增加 runtime binding DTO。
- [ ] 更新 request/response DTO 的 From/Into 映射。
- [ ] 运行 contract generator，提交 `packages/app-web/src/generated/mcp-preset-contracts.ts` 更新。
- [ ] 更新前端 `types` re-export / alias（如存在）。

### 3. AgentRun Runtime Surface Facts

- [ ] 修改 `workspace_mount`，把 selected binding `detected_facts` 写入 mount metadata。
- [ ] 为 mount metadata 的 workspace facts 增加 focused tests。
- [ ] 确认 story/project/subject workspace 路径最终都能通过 VFS `main` mount 提供同一 facts。

### 4. Runtime Binding Resolver

- [ ] 在 `crates/agentdash-application/src/mcp_preset/runtime.rs` 或新文件实现 `SessionRuntimeMcpContext` 与 `resolve_preset_mcp_server`。
- [ ] 实现 source 读取：
  - `vfs.main.root_ref`
  - `vfs.main.backend_id`
  - `workspace.id`
  - `workspace.binding_id`
  - `workspace.identity.*`
  - `workspace.detected_facts.*`
- [ ] 实现 target 应用：
  - HTTP/SSE query
  - HTTP/SSE header
  - stdio env
  - stdio cwd
- [ ] 为 URL query 使用 parser，不做字符串拼接。
- [ ] 为 missing required / mismatch / invalid header / invalid cwd 输出结构化错误。
- [ ] 覆盖 resolver 单测。

### 5. Construction And Capability Resolver

- [ ] 调整 `build_project_agent_context`，避免在 final VFS 前把 preset key 提前解析成静态 `SessionMcpServer`。
- [ ] 调整 `AgentLevelMcp`，让 owner bootstrap 能在 final VFS 后统一解析 agent-level preset MCP。
- [ ] 更新 `CapabilityResolverInput`，让 `mcp:<preset>` directive 使用同一 runtime binding resolver。
- [ ] 确认 `normalize_owner_bootstrap_mcp_projection` 去重后 capability state 中的 MCP server 已是 resolved transport。
- [ ] 增加测试证明 `mcp_preset_keys` 与 `mcp:<preset>` 对同一 preset 的 resolved server 一致。

### 6. Transport Schema: stdio cwd

- [ ] 扩展 `McpTransportConfig::Stdio` 增加 `cwd: Option<String>`。
- [ ] 更新 contracts DTO、generated TS、frontend local-runtime type、relay protocol type。
- [ ] 更新所有构造点和 tests。
- [ ] 更新 `McpTransportConfigEditor`，为 stdio transport 增加 cwd 输入。
- [ ] 更新 parser/serializer：local runtime config、relay prompt mcp server parser、session-to-relay prompt projection。

### 7. Direct MCP Headers

- [ ] 扩展 `McpHttpServerSpec` 携带 headers。
- [ ] 修改 direct connect helper，使用 `StreamableHttpClientTransportConfig::custom_headers`。
- [ ] 更新 HTTP/SSE parse 行为；不支持的 transport 明确跳过或返回当前既有语义。
- [ ] 覆盖 headers 进入 config 的单元测试。

### 8. Relay MCP Resolved Transport

- [ ] 修改 `agentdash-relay` MCP payload：list/call 从 `server_name` 升级为 resolved server declaration。
- [ ] 更新云端 `BackendRegistry` / `McpRelayProvider` 实现，发送 resolved transport。
- [ ] 更新 executor relay discovery adapter，保留 tool naming / capability filtering。
- [ ] 更新本机 `handle_mcp_list_tools` / `handle_mcp_call_tool`，消费 resolved server declaration。
- [ ] 更新 protocol serialization tests。

### 9. Local MCP Manager

- [ ] 修改 `McpClientManager`，支持静态 config server 与 session resolved server 两种输入。
- [ ] 连接池 key 使用 `server_name + stable_hash(transport)`。
- [ ] stdio spawn 设置 env 与 cwd。
- [ ] HTTP/SSE worker 使用 headers。
- [ ] close 逻辑按 server name 清理所有相关连接，或按 key 清理并补充调用方。
- [ ] 增加测试覆盖同名不同 transport 不共用连接。

### 10. Probe Semantics

- [ ] 更新 application probe：含 required runtime binding 且无 runtime context 时返回 unsupported diagnostic。
- [ ] 更新前端展示文案，避免把 unsupported 当普通失败。
- [ ] 添加 focused tests。

### 11. Frontend Preset Editor

- [ ] 扩展 `McpPresetFormState`、create/update builder、validation。
- [ ] `McpPresetCategoryPanel` 增加 runtime binding 高级编辑区。
- [ ] 卡片/详情显示“运行时绑定”状态。
- [ ] Project Agent MCP quick-create 不丢弃 runtime_binding。
- [ ] 增加 helper 或组件 focused tests。

### 12. Documentation / Spec Update

- [ ] 若实现确认了 runtime binding contract，更新后端 capability 或 cross-layer local runtime spec，记录为什么 MCP resolved transport 以 AgentFrame final VFS 为事实源。
- [ ] 文档只记录目标设计原因，不记录旧实现问题。

## Validation Commands

按修改范围选择执行，完整收口时建议：

```powershell
pnpm run migration:guard
pnpm run contracts:check
cargo test -p agentdash-domain mcp_preset
cargo test -p agentdash-infrastructure mcp_preset
cargo test -p agentdash-application mcp_preset
cargo test -p agentdash-application capability
cargo test -p agentdash-executor mcp
cargo test -p agentdash-relay mcp
cargo test -p agentdash-local mcp
pnpm run frontend:check
```

如触及全工作区编译面，补充：

```powershell
cargo check --workspace
```

## Risky Files

- `crates/agentdash-domain/src/mcp_preset/value_objects.rs`
- `crates/agentdash-domain/src/mcp_preset/entity.rs`
- `crates/agentdash-application/src/mcp_preset/runtime.rs`
- `crates/agentdash-application/src/capability/resolver.rs`
- `crates/agentdash-application/src/session/assembler.rs`
- `crates/agentdash-application/src/session/construction_planner.rs`
- `crates/agentdash-application/src/vfs/mount.rs`
- `crates/agentdash-relay/src/protocol/mcp.rs`
- `crates/agentdash-api/src/relay/mcp_relay_impl.rs`
- `crates/agentdash-local/src/mcp_client_manager.rs`
- `crates/agentdash-local/src/handlers/mcp_relay.rs`
- `crates/agentdash-local/src/handlers/relay_mcp_servers.rs`
- `crates/agentdash-executor/src/mcp/direct.rs`
- `crates/agentdash-contracts/src/mcp_preset.rs`
- `packages/app-web/src/features/assets-panel/categories/McpPresetCategoryPanel.tsx`
- `packages/app-web/src/features/mcp-shared/helpers.ts`

## Rollback Points

- After Domain/Persistence: schema field exists but no runtime consumer; rollback by removing new code and adding a migration that drops the column only if the task has not shipped to shared environments.
- After Resolver: static preset path must still pass existing tests; if resolver breaks construction, fix runtime context assembly before continuing.
- After Relay Protocol: cloud/local protocol changes must land together; do not leave cloud sending new shape while local parser expects old shape.
- After Frontend: generated contract changes must be committed with Rust contract changes; do not hand-edit generated files.

## Review Gates Before Start

- [ ] User accepts MVP source/target matrix.
- [ ] User accepts stdio cwd as schema change in `McpTransportConfig`.
- [ ] User accepts ordinary preset probe returning unsupported for required runtime-bound presets.
- [ ] Task status is changed to `in_progress` before code edits begin.
