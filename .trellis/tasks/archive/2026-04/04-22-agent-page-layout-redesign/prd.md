# 前端 Agent 页布局优化

## Goal

当前 Agent Tab 左右两栏都用卡片流展示 Agent 和 Session，信息密度低、缺分组/筛选/搜索，Agent 和 Session 规模上来后视觉繁琐难操作。目标是通过"紧凑化 + 层级化 + 增加筛选/搜索"三步纯视觉层改造，让 Agent Tab 在 10+ Agent / 50+ Session 规模下仍然清晰可用。

## What I already know

- 当前布局：`WorkspaceLayout` 左栏 480px（`ProjectAgentView`）+ 右栏 flex-1（未选中 session 时 `ActiveSessionList`；已选中时 `SessionChatView` 内嵌聊天）
- Session 数据模型支持三类归属：`owner_type: "project" | "story" | "task"`，Task 会话带 `story_id` 字段 → 可以做 Story→Task 一级嵌套分组
- 现有 `parent_session_id` 用于 Companion session 的父子关系，与 Story→Task 分组是两套独立逻辑
- 完整会话页路由 `/session/:sessionId` 已存在（`SessionPage` 懒加载）
- 项目内没有统一的快捷键 hook，都是 `window.addEventListener("keydown", ...)` 原生实现
- 技术栈：React 19 + React Router 7 + Tailwind 4 + Zustand 5

## Consensus（讨论已达成的核心决策）

1. **整体布局宽度**：左 Agent 列表 **360px** + 右 Session 列表 flex-1
2. **内嵌聊天栏**：**保留**现有 `SessionChatView` 内嵌行为（本次不动交互模型，专注视觉层）
3. **Agent 展示**：紧凑卡片（保留 border + rounded + 弱 hover 阴影，padding 适中），默认仅显示 2-3 个核心 Tag（执行器 + 模型 + 推理级别），点击展开详情（描述全文、其他全部标签、历史会话折叠面板、底部操作按钮）；**允许多个同时展开**；卡片右上角常驻圆形图标按钮（💬 / +），点击直接新建会话
4. **Session 展示**：line-row + `border-b` 分隔；Story 会话作 root、同 `story_id` 下的 Task 会话作 child（一级嵌套）；Companion session 默认折叠，父 session 行右侧显示 `+N companion` 徽标，点击展开；hover 出操作按钮
5. **Session 头部**：加精简筛选条（搜索框 + 状态 tab）
6. **Agent 栏头部**：加搜索框（按名字/描述过滤）

## Assumptions（若有异议用 Other 指出，否则按此实现）

- 日常规模 <30 Agent + <100 Session，**不引入虚拟滚动**；规模继续上行再评估
- Story 分组默认**展开**；用户手动折叠的状态持久化到 localStorage（按 project_id + story_id 作 key）
- Agent 卡片展开**非互斥**（多个可同时展开），展开/收起状态本地 useState 管理（不持久化）
- 响应式：最小支持宽度 1024px；<1024px 时保持当前双栏（必要时水平滚动），不做抽屉/icon-only 收起

## Open Questions

（已全部收敛，保留此节便于后续迭代新增）

## Requirements（渐进完善）

- 左栏 Agent 列表：紧凑卡片展示，搜索，点击展开/收起详情；卡片右上角常驻"新建会话"快捷按钮
- 右栏 Session 列表：line-row 展示，搜索 + 状态 tab 筛选，Story→Task 一级嵌套，hover 操作按钮
- 点击 session 行的行为保持现状（切换右栏为内嵌 `SessionChatView`）
- 布局宽度 360px : flex-1

## Acceptance Criteria（渐进完善）

- [ ] 10 个 Agent + 50 个 Session 场景下，单屏不滚动可见全部 Agent 名单和至少 15 条 Session 行
- [ ] 左右栏搜索框输入即时过滤（< 200ms）
- [ ] Session 列表按 Story 正确分组，Task session 缩进到所属 Story 下
- [ ] 状态 tab 切换正确过滤 Session
- [ ] Companion session 默认折叠，父行显示 `+N companion` 徽标，点击可展开/收起
- [ ] Agent 卡片支持多个同时展开
- [ ] Agent 卡片右上角"新建会话"按钮可见、点击直接触发新建会话
- [ ] Story 分组折叠状态持久化到 localStorage
- [ ] 保持现有 SSE 实时更新、active stores 流、内嵌 `SessionChatView` 切换逻辑完全兼容

