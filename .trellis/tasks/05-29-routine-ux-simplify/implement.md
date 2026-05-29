# Implementation Plan — Routine 交互简化

## 前置条件

- 当前实现: `packages/app-web/src/features/routine/routine-tab-view.tsx` (923 行单文件)
- 无后端改动，纯前端重构
- UI 库组件可直接使用: `TextInput`, `Textarea`, `Select`, `Field`, `Button`, `CreateButton`, `DetailPanel`

## 执行步骤

### Step 1: 提取 form-state 逻辑

**目标**: 将类型定义、表单转换、校验逻辑从 view 层分离

- [ ] 创建 `features/routine/form-state.ts`
- [ ] 迁移: `RoutineFormState` 接口、`INITIAL_FORM`、`routineToForm()`、`formToPayload()`、`validateForm()`
- [ ] 移除 Plugin 相关的 UI 校验分支（保留 payload 构造能力）
- [ ] 修改 `validateForm`: session_mode 默认 "fresh"，不再强制校验 per_entity（由高级设置自行保证）

**验证**: 编译通过，原有功能不变

### Step 2: 提取子组件

**目标**: 将 routine-tab-view.tsx 拆成独立文件

- [ ] `features/routine/cron-schedule-selector.tsx` — 搬出 `CronScheduleSelector` + `cronToSegments` + `segmentsToCron` + `describeCron` + 常量
- [ ] `features/routine/routine-card.tsx` — 搬出 `RoutineCard` 组件
- [ ] `features/routine/webhook-token-alert.tsx` — 搬出 `WebhookTokenAlert`
- [ ] `features/routine/execution-history-panel.tsx` — 搬出 `ExecutionHistoryContent`
- [ ] `features/routine/routine-templates.ts` — 新建模板数据文件

**验证**: 编译通过，页面渲染不变

### Step 3: 重写 RoutineDialog — 双栏布局

**目标**: 新建 `routine-dialog.tsx` + `routine-dialog-sidebar.tsx`，替换原有单列弹窗

- [ ] `routine-dialog.tsx`: 外壳（overlay + header + footer + 左右分区容器）
  - max-w: `min(960px, 80vw)`
  - 左栏: 名称 + Runbook textarea + 模板变量折叠
  - 右栏: import RoutineDialogSidebar
- [ ] `routine-dialog-sidebar.tsx`: 右栏配置
  - Agent Select section
  - Trigger 切换 (segment toggle: 定时 | Webhook)
  - Schedule picker / Webhook info (条件渲染)
  - 高级设置折叠区 (Session 策略)
- [ ] CronScheduleSelector 中去掉 "不启用" 频率选项（选定时则必有 cron）
- [ ] Plugin trigger: 创建模式不可选；编辑模式若原类型为 plugin 则展示（disabled）

**验证**: 
- 创建 scheduled routine: 左栏填名称+指令，右栏选 agent + 定时 → 成功
- 创建 webhook routine: 右栏选 webhook，创建后弹 token modal
- 编辑 plugin routine: 表单正常加载，trigger 区域显示 plugin 信息（只读）

### Step 4: 空状态 + 模板卡片

**目标**: 列表为空时展示模板引导

- [ ] 在 `routine-tab-view.tsx` 中判断 routines.length === 0 时渲染空状态
- [ ] 空状态: icon + 文案 + 模板卡片 grid (2-3 列) + "从空白创建" 按钮
- [ ] 点击模板卡片: 打开 RoutineDialog，预填 name + prompt_template + trigger 信息
- [ ] 卡片样式: `rounded-[8px] border border-border hover:border-primary/30 p-4 cursor-pointer`

**验证**: 无 Routine 时看到模板；点击模板正确预填表单

### Step 5: Webhook Token Modal 增强

**目标**: 增加 curl 调用示例

- [ ] 在 `WebhookTokenAlert` 中增加"调用示例"区块
- [ ] 使用 `window.location.origin` 拼接完整 URL
- [ ] curl 代码块带"复制"按钮
- [ ] 保持原有 token 展示和警告文案

**验证**: 创建 webhook routine 后，modal 展示完整 curl 示例且可复制

### Step 6: 收尾清理

- [ ] 删除 `routine-tab-view.tsx` 中已搬出的旧代码（确保只剩入口组件 + import）
- [ ] 确认编辑模式下 plugin/per_entity 等遗留配置正常展示
- [ ] 整体 lint + type-check 通过
- [ ] 浏览器手动验证: 创建/编辑/删除/启停/执行历史 全路径

## 风险点

| 风险 | 缓解 |
|------|------|
| CronScheduleSelector 提取后 state 丢失 | 确保组件为受控组件，value/onChange 接口不变 |
| Plugin routine 编辑回显 | 保留 formToPayload 中 plugin 分支，仅隐藏创建入口 |
| 小屏双栏布局挤压 | `< lg` 时改为纵向堆叠（responsive flex-direction） |

## 不在本次做

- 后端 API 改动
- Rich text editor
- Trigger 1:N 关系
- Plugin UI registry
