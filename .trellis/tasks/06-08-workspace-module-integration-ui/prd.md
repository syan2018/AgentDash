# Workspace Module 集成 review 与项目层管理 UI

> Parent: `06-08-agent-runtime-surface-registry`。本 child 是第三轮（收尾），依赖 Child 1（契约）与 Child 2（操作面）落地。
> 设计依据见 parent `design.md` §9（AgentFrame/capability 边界）、§10（权限与 schema）、D5（设置页消费同一 projection）。

## Goal

收束整条 Workspace Module 链路：统一 trace / 权限 / ContextFrame 表达，落地**项目设置页的 WorkspaceModule 合并管理 UI**（消费 Child 1 的 canonical projection），并完成 slug / 文档改名收尾。

## 已锁定决策（来自 parent）

- D5：项目层"Canvas + Extension 贡献的 WorkspaceModule 合并认知与管理"复用 Child 1 的同一份 projection，UI 不引入第二份 DTO。
- 命名收口：parent slug `...-surface-registry` 在本轮统一改为 `...-workspace-module-registry`，surface 仅保留为底层 runtime projection 命名。

## Requirements

- R1 项目设置页 UI：在 Agent/项目设置页统一列出 Canvas + Extension 贡献的 WorkspaceModule，呈现 kind / source / status / 可见性，支持项目层启停与裁切认知；数据来自 Child 1 projection。
- R2 可见性裁切落地：把 D4 的 AgentFrame 预留裁切字段接到 UI，裁切意图经 Capability 通道生效，UI 仅作为 capability 决策的呈现/编辑端，不形成第二套规则。
- R3 集成 review：统一 trace 字段、权限裁决展示、ContextFrame 中 workspace module 的表达；补 UI 诊断（present 失败、module unavailable 等）。
- R4 命名与文档收尾：slug 改名、`docs/extension-system.md` 等文档补 Workspace Module 章节、明确 Workspace Module vs Runtime Surface 边界。

## Acceptance Criteria

- [ ] 项目设置页能列出并管理 Canvas + Extension 贡献的 WorkspaceModule，数据复用 Child 1 projection（无第二份 DTO）。
- [ ] 可见性裁切在 UI 编辑后，经 Capability 通道在 Agent 侧生效（list/describe 反映裁切结果）。
- [ ] trace / 权限 / ContextFrame 中 workspace module 表达一致，present/unavailable 有 UI 诊断。
- [ ] slug 改名完成，文档明确 Workspace Module 与 Runtime Surface 术语边界。
- [ ] 前后端类型生成与 e2e 通过。

## 候选修改面

- `packages/app-web` 项目/Agent 设置页相关 feature
- `packages/app-web/src/features/extension-runtime` / `workspace-runtime`（projection 消费）
- session/platform event contract（诊断事件）
- `crates/agentdash-domain/src/workflow/agent_frame.rs` + capability 解析（裁切落地）
- `docs/extension-system.md` 及相关文档
- task slug 改名（parent + children 目录与 task.json）

## 建议验证

```powershell
pnpm --filter @agentdash/app-web typecheck
pnpm --filter @agentdash/app-web test
pnpm e2e -- extension
pnpm e2e -- canvas
cargo test -p agentdash-application capability
```

## Notes

- 含前端 UI，`start` 前建议补 `design.md`（设置页信息架构 + 裁切编辑数据流）。
- 本轮是集成 + 收尾，注意不要在此引入新业务事实源或绕过 Child 1 契约。
