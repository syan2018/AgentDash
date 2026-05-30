# Design — Routine 交互简化

## 设计原则

1. **三要素心智模型**：谁（Agent）+ 做什么（Runbook）+ 什么时候（Trigger）
2. **渐进披露**：核心路径 ≤ 5 个字段，高级选项折叠隐藏
3. **与项目设计语言一致**：参考 Agent Preset Editor 的大弹窗 + 分区布局；参考 multica autopilot 的左右分栏

## 布局方案

### Create/Edit Dialog — 左右双栏

```
+──────────────────────────────────────────────────────────────────+
│  Header: [ROUTINE] 创建 Routine / 编辑 Routine                    │
│          副标题: "配置一个自动化任务"                                 │
├────────────────────────────────────┬─────────────────────────────┤
│  LEFT (flex-1, min-w-0)            │  RIGHT (w-[320px])          │
│                                    │  bg-secondary/10            │
│  ┌─ 名称 ─────────────────────┐   │                             │
│  │ daily-code-review           │   │  SECTION: 执行 Agent        │
│  └─────────────────────────────┘   │  [Agent Picker / Select]    │
│                                    │                             │
│  ┌─ 执行指令 (Runbook) ───────┐   │  ─────────────────────────  │
│  │                             │   │                             │
│  │  你是一个代码审查助手。      │   │  SECTION: 触发方式          │
│  │  请检查最新的 PR 并给出      │   │  [定时] [Webhook]           │
│  │  review 意见...              │   │   (segment toggle)          │
│  │                             │   │                             │
│  │                             │   │  ┌── Schedule Picker ──┐   │
│  │                             │   │  │ 频率: [每天 ▾]       │   │
│  │                             │   │  │ 时间: [09]:[00]      │   │
│  │                             │   │  │                      │   │
│  │                             │   │  │ cron: 0 9 * * *      │   │
│  │                             │   │  │ "每天 09:00 执行"    │   │
│  │                             │   │  └──────────────────────┘   │
│  │                             │   │                             │
│  └─────────────────────────────┘   │  ─────────────────────────  │
│                                    │                             │
│  [▸ 模板变量] (折叠)               │  SECTION: 高级设置 (折叠)    │
│                                    │  Session 策略: [fresh ▾]    │
│                                    │  Entity Key Path: [...]     │
│                                    │                             │
├────────────────────────────────────┴─────────────────────────────┤
│  Footer:  [取消]  [创建]                                          │
+──────────────────────────────────────────────────────────────────+
```

**尺寸规格**：
- Dialog: `max-w-[min(960px,80vw)]`, `min-h-[480px]`, `max-h-[75vh]`
- Left: `flex-1`, padding `p-5`, 内容纵向 `space-y-4`
- Right: `w-[320px]`, `border-l border-border`, `bg-secondary/5`, padding `p-5`, 内容 `space-y-5`
- 小屏 (`< lg`): 切回单列布局，right 栏折叠到 left 下方

### 右栏 Section 详细设计

**Section Label 样式**：复用 multica 的 `text-[11px] font-semibold tracking-[0.08em] text-muted-foreground uppercase mb-2`

**Agent Picker**：
- 当前使用 `<Select>` 即可（项目 agent 数量有限）
- 展示 agent 名称，无需头像

**触发方式切换**：
- Segment toggle（两个 pill button）：`定时` | `Webhook`
- 样式参考 Preset Editor 的 tab pill: `rounded-[8px] border px-3 py-1.5 text-xs font-medium`
- 选中态: `bg-background text-foreground shadow-sm border-border`
- 未选中: `text-muted-foreground border-transparent`

**Schedule Picker（定时触发）**：
- 沿用当前 `CronScheduleSelector` 逻辑，但精简选项
- 频率下拉: 每 N 分钟 / 每 N 小时 / 每天 / 工作日（去掉"不启用"选项，选定时即默认有 cron）
- 紧凑布局: 频率 + 参数在同一行
- 底部灰色小字显示 cron 表达式 + 自然语言描述

**Webhook 信息区（创建时）**：
- 简洁提示: "Endpoint 和 Token 将在创建后自动生成"
- 创建后: 弹出 token modal（复用现有 `WebhookTokenAlert`，增加 curl 示例）

**高级设置折叠区**：
- 使用 `<details>` + `summary` 原生折叠
- 样式: `rounded-[8px] border border-border/50 bg-secondary/10`
- 包含: Session 策略选择 + entity_key_path（仅 webhook + per_entity 时显示）

### 左栏详细设计

**名称输入**：
- 标签: "名称"
- placeholder: "如: daily-code-review"
- 组件: `TextInput` (from `@agentdash/ui`)

