# Design Language

> AgentDashboard 前端设计语言：token、surface、primitive 与编码约束。

可视化基准：`/dev/design-system`（DesignSystemPage）。
任务沿革：`05-19-frontend-design-language`（PRD/design/implement 在 .trellis/tasks/）。

---

## 1. 总则（编码前必读）

1. **不写字面色**。颜色一律走语义 token：`primary / secondary / muted / accent / destructive / warning / success / info / border / foreground / background / card / popover / ring`。Tailwind 调色板类（`bg-violet-500`、`text-emerald-600` 等）只在 OriginBadge 等语义化 primitive 内部允许，业务代码不应再写。
2. **不写任意半径**。半径只用四档：`rounded-[4px]` / `rounded-[6px]` / `rounded-[8px]`（默认 md） / `rounded-[12px]`（lg，仅用于对话框、卡片大容器）。`rounded-full` 仅在 StatusDot / Avatar 等极少数场景使用。
3. **优先用 @agentdash/ui primitive**。同一形态在两个以上文件出现就抽进 primitive，不在业务文件里复制布局。
4. **嵌套二选一**。同一区域不同时叠加边框 + 浅底色背景 + 圆角；选 *只描边* 或 *只填底* 之一。

`packages/app-web/eslint.config.js` 启用了 warn 级 `no-restricted-syntax`，对字面色 / 任意半径 / `rounded-xl|2xl|3xl|full` 给出提示。新代码应当 0 warning。

---

## 2. 颜色 Token

颜色定义在 `packages/ui/src/styles.css`，HSL CSS 变量 + Tailwind v4 `@theme inline`。

| Token | 用途 | 备注 |
|-------|------|------|
| `background` / `foreground` | 页面底 / 正文 | depth-0 |
| `card` / `card-foreground` | 卡片底 | depth-1 |
| `popover` / `popover-foreground` | 弹层底 | 含 Dialog / Menu |
| `primary` / `primary-foreground` | 品牌 / 主操作 | 蓝调，饱和度克制 |
| `secondary` / `secondary-foreground` | 中性表面 | depth-2 / 次操作 |
| `muted` / `muted-foreground` | 次要文字 / 弱底 | |
| `accent` / `accent-foreground` | 强调 | 暂为中性白；marketplace/published 在 Badge `accent` variant 中用 violet（已局部隔离） |
| `destructive` | 危险动作 / 错误 | |
| `warning` | 待审 / 注意 | 暖橙 |
| `success` | 已完成 / 已同步 | |
| `info` | 信息提示 | 与 primary 同色相，语义不同 |
| `border` / `input` / `ring` | 描边 / 输入框 / 焦点 | |

**饱和度策略**：所有交互色取中等饱和度（H 74±2%、L 50±5%），避免强彩；亮/暗模式镜像同样克制。

---

## 3. Radius

四档：

| 档位 | 值 | 用法 |
|------|----|------|
| `xs` | 4px | meta tag、内部小标签 |
| `sm` | 6px | OriginBadge、status pill |
| `md` | 8px | **默认**：input、button、card、dialog 子区块 |
| `lg` | 12px | 顶层 dialog、popover 大容器 |

token utility 类（`.agentdash-form-input`、`.agentdash-button-*`）已统一为 8px；primitive Button/Card/TextInput/... 同步。

---

## 4. Surface 深度

层数严格三档，避免再嵌套：

| Depth | 背景 | 描边 | 典型场景 |
|-------|------|------|----------|
| 0 | `background` | 无 | 页面外层 / 空状态 |
| 1 | `card` | `border-border` | 卡片、对话框、面板主体 |
| 2 | `secondary/40` 或 `secondary/20` | 可选 `border-border/60` | 次级容器（如 inspector aside） |

**禁止**在同一容器里再开 depth-3。需要更深层级时优先 SectionTitle + space-y 而不是再开盒子。

---

## 5. Typography

字体由 `--font-sans`（Inter Variable）和 `--font-mono`（JetBrains Mono Variable）定义。

| 用途 | class 约定 |
|------|-----------|
| 表单 label | `.agentdash-form-label`（12px / `text-muted-foreground` / 普通字距） |
| Section title | `text-[10px] font-semibold uppercase tracking-wider text-muted-foreground`（封装在 `SectionTitle` primitive） |
| 卡片 subtitle / mono 路径 | `font-mono text-[11px] text-foreground/80` |
| Body | `text-sm` / `text-xs` |
| 数字徽标 | `text-[10px] font-medium` |

不要写 `font-semibold uppercase tracking-[0.14em]` 这类 ad-hoc 微调；走 primitive 或 `agentdash-*` 工具类。

---

## 6. Primitive 索引

实现：`packages/ui/src/primitives/`，导出在 `packages/ui/src/index.ts`。

| Primitive | 用途 | variant / 关键 prop |
|-----------|------|---------------------|
| `Button` | 通用按钮 | `variant`: primary / secondary / danger / ghost；`size`: sm/md/icon。空心边框风格。 |
| `Badge` | 状态徽标 | `variant`: neutral / primary / success / warning / danger / info / accent |
| `Card` / `CardHeader` | 容器 | depth-1 卡片 |
| `CardMenu` | 卡片右上角三点菜单 | items: { key, label, danger?, badge?, onSelect } |
| `DetailPanel` | 右侧抽屉容器 | open / onClose / title / subtitle / headerExtra / widthClassName |
| `DetailSection` | 抽屉内分段卡片 | title / description / extra / compact |
| `DetailMenu` | 抽屉头部三点菜单 | items: { key, label, danger?, disabled?, onSelect } |
| `DangerConfirmDialog` | 危险操作确认弹窗 | expectedValue 触发"输入匹配才能确认" |
| `EmptyState` | 空态 | dashed 边框 |
| `Field` | 表单 label + control 包裹 | label slot + children |
| `CheckboxField` | 复选框 + label | |
| `TextInput` / `Textarea` / `Select` | 表单控件 | 走 `agentdash-form-*` token 类 |
| `Notice` | 行内提示 | `tone`: info / success / warning / danger |
| `OriginBadge` | 资产来源徽标 | `tone`: neutral / accent / success / info / warning；带可选 `url` 截短 |
| `InspectorRow` | 标签 + 值 行 | `mono` 控制等宽 |
| `StatusDot` | 状态指示点 | `tone`: success / warning / danger / info / primary / muted；`pulse` |
| `SectionTitle` | 区段顶栏 | title + 可选 subtitle / badge / actions / sticky |

---

## 7. 业务侧落地原则

- 出现新需求时，先在 `/dev/design-system` 看是否已有 primitive；没有再考虑新增。
- 新建 primitive 必须同时：
  1. 实现文件 `packages/ui/src/primitives/<Name>.tsx`
  2. 在 `packages/ui/src/index.ts` 导出
  3. 在 `DesignSystemPage` 增加预览
  4. 在本文档第 6 节登记
- ESLint warning 不应在新代码里出现；遗留 warning 随后续重构逐步清掉，不阻塞交付。
- 历史代码迁移按"接触原则"：碰到的文件顺手迁，不为迁移单独开任务（除非同 PRD 列了批量目标）。