## Definition of Done

- 单元/集成测试补齐（关键：列表过滤、分组渲染、Companion 折叠、localStorage 持久化）
- Lint / typecheck / CI 全绿
- 与现有内嵌 `SessionChatView` 切换逻辑验证兼容
- 行为变更同步到相关 spec/notes

## Out of Scope

- 后端 API 改动（纯前端重构）
- SessionPage 自身功能变更
- Agent / Session CRUD 流程变更
- 主题色、设计系统级别调整
- 移动端适配（桌面端为主，最小宽度 1024px）
- **移除内嵌 `SessionChatView`**（保留现有行为，本次只改视觉层）
- **左右联动筛选按钮**（"仅看此 Agent 的会话"按钮挪到后续迭代）
- **Agent 搜索快捷键**（后续迭代）

## Technical Notes

### 关键文件

- `frontend/src/features/agent/agent-tab-view.tsx:182` — Agent Tab 顶层（**左栏宽度实际在此**：`w-[480px]`）
- `frontend/src/features/project/project-agent-view.tsx:485` — Agent Hub（左栏）
- `frontend/src/features/agent/active-session-list.tsx:243` — Session 列表（右栏）
- `frontend/src/components/layout/workspace-layout.tsx:13` — 全局布局框架（`w-72` 全局侧栏，非本次目标）
- `frontend/src/App.tsx:264` — `/session/:sessionId` 路由
- `frontend/src/types/session.ts` — Session 数据模型

### 数据模型要点

- Session 归属三态：`owner_type: "project" | "story" | "task"`
- Task session 有 `story_id` → 可挂到 Story root 下作 child
- `parent_session_id` 是 Companion 语义，与 Story→Task 嵌套是两套独立逻辑 → 新布局需明确两者共存方式

## Decision (ADR-lite)

**Context**：当前 Agent Tab 用卡片流展示 Agent 和 Session，信息密度低、缺分组/筛选，大规模下难用。初轮讨论中曾设想"去掉内嵌聊天栏 + 左右联动筛选按钮"作为激进重构，但后续收敛到"只做视觉层紧凑化，保留现有交互"。

**Decision**：本次任务范围收敛至 **纯视觉层改造** —— line-row 压缩 + Story→Task 分组 + Companion 折叠 + 左右栏搜索 + Session 状态 tab；**保留** `SessionChatView` 内嵌聊天、`parent_session_id` 语义、现有路由与 stores。"移除内嵌聊天栏"和"左右联动筛选按钮"作为下一轮迭代候选。

**Consequences**：
- 短期见效快，风险小，不触碰数据模型或交互模型
- Story→Task 嵌套与 Companion 折叠两套结构并存，渲染逻辑需谨慎拆分
- 未来迭代若决定去除内嵌聊天，现有代码已紧凑化，拆除成本更低

## Implementation Plan（小步快跑）

- **PR1** — 骨架与 Agent 行紧凑化
  - `ProjectAgentView` Agent 卡片重构为 line-row 组件；默认态 + 展开态分离
  - Agent 搜索框接入（客户端字符串匹配 name/description）
  - 布局宽度从 480px 改到 360px
  - 单元测试：展开互斥、搜索过滤
- **PR2** — Session 行紧凑化 + Story→Task 分组
  - `ActiveSessionList` 卡片重构为 line-row
  - 新增按 `owner_type + story_id` 分组的树状渲染逻辑（独立于现有 companion 树）
  - Story 分组标题带折叠切换，状态持久化到 localStorage
  - 单元测试：分组正确性、折叠持久化、孤儿 session 降级
- **PR3** — Session 筛选条 + Companion 折叠徽标
  - Session 头部搜索框 + 状态 tab（客户端筛选）
  - Companion 默认折叠，父行显示 `+N` 徽标 + 展开切换
  - 集成测试：搜索/状态/分组/折叠四种交互组合正确
  - 视觉回归：与现有内嵌 `SessionChatView` 切换体验一致
