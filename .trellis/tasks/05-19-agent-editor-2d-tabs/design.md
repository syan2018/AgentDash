# Design — Agent 配置面板二维分页重构

## 1. 影响范围

| 文件 | 改动 |
|---|---|
| [packages/app-web/src/features/project/agent-preset-editor.tsx](packages/app-web/src/features/project/agent-preset-editor.tsx) | 主战场。重构 `PresetFormFields`，把 7 个 FormSection 重排到新的 Tab 容器；调整 `SinglePresetDialog` 让其把知识库下沉到「记忆」Tab。 |

**不改动**：
- `AgentPreset` / `AgentPresetConfig` / `PresetFormState` 类型与转换函数
- `presetToForm` / `formToPreset` / `validateForm`
- `McpPresetPicker` / `SkillAssetPicker` / `ToolCapabilitiesField` / `KnowledgeSection` 内部逻辑（`KnowledgeSection` 仅调整调用位置）
- `SinglePresetDialog` 的 props 契约（11 项 props 全部保留含义不变）
- 调用方 [project-agent-view.tsx:843-872](packages/app-web/src/features/project/project-agent-view.tsx) 不需改一行

## 2. 组件划分

```
SinglePresetDialog
└─ <PresetFormFields>                       ← 新增：负责 Tab/Sidebar 布局，state 仍由 Dialog 持有
   ├─ <PrimaryTabBar>                       ← 一级 Tab（基础 / 能力 / 记忆）
   └─ panel: 基础            ← 直接渲染 BasicPanel（无二级）
   │   └─ <BasicPanel>
   │       ├─ 身份组 (name/display_name/description)
   │       ├─ System Prompt 组
   │       └─ 运行组 (agent_type/model/thinking/agent_id/permission_policy)
   │
   ├─ panel: 能力            ← 渲染 SecondaryNav + 内容区
   │   └─ <CapabilityPanel>
   │       ├─ <SecondaryNav>            ← 左侧二级（工具能力/MCP/Skills/Companion）
   │       └─ 右侧动态内容：
   │           - <ToolCapabilitiesField>      ← 既有组件
   │           - <McpPresetPicker>            ← 既有组件
   │           - <SkillAssetPicker>           ← 既有组件
   │           - <CompanionPicker>            ← 把当前 Section 5 内联的 companion 列表抽出（仅搬运，不改逻辑）
   │
   └─ panel: 记忆            ← 直接渲染 MemoryPanel
       └─ <KnowledgeSection>            ← 既有组件，搬位置
```

### 内部状态

- `activeTab: 'basic' | 'capability' | 'memory'`，`useState`，默认 `'basic'`。
- `activeCapabilityKey: 'tool' | 'mcp' | 'skill' | 'companion'`，`useState`，默认 `'tool'`。
- 两个状态均放在 `PresetFormFields` 内（局部 UI state，不影响 form），切 Tab 不丢草稿因为 form state 在父组件 Dialog。

## 3. UI 契约（最小自实现，不引入 primitive）

