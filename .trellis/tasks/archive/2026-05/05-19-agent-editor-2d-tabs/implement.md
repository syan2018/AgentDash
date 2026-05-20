# Implement — Agent 配置面板二维分页重构

> 单文件改动：[packages/app-web/src/features/project/agent-preset-editor.tsx](packages/app-web/src/features/project/agent-preset-editor.tsx)。
> 调用方文件无需修改。

## 步骤

### 1. 重读 PRD/Design，对齐字段归属
- [ ] 已读 prd.md「既有 feature 完整盘点」表格
- [ ] 已读 design.md §2 组件划分、§3 UI 契约、§4 关键迁移点

### 2. 重构 PresetFormFields（主要工作）
- [ ] 在 `PresetFormFields` 函数内部新增 `activeTab` / `activeCapabilityKey` 两个 useState，默认值 `'basic'` / `'tool'`
- [ ] 删除现有 7 个 `<FormSection>` 排列，替换为：
  - 顶部 `<PrimaryTabBar>`（内联实现，不抽组件）
  - 下方按 `activeTab` 分支渲染三个 panel
- [ ] **基础 Tab**：把 Section 1（基本信息）+ Section 2（System Prompt）+ Section 3（执行器&模型）的 JSX 平铺渲染。每组之间用 `<div className="mt-4 border-t border-border/50 pt-4">` 视觉隔开，但**不再用手风琴**。
- [ ] **能力 Tab**：
  - 容器：`<div className="flex gap-3 sm:gap-4">`
  - 左侧 SecondaryNav（参考 design §3.2），4 项；窄屏（`sm:hidden`）改为顶部 chip 行
  - 右侧根据 `activeCapabilityKey` 渲染对应组件：
    - `tool` → `<ToolCapabilitiesField>`
    - `mcp` → `<McpPresetPicker>`
    - `skill` → `<SkillAssetPicker>`
    - `companion` → 内联渲染当前 [Section 5 L565-L611](packages/app-web/src/features/project/agent-preset-editor.tsx#L565-L611) 的 companion 列表 JSX，**只去掉顶部 `border-t border-border/50 pt-3`**
- [ ] **记忆 Tab**：渲染 `<KnowledgeSection>`；当 props 缺失时显示 `<EmptyState>`（如本文件无 EmptyState 则用一段 `<p className="text-xs text-muted-foreground">`）

### 3. PresetFormFields 新增透传 props
- [ ] 给 `PresetFormFields` 增加 4 个可选 props：`knowledgeEnabled?: boolean`, `onToggleKnowledge?: (b:boolean)=>void`, `knowledgeAgentId?: string`, `knowledgeLinkId?: string`（projectId 已有）

### 4. SinglePresetDialog 调整
- [ ] 在调用 `<PresetFormFields>` 处把 4 个 knowledge props 透传过去
- [ ] **删除** [L1333-L1342](packages/app-web/src/features/project/agent-preset-editor.tsx#L1333-L1342) 在 PresetFormFields 之后的 KnowledgeSection 内联渲染（避免双显示）
- [ ] Dialog 容器：`max-w-2xl` → `max-w-3xl`

### 5. 清理
- [ ] 删除 PRD 砍掉的「Section 4: 运行限制」空注释（[L550](packages/app-web/src/features/project/agent-preset-editor.tsx#L550)）
- [ ] 检查 import：`FormSection` 是否还有其它使用？若不再使用，删除其定义；若仍用于其它地方（搜索本文件），保留
- [ ] **不**删除 `ToolCapabilitiesField` / `McpPresetPicker` / `SkillAssetPicker` / `KnowledgeSection` / `presetToForm` / `formToPreset` / `validateForm`

### 6. 校验
- [ ] `pnpm --filter @agentdash/app-web typecheck`
- [ ] `pnpm --filter @agentdash/app-web lint --no-fix` 仅看修改文件 0 新增告警
- [ ] grep 自查：`Section 4` 注释已清除；KnowledgeSection 不存在双调用
- [ ] 把改动文件读一遍，确认没有未引用的 FormSection import 残留

### 7. 移交用户视觉验收
- [ ] 启动 dev server（如果未运行），告知用户可在 Agent 编辑入口验证
- [ ] **未获用户视觉验收前不 commit**

## 验证命令

```bash
# typecheck
pnpm --filter @agentdash/app-web typecheck

# lint（仅看本次修改文件）
pnpm --filter @agentdash/app-web lint

# 启动 dev（用户需要时）
pnpm --filter @agentdash/app-web dev
```

## 回滚锚点

- 实施前先确认 `git status` 仅 `.trellis/tasks/05-19-...` 一个 untracked 目录脏（已确认 prd 已写、design 已写）。
- 单文件改动：`git checkout -- packages/app-web/src/features/project/agent-preset-editor.tsx` 即回到改动前。

## 风险

- **风险 1**：`FormSection` 在文件外被 import → 已 grep 确认仅本文件内使用，删除 FormSection 调用后保留定义即可（不 export 不影响）。本任务**不**主动删除其定义，只是不再调用。
- **风险 2**：知识库 props 链路变化 → SinglePresetDialog 对 project-agent-view 的 props 契约不变，仅内部转发路径多一层；调用方零改动。
- **风险 3**：Companion 列表 JSX 嵌套较深，搬运时漏行 → 用整体复制策略，不分片，最大化保留语义。
