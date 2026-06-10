# 现状能力模型 — 事实落盘（三轮 Explore 汇总）

> 本文是设计的地基。所有引用均为本任务起点时的代码事实（main @ 65d0b1ab + 当前分支起点）。

## 1. 三层数据流总览

```
ProjectAgent.config (AgentPresetConfig)        ← 声明式真值（用户编辑）
   │  merge_field! = override.or(base)
   ▼
帧构建 (AgentFrameBuilder.build)               ← 各字段各自的 merge 规则（散乱）
   │
   ▼
AgentFrame.*_json (持久化快照, 按 revision)     ← effective_capability_json + 旁路 visible_*_json
   │  project_capability_state_from_frame / build_envelope_from_frame
   ▼
CapabilityState (运行时, spi)                   ← tool/companion/vfs/skill/workspace_module
   │  intersect() 裁切；tools 按维度过滤
   ▼
工具可见性 / VFS mount / companion 名单
```

## 2. 已存在的「事件溯源能力原语」（半成型！）

### SPI 契约层 `crates/agentdash-spi/src/session_persistence.rs`
- `CapabilityDimensionKey(String)` — 维度标识。L101-113
- `CapabilityArtifactSource { kind: String }` — 来源标识，工厂 `workflow()` / `permission_grant()`。L115-133
- `CapabilityDeclarationRecord { dimension, declaration_type, source, payload }` — 声明。L135-142
- `CapabilityContributionRecord { dimension, contribution_type, source, payload }` — **定义了但全程未用**。L144-151
- `RuntimeCapabilityEffectRecord { dimension, effect_type, payload }` — 效果 diff。L153-159
- `RuntimeCapabilityTransition { declarations: Vec<..>, effects: Vec<..> }` — 一次转移。L161-168
- `PendingCapabilityStateTransition` / `AgentFrameTransitionRecord` — transition 的**持久化载体**（带 run_id/lifecycle_key/phase_node）。L43-70
- 常量：`CAPABILITY_DIMENSION_{TOOL,MCP,COMPANION,VFS}`（**无 skill / workspace_module**）；`DECLARATION_TYPE_{CAPABILITY_DIRECTIVE,MOUNT_OPERATION}`；`EFFECT_TYPE_{SET_TOOL_ACCESS,SET_MCP_SERVER_SET,SET_COMPANION_AGENT_ROSTER,APPLY_VFS_OVERLAY,APPLY_MOUNT_OPERATIONS}`。L170-214

### Application 引擎层 `crates/agentdash-application/src/session/capability_state.rs`
- trait `CapabilityDimensionModule`：`key()` / `validate_declaration()` / `compile_declaration()`(默认 None) / `validate_effect()` / `replay_effect()` / `normalize_projection()`。L259-293
- `CapabilityDimensionRegistry::built_in()` 注册：**Vfs / Tool / Mcp / Companion** 四个模块（**无 skill / workspace_module**）。L295-321
- 重放引擎 `replay_transition()` L368-385；`replay_runtime_capability_transition(s)` L701-739；`apply_runtime_capability_transition()` L227-233。
- 投影：`project_capability_state_from_frame(frame)` L48-72（读 `effective_capability_json` 反序列化为 CapabilityState，再覆盖 vfs/mcp surface）；`capability_state_to_frame_surfaces(state)` L80-94（state → effective_capability_json / vfs_surface_json / mcp_surface_json）。

## 3. 各维度参与度（核心 slop 定位）

| 维度 | Declaration | Effect | replay 改写 | 现状 | 真值来源 |
|---|---|---|---|---|---|
| **tool** | ✓ capability_directive (Add/Remove) | ✓ set_tool_access (replace) | tool.{capabilities,enabled_clusters,tool_policy} | **canonical** | preset.capability_directives + permission grant |
| **vfs** | ✓ mount_operation | ✓ apply_vfs_overlay / apply_mount_operations (累积) | vfs.active | **canonical** | preset.vfs_access_grants |
| **mcp** | ✗ | ✓ set_server_set (replace) | tool.mcp_servers | **canonical(仅 effect)** | preset.mcp_preset_keys |
| **companion** | ✗ | effect 类型存在但**无人产生** | companion.agents | **旁路**：resolver.rs 直接赋值 | preset.allowed_companions |
| **skill** | ✗ | ✗ | skill.skills | **旁路**：frame_builder 纯 carry-forward 继承 | preset.skill_asset_keys |
| **workspace_module** | ✗ | ✗ | workspace_module | **旁路**：frame_construction 从 visible_workspace_module_refs_json 直接赋值 | preset.visible_workspace_module_refs |

