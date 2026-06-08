# Workspace Module 操作面 (invoke + present)

> Parent: `06-08-agent-runtime-surface-registry`。本 child 是第二轮（操作面），依赖 Child 1 的 projection 契约定死后再开运行代码。
> 设计依据见 parent `design.md` §6（工具形态）、§7（Extension 映射）、§8（Canvas 映射）、§10（权限与 schema）。

## Goal

在 Child 1 只读契约之上，落地两个写/展示元工具：`workspace_module_invoke`（按 module 来源分支派发的统一调用入口）与 `workspace_module_present`（best-effort GUI 推送）。Agent 只传 `module_id + operation_key/view_key + input`，宿主解析全部内部路由。

## 已锁定决策（来自 parent）

- D2：Canvas 首轮只做 read/present/invoke，`invoke` 的 canvas 分支**包现有 canvas application service**，不另起 authoring 路径；Agent 主动 create/update authoring 作为本轮可选尾段或后续任务。
- D3：channel 作为 provider module operations，invoke 对 channel method 与 runtime action 同构处理。

## Requirements

- R1 `workspace_module_invoke`：输入 `module_id + operation_key + input`。宿主按 module 来源**分支派发**：
  - extension → `RuntimeGateway` / `ExtensionRuntimeActionProvider`（要求 Session 携 Project + Backend target，复用现有路由）。
  - canvas → 现有 canvas application service / `runtime-invoke`，不另起逻辑。
  - builtin → 对应平台 service。
- R2 服务端裁决：校验 operation 属于该 module、按 operation schema 校验 input、权限裁决、backend placement 解析；拒绝未知 operation。
- R3 路由内聚：Agent 不传 Project / Backend / Workspace root / AgentFrame ID，这些由宿主从当前 execution context 解析。
- R4 `workspace_module_present`：输入 `module_id + view_key + optional payload`，请求宿主向 WorkspacePanel / CanvasPanel 展示；无前端可展示目标时返回**可操作诊断事件**，不静默失败。
- R5 trace：记录 module source、operation provenance、permission decision、backend，保证 extension 与 canvas 调用可审计。

## Acceptance Criteria

- [ ] Agent `workspace_module_invoke(module_id, operation_key, input)` 能路由到 extension runtime action 并返回结果。
- [ ] canvas 分支通过现有 canvas service 执行，仓库中不出现第二套 canvas authoring 逻辑。
- [ ] 服务端对未知 operation / schema 不匹配 / 权限不足 / 缺 backend 分别返回明确错误，不裸 panic、不静默吞。
- [ ] Agent 调用不携带任何内部路由 ID。
- [ ] `workspace_module_present` 能把 extension webview 推到 WorkspacePanel、canvas 推到 CanvasPanel；无目标时产出诊断事件。
- [ ] trace 中可还原 module source 与 operation provenance。

## 候选修改面

- `crates/agentdash-application/src/runtime_gateway` + `runtime_gateway/tool_adapter.rs`
- `crates/agentdash-application/src/runtime_gateway/extension_actions.rs`
- `crates/agentdash-application/src/canvas`（复用 invoke service）
- `crates/agentdash-api/src/routes/extension_runtime.rs` / `routes/canvases.rs`
- session event / platform event contract（present 事件）
- `packages/app-web/src/features/workspace-panel` / `extension-runtime` / `canvas-panel`（present 接收）

## 建议验证

```powershell
cargo test -p agentdash-application runtime_gateway
cargo test -p agentdash-application workspace_module
cargo test -p agentdash-application canvas
cargo test -p agentdash-api extension_runtime
pnpm --filter @agentdash/app-web typecheck
pnpm e2e -- extension
pnpm e2e -- canvas
```

## Notes

- 复杂 child：`start` 前补 `design.md`（分支派发与 schema 校验时序）+ `implement.md`。
- 风险：invoke 不能退化成无 schema 万能 JSON 调用；describe 的 schema 与服务端校验必须成对。
