# Design — Child A: 能力原语 + workspace_module 收口（含前端）

> Parent design: [../06-09-capability-update-primitives/design.md](../06-09-capability-update-primitives/design.md)（决策 DB/DE、§5 三态、§7 数据流）。事实基础：[../06-09-capability-update-primitives/research/01-current-capability-model.md](../06-09-capability-update-primitives/research/01-current-capability-model.md)。

## 0. 原语层（A 部分，纯加性，先落）

- `AccumulationPolicy { Replace, Accumulate, Ephemeral }` 放 `agentdash-spi/src/session_persistence.rs`，与 `CapabilityDimensionKey`/`CapabilityArtifactSource` 同层。
- 补 `CAPABILITY_DIMENSION_SKILL` / `CAPABILITY_DIMENSION_WORKSPACE_MODULE` 常量、`CapabilityArtifactSource::preset()`。
- `CapabilityDimensionModule` trait 加 `fn policy(&self) -> AccumulationPolicy`（无默认实现，强制各 module 声明）；tool/mcp/companion=Replace、vfs=Accumulate。
- 此部分不改任何重放/帧构建行为，单测仅验证 `policy()` 返回值。
- 实现顺序建议：先 A（编译通过、行为不变）→ 再 B（行为切换）→ 再 C（前端）。

## 核心改动(B):从"旁路字段"切到"base 维度"

### 现状(旁路)
```
preset.visible_workspace_module_refs (Option<Vec<String>>)
  → composer .unwrap_or_default()                       // 抹平 None/空
  → AgentFrameBuilder.with_visible_workspace_module_refs // if empty {None} else {Some}
  → build() 混合 match 臂                                // 非空 append / 空-或-None carry-forward ← BUG
  → AgentFrame.visible_workspace_module_refs_json
  → frame_construction L363-374 直接赋值 CapabilityState.workspace_module
```

### 目标(base 收口)
```
preset.visible_workspace_module_refs (Option<Vec<String>>, 三态保真)
  → resolver/base 投影: None|Some([]) → WorkspaceModuleDimension{mode:All}
                        Some([..])    → WorkspaceModuleDimension{mode:Allowlist, allowed_module_ids}
  → 写入 base CapabilityState.workspace_module
  → capability_state_to_frame_surfaces → effective_capability_json
  → project_capability_state_from_frame → 还原(workspace_module 随 CapabilityState 一并反序列化)
```

为何这样就修了 bug：base 经 effective_capability_json **每 revision 由 config 重新投影**（标准 `or_else` 仅对 Unspecified 继承上一版整体 CapabilityState），workspace_module 维度的值始终来自当前 config 的 mode 计算——`Cleared`(Some([])) 自然产出 `mode=All`，不再走 carry-forward 把旧名单捞回。

## 具体编辑点（实现时核对行号，代码可能漂移）

1. **投影函数**（application）：在 base CapabilityState 组装处（resolver 或 capability_state 投影链）写入 `workspace_module` 维度。确认 workspace_module 进入 `effective_capability_json`（CapabilityState 已含该字段，`#[serde(default)]`，无需改序列化）。
2. **删除** `frame_construction/mod.rs` L363-374 的 `frame.visible_workspace_module_refs()` → workspace_module 直接赋值块（base 已带该维度，不再二次覆盖）。
3. **删除** `frame_builder.rs`：
   - 字段 `visible_workspace_module_refs: Option<Vec<String>>`（L~85）；
   - `with_visible_workspace_module_refs`（L104-110）；
   - build() 混合 match 臂（L259-270）。
   - `AgentFrame.visible_workspace_module_refs_json` 不再被 builder 写入（保持 None）。
4. **composer**（composer_project_agent.rs / composer_story.rs）：移除 workspace_module 路径的 `.unwrap_or_default()`，改将 `Option<Vec<String>>` 三态传入 base 投影（决策 DE）。其余 unwrap_or_default 字段不在本 child 范围（Child 3 处理 skill）。
5. **assembly_builder.rs / assembler.rs**：移除 `with_visible_workspace_module_refs` 调用与 `SessionAssemblyBuilder.visible_workspace_module_refs` 字段、`project_assembly_to_frame` 中相关传递；workspace_module 经 capability_state 投影链流转。
6. **install.rs**（shared_library）：template 安装处对 workspace_module 字段的 None 设定随之调整（若 builder 字段删除则该调用一并清理）。

## 持久化

- `visible_workspace_module_refs_json` 列**保留**（DA/DB：零迁移）。本 child 后写入逻辑删除→列恒为 NULL。`FrameRow` / SELECT / INSERT 保留该列（仍读写 NULL），避免动 SQL 结构与 migration guard。
- doc 注释更新：标注该列为"运行时 Accumulate grant 预留，当前不写入；声明式可见性已迁至 effective_capability_json"。
- 旧 frame 反序列化：effective_capability_json 中无 workspace_module → `#[serde(default)]` → `mode=All`，向后兼容（旧 agent 默认全部可见，与旧行为一致）。

## 测试

- 单测（capability_state / frame 投影层）：
  - `project_to_base_workspace_module_all_when_none`
  - `project_to_base_workspace_module_all_when_empty`（**Cleared→All，bug 回归锁**）
  - `project_to_base_workspace_module_allowlist_when_set`
- 集成/帧构建：set→clear→All；set→保持→Allowlist；新 agent→All。
- 既有 workspace_module 工具过滤测试（list/describe/invoke/present）保持绿。

## 前端层（C）

- `agent-preset-editor/form-state.ts`：workspace_module 字段清空时，`formToPreset` 写显式空（`Some([])`）而非省略字段——确保后端收到 `Cleared`→`mode=All`，而非 `Unspecified`。
- `workspace-module-visibility-picker.tsx`：三态展示——未配置/清空=「全部可见」，勾选=受限名单；辅助文字仅留"清空=全部可见"这类副作用说明（[[feedback_no_ui_helper_text]]）。
- 复用 PR #45 已建的 picker/store/hook 基础设施，仅改清空写回与三态呈现，不重建。
- 契约：`AccumulationPolicy` 默认仅后端用，不强行上前端；若导出则走 ts_rs `export_all` + `pnpm contracts:check`。

## 风险与回滚

- 风险：base 投影链若漏掉某入口（story vs project_agent composer），会导致某路径 workspace_module 丢失→退化 All（偏宽松，非越权，可接受但需测试覆盖两路 composer）。
- 前端风险：`formToPreset` 清空写 `Some([])` 需确认 serde 反序列化端把 `[]` 当 `Some(vec![])` 而非 `None`（contracts 层 `Option<Vec<String>>` + 空数组）；测试覆盖。
- 回滚：恢复 frame_construction 直接赋值 + builder 臂；列未删，数据通路可还原。