## 4. 帧构建各字段 merge 规则（散乱，frame_builder.rs build() L220-276）

- `effective_capability_json` / `context_slice_json` / `vfs_surface_json` / `mcp_surface_json` / `execution_profile_json`：`self.X.or_else(|| current.X)` —— 有新值用新值，否则继承上一 revision。
- `visible_canvas_mount_ids_json`：纯 carry-forward（`current.as_ref().and_then(...)`），运行时 append（累积语义）。
- `visible_workspace_module_refs_json`：**混合 match 臂**（非空→`append_*`；空/None→carry-forward 上一版）。L259-270。**bug 根因**。

## 5. 三态被抹平的位置

- composer_project_agent.rs / composer_story.rs：所有 preset 字段 `.clone().unwrap_or_default()` —— `None`(未声明) 与 `Some([])`(显式清空) 合并为同一空 Vec。
- frame_builder `with_visible_workspace_module_refs`：`if refs.is_empty() { None } else { Some(refs) }` —— 再次把空折叠成 None，落入 carry-forward 臂。
- 但持久化层 `parse_opt_json` 区分 DB NULL→None vs `"[]"`→Some(Array)，三态在存储层是**保得住的**，只是上游提前抹平了。

## 6. workspace_module 运行时还原（旁路证据）

`crates/agentdash-application/src/workflow/frame_construction/mod.rs` L363-374：
```rust
let visible_module_refs = frame.visible_workspace_module_refs();
capability_state.workspace_module = if visible_module_refs.is_empty() {
    WorkspaceModuleDimension::default()          // mode = All
} else {
    WorkspaceModuleDimension { mode: Allowlist, allowed_module_ids: visible_module_refs }
};
```
对比 tool 维度：经 `effective_capability_json` 反序列化（canonical）。**workspace_module 是唯一不走 effective_capability_json 的可见性维度。**
- `WorkspaceModuleDimension { mode: All|Allowlist, allowed_module_ids }`，`allows(id)` 按 mode 判定（spi mod.rs L274-304）。**mode 字段本身就是三态载体**：All=无限制（清空态），Allowlist+items=受限。
- 工具过滤：workspace_module_list/describe/invoke/present 经 `resolve_visible_modules` → `visibility.allows()`（tools.rs L40-62）。

## 7. canvas 可见性（累积语义参照）

`crates/agentdash-application/src/canvas/visibility.rs` `append_visible_canvas_mounts` L14-37：从 `visible_canvas_mount_ids_json` 读出 → 过滤 project canvas → append 进 Vfs.mounts。纯累积，删除走别处。**这是用户口中"会积累的 grant 型 modifier"的真实样本。**

## 8. 设计支点（结论）

1. 原语**不需新发明**：`RuntimeCapabilityTransition`(declarations/effects) + `CapabilityDimensionKey`/`CapabilityArtifactSource` 就是「更新原语」与「标识原语」，只是半成型 + 三个维度绕过。
2. base vs modifier 已天然分层：**base = effective_capability_json（声明式投影）**，**modifier = transition 重放（运行时累积）**。问题是 skill/companion/workspace_module 没进这套，各搞旁路。
3. 缺一个**显式 AccumulationPolicy** 让「replace / accumulate / ephemeral」成为维度的声明属性，而非每个 module 硬编码。
4. workspace_module bug 的 canonical 解：声明式 allowlist 走 base CapabilityState.workspace_module（经 effective_capability_json + 标准 or_else），mode 字段天然承载三态；退役 visible_workspace_module_refs_json 旁路与 frame_construction 直接赋值。
