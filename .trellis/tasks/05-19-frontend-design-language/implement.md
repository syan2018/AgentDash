# Implement · 前端设计语言收敛与通用 UI 共用化

PRD: [prd.md](prd.md) · Design: [design.md](design.md)

## 执行顺序总览

```
S1.  设计语言 spec 文档                    (低风险，先落字)
S2.  预览页骨架 + Tokens / Radius 段      (验收基础，提前建好框)
S3.  Token 调整 (input/button radius)      (改 styles.css，影响全表单)
S4.  扩展 Badge variant      ──┐
S5.  新增 OriginBadge          │
S6.  新增 InspectorRow         │  primitive 批，每步同步在预览页 Section 3 补展示
S7.  新增 StatusDot            │
S8.  新增 SectionTitle        ──┘
S9.  预览页 Section 4/5/6 (Surface demo / 嵌套对比 / Form 综合)
S10. ESLint 规则上线          (warn 级，不 block CI)
S11. PublishedBadge 迁移      ─┐
S12. SkillCategoryPanel 迁移  ─┘  示范迁移
S13. 验证 + spec index 收尾
```

每一步保持独立 commit，便于回滚。S3 单独成 commit（视觉影响最大）。

## 步骤详情

### S1. 设计语言 spec 文档

**目标**：[.trellis/spec/frontend/design-language.md](../../spec/frontend/design-language.md) 落地，并写入 [.trellis/spec/frontend/index.md](../../spec/frontend/index.md)。

**内容大纲**（参 design.md §2/§3 + PRD R1）：

1. **Surface 层级**：depth 0/1/2 三层定义、最大嵌套 2、二选一规则。
2. **Radius**：`xs=4 / sm=6 / md=8 / lg=12` token 表 + 适用组件。
3. **状态色 vs 品牌色**：状态走 Badge/StatusDot，品牌走 OriginBadge；禁直接写 tailwind 字面色。
4. **Primitive 强制位**：badge / dot / inspector row / section title / form field / button / card / dialog 入口必须用 `@agentdash/ui`。
5. **Forbidden patterns**：列出 ESLint regex 与示例。
6. 例外条款：模板字符串、markdown 渲染、第三方组件穿透。
7. **预览页参考**：`/dev/design-system` 是本 spec 的可视化参考，新增 primitive / token 必须同步更新预览页。

**变更**：
- 新增 `.trellis/spec/frontend/design-language.md`
- 修改 `.trellis/spec/frontend/index.md` 增加索引项

### S2. 预览页骨架 + Tokens / Radius 段

**目标**：`/dev/design-system` 路由打通，前两段（Section 1 Tokens、Section 2 Radius）先落地。Section 3 的 primitive 区以"待补"占位。

**变更**：

- 新建 [packages/app-web/src/pages/DesignSystemPage.tsx](../../../packages/app-web/src/pages/DesignSystemPage.tsx)：
  - 顶部 anchor 导航（6 个段标题）
  - Section 1 Tokens：12 色 swatch 网格 + 局部 dark toggle
  - Section 2 Radius：4 矩形 + 适用组件标注
  - Section 3-6：占位 `<section>` 含 TODO 标注
- 修改 [packages/app-web/src/App.tsx](../../../packages/app-web/src/App.tsx)：
  - lazy import `DesignSystemPage`
  - 在 `<Routes>` 顶部加 `<Route path="/dev/design-system" element={<DesignSystemPage />} />`，与 `WorkspaceLayout` 路由组平级

**验证**：
- `pnpm --filter app-web dev`
- 浏览器访问 `http://localhost:<port>/dev/design-system`，前两段可见

### S3. Token 调整

**目标**：[packages/ui/src/styles.css](../../../packages/ui/src/styles.css) 把 input/button radius 落到 8px。

**变更**（参 design.md §5）：

```diff
- .agentdash-form-input,
- .agentdash-form-select,
- .agentdash-form-textarea {
-   ...
-   border-radius: 0.75rem;
+   border-radius: 0.5rem;
- }

- .agentdash-button-secondary,
- .agentdash-button-primary,
- .agentdash-button-danger {
-   ...
-   border-radius: 0.625rem;
+   border-radius: 0.5rem;
- }
```

**验证**：
- `pnpm --filter @agentdash/ui typecheck`
- 在 `/dev/design-system` Section 2 的 Radius 8px 矩形与 Section 6 form 控件圆角一致（Section 6 此时仍是占位，先验证 token 改动构建通过）。

### S4. Badge variant 扩展

**目标**：[packages/ui/src/primitives/Badge.tsx](../../../packages/ui/src/primitives/Badge.tsx) 新增 `info / accent`。

