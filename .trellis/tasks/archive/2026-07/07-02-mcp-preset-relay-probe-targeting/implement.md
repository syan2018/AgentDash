# 实施计划

## Checklist

- [ ] 默认目标存在多个在线 Desktop local backend 时，helper 选择最近 claimed 的可用 backend。
- [ ] 为 `ProbeMcpPresetRequest` / generated TS 合同增加 `probe_target`。
- [ ] 为 `McpProbeTransportInput` 增加 application-port target 类型。
- [ ] 新增 MCP probe backend resolver/helper：默认目标从当前用户自己的在线 Desktop local backend 解析，显式目标复用 `BackendAuthorizationService::require_backend(..., View)`。
- [ ] 更新 API route mapping，将 `probe_target`、当前用户 identity 和 Project view 权限映射到 resolver 需要的上下文。
- [ ] 将 `McpRelayProvider::probe_transport` 扩展为接收 resolver 产出的明确 backend target，删除产品路径对 `find_any_online_backend_for_setup_probe()` 的依赖。
- [ ] 保留或降级 `find_any_online_backend_for_setup_probe()` 为仅测试/诊断 helper，并避免产品 probe 调用。
- [ ] 前端 Assets / Agent preset / Workflow capability 面板提交 default-user-local target，并展示本机 runtime unavailable 文案。
- [ ] 补齐后端和前端测试。

## Validation Commands

```powershell
cargo test -p agentdash-application mcp_preset::probe
cargo test -p agentdash-api routes::mcp_presets
cargo test -p agentdash-application-runtime-gateway mcp_probe
pnpm run contracts:check
pnpm run frontend:check
```

## Risky Files

- `crates/agentdash-api/src/relay/mcp_relay_impl.rs`
- `crates/agentdash-api/src/relay/registry.rs`
- `crates/agentdash-api/src/routes/mcp_presets.rs`
- `crates/agentdash-application-ports/src/runtime_gateway_setup.rs`
- `crates/agentdash-contracts/src/integration/mcp_preset.rs`
- `packages/app-web/src/stores/mcpProbeStore.ts`
- `packages/app-web/src/services/mcpPreset.ts`
- `packages/app-web/src/features/assets-panel/categories/McpPresetCategoryPanel.tsx`

## Review Gate

- 多候选策略已确定为最近 claimed 的可用 Desktop local backend；实现时不增加额外选择交互。
