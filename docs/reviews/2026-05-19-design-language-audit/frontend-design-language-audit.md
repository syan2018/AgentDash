# AgentDash 前端设计语言审计 · 2026-05-19

> 调研性质，不引入任何代码改动。结论需要用户拍板后再选择执行路径（建 Trellis 任务、写 spec、还是分批改组件）。

## 0. 触发背景

Skill 编辑页右侧 inspector 改造期间发现：同一个抽屉内，从外到内出现了 **4 层** 矩形装饰（外壳 → section 卡 → row box → input border），视觉上严重拥挤。猜测是项目级问题，需要全局盘点而不是一处一处改。

样本对比（改造前后） · [SkillCategoryPanel.tsx:739](packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx#L739)：

```
改造前: aside.p-4 ─ section.border.bg-bg.p-3 ─ label.border.bg-secondary/20.px-3.py-2 ─ input[border]
        └─ Panel.bg-secondary/10                                                      └─ pre.border.bg-secondary/20

改造后: aside ─ sticky-header ─ space-y-5 / space-y-3 / space-y-1.5 ─ input | pre.border-only
```

---

## 1. 现状摸底

### 1.1 已有 token（[packages/ui/src/styles.css](packages/ui/src/styles.css)）

| 类别 | token | 关键属性 |
|------|-------|---------|
| 颜色 | `--background` `--foreground` `--card` `--popover` `--primary` `--secondary` `--muted` `--accent` `--destructive` `--warning` `--success` `--info` + 各自 `-foreground` | HSL 变量，dark 模式镜像 |
| 字体 | `--font-sans` Inter Variable / `--font-mono` JetBrains Mono | |
| 表单 | `.agentdash-form-label` (11px uppercase, letter-spacing 0.14em) | |
| 表单 | `.agentdash-form-input` `.agentdash-form-select` `.agentdash-form-textarea` | radius **12px** (0.75rem)，padding 0.625rem/0.875rem |
| 按钮 | `.agentdash-button-primary` `.agentdash-button-secondary` `.agentdash-button-danger` | radius **10px** (0.625rem) |
| 文件 pill | `.agentdash-file-pill` `-badge` `-label` `-remove` | radius 6.4px |
| 内容标签 | `.agentdash-panel-header-tag` | radius 8px，uppercase 11px |
| Markdown | `.agentdash-markdown` `.agentdash-chat-markdown` `.agentdash-chat-code-block` | radius 7.2px |

### 1.2 现状空缺

token 系统**只覆盖到原子控件**（input、button、pill）。没有以下层级：

- **Surface（容器层）token**：Dialog / Panel / Section / Row 该用什么背景、什么 border、什么 radius 没有约定。
- **Status tag（状态色）token**：成功、警告、信息、危险、中性的 badge / chip 没有可复用类，都是手写 `border-X-500/30 bg-X-500/10 text-X-700`。
- **Origin / Tone 调色板**：origin badge（builtin/user/github/clawhub/skills_sh）、思考级别、权限策略 — 各自字面色，互相没关系。
- **Inspector / Detail 行**：`InspectorRow` / `DetailSection` 在好几个文件被各自重新发明。
- **Radius 刻度**：实际出现的字面值有 **4 / 6 / 7 / 8 / 10 / 12 px**，没有刻度共识；token 内部已经在 6.4 / 7.2 / 8 / 10 / 12 间漂移。

### 1.3 核心结论

> **是 token 不够用，不是设计师没想清楚。** 写组件的人遇到容器层、状态层、列表行这些 token 没覆盖的位置，只能自由组合 Tailwind atoms — 这就是嵌套层数失控、字面色四处开花的根因。

---

## 2. 问题盘点（按类别）

> Explore agent 全仓扫描 + 抽样 verify。下方类别按"修起来 ROI 最高"排序。

### 2.1 `nested-card` 多层卡片嵌套（最影响视觉）

| 文件 | 位置 | 形态 |
|------|------|------|
| [SkillCategoryPanel.tsx:849](packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx#L849) | YAML meta panel | section.border.bg-secondary/20 内嵌 input + textarea + checkbox-row + pre |
| [task-drawer.tsx:209-240](packages/app-web/src/features/task/task-drawer.tsx#L209-L240) | Agent 绑定 + Story 上下文 | `.rounded-[12px].border.bg-bg` 套 `.rounded-[10px].border.bg-secondary/25` 套 `.rounded-full.border.bg-bg`（**3 层圆角**） |
| [SkillCategoryPanel.tsx:739-805](packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx#L739-L805) | VFS Inspector（已修） | 旧版 4 层，已扁平化 |
| [story-detail-panels.tsx:260-277](packages/app-web/src/features/story/story-detail-panels.tsx#L260-L277) | Context Containers Editor | section + 内部 editor 重复包装 |

**症状**：外壳已经定义了 surface（白底 + border），内层 section / row 又叠一次 surface，看起来"控件浮在卡片上、卡片浮在卡片上"。

### 2.2 `color-literal` 状态色字面量爆炸

| 文件 | 位置 | 形态 |
|------|------|------|
| [PublishedBadge.tsx:4](packages/app-web/src/features/assets-panel/_shared/PublishedBadge.tsx#L4) | "已发布 v{x}" | violet-500/30 + violet-500/10 + violet-700 (+ dark:violet-300) |
| [SkillCategoryPanel.tsx:363-397](packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx#L363-L397) | ORIGIN_STYLE map | 5 套：builtin/user(violet)/github(sky)/clawhub(emerald)/skills_sh(orange) |
| [project-agent-view.tsx:651,685](packages/app-web/src/features/project/project-agent-view.tsx#L651) | 思考级别 badge | `border-amber-400/30 bg-amber-500/8 text-amber-600` ← **400 vs 500 不一致** |
| [project-agent-view.tsx:691-699](packages/app-web/src/features/project/project-agent-view.tsx#L691-L699) | 权限策略 | AUTO=emerald, SUPERVISED=blue, 其他=secondary |
| [routine-tab-view.tsx:379,710](packages/app-web/src/features/routine/routine-tab-view.tsx#L379) | EXEC_STATUS_STYLE | amber/sky/... |
| [MarketplaceAssetDrawer.tsx:85,651](packages/app-web/src/features/assets-panel/categories/MarketplaceAssetDrawer.tsx#L85) | "已安装" badge | amber-500/30 + amber-500/10；另一处 amber-400/30 + amber-500/8 |
| [SkillCategoryPanel.tsx:531-543](packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx#L531-L543) | "explicit only" / "imported" badges | amber, secondary mixed |

**症状**：`amber-400/30` vs `amber-500/8` 这种细差完全是手抖型 typo，没有 token 兜底就会一直发生。

### 2.3 `radius-mix` 圆角刻度混用

实际出现的字面圆角（仅看 .tsx 内）：`4px / 6px / 7px / 8px / 10px / 12px`。

| 文件 | 混用值 |
|------|--------|
| [TabBar.tsx](packages/app-web/src/features/workspace-panel/TabBar.tsx) | 7px(tab) + 4px(close) |
| [SkillCategoryPanel.tsx](packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx) | 4 / 6 / 7 / 8 同时存在 |
| [task-drawer.tsx](packages/app-web/src/features/task/task-drawer.tsx) | 10 / 12 + rounded-full |
| [vfs-browser.tsx](packages/app-web/src/features/vfs/vfs-browser.tsx) | 6 / 8 |

token 自身也漂移：file-pill 6.4px、markdown 7.2px、button 10px、input 12px、panel-tag 8px。

### 2.4 `row-box` 单行控件被独立卡片包装

| 文件 | 位置 | 形态 |
|------|------|------|
| [SkillCategoryPanel.tsx:874](packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx#L874) | disable-model-invocation checkbox row（YAML meta 表单分支） | 单 checkbox 套 `rounded-[7px] border bg-bg px-3 py-2` |
| [story-detail-panels.tsx:359-402](packages/app-web/src/features/story/story-detail-panels.tsx#L359-L402) | 添加文本表单 | inline form 套 `rounded-[8px] border bg-bg/80` |

**症状**：单行控件不需要"卡片"语义，加 box 反而让信息密度变低、和其他 section 抢视觉。

### 2.5 `pre-double-bg` 代码块在已有背景上再叠一层

| 文件 | 位置 | 形态 |
|------|------|------|
| [SkillCategoryPanel.tsx:882](packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx#L882) | YAML preview pre | section bg-secondary/20 + pre bg-bg + pre border |
| [SectionRenderers.tsx:485](packages/app-web/src/features/session/ui/contextFrame/SectionRenderers.tsx#L485) | 上下文片段 pre | bg-secondary/20，与其他 pre bg-background 不一致 |

### 2.6 `bespoke-inspector-row` 同样的"label/value 对"在多处被各自重新写

InspectorRow 类的小组件至少在 3 个地方被发明：
- [SkillCategoryPanel.tsx:820](packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx#L820) `InspectorRow`
- [vfs-browser.tsx:233-278](packages/app-web/src/features/vfs/vfs-browser.tsx#L233-L278) `MountSummaryItem`（compact / 非 compact 双形态）
- [project-agent-view.tsx:672-680](packages/app-web/src/features/project/project-agent-view.tsx#L672) executor / model id pill

每处差一点点 padding、字号、圆角。

### 2.7 重复阴影 / 半透明色叠加

`bg-secondary/10` `bg-secondary/20` `bg-secondary/25` `bg-secondary/30` `bg-secondary/40` `bg-secondary/60` 全都出现过。没有"二级面 / 三级面"的清晰分层语义。

---

## 3. 设计语言提案

### 3.1 三大原则

1. **每多一层视觉装饰，必须挣到自己的存在权**。一层颜色 + border = 一份"我和外界不是一回事"的承诺。同等含义的内容只允许一层。
2. **token 优先于 atom**。如果一个组合 atom 在 ≥2 处出现且语义一致，立刻提到 utility class；不允许"再写一次"。
3. **状态色走语义，不走色名**。代码里禁止出现 `amber-` `violet-` `emerald-` `sky-` `orange-` `blue-` 字面，只允许 `success` `warning` `danger` `info` `accent` 等语义 token，或登记到 origin 调色板。

### 3.2 圆角刻度（建议收敛）

```
--radius-xs: 4px   一段文字 chip / icon button
--radius-sm: 6px   pill / badge / tag / segmented control
--radius-md: 8px   行内卡片 / inspector row / pre-block
--radius-lg: 12px  Panel / Section / Dialog
--radius-pill: 9999px
```

**操作**：把现有 7px 全部归 6 或 8；10px 归 12；新代码禁用字面圆角，改为 `rounded-md` 等 token。input radius 从 12px 降到 8px（和 button 共用 md）— 这条要拍板，因为会影响整个表单观感。

### 3.3 容器层级（Surface）

定义"深度" depth-0 / 1 / 2，最多到 depth-2。每深一层只允许有 **一种** 视觉提示（不能 border + bg + shadow 同时再叠）：

```
depth-0  Page background          bg-background, no border
depth-1  Panel / Dialog / Card    bg-card, border-border, rounded-lg, shadow?
depth-2  Section / Inspector row  bg-transparent, border-t/border-border/40, no extra bg
                                 （或 bg-secondary/40 但取消 border）
```

> **二选一规则**：depth-2 想标记"我是子区"，**只能**用一个 visual cue：要么轻分隔线，要么微弱底色，**不能两个都来**。

新增 utility（提议）：

```css
.agentdash-surface       /* depth-1 容器 */
.agentdash-section       /* depth-2 子分区，仅 border-t */
.agentdash-section-tinted /* depth-2 子分区，仅 bg-secondary/40 */
.agentdash-divider        /* 水平分隔线，比 border 更克制 */
```

### 3.4 Status Tag（替代 color-literal）

把 PublishedBadge / 思考级别 / 权限策略 / origin badge 全部归一：

```css
.agentdash-tag                           /* base：6px radius, 10px font, uppercase optional */
.agentdash-tag--neutral                  /* secondary 色（默认） */
.agentdash-tag--info                     /* primary 色 */
.agentdash-tag--success                  /* success token */
.agentdash-tag--warning                  /* warning token */
.agentdash-tag--danger                   /* destructive token */
.agentdash-tag--accent                   /* 当前的 violet 已发布等 */
```

origin（builtin/user/github/clawhub/skills_sh/marketplace）放进单独的 `--origin-{name}` 数据驱动表，定义在 css，组件只传 `data-origin="github"`。这样 typo（amber-400 vs 500）从源头消除。

### 3.5 InspectorRow / DetailRow 抽公共组件

提议新增 `packages/ui/src/components/inspector-row.tsx`：

```ts
<InspectorRow label="path" value={path} mono />
<InspectorRow label="size" value={size} />
<InspectorRow label="mode" value={mode} tone="muted" />
```

替换 SkillCategoryPanel / vfs-browser / project-agent-view 三处重复实现。

### 3.6 表单分组的"反卡片"化

**结论**：表单内分组用空白 + label 而不是盒子；只有跨语义边界（不同实体、不同操作目标）才用 surface。

操作示例：

```diff
- <section className="space-y-3 rounded-[8px] border border-border bg-secondary/20 p-3">
-   <p className="agentdash-form-label">YAML meta</p>
-   ...inputs...
- </section>
+ <fieldset className="space-y-3">
+   <legend className="agentdash-form-label">YAML meta</legend>
+   ...inputs...
+ </fieldset>
```

---

## 4. 落地路径（推荐顺序）

> 按 ROI 从高到低，每一步可以独立成 PR。

| # | 动作 | 影响面 | 估时 | 是否可逆 |
|---|------|--------|------|---------|
| 1 | 把本文落到 `.trellis/spec/frontend/design-language.md` 作为约束（含禁用色字面、圆角刻度） | 0 视觉，纯文档 | S | 可逆 |
| 2 | 在 `packages/ui/src/styles.css` 新增 `.agentdash-tag*` 系列 token | 仅扩 token | S | 可逆 |
| 3 | 替换 PublishedBadge / OriginBadge / 思考级别 / 权限策略 / EXEC_STATUS_STYLE 为 tag token | 视觉一致化 | M | 可逆 |
| 4 | 在 `packages/ui` 提取 `InspectorRow`，替换 3 处重复实现 | 重构无新功能 | M | 可逆 |
| 5 | task-drawer.tsx / story-detail-panels.tsx 的 nested-card 扁平化（参照本次 SkillVfsInspector） | 视觉收敛 | M | 可逆 |
| 6 | 圆角刻度收敛（7→6/8, 10→12, 4→6） | 全仓 sweep | L | **不完全可逆**，需要 lint 规则保护 |
| 7 | 新增 ESLint / Tailwind 规则禁止 `bg-{color}-{500}/N`、`rounded-[\d+px]` 字面，强制走 token | 防回归 | M | 可逆 |

**建议从 #1 + #2 + #3 + #7 起步**：成本可控、ROI 最高、防回归。#4/#5/#6 视后续观感再启动。

---

## 5. 待你拍板的决策点

| # | 议题 | 选项 |
|---|------|------|
| D1 | 圆角刻度 | (a) 4 档 xs/sm/md/lg；(b) 3 档 sm/md/lg 合并；(c) 维持现状 |
| D2 | input radius 是否从 12 降到 8 | 影响 ~30 个表单的观感 |
| D3 | 字面色字面圆角是否上 lint 阻挡 | 强制 vs 仅 spec 提示 |
| D4 | 是否抽 `packages/ui/src/components/` 公共组件（InspectorRow、StatusTag、SectionTitle） | yes / no / 暂缓 |
| D5 | 落地批次 | 一次 Trellis 大任务 / 拆 5-7 个小任务 / 写完 spec 后随用随改 |

---

## 6. 附：本次已就地修复的样本

[SkillCategoryPanel.tsx:739-836](packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx#L739-L836) — `SkillVfsInspector` 扁平化：

- 去 3 个 section 的 border + bg-background + p-3 卡片外壳
- disable-model-invocation 拆掉 row-box
- pre 不叠 bg-secondary，改单层细边
- 新增 sticky `InspectorTitleBar`，保存按钮上提到顶栏，状态文案合并（"已同步 / 保存 meta / 保存中…"）
- 删除原 `InspectorHeader` helper

可作为后续按 #5 推进时的样板对照。