**执行指令（Runbook）**：
- 标签: "执行指令"
- 组件: `Textarea`, `flex-1` 占满剩余空间, `min-h-[200px]`
- placeholder: "描述你希望 Agent 在每次触发时执行的任务...\n\n例：检查最近 24 小时的 PR，对每个 PR 给出代码质量评分和改进建议。"
- 不显示模板语法

**模板变量折叠提示**：
- `<details>` 折叠，summary 文本: "支持模板变量 ›"
- 展开后列出可用变量: `{{ trigger.source }}`, `{{ trigger.payload.* }}` 等
- `text-[11px] text-muted-foreground font-mono`

## 列表页改造

### 空状态 — 模板卡片

```
+──────────────────────────────────────────────────────────────────+
│                                                                  │
│          [icon]  还没有 Routine                                   │
│          创建自动化任务，让 Agent 按计划或事件自动执行              │
│                                                                  │
│   ┌─────────────┐  ┌─────────────┐  ┌─────────────┐            │
│   │ 📋 每日代码  │  │ 🔍 PR 自动   │  │ 📊 定时报告  │            │
│   │    审查      │  │    Review   │  │    生成      │            │
│   │             │  │             │  │             │            │
│   │ 每天早上检查 │  │ 新PR时自动  │  │ 每周五下午   │            │
│   │ 代码变更... │  │ review...  │  │ 汇总本周... │            │
│   └─────────────┘  └─────────────┘  └─────────────┘            │
│                                                                  │
│                   [+ 从空白创建]                                   │
│                                                                  │
+──────────────────────────────────────────────────────────────────+
```

- Grid: `grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3`
- 模板卡片: `rounded-[8px] border border-border hover:border-primary/30 bg-background p-4 cursor-pointer transition-colors`
- 点击模板: 打开 Create Dialog 并预填 name + prompt_template + trigger_config

### 预设模板数据

```typescript
const ROUTINE_TEMPLATES = [
  {
    name: "每日代码审查",
    prompt: "检查过去 24 小时内的代码提交，识别潜在问题并生成审查报告。",
    trigger: { type: "scheduled", cron: "0 9 * * *" },
  },
  {
    name: "PR 自动 Review",
    prompt: "对新提交的 Pull Request 进行代码审查，检查代码质量、安全性和性能问题。",
    trigger: { type: "webhook" },
  },
  {
    name: "定时进度报告",
    prompt: "汇总本周项目进展，包括已完成任务、进行中任务和阻塞项，生成周报。",
    trigger: { type: "scheduled", cron: "0 17 * * 5" },
  },
  {
    name: "依赖安全扫描",
    prompt: "扫描项目依赖库的安全公告，报告任何新发现的 CVE 漏洞及修复建议。",
    trigger: { type: "scheduled", cron: "0 8 * * 1" },
  },
];
```

## Webhook Token Modal 增强

在现有 `WebhookTokenAlert` 基础上增加 curl 调用示例：

```
POST {origin}/api/routine-triggers/{endpoint_id}/fire

curl -X POST {fullUrl} \
  -H "Authorization: Bearer {token}" \
  -H "Content-Type: application/json" \
  -d '{"message": "hello"}'
```

- `fullUrl` 使用 `window.location.origin + path` 拼接
- 代码块带一键复制按钮

## 组件拆分

当前 `routine-tab-view.tsx` 是 923 行单文件。重构为：

```
features/routine/
├── routine-tab-view.tsx          # 入口：列表 + 空状态 + 路由
├── routine-card.tsx              # 单个 Routine 卡片
├── routine-dialog.tsx            # 创建/编辑双栏弹窗（主体）
├── routine-dialog-sidebar.tsx    # 右栏配置区域
├── cron-schedule-selector.tsx    # Cron 分段选择器（从原文件提取）
├── webhook-token-alert.tsx       # Webhook token 展示弹窗
├── execution-history-panel.tsx   # 执行历史 DetailPanel
├── routine-templates.ts          # 预设模板数据
└── form-state.ts                 # 表单类型、转换函数、校验逻辑
```

## 兼容性处理

- 已创建的 `plugin` 类型 Routine：在列表中正常展示（badge 显示 "Plugin"），编辑时 trigger_type 字段 disabled + 仍展示 plugin 配置区
- 已创建的 `per_entity` / `reuse` Session 策略：编辑时在高级设置区正常显示
- API payload 格式不变，前端仅调整 UI 入口和默认值

## 不做的事

- 不改后端数据模型
- 不引入 rich text editor（Runbook 保持 plain textarea）
- 不做 trigger 1:N 拆分
- 不做 execution mode 概念
