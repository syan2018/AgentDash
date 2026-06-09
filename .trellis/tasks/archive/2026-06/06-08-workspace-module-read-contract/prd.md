# Workspace Module 读路径与单一契约

> Parent: `06-08-agent-runtime-surface-registry`。本 child 是三轮拆分的第一轮（读路径），零执行副作用。
> 设计依据见 parent `design.md`（核心概念、§7 Extension 映射、§9 AgentFrame 锚点修正、§10 权限与 schema、§12 拆分）。

## Goal

定义并落地**单一 Workspace Module projection 契约**，把 enabled extension、visible canvas、built-in module 聚合为同一种 module descriptor，并通过 `workspace_module_list` / `workspace_module_describe` 暴露给 Agent。本轮只读，先验证 Agent 发现路径，并把 descriptor shape 一次定准供后续 child 与项目设置页 UI 复用。

## 已锁定决策（来自 parent）

- D1：本轮只读，不做 invoke/present。
- D3：`protocol_channels.methods` 作为其 provider extension module 的 operations 投影，**不独立成 module**；descriptor DTO 只有一种 module 形态。
- D4：AgentFrame 预留可见性裁切字段，但可见性解析走完整 Capability 能力通道（`CapabilityState` / `effective_capability_json`），不另开旁路 projection。
- D5（契约侧）：同一份 projection 同时服务 Agent 工具与项目设置页 UI，单一 canonical，不做两套 DTO。
- D6：收口现有 `FrameLaunchIntent.extension_runtime` / `ConstructionProjections.extension_runtime` 死字段——接管为新 projection 或删除，不留半截。

## Requirements

- R1 单一 module descriptor DTO：覆盖 `module_id` / `kind`(extension|canvas|builtin) / `source` / UI entries / operations（含 channel-as-operation）/ status / permission summary。`list` 返回摘要，`describe` 返回含 input/output schema 的完整 descriptor。
- R2 聚合来源：从 enabled `ProjectExtensionInstallation` projection（复用 `extension_runtime.rs`）+ visible `Canvas` + built-in descriptor 聚合，**不新建业务事实源**。
- R3 可见性：默认全集 = Project enabled extension + project visible canvas；AgentFrame 预留裁切字段，裁切结果经 Capability 通道解析后进入 turn projection。
- R4 元工具：`workspace_module_list` / `workspace_module_describe` 接入 session runtime tool provider，schema delta 只新增这两个工具本身，不展开所有 extension operations。
- R5 死字段收口（D6）：让 frame/turn construction 真正消费新 projection，或删除既有 `extension_runtime` 字段；不允许新旧投影并存。

## Acceptance Criteria

- [ ] 当前 session 的 Agent 能通过 `workspace_module_list` 看到 enabled extension 与 visible canvas，返回的是摘要而非全量 schema。
- [ ] `workspace_module_describe(module_id)` 返回单个 module 的 UI entries 与 operations（extension action + channel-as-operation 同构呈现）。
- [ ] 工具 schema delta 仅新增 `workspace_module_list/describe`，不随 Project 安装数膨胀。
- [ ] projection 不成为新业务事实源，只引用 `ProjectExtensionInstallation` / `Canvas` / capability。
- [ ] 既有 `extension_runtime` 死字段被接管或删除，仓库中不再有"永远 None"的并存投影。
- [ ] 同一 projection DTO 可被项目设置页 UI 直接复用（Child 3 验证消费）。
- [ ] Rust contract generation 与前端类型生成通过。

## 候选修改面

- `crates/agentdash-contracts`（新 Workspace Module DTO）
- `crates/agentdash-application/src/extension_runtime.rs`（复用聚合）
- `crates/agentdash-application/src/canvas`（canvas → module 映射）
- `crates/agentdash-application/src/session/construction.rs` + `workflow/frame_construction/mod.rs`（D6 收口）
- `crates/agentdash-application/src/session/launch/preparation.rs` + `deps.rs`（turn 注入）
- `crates/agentdash-application/src/vfs/tools/provider.rs` 或新建 workspace module tool provider
- `crates/agentdash-domain/src/workflow/agent_frame.rs`（预留裁切字段）
- `crates/agentdash-spi/src/platform/tool_capability.rs`
- `packages/app-web/src/generated`（类型生成）

## 建议验证

```powershell
cargo test -p agentdash-application extension_runtime
cargo test -p agentdash-application workspace_module
cargo test -p agentdash-application capability
cargo test -p agentdash-contracts
pnpm contracts:check
```

## Notes

- 复杂 child：`task.py start` 前补 `design.md`（DTO 字段与 capability 解析时序）+ `implement.md`（有序 checklist）。
- 风险：descriptor DTO 一旦定错，Child 2/3 全部受影响——本轮是契约定锚轮，优先把 shape 评审到位。