```diff
-export type BadgeVariant = 'neutral' | 'primary' | 'success' | 'warning' | 'danger'
+export type BadgeVariant =
+  | 'neutral' | 'primary'
+  | 'success' | 'warning' | 'danger'
+  | 'info' | 'accent'

 const variantClass: Record<BadgeVariant, string> = {
   ...,
+  info:   'border-info/25 bg-info/10 text-info',
+  accent: 'border-violet-500/25 bg-violet-500/10 text-violet-600 dark:text-violet-300',
 }
```

**预览页同步**：Section 3 - Badge 子区落地（7 variant 横排，每个带短文字示例）。

**验证**：
- `pnpm --filter @agentdash/ui typecheck`
- 浏览器访问预览页，Badge 全部 variant 显示正常

### S5. OriginBadge

**目标**：新建 [packages/ui/src/primitives/OriginBadge.tsx](../../../packages/ui/src/primitives/OriginBadge.tsx) + 在 [index.ts](../../../packages/ui/src/index.ts) 导出。参 design.md §4.2。

**预览页同步**：Section 3 - OriginBadge 子区（6 origin × 是否带 subText 示例）。

**验证**：
- `pnpm --filter @agentdash/ui typecheck`
- 预览页 OriginBadge 段视觉正常

### S6. InspectorRow

**目标**：新建 [packages/ui/src/primitives/InspectorRow.tsx](../../../packages/ui/src/primitives/InspectorRow.tsx) + 导出。参 design.md §4.3。

**预览页同步**：Section 3 - InspectorRow 子区（多 tone × mono / 普通示例）。

### S7. StatusDot

**目标**：新建 [packages/ui/src/primitives/StatusDot.tsx](../../../packages/ui/src/primitives/StatusDot.tsx) + 导出。参 design.md §4.4。

**预览页同步**：Section 3 - StatusDot 子区（5 tone × 2 size + pulse 动画示例）。

### S8. SectionTitle

**目标**：新建 [packages/ui/src/primitives/SectionTitle.tsx](../../../packages/ui/src/primitives/SectionTitle.tsx) + 导出。参 design.md §4.5。

**预览页同步**：Section 3 - SectionTitle 子区（默认 / sticky / 含 actions / 含 badge 四态）。

### S9. 预览页 Section 4/5/6 收尾

**目标**：补完预览页剩余三段。

- **Section 4 · Surface depth demo**：3 个并排示例（depth-1 合法 / depth-2 合法 / 错误反例）。
- **Section 5 · 嵌套对比**：左右两栏，左为旧 4 层嵌套样式（在页面内重建一份 mock），右为新扁平化（用 SectionTitle + InspectorRow）。
- **Section 6 · Form 综合**：模拟 Skill 编辑表单（display_name / key / description / disable-model-invocation / textarea / 提交）+ Dialog 内嵌套表单。

**验证**：
- 全 6 段在 `/dev/design-system` 完整可见
- 用户初步视觉评估：input radius=8 是否接受，状态色对比是否合理

### S10. ESLint 规则上线

**目标**：[packages/app-web/eslint.config.js](../../../packages/app-web/eslint.config.js) 增加 `no-restricted-syntax` 规则。

**变更**（参 design.md §6）：

在 `defineConfig` 数组追加：

```js
{
  files: ['src/**/*.{ts,tsx}'],
  rules: {
    'no-restricted-syntax': [
      'warn',
      {
        selector: "JSXAttribute[name.name='className'] Literal[value=/\\b(bg|text|border)-(red|emerald|amber|sky|violet|orange|blue|green|rose|yellow|teal|cyan|fuchsia|pink|indigo)-\\d+(\\/\\d+)?\\b/]",
        message: "禁止 Tailwind 字面色。状态色用 <Badge variant=\"...\"> / <StatusDot tone=\"...\"> / <OriginBadge>，参考 .trellis/spec/frontend/design-language.md",
      },
      {
        selector: "JSXAttribute[name.name='className'] Literal[value=/\\brounded-\\[(?!4px|6px|8px|12px)\\d+px\\]/]",
        message: "禁止非 sentinel 字面圆角，仅允许 4 / 6 / 8 / 12 px。参考 .trellis/spec/frontend/design-language.md",
      },
    ],
  },
},
```

**验证**：
- `pnpm --filter app-web lint 2>&1 | grep -c "no-restricted-syntax"` — 应有数十至上百条 warning（存量违反点）
- 预览页 `DesignSystemPage.tsx` warn=0（页面内只用 primitive，不应有字面色／字面圆角）
- 不 block CI

