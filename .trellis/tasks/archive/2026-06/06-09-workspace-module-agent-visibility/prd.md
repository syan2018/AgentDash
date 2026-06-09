# ProjectAgent Workspace Module 可见性配置

> Parent: `06-08-workspace-module-registry`（第 4 个 child）。承接 D4：把 Child 1 预留、Child 3 持久化的 workspace module 可见性裁切补上**配置入口**，闭合链路。改动并入 PR #45（分支 feat/workspace-module-registry）。

## Goal

把 workspace module 可见性 allowlist 作为 `AgentPresetConfig` 的第 6 个能力维度，加到 ProjectAgent 配置页编辑，经 frame construction 流入 `AgentFrame.visible_workspace_module_refs_json` → `CapabilityState.WorkspaceModuleDimension` → 工具过滤。事实源是 ProjectAgent 定义，frame 只承派生。

## 背景事实（探索确认）

- ProjectAgent 配置页 [project-agent-view.tsx](packages/app-web/src/features/project/project-agent-view.tsx) 已统一编辑 5 个能力维度（tools/MCP/VFS/skills/companions），全部经 `AgentPresetConfig`。
- `AgentPresetConfig`（[agent_config.rs](crates/agentdash-domain/src/common/agent_config.rs)）当前**无** workspace module 可见性字段；有 `merge_field!` 字段级合并 macro 需同步。
- frame construction [mod.rs:364](crates/agentdash-application/src/workflow/frame_construction/mod.rs#L364) 已读 `frame.visible_workspace_module_refs()` 组装 capability 维度，但**没有任何地方从 agent 配置去填该 frame 字段** → allowlist 实际恒为空（默认 all）。本 child 补这个填充。
- Child 3 已建 `GET /projects/{id}/workspace-modules`，前端 picker 可直接消费。

## 需求

- R1 `AgentPresetConfig` 新增 `visible_workspace_module_refs: Option<Vec<String>>`，纳入 serde 与 `merge_field!` 合并。
- R2 frame construction 从 preset_config 填 `AgentFrame.visible_workspace_module_refs_json`（镜像 `visible_canvas_mount_ids` 的 compose 路径），不再仅运行时追加。空 → All；非空 → Allowlist。
- R3 contracts：ProjectAgent preset 的 create/update 契约带新字段，ts 生成同步。
- R4 前端：`PresetFormState` 加字段 + agent-preset-editor 新增 WorkspaceModuleVisibilityPicker 区块，消费 `/projects/{id}/workspace-modules` 列出可选 module，多选 module_id 写回 config。
- R5 默认行为不变：未配置 = 全集可见（不破坏现有 agent）。

## 验收标准

- [ ] 在 ProjectAgent 配置页可勾选该 agent 可见的 workspace modules，保存到 ProjectAgent.config。
- [ ] 新建/编辑后，frame construction 把所选 refs 写入 AgentFrame，运行时 `workspace_module_list` 仅返回 allowlist 内 module；未配置时返回全集。
- [ ] `merge_field!` 包含新字段（模板/项目实例/运行态合并一致）。
- [ ] contracts:check + app-web typecheck + 相关 cargo test 通过。
- [ ] 不引入第二份 module DTO；picker 复用生成类型与 Child 3 路由。

## 边界

- 不改 Child 1/2/3 已定的工具行为与 projection 形状。
- 可见性粒度到 module_id（不做 per-operation 裁切）。
- 不做 per-session 临时覆盖（事实源是 agent 定义）。