> 沿用 [SettingsPage.ScopeTabs](packages/app-web/src/pages/SettingsPage.tsx#L1828-L1858) 已有的 pill 风格，避免造新 primitive。

### 3.1 一级 PrimaryTabBar

```tsx
<div className="flex gap-1.5 border-b border-border px-1 pb-2">
  {tabs.map(t => (
    <button
      role="tab"
      aria-selected={active === t.key}
      onClick={() => setActive(t.key)}
      className={cx(
        "rounded-[8px] px-3 py-1.5 text-xs font-medium transition-colors duration-160",
        active === t.key
          ? "bg-primary/10 text-foreground border border-primary/30"
          : "text-muted-foreground hover:bg-secondary hover:text-foreground border border-transparent",
      )}
    >
      {t.label}
      {t.badge && <span className="ml-1.5 text-[10px] text-muted-foreground/70">{t.badge}</span>}
    </button>
  ))}
</div>
```

- **Tab 数量**：3（基础 / 能力 / 记忆）。
- **Badge 来源**：
  - 基础：无（必填项均在内）
  - 能力：4 个子模块的 badge 之和（仅展示已选/已勾选项数总和，例如 "3"），如全为 0 则不显示
  - 记忆：`knowledgeEnabled` 为 true 时显示一个圆点（用 `inline-block size-1.5 rounded-full bg-primary`）

### 3.2 二级 SecondaryNav（仅「能力」Tab）

```tsx
<aside className="w-[140px] shrink-0 border-r border-border/70 pr-2">
  <ul className="space-y-0.5">
    {capabilityKeys.map(k => (
      <li>
        <button
          className={cx(
            "w-full text-left rounded-[8px] px-2.5 py-1.5 text-xs",
            activeKey === k.key
              ? "bg-secondary/60 text-foreground"
              : "text-muted-foreground hover:bg-secondary/30 hover:text-foreground"
          )}
        >
          {k.label}
          {k.badge && <span className="float-right text-[10px] text-muted-foreground/60">{k.badge}</span>}
        </button>
      </li>
    ))}
  </ul>
</aside>
```

- **Badge 复用既有 FormSection 逻辑**：
  - 工具能力：`${capability_directives.length}/${CAPABILITY_OPTIONS.length}`，length=0 时显示 `全部`（沿用 `ToolCapabilitiesField` 的语义：空数组=全开）
  - MCP：`${mcp_preset_keys.length}` 个（0 时不显示）
  - Skills：`${skill_asset_keys.length}` 个（0 时不显示）
  - Companion：`${allowed_companions.length}/${companionCount}`（数组为空表示全部，显示 `全部`；若 companionCount=0 则该项整条置灰禁用）

### 3.3 Dialog 宽度调整

- 当前 `max-w-2xl`（≈ 672px）→ 改为 `max-w-3xl`（≈ 768px），仅在「能力」Tab 内出现 Sidebar，单元格仍在 Dialog 容器内。
- 对话框 `max-h-[70vh]` overflow-y 保持。
- 窄屏（容器 < 640px）通过 `sm:` 断点降级：SecondaryNav 改为 chip 行（横向 pill 同款），不显示 `aside`。

## 4. 关键迁移点

### 4.1 工具能力 vs Companion 拆分

当前 [Section 5 「工具 & 协作」](packages/app-web/src/features/project/agent-preset-editor.tsx#L552-L612) 把 `ToolCapabilitiesField` 和 companion 列表塞在同一个 FormSection 里。重构后分别落在二级 Sidebar 的两个独立项。**逻辑不变**，仅容器变更：

- 工具能力：直接渲染 `<ToolCapabilitiesField>`，无 `border-t pt-3` 分隔（独立面板不需要）。
- Companion：把现有 [L565-L611](packages/app-web/src/features/project/agent-preset-editor.tsx#L565-L611) 里的 JSX 整体搬到一个新的内联渲染块（不抽组件、避免膨胀），唯一调整是去掉顶部的 `border-t pt-3`。当 `companionCount === 0` 时该 Sidebar 项置灰且面板内显示空状态文案 `"项目内暂无其它 Agent 可作为 companion"`。

### 4.2 知识库下沉

- 删除 [SinglePresetDialog L1333-L1342](packages/app-web/src/features/project/agent-preset-editor.tsx#L1333-L1342) 在 `PresetFormFields` 之后的内联 KnowledgeSection 渲染。
- 把 `KnowledgeSection` 调用搬入 `PresetFormFields` 的「记忆」Tab。`PresetFormFields` 增加四个透传 props：`knowledgeEnabled / onToggleKnowledge / knowledgeAgentId / knowledgeLinkId`（projectId 已存在）。
- 「记忆」Tab 在 `knowledgeEnabled === undefined || onToggleKnowledge === undefined` 时显示 `<EmptyState>` 风格提示 `"该 Agent 尚未在项目中链接，知识库需要 ProjectAgentLink"`。**这是既有行为的延续** —— 当前逻辑就是 props 缺失时整段不渲染，迁移后我们让 Tab 永远存在但内容为空提示，避免 Tab 数量动态变化。

### 4.3 验证错误显示位置

当前 [validationError 在 Dialog 底部](packages/app-web/src/features/project/agent-preset-editor.tsx#L1344-L1346)。重构后：
- 错误内容里若提及具体字段（如 "name 重复"），不主动跳转 Tab —— 简单实现即可，错误依然在 Dialog 底部。
- 因为校验只发生在点「保存」时，用户主动操作后看到错误就近修改即可。**不做** auto-tab-jump 逻辑（无既有需求）。

## 5. 数据流不变性证明

```
Dialog state (form: PresetFormState)
    │
    └─→ PresetFormFields(form, patchForm, ...)
            │
            ├─→ BasicPanel    → patchForm({...})    （同当前）
            ├─→ CapabilityPanel
            │       ├─→ ToolCapabilitiesField  → onChange → patchForm（同当前）
            │       ├─→ McpPresetPicker        → onChange → patchForm（同当前）
            │       ├─→ SkillAssetPicker       → onChange → patchForm（同当前）
            │       └─→ Companion 列表         → patchForm（同当前）
            └─→ MemoryPanel   → onToggleKnowledge / 透传给 KnowledgeSection（同当前）
```

`patchForm` 全程是同一个 ref。Tab 切换不卸载子树（用 conditional render 而非 mount/unmount 控制？答：用 conditional render `{activeTab === 'basic' && <BasicPanel ... />}`，因为 `form` 由父级保持，被切走 Tab 内的 input 状态完全在 form 里，没有未提升的本地 state，**所以 unmount/remount 不会丢草稿**）。

## 6. 不做项

- 不在 `packages/ui/primitives/` 新增 Tab/Sidebar primitive。这是单点使用，且既有 SettingsPage 也没抽，避免污染设计系统。
- 不动 `agentdash-form-*` 工具类。
- 不引入路由级 deeplink（不放 URL hash，因为 Dialog 是模态，本身不应可链接）。
- 不做未保存指示点（既有 feature 集合中没有）。
- 不动 `validateForm` / 字段顺序在持久化时的语义。

## 7. 兼容性 & 回滚

- 无后端改动；纯前端单文件级变更。
- 回滚策略：单文件 revert 即可（`git revert <commit>` on agent-preset-editor.tsx）。
- 类型不变 → 不影响其它使用 `PresetFormState` / `presetToForm` / `formToPreset` 的导入方（Grep 确认仅 `project-agent-view.tsx` 使用 SinglePresetDialog；PresetFormState 工具函数仅在文件内部）。
