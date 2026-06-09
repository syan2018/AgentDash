# 实现微调记录：前端「清空写显式 []」不采纳

## design.md §前端层(C) 原计划
`formToPreset` 清空 workspace_module 时写显式空 `Some([])` 而非省略字段，以区分 `Unspecified` 与 `Cleared`。

## 实际决定：保持现有「空→省略字段(None)」，不写显式 []

### 理由
1. **后端修复后 None 已正确**：workspace_module 属 `Replace` 策略，base 每 revision 由 config 重新投影；`project_workspace_module_dimension(None)` → `mode=All`。清空→省略→`None`→`mode=All`（全部可见），bug 已在后端根除，前端无需区分 None/空。
2. **form-state 无法区分三态**：`PresetFormState.visible_workspace_module_refs: string[]` 已把三态压成数组，无法区分"用户从未设置"与"用户清空"。若一律写 `[]`，会给**所有**不使用该特性的 preset 的 config 注入空数组，改变 merge_over 语义且污染配置。
3. **前端早已正确**：`workspace-module-visibility-picker` 的 hint 本就写明"清空后回到默认（全部可见）"；之前是后端 carry-forward 食言，现在后端兑现。

### 残留边界（已知，非本期 bug）
若未来允许在**base/template preset** 上设置 `visible_workspace_module_refs`，则 child override 清空(None)会经 `merge_over`(`override.or(base)`) 继承 base 名单——无法清空。当前 `shared_library/install.rs` 安装模板时该字段为 None，无此场景。若将来需要，应让 form-state 升级为真三态（`string[] | undefined` + 显式 toggle），而非无脑写 `[]`。已在 docs 标注。

## 验证
- `pnpm contracts:check` 通过（AccumulationPolicy 仅后端，AgentPresetConfig 未变，无漂移）。
- `pnpm --filter app-web typecheck` 通过。
- 前端零代码改动，仅 docs/extension-system.md 更新可见性裁切与能力原语说明。
