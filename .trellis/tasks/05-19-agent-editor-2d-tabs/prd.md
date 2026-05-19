# Agent 配置面板二维分页重构

## Goal

把 `SinglePresetDialog` 当前的 7 段纵向手风琴 + 顶层附加的 VFS 知识库块，重构为「横向 Tab + 必要时纵向 Sidebar」的二维分页布局，提升信息密度与可扩展性，**只迁移既有 feature，不新增任何配置项**。

## Background

当前 [agent-preset-editor.tsx](packages/app-web/src/features/project/agent-preset-editor.tsx) 存在三处密度问题：

1. 身份/运行/能力三类性质完全不同的配置共用一条纵轴（[L335-L638](packages/app-web/src/features/project/agent-preset-editor.tsx#L335-L638)），找一项需滚动 + 展开手风琴两次。
2. MCP / Skills / Companion 都是「列表选择 + 搜索」形态，挤在窄手风琴里看不到必要元数据。
3. 知识库（`knowledge_enabled` + VfsBrowser）当前游离在 PresetForm 之外，挂在 Dialog 顶层（[L1333-L1342](packages/app-web/src/features/project/agent-preset-editor.tsx#L1333-L1342)），与其它配置不在同一个心智模型下。

参考图给的是四 Tab + 二级 Sidebar，但**不直接照抄**：
- 参考图把"Sub Agent 编排"独立成一栏 → 本项目 `allowed_companions` 就是 SubAgent，不能重复编排。
- 参考图的"示例与模板"在本项目无对应 feature → 不创建。
- 参考图的"运行限制"在本项目无字段填充（Section 4 是空壳）→ 不保留位置。

## Scope

### 既有 feature 完整盘点（必须全部迁移）

| 来源 | 字段 | 形态 |
|---|---|---|
| `PresetFormState.name` | 预设名称 (key) | input |
| `PresetFormState.display_name` | 显示名称 | input |
| `PresetFormState.description` | 描述 | textarea |
| `PresetFormState.system_prompt` | System Prompt 内容 | textarea |
| `PresetFormState.system_prompt_mode` | append / override | pill toggle |
| `PresetFormState.agent_type` | Agent 类型 | select |
| `PresetFormState.model_id` (+`provider_id`) | 模型 | select with optgroup |
| `PresetFormState.thinking_level` | 推理级别 | select |
| `PresetFormState.agent_id` | Agent | select / input |
| `PresetFormState.permission_policy` | 权限策略 | select |
| `PresetFormState.capability_directives` | 工具能力（pill + extended switches） | `ToolCapabilitiesField` |
| `PresetFormState.allowed_companions` | Companion / SubAgent 白名单 | switch list |
| `PresetFormState.mcp_preset_keys` | MCP Presets | `McpPresetPicker` |
| `PresetFormState.skill_asset_keys` | Skills | `SkillAssetPicker` |
| `ProjectAgentLink.knowledge_enabled` | 知识库启用 | switch |
| `VfsBrowser` (mount=`agent-knowledge`) | 知识库文件浏览 | embedded browser |

### 信息架构（一级 Tab + 二级 Sidebar）

```
[ 基础 ] [ 能力 ] [ 记忆 ]   ← 一级 Tab，3 个

基础 Tab（一级即铺开，无二级）
  ├─ 身份：name / display_name / description
  ├─ System Prompt：system_prompt + mode
  └─ 运行：agent_type / model + thinking_level / agent_id / permission_policy

能力 Tab（左侧二级 Sidebar，右侧大空间）
  ├─ 工具能力          ← capability_directives
  ├─ MCP               ← mcp_preset_keys
  ├─ Skills            ← skill_asset_keys
  └─ Companion         ← allowed_companions（命名沿用项目术语「Companion Agents」）

记忆 Tab（一级即铺开，无二级）
  └─ 知识库：knowledge_enabled + VfsBrowser
```

**为什么是 3 Tab 而非 4 Tab**：
- 参考图的"可见性与权限"在本项目里只有 `permission_policy` 一项 select，单立 Tab 内容空 → 并入"基础/运行"。
- 参考图的"示例与模板"无对应 feature → 不创建。
- 二级 Sidebar 仅在「能力」Tab 出现，因为只有这 4 个 picker 形态的子模块需要切换；其它 Tab 内容量小，强加二级反而增点击层级。

### 不在范围内（明确不做）

- ❌ 不新增任何配置字段（包括 sandbox / 运行限制 / 示例模板 / 跨项目可见性等参考图里有但本项目没有的）。
- ❌ 不修改 `PresetFormState`、`presetToForm`、`formToPreset` 的 schema。
- ❌ 不修改 `AgentPresetConfig` 类型或后端协议。
- ❌ 不动 `McpPresetPicker` / `SkillAssetPicker` / `ToolCapabilitiesField` / `VfsBrowser` 的内部逻辑，仅做容器迁移。
- ❌ 不改 SinglePresetDialog 的 props 契约（`knowledgeEnabled` / `onToggleKnowledge` / `knowledge*Id` 全部保留）。

## Acceptance Criteria

### 功能完整性
- [ ] 既有 16 项配置字段全部可在新布局中编辑，行为与重构前完全一致（含 append/override pill、工具能力的 basic+extended 两段式、MCP/Skill/Companion 的 picker 状态）。
- [ ] 知识库开关 + VFS 浏览器从 Dialog 顶层迁入「记忆」Tab，但 `SinglePresetDialog` 对外 props 保持不变。
- [ ] `presetToForm` / `formToPreset` 行为零变更（同一份 PresetFormState 进出仍得到等价 AgentPreset）。

### 交互
- [ ] 一级 Tab 切换：默认停留在「基础」，已编辑未保存内容在切 Tab 时不丢失（form state 在 Dialog 层）。
- [ ] 二级 Sidebar（能力 Tab 内）：默认停留在「工具能力」；切换不丢草稿。
- [ ] Tab 上显示「未保存指示点」？—— **不做**。当前实现就没这个 feature，不在迁移范围。
- [ ] Tab 上显示数量 badge：保留既有 FormSection 的 badge 逻辑（MCP `${n} 个` / Skills `${n} 个` / 工具能力 `${n}/${total}` / Companion 已选数）。

### 视觉与适配
- [ ] 沿用现有 design tokens（`agentdash-form-*` / `agentdash-button-*` / 自定义 pill 等），不引入新颜色或字体。
- [ ] Dialog 宽度从 `max-w-2xl` 上调以容纳左侧 Sidebar；具体值在 design.md 决定。
- [ ] 在 Dialog 容器最大宽度内（约 < 720px）需要降级：二级 Sidebar 折叠成 Tab 内的横向 chip 行。窄到无 Sidebar 也可以。
- [ ] 一级 Tab 始终为顶部水平条，即使在窄屏也不变形。

### 验收命令
- [ ] `pnpm --filter @agentdash/app-web typecheck` 通过。
- [ ] `pnpm --filter @agentdash/app-web lint` 对修改文件无新增告警。
- [ ] 视觉/交互由用户在浏览器中验收（用户偏好）；未获用户视觉确认前不 commit。

## Open Questions

均已在 Scope 中收敛，无未决项。

## 实际交付（在原 Scope 基础上的演进）

实施过程中根据用户反馈做了三轮打磨，超出原 PRD 范围但仍守"只迁既有 feature"红线：

1. **MCP / Skill 列表 → 卡片**：列表样式信息密度低，改为 2 列卡片（含 display name / mono key / description / 标签芯片 / 自绘选中态 checkmark），整张卡片可点击。
2. **抽通用 `CapabilityCard / CapabilityZone / CapabilityPicker`**：MCP / Skill / Companion 三处共用同一份模板，通过 `itemKey` + `itemToCardProps` 注入差异，消除三段重复布局。Companion 从 switch 列表对齐为同款卡片。
3. **双区交互（已启用 / 可添加）**：每个 picker 上下分两区，已选项在上、未选项在下；点击对应区卡片即添加/移除。MCP 的"+ 创建"占位卡作为"可添加"末尾的虚线卡。
4. **文件拆分**：原单文件 1669 行拆成 9 个子模块（form-state / capability-picker / tool-capabilities-field / mcp-preset-picker / skill-asset-picker / knowledge-section / preset-form-fields / single-preset-dialog / agent-preset-list-editor + index barrel）。`from "./agent-preset-editor"` 导入路径不变，调用方零改动。
5. **Dialog 宽度**：从 `max-w-2xl` (672px) 演进到 `max-w-[min(1080px,75vw)]`，视口越宽留白越多，cap 1080px 防止上限失控。

未做：
- 搜索 / 过滤、未保存指示点、Tab deeplink、auto-jump 错误 Tab —— 既有 feature 集合中没有，不在迁移范围。
- 不新增配置字段（`project_container_ids` / sandbox / 示例模板等参考图里有但项目里无 UI 的全部跳过）。
