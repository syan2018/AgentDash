# Design · ProjectAgent Workspace Module 可见性配置

> Parent design §9（capability 通道）。承接 Child 1（WorkspaceModuleDimension）/ Child 3（DB 列 + 路由）。事实源 = ProjectAgent，frame 派生。

## 1. 数据流（目标闭环）

```
ProjectAgent.config.visible_workspace_module_refs   (用户配置，事实源)
  → frame_construction 读取 preset_config
  → AgentFrame.visible_workspace_module_refs_json   (Child 1 字段 / Child 3 DB 列)
  → frame.visible_workspace_module_refs()
  → CapabilityState.WorkspaceModuleDimension {All|Allowlist}   (mod.rs:364 已实现)
  → workspace_module_list/describe/invoke 过滤   (Child 1/2 已实现)
```

本 child 只补**头两段**：config 字段 + construction 填充。后段已通。

## 2. 后端 domain（agent_config.rs）

- `AgentPresetConfig` 新增：
  ```rust
  #[serde(skip_serializing_if = "Option::is_none")]
  pub visible_workspace_module_refs: Option<Vec<String>>,
  ```
- 同步 `merge_field!` 调用，加入该字段（override.or(base) 语义与其它维度一致）。
- 若有 `to_agent_config()` / 其它构造点遗漏字段编译会报，逐个补。

## 3. frame construction（frame_construction/mod.rs 附近）

- 找到 `visible_canvas_mount_ids` 从 agent context 写入 AgentFrame 的 compose 点（探索指 mod.rs:364-374 区域 + construction_planner / session assembler `compose_owner_bootstrap_to_frame`）。
- 镜像它：从 `preset_config.visible_workspace_module_refs` 取值 → 写入新建 frame 的 `visible_workspace_module_refs_json`（用 AgentFrame 现有 setter/append 或构造参数）。
- 语义：`None`/空 → 不设（frame 字段空 → 下游 All）；非空 → 写入 refs（下游 Allowlist）。**不改下游** `mod.rs:364` 的解析逻辑。
- module_id 形如 `ext:{key}` / `canvas:{mount_id}`（Child 1 约定），config 存的就是这些 id。

## 4. contracts（project-agent preset）

- ProjectAgent preset 的 create/update DTO（对应 `project-agent-contracts.ts` 的 `CreateProjectAgentRequest` / `UpdateProjectAgentRequest` 或其内嵌 preset config 类型）加 `visible_workspace_module_refs: Option<Vec<String>>`。
- `generate_ts` 同步；`contracts:check` 通过。
- 注意：preset config 在后端是 `serde_json::Value` 透传存储（agent entity.config），契约层若已是结构化 DTO 则加字段；若透传则确认前端类型与 AgentPresetConfig 对齐即可。

## 5. 前端（agent-preset-editor）

- `PresetFormState`（form-state.ts）加 `visible_workspace_module_refs: string[]`；form ↔ config 双向转换补该字段（默认空数组 = 不裁切）。
- preset-form-fields.tsx 新增区块 **Workspace Module Visibility**：
  - 复用 Child 3 的数据：`useProjectWorkspaceModules(projectId)` 或 `fetchProjectWorkspaceModules` 列出当前项目 workspace modules（kind/title/module_id）。
  - 多选 checkbox/picker（模板可参考 SkillAssetPicker / McpPresetPicker），勾选写 module_id 数组。
  - 空选 = "全部可见"（显式提示），非空 = 仅勾选项。
- 用生成类型 `workspace-module-contracts.ts`，不手写 module 类型。

## 6. 边界 / 不做

- 粒度到 module_id；不做 per-operation。
- 不做 per-session 覆盖。
- 不动 Child 1/2/3 的工具/projection/路由形状。

## 7. 验收对应

| 验收 | 落点 |
|---|---|
| 配置页可勾选并保存到 config | §4 contracts + §5 前端 |
| frame 写入 + 运行时过滤生效；未配置=全集 | §3 construction（下游 §1 已通） |
| merge_field 含新字段 | §2 |
| 类型生成/typecheck/test 通过 | 全量 |
| 复用生成类型 + Child 3 路由 | §5 |
