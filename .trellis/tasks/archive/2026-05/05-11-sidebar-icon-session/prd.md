# Sidebar 导航重构：横排 icon + 底部状态 + session 快捷列表

## 背景与动机

当前 [workspace-layout.tsx](../../../frontend/src/components/layout/workspace-layout.tsx) 的左侧栏（w-72）是"三明治"结构：
- 顶部：APP 徽标 + AgentDash 标题 + 事件流状态文字
- 中段：ProjectSelector + 4 项竖排导航卡（Agent/Story/Assets/Routine）
- 底部：后端连接 card + 当前身份 card + 设置&主题切换

存在三个问题：

1. **纵向空间挤占**：4 项竖排导航 ~200px + 两个 card ~180px，高频访问的 session 列表在 sidebar 无位置，只能挤进 Agent tab 主视图。
2. **视觉语言割裂**：三处 `rounded-[12px] border bg-secondary/35 p-2.5` 的 card 嵌套，和主内容区越来越 flat 的语言不统一。
3. **信息密度错配**：后端连接展开后显示执行器 + 可访问路径，属"深度设置面板"，不该在 sidebar 常驻占地；但连接状态点仍需能扫视。

## 目标

把 sidebar 从"导航+设置面板"重构为"横排导航 + session 快捷入口 + 底部状态条"，释放纵向空间给高频 session 列表，统一到更 flat 的视觉语言。

## 范围与非目标

### In scope

1. **顶部 Nav 横排化**：4 个视图（Agent/Story/Assets/Routine）压成一排纯 icon 按钮 + tooltip，去掉外层 card 包裹；保留现有 `isAgentActive` / `isStoryActive` / `isAssetsActive` / `isRoutineActive` 的子路由继承高亮逻辑（`/workflow/:id` 仍高亮 Assets、`/session/:id` 仍高亮 Agent 等）。
2. **中段 Session 快捷列表**：sidebar 中段常驻一个 session 快捷入口列表（精简形态，仅行式展示），点击直接跳转到 `/session/:id`。用于跨 tab 快速切换到会话；**不实现 filter / 分组头 / 搜索等完整管理能力**。
3. **底部 Status Strip**：后端连接 + 当前身份合并到底部一个区域，使用 **就地展开**（沿用当前点击展开内联的交互），但去掉 card 包裹换成更 flat 的样式（分隔线 + hover 反馈）。
4. **事件流状态位置调整**：从 header 的第二行文字挪到底部状态区（与"连接"语义同类，集中在一处）。
5. **设置 + 主题切换**：保留在底部最末，作为 icon button（与新的底部状态条视觉协调）。
6. **去 card 化**：移除 sidebar 内所有 `rounded-[12px] border bg-secondary/35 p-2.5` 的包裹卡片样式，统一为 flat 布局 + 分隔线。

### Out of scope

- **Agent tab 主视图不动**：[active-session-list.tsx](../../../frontend/src/features/agent/active-session-list.tsx) 的完整版（带 filter/分组头/Story 折叠/Companion 展开）保留在 Agent tab，作为"会话查看与管理"的主入口。
- **Sidebar session 快捷列表 vs Agent tab 主视图的代码抽象**：MVP 阶段可以复用数据源（`useProjectStore` / 同类 hook），但 **不强制抽公共组件**，因为精简形态和完整形态的交互差异较大；后续如出现维护成本再议。
- **ProjectSelector 位置**：保留在当前位置（header 下方或顶部 Nav 附近），不做大改。
- **全局设计语言扫盲**：本任务只改 sidebar；主内容区若有残留 card 风格不在本任务范围。
- **移动端/窄屏 sidebar 折叠**：本任务仍按桌面端 w-72 设计，不做响应式折叠。

## 设计决策（已对齐）

| 决策点 | 选择 |
|--------|------|
| Agent tab 去留 | **保留**，Sidebar session 列表是快捷入口，完整管理仍在 Agent tab |
| Nav 形态 | **纯 icon + tooltip**（最节省纵向空间） |
| 状态区展开方式 | **就地展开**（沿用当前行为，只改 flat 样式） |

## 布局草图

```
┌─────────────────────┐
│ [APP] AgentDash     │  ← header，去掉事件流第二行
├─────────────────────┤
│ 🎯  📖  📦  🔁      │  ← 横排 icon Nav（纯 icon + tooltip + 激活态）
├─────────────────────┤
│ ProjectSelector     │
├─────────────────────┤
│ Sessions            │  ← 小标题（可选）
│ ● session A  2m     │
│ ● session B  1h     │  ← session 快捷列表（flex-1，占据大部分空间）
│ ● session C  3h     │
│ ...                 │
│ ↓                   │
├─────────────────────┤
│ ● 后端 (3/4)  展开  │  ← flat 就地展开
│ ● 事件流已连接      │
│ 👤 yihao     展开   │
├─────────────────────┤
│ ⚙ 设置   🌓 主题    │  ← icon button 行
└─────────────────────┘
```

## 风险与回归点

1. **路由高亮继承逻辑**：`useMatch` 相关的 4 个 hook 调用必须保留顺序（见 [workspace-layout.tsx:49-57](../../../frontend/src/components/layout/workspace-layout.tsx#L49-L57) 注释）；`agentNavTarget` / `storyNavTarget` / `assetsNavTarget` / `routineNavTarget` 的 `rememberedPath` 记忆逻辑不能丢。
2. **icon 语义识别成本**：纯 icon 对新用户不友好，必须配 tooltip，且激活态视觉（底色 / 下划线）要足够明显。
3. **Session 快捷列表数据源**：需要确认是否已有现成 store/hook（`useProjectStore` / sessions 相关），避免另起数据源。
4. **事件流状态迁移**：`connectionState` 从 header 挪到底部状态区，4 种状态文案（connected/reconnecting/connecting/其他）保留语义。
5. **后端展开面板内容**：原 `BackendConnectionPanel` 展开后的执行器列表 + 可访问路径 + backend_type/online/id 元信息需原样保留，只改外层包裹。

## 验收标准

- [ ] sidebar 宽度保持 w-72（不变）
- [ ] 4 视图切换使用横排纯 icon + tooltip，点击跳转与原行为一致
- [ ] 子路由激活态保留（`/workflow/:id` → Assets 高亮；`/session/:id` → Agent 高亮；`/story/:id` → Story 高亮）
- [ ] Settings 页面返回时 `rememberedPath` 逻辑保持工作（退出 /settings 能回到进入前的路径）
- [ ] Session 快捷列表点击能跳转到 `/session/:id`
- [ ] 底部后端连接点击展开显示原有全部信息（执行器列表 / 可访问路径 / backend_type / online / id）
- [ ] 底部当前身份点击展开显示原有全部信息（display_name / email / provider / groups / admin / mode）
- [ ] 事件流状态在底部状态区可见，4 种状态文案正确
- [ ] 设置 + 主题切换 icon button 保留功能
- [ ] sidebar 内无 `rounded-[12px] border bg-secondary/35` 的 card 包裹
- [ ] 类型检查通过，无新增 lint 错误