### S11. PublishedBadge 迁移

**目标**：[packages/app-web/src/features/assets-panel/_shared/PublishedBadge.tsx](../../../packages/app-web/src/features/assets-panel/_shared/PublishedBadge.tsx) 全文改写为使用 `<Badge variant="accent">`。参 design.md §7.1。

**验证**：
- `pnpm --filter app-web typecheck`
- `pnpm --filter app-web lint <path>` 该文件 0 warning
- 预览页 Badge 段与生产页面 PublishedBadge 渲染视觉一致

### S12. SkillCategoryPanel 全文迁移

**目标**：[packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx](../../../packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx) 1134 行，按 design.md §7.2 表格分段处理。

**分段**（每段独立 commit）：

1. `import { Badge, OriginBadge, InspectorRow, SectionTitle } from '@agentdash/ui'`
2. 删除本地 ORIGIN_STYLE / OriginBadge 实现，改 import
3. 卡片 meta tags（file count / explicit only / imported / marketplace）改 Badge variant
4. SkillVfsInspector 的 InspectorTitleBar 改 SectionTitle
5. 删除本地 InspectorRow helper，改 import
6. YAML meta panel 改 fieldset，去 nested card
7. 保存 meta 按钮：先看 Button primitive 是否能复用（如不能，留 inline 但不字面色：用 success token）

**验证**（每个 commit 后）：
- `pnpm --filter app-web typecheck`
- 末尾 commit 后 `pnpm --filter app-web lint <path>` warn=0

### S13. 验证 + spec index 收尾

**步骤**：

1. 全量验证：
   - `pnpm --filter @agentdash/ui typecheck`
   - `pnpm --filter app-web typecheck`
   - `pnpm --filter app-web build`
   - `pnpm --filter app-web lint`（统计 warn 数；SkillCategoryPanel.tsx + PublishedBadge.tsx + DesignSystemPage.tsx 应为 0）
2. 视觉验收：启动 dev server，逐项核对：
   - **首先访问** `/dev/design-system`：6 段全展示、token 颜色 / radius / 所有 primitive 视觉与 spec 一致
   - 切回业务页面：Skill 列表卡 origin badge / PublishedBadge / 编辑抽屉 inspector / 任意表单 input·button 圆角变化
   - 对照 design-system 页面 Section 5 嵌套对比与生产 SkillCategoryPanel 是否一致
3. 等用户视觉验收通过后，再 git commit 完成稿。
4. 后续子任务清单已在 design.md §11 记录；不在本任务创建实际 task。

## 验证命令汇总

```bash
# 类型
pnpm --filter @agentdash/ui typecheck
pnpm --filter app-web typecheck

# Lint
pnpm --filter app-web lint
pnpm --filter app-web lint packages/app-web/src/features/assets-panel/_shared/PublishedBadge.tsx
pnpm --filter app-web lint packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx
pnpm --filter app-web lint packages/app-web/src/pages/DesignSystemPage.tsx

# 构建
pnpm --filter app-web build

# 启动调试
pnpm --filter app-web dev
# 然后浏览器打开 http://localhost:<port>/dev/design-system
```

## 风险与回滚点

| 步骤 | 风险 | 回滚 |
|------|------|------|
| S2 | 预览页路由与 AuthGate 冲突 | 改为放在 `<AuthGate>` 之外（无需登录即可访问），权衡后决定 |
| S3 | input/button 视觉收紧后用户不接受 | `git revert` 该 commit；spec 改为标注 `md-form=10` 例外；预览页 Section 6 立即反馈 |
| S4 | accent variant 字面色被 lint 命中 | Badge.tsx 内 `// eslint-disable-next-line` 标注 |
| S10 | warn 噪音爆炸阻塞日常开发 | 缩小 files glob 到 `src/features/**`；或直接 revert lint 规则 |
| S12 | SkillCategoryPanel 1134 行迁移引入 regression | 分段 commit；每段 typecheck；最末一段 + 视觉对比 |

## 完成标志

PRD §Acceptance Criteria 9 项（含预览页）全部勾选 + 用户视觉验收通过 + 后续子任务清单已落入 design.md §11。

## 提交策略

- 每个 S 步骤独立 commit。
- S3 (token 调整) 单独 commit 以便 `git revert` 不连带 primitive 改动。
- S12 内部按子段分 commit。
- **commit 前等用户视觉验收**（参 [feedback_no_commit_until_approved](C:\Users\yihao.liao\.claude\projects\d--ABCTools-Dev-AgentDashboard\memory\feedback_no_commit_until_approved.md)）。
